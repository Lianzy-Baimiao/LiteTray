//! Native UI-thread watchdog. It never calls a XAML object from a worker thread.
//! The owner of this object must keep the adapter module loaded until Drop.
use crate::{
    session::{InputTarget, Session, State},
    wide,
};
use std::{
    cell::{Cell, RefCell},
    ptr::{null, null_mut},
    rc::Rc,
};
use windows_sys::Win32::{
    Foundation::*,
    System::{
        DataExchange::COPYDATASTRUCT, LibraryLoader::*, SystemInformation::GetTickCount64,
        Threading::*,
    },
    UI::WindowsAndMessaging::*,
};
const CLASS: &str = "LiteTray.ClockWatchdog.v1";
const TIMER: usize = 1;
const INTERVAL: u32 = 1000;

struct Context {
    session: RefCell<Option<Session>>,
    process: HANDLE,
    receiver: HWND,
    process_id: u32,
    error: RefCell<Option<String>>,
    detached: Cell<bool>,
}
impl Context {
    fn status(&self) -> litetray_adapter_protocol::clock::SessionStatus {
        use litetray_adapter_protocol::clock::SessionStatus as Status;
        let Ok(session) = self.session.try_borrow() else {
            return Status::Busy;
        };
        match session.as_ref().map(Session::state) {
            None => Status::Busy,
            Some(State::Prepared) => Status::Prepared,
            Some(State::Active) => Status::Active,
            Some(State::Restoring) => Status::Restoring,
            Some(State::Closed) => Status::Closed,
        }
    }
    // Detach before COM calls. Reentrant messages observe an unavailable
    // session rather than borrowing it while a native setter pumps messages.
    fn with_session(
        &self,
        apply: impl FnOnce(&mut Session) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut session = self.session.borrow_mut().take().ok_or("时钟更新正在进行")?;
        let result = apply(&mut session);
        // Destruction can reenter during `apply`; complete restoration before
        // returning the session to a context whose timer no longer exists.
        let restore = if self.detached.get() {
            session.stop()
        } else {
            Ok(())
        };
        if let Err(error) = &restore {
            *self.error.borrow_mut() = Some(error.clone());
        }
        *self.session.borrow_mut() = Some(session);
        restore.and(result)
    }
    fn receive(&self, bytes: &[u8]) -> Result<(), String> {
        self.with_session(|session| {
            if unsafe { self.alive() } {
                session.receive(bytes, unsafe { GetTickCount64() })
            } else {
                session.stop()?;
                Err("时钟宿主已失联".into())
            }
        })
    }
    unsafe fn alive(&self) -> bool {
        unsafe {
            if self.detached.get() || WaitForSingleObject(self.process, 0) != WAIT_TIMEOUT {
                return false;
            }
            let mut pid = 0;
            GetWindowThreadProcessId(self.receiver, &mut pid) != 0 && pid == self.process_id
        }
    }
}
impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            if !self.process.is_null() {
                CloseHandle(self.process);
            }
        }
    }
}
pub struct Watchdog {
    hwnd: HWND,
    // Stable address even while native calls reenter the message procedure.
    context: Rc<Context>,
    // Registration is shared only on this thread; no DLL callback can remain
    // registered when the final owner releases its module.
    class: Vec<u16>,
}
thread_local! {static WINDOWS:Cell<u32>=const{Cell::new(0)};}

