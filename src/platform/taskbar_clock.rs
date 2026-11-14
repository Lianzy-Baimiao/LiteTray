//! Experimental real-taskbar transport. No activation occurs during preflight;
//! activation follows the saved enable setting. Preparation returns authenticated
//! source windows before a Start command is sent to the clock endpoint.
use crate::{
    config::{ClickAction, Config},
    platform::{diagnostics, wide},
    taskbar_safety::{Journal, Permit},
    taskbar_service::{self, ConnectFailure, PreparationFailure},
};
use litetray_adapter_protocol::{
    HostLayout, MAX_REQUEST, VERSION,
    activation::{Activation, EVENT_TAG, Gate, MAX_EVENT},
    clock::{COMMAND_TAG, Change, Command, Frame, Owner, SessionStatus},
    routing::Route,
    taskbar::*,
};
use std::{
    cell::{Cell, RefCell},
    ptr::{null, null_mut},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use windows_sys::Win32::{
    Foundation::*,
    System::{DataExchange::COPYDATASTRUCT, LibraryLoader::*, Threading::*},
    UI::WindowsAndMessaging::*,
};
pub const WM_ACTIVATED: u32 = WM_APP + 12;
const PERIODIC_RPC_MILLIS: u32 = 250;
#[path = "taskbar_live_checks.rs"]
mod live_checks;
pub use live_checks::{
    check_clock_compatibility, check_compatibility, check_secondary, check_secondary_tray,
};
pub struct Native {
    main: HWND,
    journal: Journal,
    retry: Arc<AtomicBool>,
}
pub type Service = taskbar_service::Service<Native>;
pub fn service(main: HWND, config: Config, now: u64) -> Result<Service, String> {
    Service::new(
        Native {
            main,
            journal: crate::taskbar_safety::for_process(
                crate::config::data_dir().join("taskbar-safety.json"),
            )?,
            retry: Arc::new(AtomicBool::new(false)),
        },
        config,
        now,
    )
}
impl taskbar_service::Backend for Native {
    type Prepared = Preflight;
    type Session = Connection;
    fn prepare(
        &self,
        config: &Config,
    ) -> std::sync::mpsc::Receiver<Result<Preflight, PreparationFailure>> {
        let config = config.clone();
        let journal = self.journal.clone();
        let retry = self.retry.swap(false, Ordering::AcqRel);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let prepared = (|| {
                let mut prepared = preflight(&config).map_err(PreparationFailure::temporary)?;
                let image = super::dll::prepare_current(super::dll::Kind::Taskbar)
                    .map_err(PreparationFailure::temporary)?;
                let key = crate::taskbar_safety::identity(
                    &image.sha256,
                    &prepared.layout,
                    prepared.clocks,
                    prepared.mirror_icons,
                )
                .map_err(PreparationFailure::suspended)?;
                prepared.permit = Some(
                    journal
                        .arm(&key, retry)
                        .map_err(PreparationFailure::suspended)?,
                );
                prepared.image = Some(image);
                Ok(prepared)
            })();
            let _ = tx.send(prepared);
        });
        rx
    }
    fn retry(&self) {
        self.retry.store(true, Ordering::Release);
    }
    fn connect(
        &self,
        prepared: Preflight,
        frame: &Frame,
    ) -> Result<Connection, ConnectFailure<Connection>> {
        if prepared.permit.is_none() || prepared.image.is_none() {
            return Err(ConnectFailure {
                error: "安全检查尚未完成，任务栏接入已暂停，请重试".into(),
                pending: None,
                suspended: true,
            });
        }
        let connection =
            Connection::prepare(self.main, prepared).map_err(|error| ConnectFailure {
                error,
                pending: None,
                suspended: false,
            })?;
        if let Err(error) = connection.start(frame.clone()) {
            connection.record_fault();
            return Err(ConnectFailure {
                error,
                pending: Some(connection),
                suspended: false,
            });
        }
        Ok(connection)
    }
}
impl taskbar_service::Session for Connection {
    fn fault(&self) {
        self.record_fault();
    }
    fn healthy(&self) {
        if let Some(permit) = &self.permit {
            permit.healthy();
        }
    }
    fn restored(&self) {
        if self.restored.get() {
            self.confirm_restored();
        }
    }
    fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }
    fn environment_matches(&self) -> bool {
        self.verify_endpoint().is_ok()
            && self.environment.as_ref().is_some_and(|original| {
                super::taskbar_environment::Environment::from_diagnostics(&diagnostics::collect())
                    .as_ref()
                    == Some(original)
            })
    }
    fn process_exited(&self) -> bool {
        unsafe { WaitForSingleObject(self.process, 0) == WAIT_OBJECT_0 }
    }
    fn update(&self, frame: &Frame) -> Result<(), String> {
        self.update(frame.clone())
    }
    fn stop(&self) -> Result<(), String> {
        self.stop()
    }
    fn restore_step(&self) -> Result<taskbar_service::RestoreProgress, String> {
        self.restore_step()
    }
    fn restoration_confirmed(&self) -> bool {
        self.restoration_confirmed()
    }
    fn take_activation(&self) -> Option<Activation> {
        self.take_activation()
    }
}
pub struct Preflight {
    layout: HostLayout,
    routes: Vec<Route>,
    custom_calendar: bool,
    volume_scroll: bool,
    volume_mute: bool,
    source: Option<Route>,
    clocks: bool,
    mirror_icons: bool,
    notice: Option<String>,
    environment: Option<super::taskbar_environment::Environment>,
    image: Option<super::dll::Prepared>,
    permit: Option<Permit>,
}
pub fn preflight(config: &Config) -> Result<Preflight, String> {
    let system = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .ok_or("SystemRoot 不可用")?;
    // Recognition is per module; unsupported tray capabilities do not disable
    // a requested, independently recognized clock. No startup PDB download.
    let (layout, capabilities) = crate::compatibility::negotiate(
        &system,
        config.tray.clock_enabled,
        config.tray.enabled && config.tray.show_icons,
    )?;
    let mut tray = config.tray.clone();
    tray.enabled = capabilities.tray;
    tray.clock_enabled = capabilities.clocks;
    tray.show_control_center = false;
    tray.copied_taskbars_only = true;
    let snapshot = diagnostics::collect_for(&tray);
    let environment = super::taskbar_environment::Environment::from_diagnostics(&snapshot);
    if let Some(error) = snapshot.target_plan_error {
        return Err(error);
    }
    let plan = snapshot.target_plan.ok_or("无法建立时钟目标计划")?;
    let routes = capabilities.select_routes(plan.targets.into_iter().map(|t| t.route).collect())?;
    Ok(Preflight {
        layout,
        routes,
        custom_calendar: config.clock.click_action == ClickAction::CalendarWeather,
        volume_scroll: config.clock.volume_scroll,
        volume_mute: config.clock.volume_mute,
        source: plan.source,
        clocks: capabilities.clocks,
        mirror_icons: capabilities.tray,
        notice: capabilities.notice,
        environment,
        image: None,
        permit: None,
    })
}
struct Context {
    request: Prepare,
    reply: RefCell<Option<Reply>>,
    gate: RefCell<Option<Gate>>,
    pending: RefCell<Option<Activation>>,
    active: Cell<bool>,
    main: HWND,
    pid: u32,
    tid: u32,
}
pub struct Connection {
    receiver: HWND,
    endpoint: Cell<HWND>,
    process: HANDLE,
    context: Rc<Context>,
    sequence: Cell<u64>,
    last: RefCell<Option<Frame>>,
    busy: Cell<bool>,
    failure: RefCell<Option<String>>,
    restored: Cell<bool>,
    stop_sent: Cell<bool>,
    notice: Option<String>,
    environment: Option<super::taskbar_environment::Environment>,
    permit: Option<Permit>,
}
struct Call<'a>(&'a Cell<bool>);
impl Drop for Call<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
struct Hook {
    hook: HHOOK,
    module: super::dll::Library,
}
impl Drop for Hook {
    fn drop(&mut self) {
        unsafe {
            if !self.hook.is_null() {
                UnhookWindowsHookEx(self.hook);
            }
        }
    }
}
impl Connection {
    fn prepare(main: HWND, preflight: Preflight) -> Result<Self, String> {
        unsafe {
            let image = preflight.image;
            let route = preflight.routes.first().ok_or("没有时钟目标")?;
            let (pid, tid, taskbar) = (route.process_id, route.thread_id, route.window as HWND);
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos() as u64;
            let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if process.is_null() {
                return Err("无法监视任务栏进程".into());
            }
            let context = Rc::new(Context {
                request: Prepare {
                    version: VERSION,
                    owner: Owner {
                        nonce,
                        process_id: GetCurrentProcessId(),
                        receiver: 0,
                    },
                    layout: preflight.layout,
                    routes: preflight.routes,
                    custom_calendar: preflight.custom_calendar,
                    volume_scroll: preflight.volume_scroll,
                    volume_mute: preflight.volume_mute,
                    source: preflight.source,
                    clocks: preflight.clocks,
                    mirror_icons: preflight.mirror_icons,
                },
                reply: RefCell::new(None),
                gate: RefCell::new(None),
                pending: RefCell::new(None),
                active: Cell::new(false),
                main,
                pid,
                tid,
            });
            let mut connection = Self {
                receiver: null_mut(),
                endpoint: Cell::new(null_mut()),
                process,
                context,
                sequence: Cell::new(0),
                last: RefCell::new(None),
                busy: Cell::new(false),
                failure: RefCell::new(None),
                restored: Cell::new(false),
                stop_sent: Cell::new(false),
                notice: preflight.notice,
                environment: preflight.environment,
                permit: preflight.permit,
            };
            let class = wide(RECEIVER_CLASS);
            let instance = GetModuleHandleW(null());
            let wc = WNDCLASSW {
                lpfnWndProc: Some(receive),
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                ..std::mem::zeroed()
            };
            if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return Err("无法注册任务栏时钟接收窗口".into());
            }
            // Created without user data; establish owner HWND before publishing
            // the immutable context to any synchronous native callback.
            connection.receiver = CreateWindowExW(
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
                null(),
            );
            if connection.receiver.is_null() {
                return Err("无法创建任务栏时钟接收窗口".into());
            }
            Rc::get_mut(&mut connection.context)
                .ok_or("任务栏接收上下文已共享")?
                .request
                .owner
                .receiver = connection.receiver as u64;
            SetWindowLongPtrW(
                connection.receiver,
                GWLP_USERDATA,
                Rc::as_ptr(&connection.context) as isize,
            );
            let request = &connection.context.request;
            request.plan()?;
            let mut hook = Hook {
                hook: null_mut(),
                module: match image {
                    Some(image) => image.load()?,
                    None => super::dll::load(super::dll::Kind::Taskbar)?,
                },
            };
            let entry =
                GetProcAddress(hook.module.handle(), c"LiteTrayCallWndProc".as_ptr().cast())
                    .ok_or("DLL 缺少任务栏入口")?;
            let mut actual = 0;
            if GetWindowThreadProcessId(taskbar, &mut actual) != tid
                || actual != pid
                || WaitForSingleObject(connection.process, 0) != WAIT_TIMEOUT
            {
                return Err("任务栏已重启，请重试".into());
            }
            // The worker's durable receipt exists before the first call that
            // can load our adapter into the retained native process.
            if let Some(permit) = &connection.permit {
                permit.mark_used();
            }
            hook.hook = SetWindowsHookExW(
                WH_CALLWNDPROC,
                Some(std::mem::transmute::<
                    unsafe extern "system" fn() -> isize,
                    unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT,
                >(entry)),
                hook.module.handle(),
                tid,
            );
            if hook.hook.is_null() {
                return Err(format!(
                    "任务栏线程接入失败：{}",
                    std::io::Error::last_os_error()
                ));
            }
            // Before installing anything writable, verify the target HWND still
            // belongs to the preflight process/thread snapshot.
            let mut actual = 0;
            if GetWindowThreadProcessId(taskbar, &mut actual) != tid || actual != pid {
                return Err("任务栏已重启，请重试".into());
            }
            let bytes = serde_json::to_vec(request).map_err(|e| e.to_string())?;
            if bytes.len() > MAX_REQUEST {
                return Err("时钟准备请求过大".into());
            }
            let sent = send(taskbar, connection.receiver, PREPARE_TAG, &bytes, 5000);
            drop(hook);
            // A late/timeout reply can leave only Prepared originals. Destroying
            // this receiver invalidates its owner and the watchdog restores it.
            let reply = connection
                .context
                .reply
                .borrow_mut()
                .take()
                .ok_or_else(|| {
                    format!(
                        "任务栏未返回已验证的时钟对象：{}",
                        sent.err().unwrap_or_else(|| "准备失败".into())
                    )
                })?;
            reply.validate(request)?;
            connection.endpoint.set(reply.endpoint as HWND);
            connection.verify_endpoint()?;
            if !reply.targets.is_empty() {
                *connection.context.gate.borrow_mut() =
                    Some(Gate::new(request.owner.clone(), reply.targets)?);
            }
            Ok(connection)
        }
    }
    fn start(&self, frame: Frame) -> Result<(), String> {
        frame.validate()?;
        let _call = self.enter()?;
        self.command(if self.context.request.clocks {
            Change::Start {
                frame: frame.clone(),
            }
        } else {
            Change::StartTray
        })
        .map_err(|error| format!("发送任务栏启动命令失败：{error}"))?;
        let status = self
            .status()
            .map_err(|error| format!("确认任务栏启动状态失败：{error}"))?;
        if status != SessionStatus::Active {
            return Err(format!("任务栏未确认接管成功，当前状态为 {status:?}"));
        }
        self.context.active.set(true);
        *self.last.borrow_mut() = Some(frame);
        Ok(())
    }
    fn verify_endpoint(&self) -> Result<(), String> {
        unsafe {
            if WaitForSingleObject(self.process, 0) != WAIT_TIMEOUT {
                return Err("Explorer 已退出，原连接失效".into());
            }
            let hwnd = self.endpoint.get();
            let mut pid = 0;
            if hwnd.is_null()
                || GetWindowThreadProcessId(hwnd, &mut pid) != self.context.tid
                || pid != self.context.pid
            {
                return Err("任务栏会话端点失效".into());
            }
            let mut name = [0u16; 128];
            let n = GetClassNameW(hwnd, name.as_mut_ptr(), 128).max(0) as usize;
            if !String::from_utf16_lossy(&name[..n])
                .starts_with(&format!("LiteTray.ClockWatchdog.v1.{}.", self.context.tid))
            {
                return Err("任务栏会话窗口类型不匹配".into());
            }
            Ok(())
        }
    }
    fn command(&self, change: Change) -> Result<(), String> {
        self.command_with_timeout(change, 1500)
    }
    fn command_with_timeout(&self, change: Change, timeout: u32) -> Result<(), String> {
        self.verify_endpoint()?;
        let sequence = self
            .sequence
            .get()
            .checked_add(1)
            .ok_or("时钟命令序号耗尽")?;
        self.sequence.set(sequence);
        let command = Command {
            version: VERSION,
            owner: self.context.request.owner.clone(),
            sequence,
            change,
        };
        command.validate()?;
        let bytes = serde_json::to_vec(&command).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_REQUEST {
            return Err("时钟命令过大".into());
        }
        if unsafe {
            send(
                self.endpoint.get(),
                self.receiver,
                COMMAND_TAG,
                &bytes,
                timeout,
            )
        }? != 1
        {
            return Err("任务栏拒绝时钟命令，原状态可能正在恢复".into());
        }
        Ok(())
    }
    pub fn status(&self) -> Result<SessionStatus, String> {
        self.status_with_timeout(1000)
    }
    fn status_with_timeout(&self, timeout: u32) -> Result<SessionStatus, String> {
        self.verify_endpoint()?;
        let bytes = serde_json::to_vec(&self.context.request.owner).map_err(|e| e.to_string())?;
        let status = unsafe {
            send(
                self.endpoint.get(),
                self.receiver,
                STATUS_TAG,
                &bytes,
                timeout,
            )
        }?;
        SessionStatus::from_code(status as i32)
    }
    fn enter(&self) -> Result<Call<'_>, String> {
        if self.busy.replace(true) {
            Err("任务栏调用正在进行".into())
        } else {
            Ok(Call(&self.busy))
        }
    }
    pub fn update(&self, frame: Frame) -> Result<(), String> {
        if let Some(error) = self.failure.borrow().as_ref() {
            return Err(error.clone());
        }
        let _call = self.enter()?;
        frame.validate()?;
        let change = if !self.context.request.clocks {
            Change::Heartbeat
        } else {
            match self.last.borrow().as_ref() {
                Some(old) if old == &frame => Change::Heartbeat,
                Some(old) if old.style == frame.style => Change::Tick {
                    text: frame.text.clone(),
                },
                _ => Change::Update {
                    frame: frame.clone(),
                },
            }
        };
        if let Err(error) = self.command_with_timeout(change, PERIODIC_RPC_MILLIS) {
            self.record_fault();
            self.context.active.set(false);
            *self.failure.borrow_mut() = Some(error.clone());
            return Err(error);
        }
        *self.last.borrow_mut() = Some(frame);
        Ok(())
    }
    fn record_fault(&self) {
        if let Some(permit) = &self.permit {
            permit.fault();
        }
    }
    fn confirm_restored(&self) {
        if let Some(permit) = &self.permit {
            if unsafe { WaitForSingleObject(self.process, 0) } == WAIT_OBJECT_0 {
                permit.fault();
            }
            permit.restored();
        }
        self.restored.set(true);
    }
    pub fn stop(&self) -> Result<(), String> {
        let _call = self.enter()?;
        self.context.active.set(false);
        self.context.pending.borrow_mut().take();
        if self.restored.get() {
            return Ok(());
        }
        if unsafe { WaitForSingleObject(self.process, 0) } == WAIT_OBJECT_0 {
            self.confirm_restored();
            return Ok(());
        }
        self.command(Change::Stop)
            .map_err(|error| format!("发送任务栏停止命令失败：{error}"))?;
        self.stop_sent.set(true);
        let status = self
            .status()
            .map_err(|error| format!("确认任务栏恢复状态失败：{error}"))?;
        if status != SessionStatus::Closed {
            return Err(format!(
                "任务栏尚未确认恢复完成，当前状态为 {status:?}，连接继续保留"
            ));
        }
        self.confirm_restored();
        Ok(())
    }
    fn restore_step(&self) -> Result<taskbar_service::RestoreProgress, String> {
        use taskbar_service::RestoreProgress;
        let _call = self.enter()?;
        self.context.active.set(false);
        self.context.pending.borrow_mut().take();
        if self.restored.get() || unsafe { WaitForSingleObject(self.process, 0) } == WAIT_OBJECT_0 {
            self.confirm_restored();
            return Ok(RestoreProgress::Confirmed);
        }
        if !self.stop_sent.get() {
            self.command_with_timeout(Change::Stop, PERIODIC_RPC_MILLIS)
                .map_err(|error| format!("发送任务栏停止命令失败：{error}"))?;
            self.stop_sent.set(true);
            return Ok(RestoreProgress::Pending);
        }
        let status = self
            .status_with_timeout(PERIODIC_RPC_MILLIS)
            .map_err(|error| format!("确认任务栏恢复状态失败：{error}"))?;
        if status != SessionStatus::Closed {
            return Err(format!(
                "任务栏尚未确认恢复完成，当前状态为 {status:?}，连接继续保留"
            ));
        }
        self.confirm_restored();
        Ok(RestoreProgress::Confirmed)
    }
    fn restoration_confirmed(&self) -> bool {
        let Ok(_call) = self.enter() else {
            return false;
        };
        if self.restored.get() {
            return true;
        }
        // A missing endpoint or a failed/Busy query cannot discharge the
        // retained restore obligation. Only the original process exiting or
        // its authenticated session reporting Closed proves completion.
        if unsafe { WaitForSingleObject(self.process, 0) } == WAIT_OBJECT_0
            || self.status_with_timeout(PERIODIC_RPC_MILLIS) == Ok(SessionStatus::Closed)
        {
            self.context.active.set(false);
            self.context.pending.borrow_mut().take();
            self.confirm_restored();
            return true;
        }
        false
    }
    pub fn take_activation(&self) -> Option<Activation> {
        let event = self.context.pending.borrow_mut().take()?;
        // The sender already validated the live lease and the receiver accepted
        // its nonce, source HWND and sequence. A synchronous status roundtrip
        // here could wait 1s or discard that click on a transient Busy response.
        (self.context.active.get()
            && self.failure.borrow().is_none()
            && self.verify_endpoint().is_ok())
        .then_some(event)
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        if self.stop().is_err() {
            self.record_fault();
        }
        unsafe {
            if !self.receiver.is_null() {
                SetWindowLongPtrW(self.receiver, GWLP_USERDATA, 0);
                DestroyWindow(self.receiver);
            }
            if !self.process.is_null() {
                CloseHandle(self.process);
            }
        }
    }
}
unsafe fn send(
    target: HWND,
    sender: HWND,
    tag: usize,
    bytes: &[u8],
    timeout: u32,
) -> Result<usize, String> {
    unsafe {
        let copy = COPYDATASTRUCT {
            dwData: tag,
            cbData: bytes.len() as u32,
            lpData: bytes.as_ptr() as *mut _,
        };
        let mut result = 0;
        // SendMessageTimeoutW can return zero without setting last error. Do
        // not attribute an unrelated earlier Win32 error to this request.
        SetLastError(ERROR_SUCCESS);
        if SendMessageTimeoutW(
            target,
            WM_COPYDATA,
            sender as usize,
            &copy as *const _ as isize,
            SMTO_ABORTIFHUNG,
            timeout,
            &mut result,
        ) == 0
        {
            let error = GetLastError();
            if error == ERROR_SUCCESS {
                Err(format!(
                    "任务栏通信未完成（等待上限 {timeout} 毫秒，系统未提供错误码）"
                ))
            } else {
                Err(format!(
                    "任务栏通信失败（等待上限 {timeout} 毫秒）：{}",
                    std::io::Error::from_raw_os_error(error as i32)
                ))
            }
        } else {
            Ok(result)
        }
    }
}
unsafe extern "system" fn receive(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if msg != WM_COPYDATA || l == 0 {
            return DefWindowProcW(hwnd, msg, w, l);
        }
        let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Context;
        if raw.is_null() {
            return 0;
        }
        Rc::increment_strong_count(raw);
        let context = Rc::from_raw(raw);
        let mut pid = 0;
        if GetWindowThreadProcessId(w as HWND, &mut pid) != context.tid || pid != context.pid {
            return 0;
        }
        let copy = &*(l as *const COPYDATASTRUCT);
        if copy.cbData == 0 || copy.cbData as usize > MAX_REQUEST || copy.lpData.is_null() {
            return 0;
        }
        let bytes = std::slice::from_raw_parts(copy.lpData.cast::<u8>(), copy.cbData as usize);
        if copy.dwData == REPLY_TAG {
            if !context.request.routes.iter().any(|r| r.window == w as u64) {
                return 0;
            }
            let Ok(reply) = serde_json::from_slice::<Reply>(bytes) else {
                return 0;
            };
            if reply.owner != context.request.owner
                || reply.version != VERSION
                || context.reply.borrow().is_some()
            {
                return 0;
            }
            *context.reply.borrow_mut() = Some(reply);
            return 1;
        }
        if copy.dwData == EVENT_TAG && bytes.len() <= MAX_EVENT && context.active.get() {
            let Ok(event) = Activation::decode(bytes) else {
                return 0;
            };
            let calendar = matches!(
                event.input,
                litetray_adapter_protocol::activation::Input::Pointer
                    | litetray_adapter_protocol::activation::Input::Keyboard
            );
            if event.source_window != w as u64
                || !context.request.input_options().allows(event.input)
                || calendar && context.pending.borrow().is_some()
            {
                return 0;
            }
            let Ok(mut gate) = context.gate.try_borrow_mut() else {
                return 0;
            };
            let Some(gate) = gate.as_mut() else {
                return 0;
            };
            let Ok(event) = gate.accept(bytes) else {
                return 0;
            };
            if !calendar {
                return super::volume::dispatch(event.input, context.main as usize, event.anchor)
                    as isize;
            }
            if PostMessageW(context.main, WM_ACTIVATED, 0, 0) == 0 {
                return 0;
            }
            *context.pending.borrow_mut() = Some(event);
            return 1;
        }
        0
    }
}

