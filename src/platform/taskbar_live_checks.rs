//! Explicit opt-in integration checks. The primary compatibility check is for
//! a disposable test system, and restores the selected clock before returning.
//! No visible host, focus changes or synthetic pointer/keyboard input is used.
use super::*;
use crate::config::MonitorScope;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

use litetray_adapter_protocol::{ElementInfo, InspectResponse, TaskbarInspection};

#[derive(Clone, serde::Serialize)]
struct ClockObservation {
    time: ElementInfo,
    date: ElementInfo,
    container: ElementInfo,
}
fn primary_bar(snapshot: &InspectResponse, hwnd: u64) -> Result<&TaskbarInspection, String> {
    if let Some(error) = &snapshot.error {
        return Err(error.clone());
    }
    let mut matches = snapshot
        .taskbars
        .iter()
        .filter(|bar| !bar.secondary && bar.hwnd == hwnd);
    let bar = matches.next().ok_or("主任务栏检查结果缺失")?;
    if matches.next().is_some() || bar.truncated {
        return Err("主任务栏检查结果重复或被截断".into());
    }
    if let Some(error) = &bar.error {
        return Err(error.clone());
    }
    if let Some(error) = &bar.geometry_error {
        return Err(format!("主任务栏 XAML source/几何检查失败：{error}"));
    }
    Ok(bar)
}
fn observe_clock(snapshot: &InspectResponse, hwnd: u64) -> Result<ClockObservation, String> {
    let bar = primary_bar(snapshot, hwnd)?;
    let topology = bar.topology.as_ref().ok_or("主任务栏缺少结构报告")?;
    if let Some(error) = &topology.error {
        return Err(error.clone());
    }
    let selection = topology
        .selection
        .as_ref()
        .ok_or("主任务栏结构未通过验证")?;
    let clock = selection.clock.as_ref().ok_or_else(|| {
        selection
            .clock_error
            .clone()
            .unwrap_or_else(|| "主屏原生时钟未找到".into())
    })?;
    let element = |index| {
        topology
            .elements
            .get(index)
            .cloned()
            .ok_or("时钟结构索引越界".to_string())
    };
    Ok(ClockObservation {
        time: element(clock.time)?,
        date: element(clock.date)?,
        container: element(clock.container)?,
    })
}
fn observed_text(element: &ElementInfo) -> &str {
    element.text.as_deref().unwrap_or("").trim_end_matches('\0')
}
fn close_dimension(a: Option<f64>, b: Option<f64>) -> bool {
    a.zip(b)
        .is_some_and(|(a, b)| a.is_finite() && b.is_finite() && (a - b).abs() <= 0.5)
}
fn require_active(observed: &ClockObservation, frame: &Frame) -> Result<(), String> {
    let expected_top = if observed.date.visible == Some(false) {
        format!("{} · {}", frame.text.top, frame.text.bottom)
    } else {
        frame.text.top.clone()
    };
    if observed.time.visible != Some(true)
        || observed_text(&observed.time) != expected_top
        || observed_text(&observed.date) != frame.text.bottom
        || !close_dimension(observed.container.width, Some(frame.style.width as f64))
    {
        return Err("主屏时钟的实际文字或固定宽度与测试帧不一致".into());
    }
    Ok(())
}
fn require_restored(before: &ClockObservation, after: &ClockObservation) -> Result<(), String> {
    for (original, current) in [
        (&before.time, &after.time),
        (&before.date, &after.date),
        (&before.container, &after.container),
    ] {
        if original.name != current.name
            || original.class_name != current.class_name
            || original.visible != current.visible
            || !close_dimension(original.height, current.height)
        {
            return Err("停止后时钟节点类型、可见性或布局高度未恢复".into());
        }
    }
    if observed_text(&before.time) != observed_text(&after.time)
        || observed_text(&before.date) != observed_text(&after.date)
    {
        return Err("停止后原生文字与接管前采样不同；若系统时间在检查期间变化，请重新运行，不能将此采样标为恢复通过".into());
    }
    if !close_dimension(before.container.width, after.container.width) {
        return Err("停止后原生时钟布局宽度未恢复".into());
    }
    Ok(())
}
fn inspect_primary(pid: u32, tid: u32, hwnd: u64) -> Result<InspectResponse, String> {
    let desktop = diagnostics::collect();
    if !desktop.taskbars.iter().any(|bar| {
        bar.class == "Shell_TrayWnd"
            && bar.hwnd as u64 == hwnd
            && bar.process_id == pid
            && bar.thread_id == tid
    }) {
        return Err("Explorer 主任务栏进程、线程或窗口在检查期间已变化".into());
    }
    let snapshot = crate::platform::adapter::inspect()?;
    if snapshot.process_id != pid || snapshot.thread_id != tid {
        return Err("Explorer 检查响应来自其他进程或线程".into());
    }
    primary_bar(&snapshot, hwnd)?;
    Ok(snapshot)
}
fn stop_test_connection(connection: &Option<Connection>) -> Result<(), String> {
    let Some(connection) = connection else {
        return Ok(());
    };
    let mut result = connection.stop();
    for _ in 0..2 {
        if result.is_ok() {
            break;
        }
        pump(Duration::from_millis(100));
        result = connection.stop();
    }
    result
}

