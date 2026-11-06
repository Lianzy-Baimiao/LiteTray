//! Thread-bound owner of the in-process XAML preview DLL and event gate.
use crate::{clock, config::ClockConfig};
use litetray_adapter_protocol::{
    activation::{Activation, Gate},
    clock::{Frame, Owner, SessionStatus, Style, Text},
};
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    marker::PhantomData,
    ptr::{null, null_mut},
    rc::Rc,
};
use windows_sys::Win32::{
    Foundation::*,
    System::{LibraryLoader::*, Threading::GetCurrentProcessId},
    UI::WindowsAndMessaging::*,
};
type Create = unsafe extern "system" fn(*const u8, usize, *mut u64, *mut u8, usize) -> *mut c_void;
type Update = unsafe extern "system" fn(*mut c_void, *const u8, usize) -> i32;
type Message = unsafe extern "system" fn(*mut c_void, *const MSG) -> i32;
type Action = unsafe extern "system" fn(*mut c_void);
type Heartbeat = unsafe extern "system" fn(*mut c_void) -> i32;
type Present = unsafe extern "system" fn(*mut c_void, i32);
struct Connection {
    _environment: Rc<Environment>,
    handle: *mut c_void,
    update: Update,
    message: Message,
    present: Present,
    destroy: Action,
    heartbeat: Heartbeat,
    state: Heartbeat,
    stop: Heartbeat,
    gate: RefCell<Gate>,
    island: HWND,
    owner: Owner,
    busy: Cell<bool>,
    _thread: PhantomData<Rc<()>>,
}
/// One XAML hosting lifetime per service/STA, spanning every connection.
/// Connections also pin it so a reentrant service drop cannot unload native code.
struct Environment {
    module: HMODULE,
    handle: *mut c_void,
    destroy: Action,
    _thread: PhantomData<Rc<()>>,
}
impl Environment {
    fn open() -> Result<Rc<Self>, String> {
        unsafe {
            let dll = std::env::current_exe()
                .map_err(|e| e.to_string())?
                .with_file_name("litetray_taskbar_adapter.dll");
            let module = LoadLibraryExW(
                super::wide(&dll.to_string_lossy()).as_ptr(),
                null_mut(),
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            );
            if module.is_null() {
                return Err("无法加载原生时钟预览组件".into());
            }
            struct Library(HMODULE);
            impl Drop for Library {
                fn drop(&mut self) {
                    unsafe {
                        FreeLibrary(self.0);
                    }
                }
            }
            let guard = Library(module);
            let create: unsafe extern "system" fn(*mut u8, usize) -> *mut c_void =
                std::mem::transmute(
                    GetProcAddress(module, c"LiteTrayPreviewRuntimeCreate".as_ptr().cast())
                        .ok_or("缺少预览运行环境入口")?,
                );
            let destroy: Action = std::mem::transmute(
                GetProcAddress(module, c"LiteTrayPreviewRuntimeDestroy".as_ptr().cast())
                    .ok_or("缺少预览运行环境关闭入口")?,
            );
            let mut error = vec![0u8; 2048];
            let handle = create(error.as_mut_ptr(), error.len());
            if handle.is_null() {
                let n = error.iter().position(|b| *b == 0).unwrap_or(error.len());
                return Err(String::from_utf8_lossy(&error[..n]).into());
            }
            std::mem::forget(guard);
            Ok(Rc::new(Self {
                module,
                handle,
                destroy,
                _thread: PhantomData,
            }))
        }
    }
}
impl Drop for Environment {
    fn drop(&mut self) {
        unsafe {
            (self.destroy)(self.handle);
            FreeLibrary(self.module);
        }
    }
}
pub fn frame(config: &ClockConfig, weather: &str) -> Frame {
    let now = chrono::Local::now();
    Frame {
        text: Text {
            top: clock::format(&config.top_line, config, now, weather),
            bottom: clock::format(&config.bottom_line, config, now, weather),
            tooltip: (!config.tooltip_line.is_empty())
                .then(|| clock::format(&config.tooltip_line, config, now, weather)),
        },
        style: Style {
            font_family: config.font_family.clone(),
            font_size: config.font_size,
            width: config.width,
            top_color: (!config.top_color.is_empty()).then(|| config.top_color.clone()),
            bottom_color: (!config.bottom_color.is_empty()).then(|| config.bottom_color.clone()),
            alignment: config.alignment,
            top_style: config.top_style.clone(),
            bottom_style: config.bottom_style.clone(),
        },
    }
}
#[cfg(test)]
mod frame_tests {
    use super::*;
    #[test]
    fn host_formats_tooltip_and_preserves_weather_as_literal_text() {
        let config = ClockConfig {
            tooltip_line: "%weather%\n%year%".into(),
            top_color: "#123456".into(),
            bottom_color: "#80112233".into(),
            alignment: litetray_adapter_protocol::clock::Alignment::Center,
            top_style: litetray_adapter_protocol::clock::LineStyle {
                font_family: Some("Consolas".into()),
                font_size: Some(18),
                alignment: Some(litetray_adapter_protocol::clock::Alignment::Native),
            },
            ..ClockConfig::default()
        };
        let frame = frame(&config, "Humidity 80% <literal> %time%");
        frame.validate().unwrap();
        assert!(
            frame
                .text
                .tooltip
                .as_ref()
                .unwrap()
                .starts_with("Humidity 80% <literal> %time%\n")
        );
        assert_eq!(frame.style.top_color, Some("#123456".into()));
        assert_eq!(frame.style.bottom_color, Some("#80112233".into()));
        assert_eq!(frame.style.top_style, config.top_style);
        assert_eq!(frame.style.bottom_style, Default::default());
        assert_eq!(
            frame.style.alignment,
            litetray_adapter_protocol::clock::Alignment::Center
        );
    }
}
impl Connection {
    fn open(environment: Rc<Environment>, receiver: HWND, frame: Frame) -> Result<Self, String> {
        unsafe {
            let module = environment.module;
            let create: Create = std::mem::transmute(
                GetProcAddress(module, c"LiteTrayPreviewCreate".as_ptr().cast())
                    .ok_or("缺少原生预览入口")?,
            );
            let update: Update = std::mem::transmute(
                GetProcAddress(module, c"LiteTrayPreviewUpdate".as_ptr().cast())
                    .ok_or("缺少原生预览更新入口")?,
            );
            let message: Message = std::mem::transmute(
                GetProcAddress(module, c"LiteTrayPreviewMessage".as_ptr().cast())
                    .ok_or("缺少原生预览消息入口")?,
            );
            let present: Present = std::mem::transmute(
                GetProcAddress(module, c"LiteTrayPreviewPresent".as_ptr().cast())
                    .ok_or("缺少原生预览显示入口")?,
            );
            let destroy: Action = std::mem::transmute(
                GetProcAddress(module, c"LiteTrayPreviewDestroy".as_ptr().cast())
                    .ok_or("缺少原生预览关闭入口")?,
            );
            let heartbeat: Heartbeat = std::mem::transmute(
                GetProcAddress(module, c"LiteTrayPreviewHeartbeat".as_ptr().cast())
                    .ok_or("缺少原生预览心跳入口")?,
            );
            let state: Heartbeat = std::mem::transmute(
                GetProcAddress(module, c"LiteTrayPreviewState".as_ptr().cast())
                    .ok_or("缺少原生预览状态入口")?,
            );
            let stop: Heartbeat = std::mem::transmute(
                GetProcAddress(module, c"LiteTrayPreviewStop".as_ptr().cast())
                    .ok_or("缺少原生预览停止入口")?,
            );
            let owner = Owner {
                nonce: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos() as u64,
                process_id: GetCurrentProcessId(),
                receiver: receiver as u64,
            };
            let bytes = serde_json::to_vec(
                &serde_json::json!({"owner":owner,"frame":frame,"visible":false}),
            )
            .map_err(|e| e.to_string())?;
            let mut island = 0;
            let mut error = vec![0u8; 2048];
            let handle = create(
                bytes.as_ptr(),
                bytes.len(),
                &mut island,
                error.as_mut_ptr(),
                error.len(),
            );
            if handle.is_null() {
                let n = error.iter().position(|b| *b == 0).unwrap_or(error.len());
                return Err(String::from_utf8_lossy(&error[..n]).into());
            }
            let gate = match Gate::new(owner.clone(), vec![(1, island)]) {
                Ok(g) => g,
                Err(e) => {
                    destroy(handle);
                    return Err(e);
                }
            };
            Ok(Self {
                _environment: environment,
                handle,
                update,
                message,
                present,
                destroy,
                heartbeat,
                state,
                stop,
                gate: RefCell::new(gate),
                island: island as HWND,
                owner,
                busy: Cell::new(false),
                _thread: PhantomData,
            })
        }
    }
    pub fn accept(&self, sender: HWND, bytes: &[u8]) -> Result<Activation, String> {
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(sender, &mut pid);
            if sender != self.island || pid != GetCurrentProcessId() {
                return Err("原生预览点击窗口不匹配".into());
            }
            self.gate
                .try_borrow_mut()
                .map_err(|_| "点击接收正忙")?
                .accept(bytes)
        }
    }
    fn enter(&self) -> Option<Call<'_>> {
        if self.busy.replace(true) {
            None
        } else {
            Some(Call(&self.busy))
        }
    }
    fn update(&self, frame: &Frame) -> Result<bool, String> {
        let Some(_call) = self.enter() else {
            return Ok(false);
        };
        let bytes = serde_json::to_vec(frame).map_err(|e| e.to_string())?;
        if unsafe { (self.update)(self.handle, bytes.as_ptr(), bytes.len()) } == 0 {
            Err("原生预览更新失败".into())
        } else {
            Ok(true)
        }
    }
    pub fn visible(&self) -> bool {
        let Some(_call) = self.enter() else {
            return false;
        };
        unsafe { (self.message)(self.handle, null()) != 0 }
    }
    fn show(&self, activate: bool) {
        let Some(_call) = self.enter() else {
            return;
        };
        unsafe {
            (self.present)(self.handle, i32::from(activate));
        }
    }
    fn heartbeat(&self) -> Result<bool, String> {
        let Some(_call) = self.enter() else {
            return Ok(false);
        };
        if unsafe { (self.heartbeat)(self.handle) } != 0 {
            Ok(true)
        } else {
            Err("原生预览心跳失败".into())
        }
    }
    pub fn pretranslate(&self, message: &MSG) -> bool {
        let Some(_call) = self.enter() else {
            return false;
        };
        unsafe { (self.message)(self.handle, message) != 0 }
    }
    fn status(&self) -> Result<SessionStatus, String> {
        let Some(_call) = self.enter() else {
            return Ok(SessionStatus::Busy);
        };
        SessionStatus::from_code(unsafe { (self.state)(self.handle) })
    }
    fn stop(&self) -> Result<bool, String> {
        let Some(_call) = self.enter() else {
            return Ok(false);
        };
        if unsafe { (self.stop)(self.handle) } == 0 {
            Err("原生预览恢复尚未完成".into())
        } else {
            Ok(true)
        }
    }
}
struct Call<'a>(&'a Cell<bool>);
impl Drop for Call<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        unsafe {
            (self.destroy)(self.handle);
        }
    }
}