#[cfg(test)]
mod restoration_checks {
    use super::*;
    use std::{io::Write, os::windows::process::CommandExt, process::Stdio};

    #[derive(Default)]
    struct EndpointState {
        status: Cell<LRESULT>,
        queries: Cell<usize>,
        commands: Cell<usize>,
        reject_commands: Cell<bool>,
    }
    unsafe extern "system" fn endpoint(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        unsafe {
            let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const EndpointState;
            if msg == WM_COPYDATA && !state.is_null() {
                let state = &*state;
                let copy = &*(l as *const COPYDATASTRUCT);
                if copy.dwData == STATUS_TAG {
                    state.queries.set(state.queries.get() + 1);
                    return state.status.get();
                }
                if copy.dwData == COMMAND_TAG {
                    state.commands.set(state.commands.get() + 1);
                    return (!state.reject_commands.get()) as LRESULT;
                }
            }
            DefWindowProcW(hwnd, msg, w, l)
        }
    }
    struct Fixture {
        connection: Option<Connection>,
        hwnd: HWND,
        state: Box<EndpointState>,
    }
    impl Fixture {
        fn new() -> Self {
            unsafe {
                let tid = GetCurrentThreadId();
                let pid = GetCurrentProcessId();
                let class = wide(&format!("LiteTray.ClockWatchdog.v1.{tid}.RestorationTest"));
                RegisterClassW(&WNDCLASSW {
                    lpfnWndProc: Some(endpoint),
                    hInstance: GetModuleHandleW(null()),
                    lpszClassName: class.as_ptr(),
                    ..std::mem::zeroed()
                });
                let hwnd = CreateWindowExW(
                    0,
                    class.as_ptr(),
                    null(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    HWND_MESSAGE,
                    null_mut(),
                    GetModuleHandleW(null()),
                    null(),
                );
                assert!(!hwnd.is_null());
                let state = Box::<EndpointState>::default();
                SetWindowLongPtrW(
                    hwnd,
                    GWLP_USERDATA,
                    &*state as *const EndpointState as isize,
                );
                let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
                assert!(!process.is_null());
                Self {
                    connection: Some(Connection {
                        receiver: null_mut(),
                        endpoint: Cell::new(hwnd),
                        process,
                        context: Rc::new(Context {
                            request: Prepare {
                                version: VERSION,
                                owner: Owner {
                                    nonce: 1,
                                    process_id: pid,
                                    receiver: hwnd as u64,
                                },
                                layout: HostLayout {
                                    schema: 1,
                                    module: litetray_adapter_protocol::ModuleIdentity {
                                        machine: 0x8664,
                                        timestamp: 0,
                                        image_size: 4096,
                                        pdb_guid: [1; 16],
                                        pdb_age: 1,
                                    },
                                    symbols: vec![],
                                    element_offset: Some(0),
                                },
                                routes: vec![],
                                custom_calendar: false,
                                volume_scroll: false,
                                volume_mute: false,
                                source: None,
                                clocks: true,
                                mirror_icons: false,
                            },
                            reply: RefCell::new(None),
                            gate: RefCell::new(None),
                            pending: RefCell::new(None),
                            active: Cell::new(false),
                            main: hwnd,
                            pid,
                            tid,
                        }),
                        sequence: Cell::new(0),
                        last: RefCell::new(None),
                        busy: Cell::new(false),
                        failure: RefCell::new(None),
                        restored: Cell::new(false),
                        stop_sent: Cell::new(false),
                        notice: None,
                        environment: None,
                        permit: None,
                    }),
                    hwnd,
                    state,
                }
            }
        }
        fn connection(&self) -> &Connection {
            self.connection.as_ref().unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            self.connection.take();
            unsafe {
                DestroyWindow(self.hwnd);
            }
        }
    }