/// Explicitly exercises the real primary Explorer clock. This is not part of
/// normal startup or `--self-test-clock`, and must run in a disposable guest.
/// A single monitor can validate tray preparation/ABI only, never replication.
pub fn check_compatibility(config: &Config) -> Value {
    let mut report = json!({
        "schema":1,"passed":false,"status":"running","scope":"primary_taskbar_live",
        "explorer_accessed":false,"writes_requested":false,"input_injected":false,
        "started_at":chrono::Utc::now().to_rfc3339(),"clock":{"passed":false},
        "tray":{"preparation_passed":false,"activated":false,"replication":"not_run"},
        "interaction":{"calendar_click":"not_run","volume_scroll":"not_run","tooltip_hover":"not_run"},
        "property_observation":"effective layout width/height and visibility; font/color/tooltip values and original bindings are not independently exported by this probe",
        "native_refresh_observation":"samples are taken after 1100 ms message-pump intervals; this probe does not independently count Shell refresh callbacks",
        "samples":[],"checks":[]
    });
    let mut connection = None;
    let mut baseline: Option<ClockObservation> = None;
    let mut identity = None;
    let outcome = (|| -> Result<(), String> {
        // Avoid an ordinary minute/date rollover during the short snapshot
        // comparison. Native seconds remain an explicit conservative failure.
        let settle = Instant::now();
        while chrono::Timelike::second(&chrono::Local::now()) >= 47 {
            if settle.elapsed() > Duration::from_secs(14) {
                return Err("等待稳定的原生时间采样窗口超时，未接管任务栏".into());
            }
            pump(Duration::from_millis(100));
        }
        let desktop = diagnostics::collect();
        let primaries: Vec<_> = desktop
            .taskbars
            .iter()
            .filter(|bar| bar.class == "Shell_TrayWnd")
            .collect();
        if primaries.len() != 1 {
            return Err("未找到唯一的原生主任务栏，未接管".into());
        }
        let primary = primaries[0];
        let (pid, tid, hwnd) = (primary.process_id, primary.thread_id, primary.hwnd as u64);
        identity = Some((pid, tid, hwnd));
        report["explorer_before"] = json!({"process_id":pid,"thread_id":tid,"taskbar":hwnd});
        report["monitor_count"] = json!(desktop.monitors.len());
        report["tray"]["replication"] = json!(if desktop.monitors.len() < 2 {
            "not_applicable_single_monitor"
        } else {
            "not_run_primary_only_check"
        });
        let mut selected = config.clone();
        selected.tray.scope = MonitorScope::Primary;
        selected.tray.selected_devices.clear();
        selected.tray.clock_enabled = true;
        selected.tray.enabled = false;
        selected.tray.show_icons = true;
        selected.tray.qq_popup_compatibility = false;
        selected.tray.show_control_center = false;
        selected.tray.copied_taskbars_only = true;
        selected.clock.click_action = ClickAction::CalendarWeather;
        selected.clock.volume_scroll = true;
        selected.clock.volume_mute = true;
        selected.clock.width = 240;
        selected.clock.compact_two_lines = false;
        let prepared = preflight(&selected)?;
        if prepared.routes.len() != 1
            || prepared.routes[0].secondary
            || prepared.routes[0].window != hwnd
        {
            return Err("测试路由未限定为原生主屏，未接管".into());
        }
        report["routes"] = json!(prepared.routes);
        report["explorer_accessed"] = json!(true);
        let before = inspect_primary(pid, tid, hwnd)?;
        let original = observe_clock(&before, hwnd)?;
        if observed_text(&original.time).is_empty()
            || observed_text(&original.time).contains("LT LIVE")
        {
            return Err("原生时间基线为空或已有测试内容，拒绝重复接管".into());
        }
        report["before"] = json!(before);
        report["clock"]["before"] = json!(original);
        baseline = Some(original);
        connection = Some(Connection::prepare(null_mut(), prepared)?);
        let active = connection.as_ref().expect("prepared");
        if active.status()? != SessionStatus::Prepared {
            return Err("时钟准备未确认 Prepared".into());
        }
        let mut frame = crate::clock::frame(&selected.clock, "", chrono::Local::now());
        frame.text.top = "LT LIVE 01".into();
        frame.text.bottom = "Sandbox check A".into();
        frame.text.tooltip = None;
        report["writes_requested"] = json!(true);
        active.start(frame.clone())?;
        report["checks"]
            .as_array_mut()
            .unwrap()
            .push(json!("clock Prepared -> Active acknowledged"));
        for step in 0..3 {
            if step == 1 {
                frame.text.top = "LT LIVE 02".into();
                frame.text.bottom = "Sandbox check B".into();
                frame.style.width = 320;
            } else if step == 2 {
                frame.text.top = "LT LIVE 03".into();
                frame.text.bottom = "Sandbox check C".into();
            }
            active.update(frame.clone())?;
            pump(Duration::from_millis(1100));
            let sample = inspect_primary(pid, tid, hwnd)?;
            let observed = observe_clock(&sample, hwnd)?;
            report["samples"]
                .as_array_mut()
                .unwrap()
                .push(json!({"frame":frame,"clock":observed,"inspection":sample}));
            require_active(&observed, &frame)?;
            if active.status()? != SessionStatus::Active {
                return Err("时钟在更新或系统刷新后失去 Active 状态".into());
            }
        }
        stop_test_connection(&connection)?;
        report["clock"]["stop_acknowledged"] = json!(true);
        pump(Duration::from_millis(300));
        let after = inspect_primary(pid, tid, hwnd)?;
        let observed = observe_clock(&after, hwnd)?;
        report["clock"]["after"] = json!(observed);
        report["after_clock"] = json!(after);
        require_restored(baseline.as_ref().expect("baseline"), &observed)?;
        if unsafe { WaitForSingleObject(active.process, 0) } != WAIT_TIMEOUT {
            return Err("Explorer 原进程已退出，不能用重启后的原生外观冒充恢复".into());
        }
        report["clock"]["passed"] = json!(true);
        report["checks"].as_array_mut().unwrap().push(json!("native text and 240 -> 320 width updates were observed after message-pump intervals; Stop restored original text, dimensions and visibility"));
        connection.take();

        // Primary-only + copied_taskbars_only produces no model-sharing target.
        // mirror_icons still validates and prepares the native tray runtime and
        // its five known surfaces. Keep it Prepared; never claim replication.
        selected.tray.enabled = true;
        let tray_preflight = preflight(&selected)?;
        if tray_preflight.routes.len() != 1
            || tray_preflight.routes[0].window != hwnd
            || tray_preflight.source.is_some()
            || !tray_preflight.mirror_icons
        {
            return Err("托盘检查范围或原生来源不符合主屏准备测试".into());
        }
        connection = Some(Connection::prepare(null_mut(), tray_preflight)?);
        let tray = connection.as_ref().expect("prepared tray check");
        if tray.status()? != SessionStatus::Prepared {
            return Err("托盘接口准备未确认 Prepared".into());
        }
        let prepared_snapshot = inspect_primary(pid, tid, hwnd)?;
        let bar = primary_bar(&prepared_snapshot, hwnd)?;
        let selection = bar
            .topology
            .as_ref()
            .and_then(|t| t.selection.as_ref())
            .ok_or("托盘结构报告缺失")?;
        let roles = [
            litetray_adapter_protocol::topology::Role::HiddenIcons,
            litetray_adapter_protocol::topology::Role::NotificationIcons,
            litetray_adapter_protocol::topology::Role::SystemIcons,
            litetray_adapter_protocol::topology::Role::PrivacyIndicators,
            litetray_adapter_protocol::topology::Role::ControlCenter,
        ];
        for role in roles {
            if !selection
                .surfaces
                .iter()
                .any(|surface| surface.role == role && surface.error.is_none())
            {
                return Err(format!("托盘准备缺少可验证的区域：{role:?}"));
            }
        }
        report["tray"]["surfaces"] = json!(selection.surfaces);
        report["tray"]["model_observations"] = json!(bar.geometry.iter().filter_map(|g| {
            g.model_identity.and_then(|identity|bar.elements.get(g.index).map(|node|json!({"name":node.name,"class_name":node.class_name,"model_identity":identity})))
        }).collect::<Vec<_>>());
        report["tray"]["inspection"] = json!(prepared_snapshot);
        report["tray"]["model_replication_validation"] =
            json!("not_run: no second XamlRoot or replica target in this primary-only check");
        stop_test_connection(&connection)?;
        report["tray"]["preparation_passed"] = json!(true);
        report["tray"]["stop_acknowledged"] = json!(true);
        report["checks"].as_array_mut().unwrap().push(json!("tray native entry points and five surfaces prepared; no replica or active tray runtime was created"));
        Ok(())
    })();
    let cleanup = stop_test_connection(&connection);
    report["cleanup_acknowledged"] = json!(cleanup.is_ok());
    if let Err(error) = &cleanup {
        report["cleanup_error"] = json!(error);
    }
    pump(Duration::from_millis(300));
    let inspected_identity = identity.filter(|_| report["explorer_accessed"] == true);
    let final_result = if let Some((pid, tid, hwnd)) = inspected_identity {
        inspect_primary(pid, tid, hwnd).and_then(|snapshot| {
            let observed = observe_clock(&snapshot, hwnd)?;
            report["after"] = json!(snapshot);
            report["explorer_after"] = json!({"process_id":pid,"thread_id":tid,"taskbar":hwnd});
            if let Some(original) = &baseline {
                require_restored(original, &observed)?;
            }
            if let Some(active) = &connection
                && unsafe { WaitForSingleObject(active.process, 0) } != WAIT_TIMEOUT
            {
                return Err("Explorer 原进程已退出".into());
            }
            Ok(())
        })
    } else {
        Ok(())
    };
    if let Err(error) = &final_result {
        report["final_inspection_error"] = json!(error);
    }
    report["explorer_identity_preserved"] =
        json!(inspected_identity.is_some() && final_result.is_ok());
    match outcome.and(cleanup).and(final_result) {
        Ok(()) => {
            report["passed"] = json!(true);
            report["status"] = json!("completed");
        }
        Err(error) => {
            report["error"] = json!(error);
            report["status"] = json!("failed");
        }
    }
    report["completed_at"] = json!(chrono::Utc::now().to_rfc3339());
    report
}

