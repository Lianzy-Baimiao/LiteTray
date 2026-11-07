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

pub trait Session {
    fn update(&self, frame: &Frame) -> Result<(), String>;
    /// Success proves restoration, or that the retained native process exited.
    fn stop(&self) -> Result<(), String>;
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
            Some(reason) if !matches!(self.status, Status::Running | Status::Disabled) => {
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
            })),
            invalidated: Cell::new(false),
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
            if self.invalidated.replace(false) && driver.enabled {
                driver.restart(now, "任务栏或显示环境已变化");
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
            reason: driver.recovery.reason().map(str::to_owned),
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
    fn restart(&mut self, now: u64, reason: &str) {
        self.revision = self.revision.wrapping_add(1);
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
        self.restore(now)
    }
    fn restore(&mut self, now: u64) -> Result<(), String> {
        if let Some(connection) = &self.connection {
            if let Err(error) = connection.stop() {
                self.recovery.restore_failed(&error);
                return Err(error);
            }
            self.connection.take();
            self.recovery.restored(now);
        }
        Ok(())
    }
    fn tick(&mut self, backend: &B, now: u64, wall_time: DateTime<Local>) {
        if matches!(self.recovery.phase(), Phase::Running { .. })
            && let Some(connection) = &self.connection
        {
            let frame =
                clock::frame_with_weather(&self.config.clock, self.weather.as_ref(), wall_time);
            match connection.update(&frame) {
                Ok(()) => self.recovery.healthy(now),
                Err(error) => self.recovery.fault(&error),
            }
        }
        if self.recovery.phase() == Phase::Restoring {
            let _ = self.restore(now);
        }
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
                        self.connection = Some(connection);
                        self.recovery.connected(now);
                    }
                    Err(failure) => {
                        self.connection = failure.pending;
                        if self.connection.is_some() {
                            self.recovery.connected(now);
                            self.recovery.fault(&failure.error);
                            let _ = self.restore(now);
                        } else {
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
    #[derive(Default)]
    struct NativeState {
        prepared: Cell<usize>,
        senders: RefCell<VecDeque<Sender<Result<(), String>>>>,
        frames: RefCell<Vec<Frame>>,
        connects: Cell<usize>,
        stops: Cell<usize>,
        fail_start: Cell<bool>,
        fail_update: Cell<bool>,
        fail_restore: Cell<bool>,
    }
    struct Fake(Rc<NativeState>);
    struct Lease(Rc<NativeState>);
    impl Backend for Fake {
        type Prepared = ();
        type Session = Lease;
        fn prepare(&self, _: &Config) -> Receiver<Result<(), String>> {
            let (tx, rx) = mpsc::channel();
            self.0.prepared.set(self.0.prepared.get() + 1);
            self.0.senders.borrow_mut().push_back(tx);
            rx
        }
        fn connect(&self, _: (), frame: &Frame) -> Result<Lease, ConnectFailure<Lease>> {
            self.0.connects.set(self.0.connects.get() + 1);
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
        native
            .senders
            .borrow_mut()
            .pop_front()
            .unwrap()
            .send(result)
            .unwrap();
    }
    fn connect(service: &Service<Fake>, native: &NativeState) {
        service.tick(0, time(0)).unwrap();
        complete(native, Ok(()));
        service.tick(1, time(0)).unwrap();
        assert_eq!(service.snapshot().status, Status::Running);
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
        service.tick(90000, time(90)).unwrap();
        assert_eq!(native.prepared.get(), 3);
        service.retry(90000).unwrap();
        service.tick(90000, time(90)).unwrap();
        assert_eq!(native.prepared.get(), 4);
    }
}
