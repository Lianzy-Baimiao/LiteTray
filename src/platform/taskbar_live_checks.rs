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