fn clocks() -> Result<Value, String> {
    let report = crate::platform::adapter::inspect()?;
    if let Some(error) = report.error {
        return Err(error);
    }
    Ok(json!(report.taskbars.iter().map(|bar| json!({
        "hwnd": bar.hwnd, "secondary": bar.secondary, "error": bar.error,
        "icons": bar.elements.iter().filter(|e| e.class_name == "SystemTray.NotifyIconView").map(|e| json!({"width":e.width,"visible":e.visible})).collect::<Vec<_>>(),
        "surfaces":bar.elements.iter().filter(|e| matches!(e.name.as_str(),"NotifyIconStack"|"NotificationAreaIcons"|"DateTimeStackPanel")).map(|e| json!({"name":e.name,"width":e.width,"visible":e.visible})).collect::<Vec<_>>(),
        "text": bar.elements.iter().filter(|e| matches!(e.name.as_str(), "TimeInnerTextBlock" | "DateInnerTextBlock"))
            .map(|e| json!({"name":e.name,"text":e.text.as_deref().unwrap_or("").trim_end_matches('\0'),"visible":e.visible})).collect::<Vec<_>>()
    })).collect::<Vec<_>>()))
}
fn pump(duration: Duration) {
    let until = Instant::now() + duration;
    while Instant::now() < until {
        unsafe {
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
pub fn check_secondary(config: &Config) -> Value {
    check(config, false)
}
pub fn check_secondary_tray(config: &Config) -> Value {
    check(config, true)
}
fn check(config: &Config, mirror: bool) -> Value {
    let mut report = json!({"passed":false,"scope":"secondary_only","checks":[],"samples":[]});
    let mut connection: Option<Connection> = None;
    let outcome = (|| -> Result<(), String> {
        let desktop = diagnostics::collect();
        let mut selected = config.clone();
        selected.tray.scope = MonitorScope::Selected;
        selected.tray.clock_enabled = true;
        selected.tray.enabled = mirror;
        selected.tray.show_icons = true;
        selected.tray.show_control_center = false;
        selected.tray.copied_taskbars_only = true;
        selected.clock.width = 0;
        selected.tray.selected_devices = desktop
            .monitors
            .iter()
            .filter(|m| !m.primary)
            .flat_map(|m| m.targets.iter().map(|t| t.id.clone()))
            .collect();
        if selected.tray.selected_devices.is_empty() {
            return Err("没有可明确识别的副屏，未修改任何任务栏".into());
        }
        // Calendar delegates are attached, but this check never generates input.
        selected.clock.click_action = ClickAction::CalendarWeather;
        let prepared = preflight(&selected)?;
        if prepared.routes.is_empty()
            || prepared.routes.iter().any(|r| {
                !r.secondary
                    || desktop
                        .monitors
                        .iter()
                        .any(|m| m.primary && m.device == r.device)
            })
        {
            return Err("副屏隔离检查失败，未修改任何任务栏".into());
        }
        report["routes"] = json!(prepared.routes);
        report["before"] = clocks()?;
        connection = Some(Connection::prepare(null_mut(), prepared)?);
        let active = connection.as_ref().expect("prepared connection");
        let mut frame = crate::clock::frame(&selected.clock, "副屏连接测试", chrono::Local::now());
        frame.text.top = "LT CHECK 01".into();
        frame.text.bottom = "LiteTray 副屏".into();
        active.start(frame.clone())?;
        report["checks"]
            .as_array_mut()
            .unwrap()
            .push(json!("Start acknowledged Active"));
        for tick in 1..=6 {
            frame.text.top = format!("LT CHECK {tick:02}");
            frame.text.bottom = if tick == 3 {
                "LiteTray 自适应宽度检查：这是一行更长的天气文字"
            } else {
                "LiteTray 副屏"
            }
            .into();
            active.update(frame.clone())?;
            pump(Duration::from_millis(1100));
            let observed = clocks()?;
            if mirror {
                let bars = observed.as_array().unwrap();
                let primary = bars
                    .iter()
                    .find(|b| b["secondary"] == false)
                    .ok_or("未找到主屏图标来源")?;
                let expected = primary["icons"].as_array().unwrap().len();
                for bar in bars.iter().filter(|b| b["secondary"] == true) {
                    if expected == 0
                        || bar["icons"].as_array().unwrap().len() != expected
                        || bar["surfaces"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .filter(|s| s["name"] == "NotificationAreaIcons")
                            .any(|s| s["width"].as_f64().unwrap_or(0.0) <= 0.0)
                    {
                        report["samples"].as_array_mut().unwrap().push(observed);
                        return Err("副屏未生成与主屏一致的原生应用托盘图标".into());
                    }
                }
            }
            for route in &active.context.request.routes {
                let bar = observed
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|b| b["hwnd"].as_u64() == Some(route.window))
                    .ok_or("副屏任务栏已变化")?;
                if !bar["text"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|t| t["name"] == "TimeInnerTextBlock" && t["text"] == frame.text.top)
                {
                    report["samples"].as_array_mut().unwrap().push(observed);
                    return Err("副屏原生 TextBlock 未保持指定时间内容".into());
                }
            }
            if observed
                .as_array()
                .unwrap()
                .iter()
                .filter(|b| b["secondary"] == false)
                .any(|b| b["text"].to_string().contains("LT CHECK"))
            {
                return Err("测试内容错误地出现在主屏".into());
            }
            report["samples"].as_array_mut().unwrap().push(observed);
        }
        let width = |sample: usize| -> Result<f64, String> {
            report["samples"][sample]
                .as_array()
                .ok_or("测试采样缺失")?
                .iter()
                .find(|b| b["secondary"] == true)
                .and_then(|b| b["surfaces"].as_array())
                .and_then(|surfaces| surfaces.iter().find(|s| s["name"] == "DateTimeStackPanel"))
                .and_then(|s| s["width"].as_f64())
                .ok_or("原生时钟布局尺寸缺失".into())
        };
        if width(2)? <= width(0)? + 30.0 || (width(5)? - width(0)?).abs() > 2.0 {
            return Err("原生时钟未随内容自动扩展并收缩".into());
        }
        report["checks"].as_array_mut().unwrap().push(json!(
            "Native Auto width grows for longer content and shrinks again"
        ));
        report["checks"].as_array_mut().unwrap().push(json!(
            "6 native secondary-clock updates survived system refresh; primary excluded"
        ));
        if mirror {
            report["checks"].as_array_mut().unwrap().push(json!(
                "Native application icons present on secondary with same count as primary"
            ));
            active.stop()?;
            connection.take();
            selected.tray.clock_enabled = false;
            connection = Some(Connection::prepare(null_mut(), preflight(&selected)?)?);
            let tray = connection.as_ref().expect("prepared tray-only connection");
            tray.start(frame.clone())?;
            tray.update(frame)?;
            pump(Duration::from_millis(1100));
            let observed = clocks()?;
            let bars = observed.as_array().ok_or("独立托盘采样缺失")?;
            let source = bars
                .iter()
                .find(|b| b["secondary"] == false)
                .ok_or("独立托盘来源缺失")?;
            if bars.iter().filter(|b| b["secondary"] == true).any(|b| {
                b["icons"].as_array().map(Vec::len) != source["icons"].as_array().map(Vec::len)
                    || b["text"].to_string().contains("LT CHECK")
            }) {
                report["tray_only"] = observed;
                return Err("独立托盘复制或系统时钟保留失败".into());
            }
            report["tray_only"] = observed;
            report["checks"].as_array_mut().unwrap().push(json!(
                "StartTray preserves native system clocks and copies application icons"
            ));
        }
        Ok(())
    })();
    // Cleanup runs even after failed Start/update/inspection.
    let restored = connection.as_ref().map(|c| c.stop()).unwrap_or(Ok(()));
    report["restored"] = json!(restored.is_ok());
    if let Err(error) = &restored {
        report["restore_error"] = json!(error);
    }
    pump(Duration::from_millis(200));
    match clocks() {
        Ok(after) => {
            report["after"] = after;
        }
        Err(error) => {
            report["after_error"] = json!(error);
        }
    }
    match outcome.and(restored) {
        Ok(()) => {
            let lingering = report["after"].to_string().contains("LT CHECK");
            report["passed"] = json!(!lingering && report["after_error"].is_null());
            if mirror {
                let before = report["before"].as_array();
                let after = report["after"].as_array();
                let restored_icons = before.zip(after).is_some_and(|(before, after)| {
                    before.iter().all(|b| {
                        after
                            .iter()
                            .find(|a| a["hwnd"] == b["hwnd"])
                            .is_some_and(|a| a["icons"] == b["icons"])
                    })
                });
                if !restored_icons {
                    report["passed"] = json!(false);
                    report["error"] = json!("停止后托盘图标未恢复原状");
                } else {
                    report["checks"].as_array_mut().unwrap().push(json!(
                        "Both taskbars restored their original application icon counts"
                    ));
                }
            }
            if lingering {
                report["error"] = json!("停止后测试文字仍存在");
            } else {
                report["checks"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("Stop acknowledged Closed; original text restored"));
            }
        }
        Err(error) => report["error"] = json!(error),
    }
    report
}

#[cfg(test)]
mod compatibility_report_tests {
    use super::*;
    fn node(name: &str, text: Option<&str>, width: f64) -> ElementInfo {
        ElementInfo {
            name: name.into(),
            class_name: "fixture".into(),
            depth: 0,
            width: Some(width),
            height: Some(16.),
            text: text.map(str::to_owned),
            visible: Some(true),
        }
    }
    fn native() -> ClockObservation {
        ClockObservation {
            time: node("TimeInnerTextBlock", Some("12:34"), 80.),
            date: node("DateInnerTextBlock", Some("2026/11/08"), 80.),
            container: node("DateTimeStackPanel", None, 80.),
        }
    }
    #[test]
    fn restored_report_rejects_lingering_text_or_layout_changes() {
        let before = native();
        assert!(require_restored(&before, &before).is_ok());
        let mut after = before.clone();
        after.time.text = Some("LT LIVE 03".into());
        assert!(require_restored(&before, &after).is_err());
        after = before.clone();
        after.container.width = Some(320.);
        assert!(require_restored(&before, &after).is_err());
        after = before.clone();
        after.date.visible = Some(false);
        assert!(require_restored(&before, &after).is_err());
        after = before.clone();
        after.time.text = Some("12:35".into());
        assert!(
            require_restored(&before, &after).is_err(),
            "changing native time must not manufacture an exact-restore proof"
        );
    }
    #[test]
    fn active_report_requires_actual_text_width_and_compact_display() {
        let mut frame = crate::clock::frame(&Config::default().clock, "", chrono::Local::now());
        frame.text.top = "LT LIVE 02".into();
        frame.text.bottom = "Sandbox check B".into();
        frame.style.width = 320;
        let mut observed = native();
        assert!(require_active(&observed, &frame).is_err());
        observed.time.text = Some(frame.text.top.clone());
        observed.date.text = Some(frame.text.bottom.clone());
        observed.container.width = Some(320.);
        assert!(require_active(&observed, &frame).is_ok());
        observed.date.visible = Some(false);
        assert!(require_active(&observed, &frame).is_err());
        observed.time.text = Some("LT LIVE 02 · Sandbox check B".into());
        assert!(require_active(&observed, &frame).is_ok());
    }
}