    #[test]
    fn production_backend_refuses_unarmed_preflight_before_loading_or_hooking_native_code() {
        let fixture = Fixture::new();
        let path = std::env::temp_dir().join(format!(
            "litetray-unarmed-preflight-{}.json",
            std::process::id()
        ));
        let existed = path.exists();
        let native = Native {
            main: fixture.hwnd,
            journal: Journal::new(path.clone()),
            retry: Arc::new(AtomicBool::new(false)),
        };
        let prepared = Preflight {
            layout: fixture.connection().context.request.layout.clone(),
            routes: vec![],
            custom_calendar: false,
            volume_scroll: false,
            volume_mute: false,
            source: None,
            clocks: true,
            mirror_icons: false,
            notice: None,
            environment: None,
            image: None,
            permit: None,
        };
        let frame = crate::clock::frame(&Config::default().clock, "", chrono::Local::now());
        let Err(failure) = taskbar_service::Backend::connect(&native, prepared, &frame) else {
            panic!("unarmed preflight reached native activation");
        };
        assert!(failure.suspended);
        assert!(failure.pending.is_none());
        assert_eq!(path.exists(), existed);
    }

    #[test]
    fn stalled_endpoint_cannot_combine_tick_and_stop_waits_in_one_ui_dispatch() {
        use std::{
            sync::{
                Arc,
                atomic::{AtomicBool, AtomicUsize, Ordering},
                mpsc,
            },
            time::{Duration, Instant},
        };
        struct Control {
            stall_next_tick: AtomicBool,
            stops: AtomicUsize,
            entered: mpsc::Sender<()>,
        }
        struct Endpoint {
            control: Arc<Control>,
            gate: RefCell<Option<mpsc::Receiver<()>>>,
            closed: Cell<bool>,
        }
        unsafe extern "system" fn procedure(
            hwnd: HWND,
            message: u32,
            w: WPARAM,
            l: LPARAM,
        ) -> LRESULT {
            unsafe {
                if message == WM_CLOSE {
                    DestroyWindow(hwnd);
                    PostQuitMessage(0);
                    return 0;
                }
                let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Endpoint;
                if message == WM_COPYDATA
                    && let Some(state) = state.as_ref()
                {
                    let copy = &*(l as *const COPYDATASTRUCT);
                    if copy.dwData == STATUS_TAG {
                        return if state.closed.get() {
                            SessionStatus::Closed as LRESULT
                        } else {
                            SessionStatus::Active as LRESULT
                        };
                    }
                    if copy.dwData == COMMAND_TAG {
                        let bytes =
                            std::slice::from_raw_parts(copy.lpData.cast(), copy.cbData as usize);
                        let Ok(command) = Command::decode(bytes) else {
                            return 0;
                        };
                        match command.change {
                            Change::Stop => {
                                state.control.stops.fetch_add(1, Ordering::Relaxed);
                                state.closed.set(true);
                            }
                            Change::Tick { .. } | Change::Update { .. } | Change::Heartbeat
                                if state.control.stall_next_tick.swap(false, Ordering::Relaxed) =>
                            {
                                let gate = state.gate.borrow_mut().take();
                                let _ = state.control.entered.send(());
                                if let Some(gate) = gate {
                                    let _ = gate.recv_timeout(Duration::from_secs(5));
                                }
                            }
                            _ => {}
                        }
                        return 1;
                    }
                }
                DefWindowProcW(hwnd, message, w, l)
            }
        }
        struct Backend(RefCell<Option<Connection>>);
        impl taskbar_service::Backend for Backend {
            type Prepared = ();
            type Session = Connection;
            fn prepare(&self, _: &Config) -> mpsc::Receiver<Result<(), PreparationFailure>> {
                let (tx, rx) = mpsc::channel();
                let _ = tx.send(Ok(()));
                rx
            }
            fn connect(
                &self,
                _: (),
                frame: &Frame,
            ) -> Result<Connection, ConnectFailure<Connection>> {
                let connection = self.0.borrow_mut().take().unwrap();
                match connection.start(frame.clone()) {
                    Ok(()) => Ok(connection),
                    Err(error) => Err(ConnectFailure {
                        error,
                        pending: Some(connection),
                        suspended: false,
                    }),
                }
            }
        }
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let control = Arc::new(Control {
            stall_next_tick: AtomicBool::new(false),
            stops: AtomicUsize::new(0),
            entered: entered_tx,
        });
        let thread_control = control.clone();
        let endpoint_thread = std::thread::spawn(move || unsafe {
            let tid = GetCurrentThreadId();
            let class = wide(&format!("LiteTray.ClockWatchdog.v1.{tid}.StalledTest"));
            let instance = GetModuleHandleW(null());
            assert_ne!(
                RegisterClassW(&WNDCLASSW {
                    lpfnWndProc: Some(procedure),
                    hInstance: instance,
                    lpszClassName: class.as_ptr(),
                    ..std::mem::zeroed()
                }),
                0
            );
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                null_mut(),
                instance,
                null(),
            );
            assert!(!hwnd.is_null());
            let state = Box::new(Endpoint {
                control: thread_control,
                gate: RefCell::new(Some(release_rx)),
                closed: Cell::new(false),
            });
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, &*state as *const Endpoint as isize);
            ready_tx.send((hwnd as usize, tid)).unwrap();
            let mut message = std::mem::zeroed();
            while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
                DispatchMessageW(&message);
            }
            UnregisterClassW(class.as_ptr(), instance);
        });
        let (endpoint, tid) = ready_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let mut fixture = Fixture::new();
        let mut connection = fixture.connection.take().unwrap();
        connection.endpoint.set(endpoint as HWND);
        Rc::get_mut(&mut connection.context).unwrap().tid = tid;
        let mut config = Config::default();
        config.tray.clock_enabled = true;
        let service =
            taskbar_service::Service::new(Backend(RefCell::new(Some(connection))), config, 0)
                .unwrap();
        let wall_time = chrono::Local::now();
        service.tick(0, wall_time).unwrap();
        service.tick(1, wall_time).unwrap();
        assert_eq!(service.snapshot().status, taskbar_service::Status::Running);
        control.stall_next_tick.store(true, Ordering::Relaxed);
        let started = Instant::now();
        service.tick(20, wall_time).unwrap();
        let elapsed = started.elapsed();
        let entered = entered_rx.try_recv().is_ok();
        let phase_after_fault = service.snapshot().status;
        let _ = release_tx.send(());
        service.tick(40, wall_time).unwrap();
        let phase_after_stop_ack = service.snapshot().status;
        service.tick(41, wall_time).unwrap();
        let phase_after_confirmation = service.snapshot().status;
        let periodic_stops = control.stops.load(Ordering::Relaxed);
        let stopped = service.stop(1000);
        drop(service);
        drop(fixture);
        unsafe { PostMessageW(endpoint as HWND, WM_CLOSE, 0, 0) };
        endpoint_thread.join().unwrap();
        stopped.unwrap();
        assert!(
            entered,
            "the real cross-thread WM_COPYDATA endpoint must stall"
        );
        assert_eq!(phase_after_fault, taskbar_service::Status::Restoring);
        assert_eq!(phase_after_stop_ack, taskbar_service::Status::Restoring);
        assert_eq!(phase_after_confirmation, taskbar_service::Status::Waiting);
        assert_eq!(
            periodic_stops, 1,
            "the acknowledged Stop must not be repeated for its confirmation"
        );
        eprintln!(
            "stalled periodic dispatch: {:.1}ms",
            elapsed.as_secs_f64() * 1000.0
        );
        assert!(
            elapsed < Duration::from_millis(600),
            "one periodic UI dispatch waited {elapsed:?}; a failed Tick must yield before a bounded restoration step"
        );
    }
    #[test]
    fn blocked_transport_requires_closed_and_cached_confirmation_never_writes_stop() {
        let mut fixture = Fixture::new();
        for status in [0, 1, 2, 3, -1, 42] {
            fixture.state.status.set(status);
            assert!(!fixture.connection().restoration_confirmed());
        }
        assert_eq!(fixture.state.queries.get(), 6);
        assert_eq!(fixture.state.commands.get(), 0);
        fixture.state.status.set(SessionStatus::Closed as LRESULT);
        fixture.connection().busy.set(true);
        assert!(!fixture.connection().restoration_confirmed());
        assert_eq!(fixture.state.queries.get(), 6, "no reentrant native call");
        fixture.connection().busy.set(false);
        fixture.connection().endpoint.set(null_mut());
        assert!(!fixture.connection().restoration_confirmed());
        assert!(!fixture.connection().restored.get());
        fixture.connection().endpoint.set(fixture.hwnd);
        assert!(fixture.connection().restoration_confirmed());
        assert_eq!(fixture.state.queries.get(), 7);
        assert!(fixture.connection().restoration_confirmed());
        assert_eq!(
            fixture.state.queries.get(),
            7,
            "Closed is terminal and cached"
        );
        fixture.connection.take();
        assert_eq!(
            fixture.state.commands.get(),
            0,
            "Drop must not send Stop again"
        );
    }
    #[test]
    fn stop_caches_only_an_acknowledged_closed_session() {
        let mut fixture = Fixture::new();
        fixture
            .state
            .status
            .set(SessionStatus::Restoring as LRESULT);
        assert!(fixture.connection().stop().is_err());
        assert!(!fixture.connection().restored.get());
        fixture.state.status.set(SessionStatus::Closed as LRESULT);
        fixture.connection().stop().unwrap();
        assert!(fixture.connection().restored.get());
        assert_eq!(fixture.state.commands.get(), 2);
        fixture.connection.take();
        assert_eq!(
            fixture.state.commands.get(),
            2,
            "Drop preserves the confirmation"
        );
    }

    #[test]
    fn retained_process_exit_observation_never_sends_a_native_command() {
        use std::{
            io::Write,
            os::windows::{io::AsRawHandle, process::CommandExt},
            process::{Command, Stdio},
            time::{Duration, Instant},
        };
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "platform::taskbar_clock::restoration_checks::retained_process_observation_child",
                "--ignored",
                "--test-threads=1",
            ])
            .env("LITETRAY_PROCESS_OBSERVATION_CHILD", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .unwrap();
        let mut fixture = Fixture::new();
        let mut retained = null_mut();
        unsafe {
            assert_ne!(
                DuplicateHandle(
                    GetCurrentProcess(),
                    child.as_raw_handle(),
                    GetCurrentProcess(),
                    &mut retained,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                ),
                0
            );
            let connection = fixture.connection.as_mut().unwrap();
            CloseHandle(connection.process);
            connection.process = retained;
        }
        assert!(!taskbar_service::Session::process_exited(
            fixture.connection()
        ));
        child.stdin.take().unwrap().write_all(b"x").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let _ = child.wait();
                panic!("isolated process observation child exceeded its deadline");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(taskbar_service::Session::process_exited(
            fixture.connection()
        ));
        assert_eq!(fixture.state.commands.get(), 0);
        assert_eq!(fixture.state.queries.get(), 0);
    }

    #[test]
    #[ignore = "only invoked by the isolated retained-process observation test"]
    fn retained_process_observation_child() {
        use std::io::Read;
        if std::env::var("LITETRAY_PROCESS_OBSERVATION_CHILD").as_deref() == Ok("1") {
            std::io::stdin().read_exact(&mut [0; 1]).unwrap();
        }
    }
    #[test]
    fn periodic_restore_ack_and_confirmation_are_separate_and_keep_the_lease() {
        use taskbar_service::RestoreProgress;
        let mut fixture = Fixture::new();
        fixture
            .state
            .status
            .set(SessionStatus::Restoring as LRESULT);
        assert_eq!(
            fixture.connection().restore_step(),
            Ok(RestoreProgress::Pending)
        );
        assert_eq!(fixture.state.commands.get(), 1);
        assert_eq!(
            fixture.state.queries.get(),
            0,
            "Stop must yield before querying"
        );
        assert!(
            !fixture.connection().restored.get(),
            "an acknowledged command is not restoration proof"
        );
        assert!(fixture.connection().restore_step().is_err());
        assert_eq!(
            fixture.state.commands.get(),
            1,
            "confirmation must not repeat Stop"
        );
        assert_eq!(fixture.state.queries.get(), 1);
        fixture.state.status.set(SessionStatus::Closed as LRESULT);
        assert_eq!(
            fixture.connection().restore_step(),
            Ok(RestoreProgress::Confirmed)
        );
        assert!(fixture.connection().restored.get());
        assert_eq!(fixture.state.queries.get(), 2);
        fixture.connection.take();
        assert_eq!(
            fixture.state.commands.get(),
            1,
            "Drop preserves terminal confirmation"
        );
    }
    #[test]
    fn activation_diagnostic_distinguishes_command_and_status_failures() {
        let fixture = Fixture::new();
        let frame = crate::clock::frame(&Config::default().clock, "", chrono::Local::now());
        fixture.state.reject_commands.set(true);
        let error = fixture.connection().start(frame.clone()).unwrap_err();
        assert!(error.starts_with("发送任务栏启动命令失败："), "{error}");
        assert_eq!(fixture.state.queries.get(), 0);
        assert!(!fixture.connection().context.active.get());
        fixture.state.reject_commands.set(false);
        fixture.state.status.set(42);
        let error = fixture.connection().start(frame.clone()).unwrap_err();
        assert!(error.starts_with("确认任务栏启动状态失败："), "{error}");
        assert_eq!(fixture.state.queries.get(), 1);
        assert!(!fixture.connection().context.active.get());
        fixture
            .state
            .status
            .set(SessionStatus::Restoring as LRESULT);
        let error = fixture.connection().start(frame).unwrap_err();
        assert!(error.contains("Restoring"), "{error}");
        assert!(!fixture.connection().context.active.get());
        assert!(!fixture.connection().restored.get());
    }
    #[test]
    fn cleanup_diagnostic_preserves_unconfirmed_restore_obligation() {
        let fixture = Fixture::new();
        fixture.state.reject_commands.set(true);
        let error = fixture.connection().stop().unwrap_err();
        assert!(error.starts_with("发送任务栏停止命令失败："), "{error}");
        assert_eq!(fixture.state.queries.get(), 0);
        assert!(!fixture.connection().restored.get());
        fixture.state.reject_commands.set(false);
        fixture.state.status.set(42);
        let error = fixture.connection().stop().unwrap_err();
        assert!(error.starts_with("确认任务栏恢复状态失败："), "{error}");
        assert_eq!(fixture.state.queries.get(), 1);
        assert!(!fixture.connection().restored.get());
        fixture.state.status.set(SessionStatus::Closed as LRESULT);
        fixture.connection().stop().unwrap();
        assert!(fixture.connection().restored.get());
    }
    #[test]
    fn retained_process_exit_confirms_restoration_without_an_endpoint() {
        let mut fixture = Fixture::new();
        let system = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap());
        let mut child = std::process::Command::new(system.join("System32/cmd.exe"))
            .args(["/d", "/q"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .unwrap();
        let connection = fixture.connection.as_mut().unwrap();
        unsafe {
            CloseHandle(connection.process);
            connection.process = OpenProcess(PROCESS_SYNCHRONIZE, 0, child.id());
        }
        assert!(!connection.process.is_null());
        connection.endpoint.set(null_mut());
        assert!(
            !connection.restoration_confirmed(),
            "a live process is insufficient"
        );
        child.stdin.take().unwrap().write_all(b"exit\r\n").unwrap();
        // Any terminal exit code proves that the retained process is gone.
        child.wait().unwrap();
        assert!(connection.restoration_confirmed());
        fixture.connection.take();
        assert_eq!(fixture.state.queries.get(), 0);
        assert_eq!(fixture.state.commands.get(), 0);
    }
}