/// Recovery ownership lives outside App's modal borrow. Every native call
/// retains a Connection Rc; a reentrant callback cannot unload its own DLL.
pub struct Preview {
    receiver: HWND,
    environment: Rc<Environment>,
    started: std::time::Instant,
    state: RefCell<Option<PreviewState>>,
}
struct PreviewState {
    connection: Option<Rc<Connection>>,
    recovery: crate::recovery::Recovery,
    desired: Frame,
    visible: bool,
    next_restore: u64,
    applied_error: Option<String>,
}
impl Preview {
    pub fn open(receiver: HWND, frame: Frame) -> Result<Self, String> {
        frame.validate()?;
        let environment = Environment::open()?;
        let mut recovery = crate::recovery::Recovery::connecting(0);
        let connection = match Connection::open(environment.clone(), receiver, frame.clone()) {
            Ok(connection) => {
                let connection = Rc::new(connection);
                connection.show(true);
                recovery.connected(0);
                Some(connection)
            }
            Err(error) => {
                // Keep the STA runtime available for a later explicit retry,
                // including when the first island could not be constructed.
                recovery.connect_failed(0, &error);
                None
            }
        };
        Ok(Self {
            receiver,
            environment,
            started: std::time::Instant::now(),
            state: RefCell::new(Some(PreviewState {
                connection,
                recovery,
                desired: frame,
                visible: true,
                next_restore: 0,
                applied_error: None,
            })),
        })
    }
    fn now(&self) -> u64 {
        self.started.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
    fn connection(&self) -> Option<Rc<Connection>> {
        self.state.borrow().as_ref()?.connection.clone()
    }
    fn pump(&self, frame: Option<&Frame>, retry: bool) -> Result<(), String> {
        let Some(mut state) = self.state.borrow_mut().take() else {
            return Ok(());
        };
        let now = self.now();
        if let Some(frame) = frame {
            if let Err(error) = frame.validate() {
                state.applied_error = Some(error.clone());
                *self.state.borrow_mut() = Some(state);
                return Err(error);
            }
            state.desired = frame.clone();
            state.applied_error = None;
        }
        if retry {
            state.recovery.retry(now);
            state.next_restore = now;
        }
        self.advance(&mut state, now, frame.is_some());
        *self.state.borrow_mut() = Some(state);
        Ok(())
    }
    fn advance(&self, state: &mut PreviewState, now: u64, update: bool) {
        use crate::recovery::Phase;
        if matches!(state.recovery.phase(), Phase::Running { .. }) {
            let Some(connection) = state.connection.clone() else {
                state.recovery.fault("预览连接丢失");
                return;
            };
            if connection.busy.get() {
                return;
            }
            state.visible = connection.visible();
            let result = match connection.status() {
                Ok(SessionStatus::Active) => {
                    if update {
                        connection.update(&state.desired)
                    } else {
                        connection.heartbeat()
                    }
                }
                Ok(SessionStatus::Busy) => return,
                Ok(_) => Err("原生预览会话已停止或正在恢复".into()),
                Err(error) => Err(error),
            };
            match result {
                Ok(true) => state.recovery.healthy(now),
                Ok(false) => return,
                Err(error) => {
                    state.recovery.fault(&error);
                    state.next_restore = now;
                }
            }
        }
        if state.recovery.phase() == Phase::Restoring && now >= state.next_restore {
            if let Some(connection) = &state.connection {
                if connection.busy.get() {
                    return;
                }
                // A failed stop may nevertheless have restored everything.
                // Observe status independently; never infer Closed from an error.
                let stopped = connection.stop();
                match connection.status() {
                    Ok(SessionStatus::Closed) => {}
                    Ok(SessionStatus::Busy) => return,
                    status => {
                        let reason = status
                            .err()
                            .or_else(|| stopped.err())
                            .unwrap_or("原生预览尚未确认恢复完成".into());
                        state.recovery.restore_failed(&reason);
                        state.next_restore = now.saturating_add(1000);
                        return;
                    }
                }
            } else {
                state
                    .recovery
                    .restore_failed("缺少旧连接，无法确认原状态已恢复");
                state.next_restore = now.saturating_add(1000);
                return;
            }
            state.connection.take();
            state.recovery.restored(now);
        }
        if state.recovery.ready(now) {
            // The old connection must have a Closed proof and be released.
            if state.connection.is_some() {
                return;
            }
            match Connection::open(
                self.environment.clone(),
                self.receiver,
                state.desired.clone(),
            ) {
                Ok(connection) => {
                    let connection = Rc::new(connection);
                    if state.visible {
                        connection.show(false);
                    }
                    state.connection = Some(connection);
                    state.recovery.connected(now);
                }
                Err(error) => state.recovery.connect_failed(now, &error),
            }
        }
    }
    pub fn update(&self, frame: &Frame) -> Result<(), String> {
        self.pump(Some(frame), false)
    }
    pub fn heartbeat(&self) -> Result<(), String> {
        self.pump(None, false)
    }
    pub fn retry(&self) {
        let _ = self.pump(None, true);
    }
    pub fn show(&self) {
        self.retry();
        if let Some(state) = self.state.borrow_mut().as_mut() {
            state.visible = true;
        }
        let running =
            self.state.borrow().as_ref().is_some_and(|s| {
                matches!(s.recovery.phase(), crate::recovery::Phase::Running { .. })
            });
        if running && let Some(connection) = self.connection() {
            connection.show(true);
        }
    }
    pub fn visible(&self) -> bool {
        self.connection().is_some_and(|c| c.visible())
    }
    pub fn pretranslate(&self, message: &MSG) -> bool {
        self.connection().is_some_and(|c| c.pretranslate(message))
    }
    pub fn accept(&self, sender: HWND, bytes: &[u8]) -> Result<Activation, String> {
        let connection = {
            let state = self.state.borrow();
            let state = state.as_ref().ok_or("预览正在恢复")?;
            if !matches!(
                state.recovery.phase(),
                crate::recovery::Phase::Running { .. }
            ) {
                return Err("预览会话已失效".into());
            }
            state.connection.clone().ok_or("预览连接缺失")?
        };
        connection.accept(sender, bytes)
    }
    pub fn owns_activation(&self, event: &Activation) -> bool {
        let state = self.state.borrow();
        state.as_ref().is_some_and(|state| {
            matches!(
                state.recovery.phase(),
                crate::recovery::Phase::Running { .. }
            ) && state.connection.as_ref().is_some_and(|c| {
                c.owner == event.owner
                    && c.island as u64 == event.source_window
                    && c.status() == Ok(SessionStatus::Active)
            })
        })
    }
    pub fn tick_millis(&self) -> u32 {
        use crate::recovery::Phase;
        self.state
            .borrow()
            .as_ref()
            .map_or(1000, |state| match state.recovery.phase() {
                Phase::Restoring | Phase::Waiting { .. } => 1000,
                Phase::Running { .. } => 5000,
                _ => 60000,
            })
    }
    pub fn status_text(&self) -> String {
        use crate::recovery::Phase;
        let state = self.state.borrow();
        let Some(state) = state.as_ref() else {
            return "原生预览：正在处理…".into();
        };
        let label = match state.recovery.phase() {
            Phase::Running { .. } => "运行中",
            Phase::Restoring => "正在恢复原状态",
            Phase::Waiting { .. } => "等待重新连接",
            Phase::Suspended => "连续失败，自动恢复已暂停",
            Phase::RestoreBlocked => "原状态恢复未完成，已暂停重建",
        };
        let detail = state.applied_error.as_deref().or(state.recovery.reason());
        format!(
            "原生预览：{label}{}",
            detail.map(|s| format!("\r\n{s}")).unwrap_or_default()
        )
    }
}

#[path = "clock_preview_checks.rs"]
mod checks;
#[doc(hidden)]
pub use checks::self_test;
