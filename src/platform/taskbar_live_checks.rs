//! Explicit opt-in integration check. No visible host, focus changes or input
//! injection; selected secondary taskbars are restored before returning.
use super::*;
use crate::config::MonitorScope;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn clocks() -> Result<Value, String> {
    let report = crate::platform::adapter::inspect()?;
    if let Some(error) = report.error {
        return Err(error);
    }
    Ok(json!(report.taskbars.iter().map(|bar| json!({
        "hwnd": bar.hwnd, "secondary": bar.secondary, "error": bar.error,
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
    let mut report = json!({"passed":false,"scope":"secondary_only","checks":[],"samples":[]});
    let mut connection: Option<Connection> = None;
    let outcome = (|| -> Result<(), String> {
        let desktop = diagnostics::collect();
        let mut selected = config.clone();
        selected.tray.scope = MonitorScope::Selected;
        selected.tray.clock_enabled = true;
        selected.tray.enabled = false;
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
            active.update(frame.clone())?;
            pump(Duration::from_millis(1100));
            let observed = clocks()?;
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
        report["checks"].as_array_mut().unwrap().push(json!(
            "6 native secondary-clock updates survived system refresh; primary excluded"
        ));
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
