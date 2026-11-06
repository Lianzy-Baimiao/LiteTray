//! Native UI-thread watchdog. It never calls a XAML object from a worker thread.
//! The owner of this object must keep the adapter module loaded until Drop.
use crate::{
    session::{Session, State},
    wide,
};
use std::{
    cell::{Cell, RefCell},
    ptr::{null, null_mut},
    rc::Rc,
};
use windows_sys::Win32::{
    Foundation::*,
    System::{LibraryLoader::*, SystemInformation::GetTickCount64, Threading::*},
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
}
impl Context {
    unsafe fn alive(&self) -> bool {
        unsafe {
            if WaitForSingleObject(self.process, 0) != WAIT_TIMEOUT {
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
            let class = wide(&format!("{CLASS}.{}", GetCurrentThreadId()));
            let mut owner = Self {
                hwnd: null_mut(),
                context: Rc::new(Context {
                    session: RefCell::new(Some(session)),
                    process,
                    receiver,
                    process_id,
                    error: RefCell::new(None),
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
            if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return Err("注册时钟监视窗口失败".into());
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
        let mut session = self
            .context
            .session
            .borrow_mut()
            .take()
            .ok_or("时钟更新正在进行")?;
        let result = if unsafe { self.context.alive() } {
            session.start(frame, unsafe { GetTickCount64() })
        } else {
            Err("时钟宿主已失联".into())
        };
        *self.context.session.borrow_mut() = Some(session);
        result
    }
    pub fn receive(&self, bytes: &[u8]) -> Result<(), String> {
        // Take the session out before COM calls; a reentrant timer sees None.
        let mut session = self
            .context
            .session
            .borrow_mut()
            .take()
            .ok_or("时钟更新正在进行")?;
        let result = if unsafe { self.context.alive() } {
            session.receive(bytes, unsafe { GetTickCount64() })
        } else {
            match session.stop() {
                Ok(()) => Err("时钟宿主已失联".into()),
                Err(e) => Err(e),
            }
        };
        *self.context.session.borrow_mut() = Some(session);
        result
    }
    pub fn state(&self) -> State {
        self.context
            .session
            .borrow()
            .as_ref()
            .map_or(State::Restoring, Session::state)
    }
    pub fn error(&self) -> Option<String> {
        self.context.error.borrow().clone()
    }
    pub fn stop(&self) -> Result<(), String> {
        let mut session = self
            .context
            .session
            .borrow_mut()
            .take()
            .ok_or("时钟更新正在进行")?;
        let result = session.stop();
        *self.context.session.borrow_mut() = Some(session);
        if result.is_ok() && !self.hwnd.is_null() {
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
                let hwnd = std::mem::replace(&mut self.hwnd, null_mut());
                KillTimer(hwnd, TIMER);
                // Remove the pointer before restoring properties can reenter.
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                DestroyWindow(hwnd);
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
        if message == WM_TIMER && w == TIMER {
            let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Context;
            if !pointer.is_null() {
                // Retain Context across XAML callbacks that might synchronously
                // close the owner window. Its session is already detached.
                Rc::increment_strong_count(pointer);
                let context = Rc::from_raw(pointer);
                // Do not hold a Rust reference into Context while invoking COM.
                // XAML can pump messages synchronously on this same thread.
                let session = context.session.borrow_mut().take();
                if let Some(mut session) = session {
                    let alive = context.alive();
                    let result = session.poll(GetTickCount64(), alive);
                    let closed = session.state() == State::Closed;
                    *context.error.borrow_mut() = result.err();
                    *context.session.borrow_mut() = Some(session);
                    if closed && GetWindowLongPtrW(hwnd, GWLP_USERDATA) == pointer as isize {
                        KillTimer(hwnd, TIMER);
                    }
                }
            }
            return 0;
        }
        DefWindowProcW(hwnd, message, w, l)
    }
}
