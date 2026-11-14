//! Clock lifetime independent of the settings window. Preparation can run in a
//! worker; all session calls stay on the owner thread. No OS discovery here.
use crate::{
    clock,
    config::Config,
    recovery::{Phase, Recovery},
};
use chrono::{DateTime, Local, Timelike};
use litetray_adapter_protocol::{activation::Activation, clock::Frame};
use std::{
    cell::{Cell, RefCell},
    sync::mpsc::{Receiver, TryRecvError},
};

const RESTORATION_CHECK_MILLIS: u64 = 5000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreProgress {
    Pending,
    Confirmed,
}

pub trait Session {
    /// A live capability limitation, e.g. clock running while tray is pending.
    fn notice(&self) -> Option<&str> {
        None
    }
    fn update(&self, frame: &Frame) -> Result<(), String>;
    /// Read-only comparison with the display/taskbar snapshot used at connect.
    /// An inconclusive observation must return false.
    fn environment_matches(&self) -> bool;
    /// Observe the originally retained native process, without a message/RPC.
    /// An exited process is distinct from a normal display topology change.
    fn process_exited(&self) -> bool {
        false
    }
    /// Success proves restoration, or that the retained native process exited.
    fn stop(&self) -> Result<(), String>;
    /// One bounded periodic step. A native transport can acknowledge Stop and
    /// defer its read-only confirmation to the next UI dispatch.
    fn restore_step(&self) -> Result<RestoreProgress, String> {
        self.stop().map(|()| RestoreProgress::Confirmed)
    }
    /// Read-only proof that a previously blocked restoration has completed.
    /// Unknown, busy and missing endpoints must remain unconfirmed.
    fn restoration_confirmed(&self) -> bool {
        false
    }
    fn take_activation(&self) -> Option<Activation>;
}
pub struct ConnectFailure<S> {
    pub error: String,
    /// Keep a session whose Start may have executed until stop confirms restore.
    pub pending: Option<S>,
}
pub trait Backend {
    type Prepared;
    type Session: Session;
    fn prepare(&self, config: &Config) -> Receiver<Result<Self::Prepared, String>>;
    fn connect(
        &self,
        prepared: Self::Prepared,
        frame: &Frame,
    ) -> Result<Self::Session, ConnectFailure<Self::Session>>;
}
struct Pending<P> {
    revision: u64,
    result: Receiver<Result<P, String>>,
}
struct Driver<B: Backend> {
    config: Config,
    weather: Option<crate::weather::WeatherResult>,
    enabled: bool,
    revision: u64,
    recovery: Recovery,
    connection: Option<B::Session>,
    pending: Option<Pending<B::Prepared>>,
    restoration_check_after: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Disabled,
    Preparing,
    Running,
    Restoring,
    Waiting,
    Suspended,
    RestoreBlocked,
    Busy,
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub enabled: bool,
    pub status: Status,
    pub reason: Option<String>,
}
impl Snapshot {
    pub fn text(&self) -> String {
        let label = match self.status {
            Status::Disabled => "已停用",
            Status::Preparing => "正在检查系统并连接…",
            Status::Running => "运行中",
            Status::Restoring => "正在恢复系统时钟…",
            Status::Waiting => "等待重新连接…",
            Status::Suspended => "连续连接失败，已暂停；可点击重试",
            Status::RestoreBlocked => "恢复尚未确认，连接已保留；可点击重试恢复",
            Status::Busy => "正在处理连接…",
        };
        match &self.reason {
            Some(reason) if self.status != Status::Disabled => {
                format!("任务栏时钟：{label}\r\n{reason}")
            }
            _ => format!("任务栏时钟：{label}"),
        }
    }
}
pub struct Service<B: Backend> {
    backend: B,
    // Remove the driver before native calls; reentrant UI callbacks cannot
    // borrow a connection while its XAML/WM_COPYDATA call is in progress.
    driver: RefCell<Option<Driver<B>>>,
    invalidated: Cell<bool>,
    devices_changed: Cell<bool>,
    stop_requested: Cell<bool>,
}
impl<B: Backend> Service<B> {
    pub fn new(backend: B, config: Config, now: u64) -> Result<Self, String> {
        config.validate()?;
        Ok(Self {
            backend,
            driver: RefCell::new(Some(Driver {
                enabled: config.tray.clock_enabled || config.tray.enabled && config.tray.show_icons,
                config,
                weather: None,
                revision: 0,
                recovery: Recovery::connecting(now),
                connection: None,
                pending: None,
                restoration_check_after: None,
            })),
            invalidated: Cell::new(false),
            devices_changed: Cell::new(false),
            stop_requested: Cell::new(false),
        })
    }
    fn with_driver<T>(
        &self,
        now: u64,
        action: impl FnOnce(&mut Driver<B>) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut driver = self
            .driver
            .borrow_mut()
            .take()
            .ok_or("任务栏连接正忙，请稍后重试")?;
        let result = action(&mut driver);
        // Stop may arrive during a native call. It cancels the new connection
        // before returning control, even if its initial call had to report Busy.
        let stopped = if self.stop_requested.replace(false) {
            driver.stop(now)
        } else {
            Ok(())
        };
        *self.driver.borrow_mut() = Some(driver);
        stopped.and(result)
    }
    pub fn configure(&self, config: Config, now: u64) -> Result<(), String> {
        config.validate()?;
        self.with_driver(now, |driver| {
            let changed = driver.config.tray.scope != config.tray.scope
                || driver.config.tray.enabled != config.tray.enabled
                || driver.config.tray.clock_enabled != config.tray.clock_enabled
                || driver.config.tray.show_icons != config.tray.show_icons
                || driver.config.tray.selected_devices != config.tray.selected_devices
                || driver.config.clock.click_action != config.clock.click_action
                || driver.config.clock.volume_scroll != config.clock.volume_scroll
                || driver.config.clock.volume_mute != config.clock.volume_mute;
            let enable = config.tray.clock_enabled || config.tray.enabled && config.tray.show_icons;
            driver.config = config;
            if !enable {
                return driver.stop(now);
            }
            if !driver.enabled || changed {
                driver.enabled = true;
                driver.restart(now, "显示范围或点击设置已改变");
            }
            Ok(())
        })
    }
    pub fn set_weather(&self, weather: Option<&crate::weather::WeatherResult>) {
        if let Some(driver) = self.driver.borrow_mut().as_mut()
            && driver.weather.as_ref() != weather
        {
            driver.weather = weather.cloned();
        }
    }
    /// Coalesce display/Explorer/power notifications, including modal dialogs.
    pub fn invalidate(&self) {
        self.invalidated.set(true);
    }
    /// A device notification may concern an unrelated USB/Bluetooth device.
    pub fn devices_changed(&self) {
        self.devices_changed.set(true);
    }
    pub fn retry(&self, now: u64) -> Result<(), String> {
        self.with_driver(now, |driver| {
            driver.recovery.retry(now);
            // Never enable a manually disabled service just by retrying restore.
            Ok(())
        })
    }
    /// Runtime shutdown keeps the saved enable preference for the next launch.
    pub fn stop(&self, now: u64) -> Result<(), String> {
        self.stop_requested.set(true);
        self.with_driver(now, |_| Ok(()))
    }
    pub fn tick(&self, now: u64, wall_time: DateTime<Local>) -> Result<(), String> {
        self.with_driver(now, |driver| {
            let invalidated = self.invalidated.replace(false);
            let check_devices = self.devices_changed.replace(false);
            if invalidated && driver.enabled {
                driver.environment_changed(now, "任务栏或显示环境已变化");
            } else if check_devices
                && driver.enabled
                && matches!(driver.recovery.phase(), Phase::Running { .. })
                && let Some(connection) = &driver.connection
            {
                if connection.environment_matches() {
                    crate::event_log::record("devices unchanged", "taskbar session retained");
                } else {
                    driver.environment_changed(now, "设备通知后任务栏或显示器拓扑已变化");
                }
            } else if check_devices && driver.enabled && !invalidated {
                // A preflight worker may already hold the old topology. Keep
                // the observation pending until its connection can be checked;
                // unrelated notifications must not reset failure backoff.
                self.devices_changed.set(true);
            }
            driver.tick(&self.backend, now, wall_time);
            Ok(())
        })
    }
    pub fn take_activation(&self) -> Option<Activation> {
        if self.invalidated.get() || self.stop_requested.get() {
            return None;
        }
        self.with_driver(0, |driver| {
            Ok(
                (driver.enabled && matches!(driver.recovery.phase(), Phase::Running { .. }))
                    .then(|| {
                        driver
                            .connection
                            .as_ref()
                            .and_then(Session::take_activation)
                    })
                    .flatten(),
            )
        })
        .ok()
        .flatten()
    }
    pub fn snapshot(&self) -> Snapshot {
        let borrow = self.driver.borrow();
        let Some(driver) = borrow.as_ref() else {
            return Snapshot {
                enabled: true,
                status: Status::Busy,
                reason: None,
            };
        };
        let status = if !driver.enabled && driver.connection.is_none() {
            Status::Disabled
        } else if driver
            .pending
            .as_ref()
            .is_some_and(|p| p.revision == driver.revision)
        {
            Status::Preparing
        } else {
            match driver.recovery.phase() {
                Phase::Running { .. } => Status::Running,
                Phase::Restoring => Status::Restoring,
                Phase::Waiting { .. } => Status::Waiting,
                Phase::Suspended => Status::Suspended,
                Phase::RestoreBlocked => Status::RestoreBlocked,
            }
        };
        Snapshot {
            enabled: driver.enabled,
            status,
            reason: if status == Status::Running {
                driver
                    .connection
                    .as_ref()
                    .and_then(|session| session.notice())
                    .map(str::to_owned)
            } else {
                driver.recovery.reason().map(str::to_owned)
            },
        }
    }
    pub fn tick_millis(&self, wall_time: DateTime<Local>) -> u32 {
        let borrow = self.driver.borrow();
        let Some(driver) = borrow.as_ref() else {
            return 1000;
        };
        if !driver.enabled && driver.connection.is_none() {
            return 60_000;
        }
        if matches!(driver.recovery.phase(), Phase::Running { .. }) {
            let period = if clock::needs_second_ticks(&driver.config.clock) {
                1000
            } else {
                60_000
            };
            let elapsed = wall_time.second() * 1000 + wall_time.timestamp_subsec_millis();
            (period - elapsed % period).clamp(50, 5000)
        } else {
            1000
        }
    }
}
impl<B: Backend> Driver<B> {
    fn environment_changed(&mut self, now: u64, reason: &str) {
        crate::event_log::record("restart", reason);
        self.revision = self.revision.wrapping_add(1);
        // Observe the retained process before a TaskbarCreated notification
        // turns its exit into an ordinary route invalidation. This does not
        // diagnose a crash: repeated short-lived exits are enough to suspend.
        if self
            .connection
            .as_ref()
            .is_some_and(Session::process_exited)
        {
            self.recovery.fault("任务栏进程在接入后退出，正在确认恢复");
        }
        self.recovery
            .environment_changed(now, self.connection.is_some(), reason);
        if self.recovery.phase() != Phase::RestoreBlocked {
            self.restoration_check_after = None;
        }
    }
    fn restart(&mut self, now: u64, reason: &str) {
        crate::event_log::record("restart", reason);
        self.revision = self.revision.wrapping_add(1);
        self.restoration_check_after = None;
        self.recovery = if self.connection.is_some() {
            Recovery::restarting(reason)
        } else {
            Recovery::connecting(now)
        };
        // An outstanding worker remains tracked until terminal. Its stale
        // result is discarded; new workers cannot accumulate during changes.
    }
    fn stop(&mut self, now: u64) -> Result<(), String> {
        self.enabled = false;
        self.restart(now, "正在停止任务栏时钟");
        self.restore(now, false)
    }
    fn restore(&mut self, now: u64, periodic: bool) -> Result<(), String> {
        if let Some(connection) = &self.connection {
            let result = if periodic {
                connection.restore_step()
            } else {
                connection.stop().map(|()| RestoreProgress::Confirmed)
            };
            if let Err(error) = result {
                crate::event_log::record("restore failed", &error);
                self.recovery.restore_failed(&error);
                if self.recovery.phase() == Phase::RestoreBlocked {
                    self.restoration_check_after =
                        Some(now.saturating_add(RESTORATION_CHECK_MILLIS));
                }
                return Err(error);
            }
            if result == Ok(RestoreProgress::Confirmed) {
                self.finish_restoration(now);
            }
        }
        Ok(())
    }
    fn finish_restoration(&mut self, now: u64) {
        self.connection.take();
        self.restoration_check_after = None;
        crate::event_log::record("restored", "native clock and tray state confirmed");
        self.recovery.restored(now);
    }
    fn observe_blocked_restoration(&mut self, now: u64) {
        if self.recovery.phase() != Phase::RestoreBlocked
            || !self
                .restoration_check_after
                .is_some_and(|deadline| now >= deadline)
        {
            return;
        }
        // A watchdog may finish after the bounded Stop attempts. Observe its
        // terminal state without writing another Stop or resetting the budget.
        self.restoration_check_after = Some(now.saturating_add(RESTORATION_CHECK_MILLIS));
        if self
            .connection
            .as_ref()
            .is_some_and(Session::restoration_confirmed)
        {
            self.finish_restoration(now);
        }
    }
    fn tick(&mut self, backend: &B, now: u64, wall_time: DateTime<Local>) {
        if matches!(self.recovery.phase(), Phase::Running { .. })
            && let Some(connection) = &self.connection
        {
            let frame =
                clock::frame_with_weather(&self.config.clock, self.weather.as_ref(), wall_time);
            match connection.update(&frame) {
                Ok(()) => self.recovery.healthy(now),
                Err(error) => {
                    crate::event_log::record("session fault", &error);
                    self.recovery.fault(&error);
                    // The failed native call already spent this dispatch's
                    // wait budget. Yield to input/quit before sending Stop.
                    return;
                }
            }
        }
        if self.recovery.phase() == Phase::Restoring {
            let _ = self.restore(now, true);
        }
        self.observe_blocked_restoration(now);
        let completed = self
            .pending
            .as_ref()
            .and_then(|pending| match pending.result.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    Some(Err("系统兼容性检查线程已结束但未返回结果".into()))
                }
            });
        if let Some(completed) = completed {
            let pending = self.pending.take().expect("polled pending worker");
            if self.enabled
                && pending.revision == self.revision
                && self.connection.is_none()
                && self.recovery.ready(now)
            {
                let frame =
                    clock::frame_with_weather(&self.config.clock, self.weather.as_ref(), wall_time);
                let connected = completed
                    .map_err(|error| ConnectFailure {
                        error,
                        pending: None,
                    })
                    .and_then(|prepared| backend.connect(prepared, &frame));
                match connected {
                    Ok(connection) => {
                        crate::event_log::record("connected", "taskbar session active");
                        self.connection = Some(connection);
                        self.recovery.connected(now);
                    }
                    Err(failure) => {
                        self.connection = failure.pending;
                        if self.connection.is_some() {
                            crate::event_log::record("start failed", &failure.error);
                            self.recovery.connected(now);
                            self.recovery.fault(&failure.error);
                            // Start may have executed. Retain its restoration
                            // obligation, then yield before the next RPC.
                        } else {
                            crate::event_log::record("connect failed", &failure.error);
                            self.recovery.connect_failed(now, &failure.error);
                        }
                    }
                }
            }
        }
        if self.enabled
            && self.connection.is_none()
            && self.pending.is_none()
            && self.recovery.ready(now)
        {
            self.pending = Some(Pending {
                revision: self.revision,
                result: backend.prepare(&self.config),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::{
        collections::VecDeque,
        rc::Rc,
        sync::mpsc::{self, Sender},
    };
    type Preparation = (u64, Sender<Result<u64, String>>);
    #[derive(Default)]
    struct NativeState {
        prepared: Cell<usize>,
        senders: RefCell<VecDeque<Preparation>>,
        route_generation: Cell<u64>,
        connected_routes: RefCell<Vec<u64>>,
        frames: RefCell<Vec<Frame>>,
        connects: Cell<usize>,
        stops: Cell<usize>,
        fail_start: Cell<bool>,
        fail_update: Cell<bool>,
        fail_restore: Cell<bool>,
        restored: Cell<bool>,
        restoration_checks: Cell<usize>,
        environment_changed: Cell<bool>,
        environment_checks: Cell<usize>,
        process_exited: Cell<bool>,
        notice: RefCell<Option<String>>,
    }
    struct Fake(Rc<NativeState>);
    struct Lease(Rc<NativeState>);
    impl Backend for Fake {
        type Prepared = u64;
        type Session = Lease;
        fn prepare(&self, _: &Config) -> Receiver<Result<u64, String>> {
            let (tx, rx) = mpsc::channel();
            self.0.prepared.set(self.0.prepared.get() + 1);
            self.0
                .senders
                .borrow_mut()
                .push_back((self.0.route_generation.get(), tx));
            rx
        }
        fn connect(&self, route: u64, frame: &Frame) -> Result<Lease, ConnectFailure<Lease>> {
            self.0.connects.set(self.0.connects.get() + 1);
            self.0.connected_routes.borrow_mut().push(route);
            self.0.frames.borrow_mut().push(frame.clone());
            let lease = Lease(self.0.clone());
            if self.0.fail_start.get() {
                Err(ConnectFailure {
                    error: "Start response lost".into(),
                    pending: Some(lease),
                })
            } else {
                Ok(lease)
            }
        }
    }
    impl Session for Lease {
        fn notice(&self) -> Option<&str> {
            // A fixture flag is sufficient; the user-facing reason originates
            // in real negotiation, tested through compatibility::Report.
            if self.0.notice.borrow().is_some() {
                Some("时钟已启用；托盘复制暂未启用")
            } else {
                None
            }
        }
        fn environment_matches(&self) -> bool {
            self.0
                .environment_checks
                .set(self.0.environment_checks.get() + 1);
            !self.0.environment_changed.get()
        }
        fn process_exited(&self) -> bool {
            self.0.process_exited.get()
        }
        fn update(&self, frame: &Frame) -> Result<(), String> {
            if self.0.fail_update.get() {
                return Err("native clock lost".into());
            }
            self.0.frames.borrow_mut().push(frame.clone());
            Ok(())
        }
        fn stop(&self) -> Result<(), String> {
            self.0.stops.set(self.0.stops.get() + 1);
            if self.0.fail_restore.get() {
                Err("restoration unconfirmed".into())
            } else {
                Ok(())
            }
        }
        fn restoration_confirmed(&self) -> bool {
            self.0
                .restoration_checks
                .set(self.0.restoration_checks.get() + 1);
            self.0.restored.get()
        }
        fn take_activation(&self) -> Option<Activation> {
            None
        }
    }
    fn time(seconds: i64) -> DateTime<Local> {
        Local.timestamp_opt(1_700_000_000 + seconds, 0).unwrap()
    }
    fn fixture() -> (Service<Fake>, Rc<NativeState>, Config) {
        let native = Rc::new(NativeState::default());
        let mut config = Config::default();
        config.tray.clock_enabled = true;
        config.clock.top_line = "%time%".into();
        let service = Service::new(Fake(native.clone()), config.clone(), 0).unwrap();
        (service, native, config)
    }
    fn complete(native: &NativeState, result: Result<(), String>) {
        let (route, tx) = native.senders.borrow_mut().pop_front().unwrap();
        tx.send(result.map(|()| route)).unwrap();
    }
    fn connect(service: &Service<Fake>, native: &NativeState) {
        service.tick(0, time(0)).unwrap();
        complete(native, Ok(()));
        service.tick(1, time(0)).unwrap();
        assert_eq!(service.snapshot().status, Status::Running);
    }

    #[cfg(windows)]
    #[test]
    fn native_timer_recovery_keeps_messages_responsive_when_log_io_stalls() {
        use std::{sync::Arc, time::Duration};
        use windows_sys::Win32::{
            Foundation::*,
            System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
            UI::WindowsAndMessaging::*,
        };
        const PROBE: u32 = WM_APP + 77;
        struct WindowState {
            service: Service<Fake>,
            result: Sender<Snapshot>,
        }
        unsafe extern "system" fn procedure(
            hwnd: HWND,
            message: u32,
            w: WPARAM,
            l: LPARAM,
        ) -> LRESULT {
            unsafe {
                let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowState;
                if let Some(state) = state.as_ref() {
                    if message == WM_TIMER && w == 1 {
                        // This is the same service call made by ui::window_proc.
                        let _ = state.service.tick(20, time(1));
                        return 0;
                    }
                    if message == PROBE {
                        let _ = state.result.send(state.service.snapshot());
                        PostQuitMessage(0);
                        return 0;
                    }
                }
                DefWindowProcW(hwnd, message, w, l)
            }
        }

        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let ui = std::thread::spawn(move || {
            let mut gate = Some(release_rx);
            let log = Arc::new(
                litetray_event_log::Log::new(4, move |_| {
                    if let Some(gate) = gate.take() {
                        let _ = entered_tx.send(());
                        let _ = gate.recv_timeout(Duration::from_secs(3));
                    }
                })
                .unwrap(),
            );
            let (service, native, _) = fixture();
            connect(&service, &native);
            native.fail_update.set(true);
            let state = Box::new(WindowState {
                service,
                result: result_tx,
            });
            unsafe {
                let class: Vec<u16> =
                    format!("LiteTray.SlowLog.TimerTest.{}", GetCurrentThreadId())
                        .encode_utf16()
                        .chain(Some(0))
                        .collect();
                let instance = GetModuleHandleW(std::ptr::null());
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
                    std::ptr::null(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    HWND_MESSAGE,
                    std::ptr::null_mut(),
                    instance,
                    std::ptr::null(),
                );
                assert!(!hwnd.is_null());
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, &*state as *const WindowState as isize);
                crate::event_log::testing::with_log(log, || {
                    assert_ne!(PostMessageW(hwnd, WM_TIMER, 1, 0), 0);
                    assert_ne!(PostMessageW(hwnd, PROBE, 0, 0), 0);
                    let mut message = std::mem::zeroed();
                    while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                        DispatchMessageW(&message);
                    }
                });
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                DestroyWindow(hwnd);
                UnregisterClassW(class.as_ptr(), instance);
            }
        });
        let sink_entered = entered_rx.recv_timeout(Duration::from_secs(3)).is_ok();
        let response = result_rx.recv_timeout(Duration::from_millis(250));
        // Always release the fake I/O and join before asserting a failure.
        let _ = release_tx.send(());
        ui.join().unwrap();
        assert!(
            sink_entered,
            "the real session-fault logger must reach the slow sink"
        );
        assert!(
            response.is_ok(),
            "the WM_TIMER session-fault/recovery path blocked the next UI message for >250ms while diagnostic I/O waited"
        );
        assert_eq!(response.unwrap().status, Status::Restoring);
    }
    #[test]
    fn running_session_exposes_partial_capabilities_without_restarting_or_changing_preferences() {
        let (service, native, mut config) = fixture();
        config.tray.enabled = true;
        config.tray.show_icons = true;
        service.configure(config.clone(), 0).unwrap();
        *native.notice.borrow_mut() = Some("partial capabilities".into());
        connect(&service, &native);
        let snapshot = service.snapshot();
        assert_eq!(snapshot.status, Status::Running);
        assert!(snapshot.text().contains("托盘复制暂未启用"));
        assert_eq!(
            serde_json::to_value(&service.driver.borrow().as_ref().unwrap().config).unwrap(),
            serde_json::to_value(config).unwrap()
        );
        service.tick(2000, time(2)).unwrap();
        assert_eq!(native.connects.get(), 1);
        native.notice.borrow_mut().take();
        assert!(service.snapshot().reason.is_none());
    }

    #[test]
    fn explorer_restart_notifications_cannot_erase_short_lived_process_exit_failures() {
        let (service, native, _) = fixture();
        connect(&service, &native);
        let mut now = 10;
        for failure in 1..=3 {
            native.process_exited.set(true);
            service.invalidate();
            service.tick(now, time(now as i64 / 1000)).unwrap();
            if failure == 3 {
                assert_eq!(
                    service.snapshot().status,
                    Status::Suspended,
                    "TaskbarCreated notifications reset the observed process-exit failure budget"
                );
                break;
            }
            native.process_exited.set(false);
            now += 10_000;
            service.tick(now, time(now as i64 / 1000)).unwrap();
            complete(&native, Ok(()));
            service.tick(now + 1, time(now as i64 / 1000)).unwrap();
            assert_eq!(service.snapshot().status, Status::Running);
            now += 10;
        }
        assert_eq!(native.connects.get(), 3);
        assert_eq!(native.stops.get(), 3);
        let preparations = native.prepared.get();
        native.process_exited.set(false);
        for _ in 0..5 {
            service.invalidate();
            now += 10_000;
            service.tick(now, time(now as i64 / 1000)).unwrap();
            assert_eq!(service.snapshot().status, Status::Suspended);
        }
        assert_eq!(native.prepared.get(), preparations);
        service.retry(now).unwrap();
        service.tick(now, time(now as i64 / 1000)).unwrap();
        complete(&native, Ok(()));
        service.tick(now + 1, time(now as i64 / 1000)).unwrap();
        assert_eq!(service.snapshot().status, Status::Running);
    }

    #[test]
    fn ordinary_display_changes_do_not_count_as_process_exit_failures() {
        let (service, native, _) = fixture();
        connect(&service, &native);
        let mut now = 10;
        for _ in 0..5 {
            service.invalidate();
            service.tick(now, time(0)).unwrap();
            assert_eq!(service.snapshot().status, Status::Waiting);
            now += 10_000;
            service.tick(now, time(0)).unwrap();
            complete(&native, Ok(()));
            service.tick(now + 1, time(0)).unwrap();
            assert_eq!(service.snapshot().status, Status::Running);
            now += 10;
        }
        assert_eq!(native.connects.get(), 6);
        assert_eq!(native.stops.get(), 5);
    }

    #[test]
    fn environment_notifications_keep_connection_backoff_and_suspension() {
        let (service, native, _) = fixture();
        for now in [0, 1001, 6002] {
            service.tick(now, time(0)).unwrap();
            complete(&native, Err("no taskbar".into()));
            service.tick(now + 1, time(0)).unwrap();
            let attempts = native.prepared.get();
            for delta in [10, 20, 100] {
                service.invalidate();
                service.tick(now + delta, time(0)).unwrap();
                assert_eq!(native.prepared.get(), attempts);
            }
        }
        assert_eq!(service.snapshot().status, Status::Suspended);
        service.invalidate();
        service.tick(90_000, time(90)).unwrap();
        assert_eq!(service.snapshot().status, Status::Suspended);
        assert_eq!(native.prepared.get(), 3);
    }

    #[test]
    fn environment_notifications_keep_blocked_restore_budget_and_read_only_deadline() {
        let (service, native, _) = fixture();
        connect(&service, &native);
        native.fail_update.set(true);
        native.fail_restore.set(true);
        for now in 10..=13 {
            service.tick(now, time(0)).unwrap();
        }
        assert_eq!(service.snapshot().status, Status::RestoreBlocked);
        assert_eq!(native.stops.get(), 3);
        native.restored.set(true);
        service.invalidate();
        service.tick(14, time(0)).unwrap();
        assert_eq!(service.snapshot().status, Status::RestoreBlocked);
        assert_eq!(native.restoration_checks.get(), 0);
        service.tick(5013, time(5)).unwrap();
        assert_eq!(service.snapshot().status, Status::Waiting);
        assert_eq!(native.stops.get(), 3);
        assert_eq!(native.restoration_checks.get(), 1);
    }
    #[test]
    fn tray_can_stay_enabled_when_clock_is_disabled() {
        let (service, native, mut config) = fixture();
        config.tray.clock_enabled = false;
        config.tray.enabled = true;
        service.configure(config.clone(), 0).unwrap();
        connect(&service, &native);
        config.tray.enabled = false;
        service.configure(config, 10).unwrap();
        assert_eq!(service.snapshot().status, Status::Disabled);
        assert_eq!(native.stops.get(), 1);
    }
    #[test]
    fn hidden_host_formats_and_updates_seconds_without_view_calls() {
        let (service, native, _) = fixture();
        connect(&service, &native);
        let mut weather = clock::weather_format::sample(time(0));
        weather.weather.as_mut().unwrap().temperature_c = 26.;
        service.set_weather(Some(&weather));
        for seconds in 1..=65 {
            service.tick(seconds * 1000, time(seconds as i64)).unwrap();
        }
        let frames = native.frames.borrow();
        assert_eq!(frames.len(), 66);
        assert_ne!(frames[0].text.top, frames[65].text.top);
        assert_eq!(frames[65].text.bottom, "晴 26°C");
        assert_eq!(service.snapshot().status, Status::Running);
        assert!(service.tick_millis(time(65)) <= 1000);
    }
    #[test]
    fn structured_weather_reaches_both_lines_and_tooltip_without_reconnecting() {
        let (service, native, mut config) = fixture();
        config.clock.weather_format =
            "%weather_condition% %weather_temp:1% · 湿度 %weather_humidity%".into();
        config.clock.top_line = "%time% %weather_temp%".into();
        config.clock.tooltip_line = "%weather_city% %weather_source%\n%weather_wind_ms:1%".into();
        service.configure(config.clone(), 0).unwrap();
        let weather = clock::weather_format::sample(time(0));
        service.set_weather(Some(&weather));
        connect(&service, &native);
        {
            let frames = native.frames.borrow();
            let frame = frames.last().unwrap();
            assert!(frame.text.top.ends_with("23°C"));
            assert_eq!(frame.text.bottom, "晴 23.4°C · 湿度 68%");
            assert_eq!(
                frame.text.tooltip.as_deref(),
                Some("示例城市 Open-Meteo\n3.5 m/s")
            );
        }
        config.clock.weather_format = "体感 %weather_feels:1%".into();
        service.configure(config, 2).unwrap();
        service.tick(3, time(1)).unwrap();
        assert_eq!(
            native.frames.borrow().last().unwrap().text.bottom,
            "体感 25.1°C"
        );
        assert_eq!(native.connects.get(), 1);
        service.set_weather(None);
        service.tick(4, time(2)).unwrap();
        assert_eq!(
            native.frames.borrow().last().unwrap().text.bottom,
            "天气暂无数据"
        );
    }
    #[test]
    fn disabling_during_preflight_prevents_late_start() {
        let (service, native, mut config) = fixture();
        service.tick(0, time(0)).unwrap();
        config.tray.clock_enabled = false;
        service.configure(config, 10).unwrap();
        complete(&native, Ok(()));
        service.tick(20, time(0)).unwrap();
        assert_eq!(native.connects.get(), 0);
        assert_eq!(service.snapshot().status, Status::Disabled);
    }
    #[test]
    fn invalidations_coalesce_and_discard_old_preflight_results() {
        let (service, native, _) = fixture();
        service.tick(0, time(0)).unwrap();
        for now in 1..=10 {
            service.invalidate();
            service.tick(now, time(0)).unwrap();
        }
        assert_eq!(native.prepared.get(), 1);
        complete(&native, Ok(()));
        service.tick(11, time(0)).unwrap();
        assert_eq!(native.connects.get(), 0);
        assert_eq!(native.prepared.get(), 2);
        complete(&native, Ok(()));
        service.tick(12, time(0)).unwrap();
        assert_eq!(native.connects.get(), 1);
    }
    #[test]
    fn unrelated_device_notifications_preserve_the_running_clock() {
        let (service, native, _) = fixture();
        connect(&service, &native);
        for now in 2..=5 {
            // Replay the burst of DBT_DEVNODES_CHANGED notifications seen in
            // the event log while the taskbars and displays were unchanged.
            service.devices_changed();
            service.devices_changed();
            service.tick(now, time(now as i64)).unwrap();
            assert_eq!(service.snapshot().status, Status::Running);
        }
        assert_eq!(native.stops.get(), 0, "native clock must not flash back");
        assert_eq!(native.connects.get(), 1);
        assert_eq!(native.frames.borrow().len(), 5);
        assert_eq!(native.environment_checks.get(), 4);
    }
    #[test]
    fn a_real_device_topology_change_restores_before_reconnecting() {
        let (service, native, _) = fixture();
        connect(&service, &native);
        native.environment_changed.set(true);
        service.devices_changed();
        service.tick(20, time(1)).unwrap();
        assert_eq!(native.stops.get(), 1);
        assert_eq!(service.snapshot().status, Status::Waiting);
        service.tick(1020, time(2)).unwrap();
        assert_eq!(native.prepared.get(), 2);
        complete(&native, Ok(()));
        service.tick(1021, time(2)).unwrap();
        assert_eq!(native.connects.get(), 2);
    }
    #[test]
    fn a_device_change_during_preflight_is_checked_after_connecting() {
        let (service, native, _) = fixture();
        service.tick(0, time(0)).unwrap();
        // The worker already sampled the old desktop when hotplug arrived.
        service.devices_changed();
        native.environment_changed.set(true);
        service.tick(1, time(0)).unwrap();
        complete(&native, Ok(()));
        service.tick(2, time(0)).unwrap();
        service.tick(3, time(0)).unwrap();
        assert_eq!(native.environment_checks.get(), 1);
        assert_eq!(native.stops.get(), 1, "stale routes must be restored");
        assert_eq!(service.snapshot().status, Status::Waiting);
    }
    #[test]
    fn explorer_notifications_always_restore_even_if_devices_match() {
        let (service, native, _) = fixture();
        connect(&service, &native);
        service.devices_changed();
        service.invalidate();
        service.tick(20, time(1)).unwrap();
        assert_eq!(native.stops.get(), 1);
        assert_eq!(native.environment_checks.get(), 0);
    }
    #[test]
    fn failed_start_retains_restore_obligation_and_blocks_reconnect() {
        let (service, native, _) = fixture();
        native.fail_start.set(true);
        native.fail_restore.set(true);
        service.tick(0, time(0)).unwrap();
        complete(&native, Ok(()));
        for now in 1..=5 {
            service.tick(now, time(0)).unwrap();
        }
        assert_eq!(service.snapshot().status, Status::RestoreBlocked);
        assert_eq!(native.prepared.get(), 1);
        assert_eq!(native.stops.get(), 3);
        native.fail_restore.set(false);
        service.retry(10).unwrap();
        service.tick(10, time(0)).unwrap();
        assert_eq!(service.snapshot().status, Status::Waiting);
        service.tick(1009, time(1)).unwrap();
        assert_eq!(native.prepared.get(), 1);
        service.tick(1010, time(1)).unwrap();
        assert_eq!(native.prepared.get(), 2);
    }
    #[test]
    fn blocked_restore_observes_later_native_close_without_repeating_stop() {
        let (service, native, _) = fixture();
        connect(&service, &native);
        native.route_generation.set(1);
        native.fail_restore.set(true);
        service.invalidate();
        for now in 10..=12 {
            service.tick(now, time(1)).unwrap();
        }
        assert_eq!(service.snapshot().status, Status::RestoreBlocked);
        assert_eq!(native.stops.get(), 3);
        // The adapter watchdog eventually finishes restoring its retained
        // objects, without another host Stop or a manual Retry action.
        native.restored.set(true);
        service.tick(5012, time(5)).unwrap();
        assert_eq!(service.snapshot().status, Status::Waiting);
        assert_eq!(native.stops.get(), 3, "confirmation must not write Stop");
        assert_eq!(native.prepared.get(), 1);
        service.tick(6012, time(6)).unwrap();
        complete(&native, Ok(()));
        service.tick(6013, time(6)).unwrap();
        assert_eq!(service.snapshot().status, Status::Running);
        assert_eq!(*native.connected_routes.borrow(), [0, 1]);
    }
    #[test]
    fn blocked_restore_checks_are_throttled_and_unknown_keeps_the_lease() {
        let (service, native, _) = fixture();
        connect(&service, &native);
        service.tick(5000, time(5)).unwrap();
        assert_eq!(native.restoration_checks.get(), 0, "no running-path probe");
        native.fail_restore.set(true);
        service.invalidate();
        for now in 5010..=5012 {
            service.tick(now, time(5)).unwrap();
        }
        for now in [5013, 6000, 10011] {
            service.tick(now, time(6)).unwrap();
        }
        assert_eq!(native.restoration_checks.get(), 0);
        service.tick(10012, time(10)).unwrap();
        assert_eq!(native.restoration_checks.get(), 1);
        for now in [10013, 11000, 15011] {
            service.tick(now, time(11)).unwrap();
        }
        assert_eq!(native.restoration_checks.get(), 1);
        service.tick(15012, time(15)).unwrap();
        assert_eq!(native.restoration_checks.get(), 2);
        assert_eq!(native.stops.get(), 3, "no repeated Stop after the budget");
        assert_eq!(
            native.prepared.get(),
            1,
            "unconfirmed state must be retained"
        );
        assert_eq!(service.snapshot().status, Status::RestoreBlocked);
    }
    #[test]
    fn late_restore_confirmation_does_not_reenable_a_disabled_service() {
        let (service, native, _) = fixture();
        connect(&service, &native);
        native.fail_restore.set(true);
        assert!(service.stop(10).is_err());
        service.tick(11, time(1)).unwrap();
        service.tick(12, time(1)).unwrap();
        assert_eq!(service.snapshot().status, Status::RestoreBlocked);
        assert!(!service.snapshot().enabled);
        native.restored.set(true);
        service.tick(5012, time(5)).unwrap();
        service.tick(10000, time(10)).unwrap();
        assert_eq!(service.snapshot().status, Status::Disabled);
        assert_eq!(native.prepared.get(), 1);
        assert_eq!(native.stops.get(), 3);
        assert_eq!(native.restoration_checks.get(), 1);
    }
    #[test]
    fn repeated_hotplug_restores_before_preparing_each_new_route() {
        let (service, native, _) = fixture();
        connect(&service, &native);
        for route in 1..=4 {
            let now = route * 10_000;
            native.route_generation.set(route);
            native.fail_restore.set(true);
            service.invalidate();
            service.tick(now, time(route as i64)).unwrap();
            assert_eq!(service.snapshot().status, Status::Restoring);
            assert_eq!(native.prepared.get(), route as usize);
            native.fail_restore.set(false);
            service.tick(now + 1, time(route as i64)).unwrap();
            assert_eq!(service.snapshot().status, Status::Waiting);
            service.tick(now + 1000, time(route as i64)).unwrap();
            assert_eq!(native.prepared.get(), route as usize);
            service.tick(now + 1001, time(route as i64)).unwrap();
            complete(&native, Ok(()));
            service.tick(now + 1002, time(route as i64)).unwrap();
            assert_eq!(service.snapshot().status, Status::Running);
            assert_eq!(native.connects.get(), route as usize + 1);
        }
        assert_eq!(*native.connected_routes.borrow(), [0, 1, 2, 3, 4]);
        assert_eq!(native.stops.get(), 8);
        assert_eq!(native.restoration_checks.get(), 0);
    }
    #[test]
    fn settings_update_in_place_but_route_changes_restore_first() {
        let (service, native, mut config) = fixture();
        connect(&service, &native);
        config.clock.top_line = "Changed %time%".into();
        service.configure(config.clone(), 10).unwrap();
        service.tick(10, time(1)).unwrap();
        assert!(
            native
                .frames
                .borrow()
                .last()
                .unwrap()
                .text
                .top
                .starts_with("Changed")
        );
        assert_eq!(native.stops.get(), 0);
        config.tray.scope = crate::config::MonitorScope::Primary;
        service.configure(config, 20).unwrap();
        service.tick(20, time(1)).unwrap();
        assert_eq!(native.stops.get(), 1);
        assert_eq!(native.prepared.get(), 1);
        service.tick(1020, time(2)).unwrap();
        assert_eq!(native.prepared.get(), 2);
    }
    #[test]
    fn failed_connections_suspend_until_explicit_retry() {
        let (service, native, _) = fixture();
        for now in [0, 1001, 6002] {
            service.tick(now, time(0)).unwrap();
            complete(&native, Err("no taskbar".into()));
            service.tick(now + 1, time(0)).unwrap();
        }
        assert_eq!(service.snapshot().status, Status::Suspended);
        service.devices_changed();
        service.tick(90000, time(90)).unwrap();
        assert_eq!(native.prepared.get(), 3);
        service.retry(90000).unwrap();
        service.tick(90000, time(90)).unwrap();
        assert_eq!(native.prepared.get(), 4);
    }
}