#[cfg(test)]
mod activation_checks {
    use super::*;
    unsafe extern "system" fn busy_endpoint(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        if msg == WM_COPYDATA {
            return SessionStatus::Busy as LRESULT;
        }
        unsafe { DefWindowProcW(hwnd, msg, w, l) }
    }
    #[test]
    fn accepted_click_survives_a_busy_shell_without_another_roundtrip() {
        unsafe {
            let tid = GetCurrentThreadId();
            let pid = GetCurrentProcessId();
            let class = wide(&format!("LiteTray.ClockWatchdog.v1.{tid}.ActivationTest"));
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(busy_endpoint),
                hInstance: GetModuleHandleW(null()),
                lpszClassName: class.as_ptr(),
                ..std::mem::zeroed()
            });
            let endpoint = CreateWindowExW(
                0,
                class.as_ptr(),
                null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!endpoint.is_null());
            let owner = Owner {
                nonce: 1,
                process_id: pid,
                receiver: endpoint as u64,
            };
            let event = Activation {
                version: VERSION,
                owner: owner.clone(),
                sequence: 1,
                clock_id: 1,
                source_window: endpoint as u64,
                anchor: crate::popup::Rect {
                    left: 10,
                    top: 10,
                    right: 100,
                    bottom: 40,
                },
                input: litetray_adapter_protocol::activation::Input::Pointer,
            };
            let context = Rc::new(Context {
                request: Prepare {
                    version: VERSION,
                    owner,
                    layout: HostLayout {
                        schema: 1,
                        module: litetray_adapter_protocol::ModuleIdentity {
                            machine: 0x8664,
                            timestamp: 0,
                            image_size: 4096,
                            pdb_guid: [1; 16],
                            pdb_age: 1,
                        },
                        symbols: vec![],
                        element_offset: Some(0),
                    },
                    routes: vec![],
                    custom_calendar: true,
                    volume_scroll: false,
                    volume_mute: false,
                    source: None,
                    clocks: true,
                    mirror_icons: false,
                },
                reply: RefCell::new(None),
                gate: RefCell::new(None),
                pending: RefCell::new(Some(event)),
                active: Cell::new(true),
                main: endpoint,
                pid,
                tid,
            });
            let mut connection = Connection {
                receiver: null_mut(),
                endpoint: Cell::new(endpoint),
                process: OpenProcess(PROCESS_SYNCHRONIZE, 0, pid),
                context,
                sequence: Cell::new(0),
                last: RefCell::new(None),
                busy: Cell::new(false),
                failure: RefCell::new(None),
                restored: Cell::new(false),
                stop_sent: Cell::new(false),
                environment: None,
                notice: None,
                permit: None,
            };
            let accepted = connection.take_activation();
            CloseHandle(connection.process);
            connection.process = null_mut();
            DestroyWindow(endpoint);
            drop(connection);
            assert!(
                accepted.is_some(),
                "an authenticated click was discarded when the shell was temporarily busy"
            );
        }
    }
}
