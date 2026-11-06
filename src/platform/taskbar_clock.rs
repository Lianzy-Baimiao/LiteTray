//! Experimental real-taskbar transport. No activation occurs during preflight;
//! open is an explicit UI action. Native preparation must return authenticated
//! source windows before a Start command is sent to the clock endpoint.
use crate::{
    config::{ClickAction, Config},
    pe,
    platform::{diagnostics, symbols, wide},
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
};
use windows_sys::Win32::{
    Foundation::*,
    System::{DataExchange::COPYDATASTRUCT, LibraryLoader::*, Threading::*},
    UI::WindowsAndMessaging::*,
};
pub const WM_ACTIVATED: u32 = WM_APP + 12;
pub struct Preflight {
    layout: HostLayout,
    routes: Vec<Route>,
    custom_calendar: bool,
}
pub fn preflight(config: &Config) -> Result<Preflight, String> {
    let system = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .ok_or("SystemRoot 不可用")?
        .join("System32");
    for (name, hash) in [
        ("taskbar.dll", TASKBAR_HASH),
        ("twinui.pcshell.dll", SHELL_HASH),
    ] {
        let bytes = std::fs::read(system.join(name)).map_err(|e| e.to_string())?;
        if pe::identity(&bytes)?.sha256 != hash {
            return Err(format!("{name} 不属于当前实验适配的系统版本"));
        }
    }
    let bytes = std::fs::read(system.join("taskbar.dll")).map_err(|e| e.to_string())?;
    let identity = pe::identity(&bytes)?;
    let cache = crate::config::data_dir()
        .join("adapters")
        .join(format!("{}.json", identity.sha256));
    let map = if cache.is_file() {
        let bytes = std::fs::read(cache).map_err(|e| e.to_string())?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err("兼容性资料尺寸无效".into());
        }
        serde_json::from_slice::<symbols::SymbolMap>(&bytes).map_err(|_| "兼容性资料损坏")?
    } else {
        symbols::prepare(&system.join("taskbar.dll"), None)?
    };
    let layout = symbols::host_layout(&map, &bytes)?;
    let mut tray = config.tray.clone();
    tray.clock_enabled = true;
    tray.enabled = false;
    let snapshot = diagnostics::collect_for(&tray);
    if let Some(error) = snapshot.target_plan_error {
        return Err(error);
    }
    let plan = snapshot.target_plan.ok_or("无法建立时钟目标计划")?;
    if plan.targets.is_empty() {
        return Err("所选范围没有可用任务栏".into());
    }
    Ok(Preflight {
        layout,
        routes: plan.targets.into_iter().map(|t| t.route).collect(),
        custom_calendar: config.clock.click_action == ClickAction::CalendarWeather,
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
}
struct Call<'a>(&'a Cell<bool>);
impl Drop for Call<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
struct Hook {
    hook: HHOOK,
    module: HMODULE,
}
impl Drop for Hook {
    fn drop(&mut self) {
        unsafe {
            if !self.hook.is_null() {
                UnhookWindowsHookEx(self.hook);
            }
            if !self.module.is_null() {
                FreeLibrary(self.module);
            }
        }
    }
}
impl Connection {
    pub fn open(main: HWND, preflight: Preflight, frame: Frame) -> Result<Self, String> {
        unsafe {
            frame.validate()?;
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
            let dll = std::env::current_exe()
                .map_err(|e| e.to_string())?
                .parent()
                .ok_or("程序目录无效")?
                .join("litetray_taskbar_adapter.dll");
            let mut hook = Hook {
                hook: null_mut(),
                module: LoadLibraryExW(
                    wide(&dll.to_string_lossy()).as_ptr(),
                    null_mut(),
                    LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
                ),
            };
            if hook.module.is_null() {
                return Err("无法加载同目录任务栏适配 DLL".into());
            }
            let entry = GetProcAddress(hook.module, c"LiteTrayCallWndProc".as_ptr().cast())
                .ok_or("DLL 缺少任务栏入口")?;
            let mut actual = 0;
            if GetWindowThreadProcessId(taskbar, &mut actual) != tid
                || actual != pid
                || WaitForSingleObject(connection.process, 0) != WAIT_TIMEOUT
            {
                return Err("任务栏已重启，请重试".into());
            }
            hook.hook = SetWindowsHookExW(
                WH_CALLWNDPROC,
                Some(std::mem::transmute::<
                    unsafe extern "system" fn() -> isize,
                    unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT,
                >(entry)),
                hook.module,
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
            *connection.context.gate.borrow_mut() =
                Some(Gate::new(request.owner.clone(), reply.targets)?);
            connection.command(Change::Start {
                frame: frame.clone(),
            })?;
            if connection.status()? != SessionStatus::Active {
                return Err("任务栏未确认时钟接管成功".into());
            }
            connection.context.active.set(true);
            *connection.last.borrow_mut() = Some(frame);
            Ok(connection)
        }
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
            if String::from_utf16_lossy(&name[..n])
                != format!("LiteTray.ClockWatchdog.v1.{}", self.context.tid)
            {
                return Err("任务栏会话窗口类型不匹配".into());
            }
            Ok(())
        }
    }
    fn command(&self, change: Change) -> Result<(), String> {
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
                1500,
            )
        }? != 1
        {
            return Err("任务栏拒绝时钟命令，原状态可能正在恢复".into());
        }
        Ok(())
    }
    pub fn status(&self) -> Result<SessionStatus, String> {
        self.verify_endpoint()?;
        let bytes = serde_json::to_vec(&self.context.request.owner).map_err(|e| e.to_string())?;
        let status = unsafe { send(self.endpoint.get(), self.receiver, STATUS_TAG, &bytes, 1000) }?;
        SessionStatus::from_code(status as i32)
    }
    fn enter(&self) -> Result<Call<'_>, String> {
        if self.busy.replace(true) {
            Err("任务栏调用正在进行".into())
        } else {
            Ok(Call(&self.busy))
        }
    }
    pub fn heartbeat(&self) -> Result<(), String> {
        if let Some(error) = self.failure.borrow().as_ref() {
            return Err(error.clone());
        }
        if self.busy.get() {
            return Ok(());
        }
        let _call = self.enter()?;
        if let Err(error) = self.command(Change::Heartbeat) {
            self.context.active.set(false);
            *self.failure.borrow_mut() = Some(error.clone());
            return Err(error);
        }
        Ok(())
    }
    pub fn update(&self, frame: Frame) -> Result<(), String> {
        if let Some(error) = self.failure.borrow().as_ref() {
            return Err(error.clone());
        }
        let _call = self.enter()?;
        frame.validate()?;
        let change = match self.last.borrow().as_ref() {
            Some(old) if old == &frame => Change::Heartbeat,
            Some(old) if old.style == frame.style => Change::Tick {
                text: frame.text.clone(),
            },
            _ => Change::Update {
                frame: frame.clone(),
            },
        };
        if let Err(error) = self.command(change) {
            self.context.active.set(false);
            *self.failure.borrow_mut() = Some(error.clone());
            return Err(error);
        }
        *self.last.borrow_mut() = Some(frame);
        Ok(())
    }
    pub fn stop(&self) -> Result<(), String> {
        let _call = self.enter()?;
        self.context.active.set(false);
        self.context.pending.borrow_mut().take();
        if unsafe { WaitForSingleObject(self.process, 0) } == WAIT_OBJECT_0 {
            return Ok(());
        }
        self.command(Change::Stop)?;
        if self.status()? != SessionStatus::Closed {
            return Err("任务栏尚未确认恢复完成，连接继续保留".into());
        }
        Ok(())
    }
    pub fn take_activation(&self) -> Option<Activation> {
        let event = self.context.pending.borrow_mut().take()?;
        (self.context.active.get() && self.status().ok() == Some(SessionStatus::Active))
            .then_some(event)
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.stop();
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
            Err(format!(
                "任务栏通信超时或访问受限：{}",
                std::io::Error::last_os_error()
            ))
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
        if copy.dwData == EVENT_TAG
            && bytes.len() <= MAX_EVENT
            && context.active.get()
            && context.pending.borrow().is_none()
        {
            let Ok(event) = Activation::decode(bytes) else {
                return 0;
            };
            if event.source_window != w as u64 {
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
            if PostMessageW(context.main, WM_ACTIVATED, 0, 0) == 0 {
                return 0;
            }
            *context.pending.borrow_mut() = Some(event);
            return 1;
        }
        0
    }
}