impl Watchdog {
    pub(crate) fn retain_sources(
        &self,
        sources: Vec<crate::taskbar_source::Source>,
    ) -> Result<(), String> {
        self.context
            .with_session(|session| session.retain_sources(sources))
    }
    pub fn attach(session: Session) -> Result<Self, String> {
        unsafe {
            // Establish the timer before touching any properties. Failure to
            // create the monitor therefore cannot strand a modified clock.
            if session.state() != State::Prepared {
                return Err("必须先监视会话再启用时钟".into());
            }
            let process_id = session.owner().process_id;
            let receiver = session.owner().receiver as HWND;
            let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, process_id);
            if process.is_null() {
                return Err("无法监视时钟宿主进程".into());
            }
            // A stopped DLL can remain pinned in Explorer. Distinct builds
            // must not call one another's registered window procedure.
            let class = wide(&format!(
                "{CLASS}.{}.{:x}",
                GetCurrentThreadId(),
                procedure as *const () as usize
            ));
            let mut owner = Self {
                hwnd: null_mut(),
                context: Rc::new(Context {
                    session: RefCell::new(Some(session)),
                    process,
                    receiver,
                    process_id,
                    error: RefCell::new(None),
                    detached: Cell::new(false),
                }),
                class,
            };
            if !owner.context.alive() {
                return Err("时钟宿主已退出或接收窗口无效".into());
            }
            let instance = GetModuleHandleW(null());
            let class = &owner.class;
            let wc = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                ..std::mem::zeroed()
            };
            if RegisterClassW(&wc) == 0 {
                if GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                    return Err("注册时钟监视窗口失败".into());
                }
                let mut existing: WNDCLASSW = std::mem::zeroed();
                if GetClassInfoW(instance, class.as_ptr(), &mut existing) == 0
                    || existing.lpfnWndProc.map(|p| p as usize)
                        != Some(procedure as *const () as usize)
                {
                    return Err("任务栏线程仍加载其他版本的时钟适配器，请先完成旧会话恢复".into());
                }
            }
            owner.hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                null_mut(),
                instance,
                Rc::as_ptr(&owner.context).cast(),
            );
            if owner.hwnd.is_null() {
                if WINDOWS.with(|v| v.get()) == 0 {
                    UnregisterClassW(class.as_ptr(), instance);
                }
                return Err("创建时钟监视窗口失败".into());
            }
            WINDOWS.with(|v| v.set(v.get() + 1));
            if SetTimer(owner.hwnd, TIMER, INTERVAL, None) == 0 {
                return Err("创建时钟失联检查计时器失败".into());
            }
            Ok(owner)
        }
    }
    pub fn start(&self, frame: litetray_adapter_protocol::clock::Frame) -> Result<(), String> {
        self.context.with_session(|session| {
            if unsafe { self.context.alive() } {
                session.start(frame, unsafe { GetTickCount64() })
            } else {
                session.stop()?;
                Err("时钟宿主已失联".into())
            }
        })
    }
    pub(crate) fn start_tray(&self) -> Result<(), String> {
        self.context.with_session(|session| {
            if unsafe { self.context.alive() } {
                session.start_tray(unsafe { GetTickCount64() })
            } else {
                session.stop()?;
                Err("托盘宿主已失联".into())
            }
        })
    }
    pub fn prepare_inputs(
        &self,
        targets: Vec<InputTarget>,
        publish: impl Fn(litetray_adapter_protocol::activation::Activation) -> bool + 'static,
    ) -> Result<(), String> {
        let context = Rc::downgrade(&self.context);
        self.context.with_session(|session| {
            session.prepare_inputs(targets, move |event| {
                let Some(context) = context.upgrade() else {
                    return false;
                };
                if !unsafe { context.alive() } {
                    return false;
                }
                // Take Session out while COM validation can pump messages.
                // Stale controls stop and detach before anything is published.
                let allowed = context
                    .with_session(|session| session.validate_input(unsafe { GetTickCount64() }));
                if let Err(error) = allowed {
                    *context.error.borrow_mut() = Some(error);
                    return false;
                }
                publish(event)
            })
        })
    }
    pub fn receive(&self, bytes: &[u8]) -> Result<(), String> {
        self.context.receive(bytes)
    }
    pub(crate) fn input_probe(&self, id: u64) -> Result<crate::input::Probe, String> {
        self.context
            .session
            .borrow()
            .as_ref()
            .ok_or("会话正忙")?
            .input_probe(id)
    }
    /// Advertised only by an established session, never a taskbar HWND.
    pub fn endpoint(&self) -> Option<HWND> {
        if self.owns_window() {
            Some(self.hwnd)
        } else {
            None
        }
    }
    fn owns_window(&self) -> bool {
        !self.hwnd.is_null()
            && !self.context.detached.get()
            && unsafe {
                GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) == Rc::as_ptr(&self.context) as isize
            }
    }
    pub fn state(&self) -> State {
        self.context
            .session
            .borrow()
            .as_ref()
            .map_or(State::Restoring, Session::state)
    }
    pub(crate) fn status(&self) -> litetray_adapter_protocol::clock::SessionStatus {
        self.context.status()
    }
    pub fn error(&self) -> Option<String> {
        self.context.error.borrow().clone()
    }
    pub fn stop(&self) -> Result<(), String> {
        let result = self.context.with_session(Session::stop);
        if result.is_ok() && self.owns_window() {
            unsafe {
                KillTimer(self.hwnd, TIMER);
            }
        }
        result
    }
}
impl Drop for Watchdog {
    fn drop(&mut self) {
        unsafe {
            if !self.hwnd.is_null() {
                let owns_window = self.owns_window();
                let hwnd = std::mem::replace(&mut self.hwnd, null_mut());
                self.context.detached.set(true);
                // Remove the pointer before restoring properties can reenter.
                // Never touch a handle recycled after external destruction.
                if owns_window {
                    KillTimer(hwnd, TIMER);
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    DestroyWindow(hwnd);
                }
                WINDOWS.with(|v| v.set(v.get().saturating_sub(1)));
                if WINDOWS.with(|v| v.get()) == 0 {
                    UnregisterClassW(self.class.as_ptr(), GetModuleHandleW(null()));
                }
            }
            let _ = self.stop();
        }
    }
}
unsafe extern "system" fn procedure(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(l as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        if message == WM_COPYDATA || message == WM_NCDESTROY || message == WM_TIMER && w == TIMER {
            let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Context;
            if !pointer.is_null() {
                // Retain Context across XAML callbacks that might synchronously
                // close the owner window. Its session is already detached.
                Rc::increment_strong_count(pointer);
                let context = Rc::from_raw(pointer);
                if message == WM_NCDESTROY {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                    context.detached.set(true);
                    KillTimer(hwnd, TIMER);
                    // An in-flight call performs this restoration when its
                    // native setter returns; there is no second mutable owner.
                    let _ = context.with_session(Session::stop);
                    return DefWindowProcW(hwnd, message, w, l);
                }
                let accepted = if message == WM_COPYDATA {
                    if w as HWND != context.receiver || l == 0 || !context.alive() {
                        return 0;
                    }
                    let copy = &*(l as *const COPYDATASTRUCT);
                    if copy.dwData == litetray_adapter_protocol::taskbar::STATUS_TAG {
                        if copy.cbData == 0 || copy.cbData > 1024 || copy.lpData.is_null() {
                            return 0;
                        }
                        let bytes = std::slice::from_raw_parts(
                            copy.lpData.cast::<u8>(),
                            copy.cbData as usize,
                        );
                        let Ok(owner) = serde_json::from_slice::<
                            litetray_adapter_protocol::clock::Owner,
                        >(bytes) else {
                            return 0;
                        };
                        let Ok(session) = context.session.try_borrow() else {
                            return 0;
                        };
                        if session.as_ref().is_none_or(|s| s.owner() != &owner) {
                            return 0;
                        }
                        return context.status() as LRESULT;
                    }
                    if copy.dwData != litetray_adapter_protocol::clock::COMMAND_TAG
                        || copy.cbData == 0
                        || copy.cbData as usize > litetray_adapter_protocol::MAX_REQUEST
                        || copy.lpData.is_null()
                    {
                        return 0;
                    }
                    // The sender's buffer is valid only during this callback.
                    // Own the bounded payload before invoking any XAML method.
                    let bytes =
                        std::slice::from_raw_parts(copy.lpData.cast::<u8>(), copy.cbData as usize)
                            .to_vec();
                    context.receive(&bytes).is_ok()
                } else {
                    // A reentrant timer is skipped, without replacing the last
                    // error or extending the session's lease.
                    if context.session.borrow().is_none() {
                        return 0;
                    }
                    let result = context
                        .with_session(|session| session.poll(GetTickCount64(), context.alive()));
                    *context.error.borrow_mut() = result.err();
                    false
                };
                let closed = context
                    .session
                    .borrow()
                    .as_ref()
                    .is_some_and(|s| s.state() == State::Closed);
                if closed
                    && !context.detached.get()
                    && GetWindowLongPtrW(hwnd, GWLP_USERDATA) == pointer as isize
                {
                    KillTimer(hwnd, TIMER);
                }
                return accepted as LRESULT;
            }
            return 0;
        }
        DefWindowProcW(hwnd, message, w, l)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn temporarily_unavailable_session_never_claims_closed() {
        let context = Context {
            session: RefCell::new(None),
            process: null_mut(),
            receiver: null_mut(),
            process_id: 0,
            error: RefCell::new(None),
            detached: Cell::new(false),
        };
        let busy = litetray_adapter_protocol::clock::SessionStatus::Busy;
        assert_eq!(context.status(), busy);
        let _borrow = context.session.borrow_mut();
        assert_eq!(context.status(), busy);
    }
}
