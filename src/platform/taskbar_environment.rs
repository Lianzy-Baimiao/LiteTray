//! Stable routing identity for filtering generic device notifications. Work
//! areas, tray content and enumeration order can change without a new session.
use super::diagnostics::Diagnostics;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Display {
    device: String,
    primary: bool,
    bounds: [i32; 4],
    targets: Vec<String>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Taskbar {
    class: String,
    window: usize,
    process: u32,
    thread: u32,
    device: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Environment {
    displays: Vec<Display>,
    taskbars: Vec<Taskbar>,
}

impl Environment {
    pub(super) fn from_diagnostics(snapshot: &Diagnostics) -> Option<Self> {
        // Failed or incomplete discovery is never proof that routing stayed
        // unchanged. Existing restore/reconnect validation handles this case.
        if !snapshot.monitor_warnings.is_empty()
            || snapshot.monitors.is_empty()
            || snapshot.taskbars.is_empty()
        {
            return None;
        }
        let mut displays = snapshot
            .monitors
            .iter()
            .map(|monitor| {
                let mut targets = monitor
                    .targets
                    .iter()
                    .map(|target| target.id.to_ascii_lowercase())
                    .collect::<Vec<_>>();
                targets.sort();
                Display {
                    device: monitor.device.to_ascii_lowercase(),
                    primary: monitor.primary,
                    bounds: monitor.rect,
                    targets,
                }
            })
            .collect::<Vec<_>>();
        let mut taskbars = snapshot
            .taskbars
            .iter()
            .map(|bar| {
                if bar.hwnd == 0 || bar.process_id == 0 || bar.thread_id == 0 {
                    return None;
                }
                Some(Taskbar {
                    class: bar.class.clone(),
                    window: bar.hwnd,
                    process: bar.process_id,
                    thread: bar.thread_id,
                    device: bar.display_device.as_ref()?.to_ascii_lowercase(),
                })
            })
            .collect::<Option<Vec<_>>>()?;
        displays.sort();
        taskbars.sort();
        Some(Self { displays, taskbars })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{monitors, platform::diagnostics};

    fn desktop() -> Diagnostics {
        Diagnostics {
            build: 26300,
            family: String::new(),
            adapter_status: String::new(),
            monitor_warnings: vec![],
            monitor_selection: None,
            target_plan: None,
            target_plan_error: None,
            monitors: (1..=2)
                .map(|i| monitors::Display {
                    device: format!(r"\\.\DISPLAY{i}"),
                    primary: i == 1,
                    rect: [(i - 1) * 1920, 0, i * 1920, 1080],
                    work: [(i - 1) * 1920, 0, i * 1920, 1040],
                    targets: vec![monitors::Target {
                        id: format!(r"\\?\DISPLAY#screen{i}#instance"),
                        name: format!("Screen {i}"),
                    }],
                })
                .collect(),
            taskbars: (1..=2)
                .map(|i| diagnostics::Taskbar {
                    class: if i == 1 {
                        "Shell_TrayWnd"
                    } else {
                        "Shell_SecondaryTrayWnd"
                    }
                    .into(),
                    hwnd: i,
                    process_id: 100,
                    thread_id: 200,
                    rect: [0, 1040, 1920, 1080],
                    display_device: Some(format!(r"\\.\DISPLAY{i}")),
                })
                .collect(),
        }
    }

    #[test]
    fn notification_noise_and_enumeration_order_do_not_change_routing() {
        let mut observed = desktop();
        observed.taskbars.reverse();
        observed.monitors.reverse();
        observed.monitors[0].targets[0].name = "Updated friendly name".into();
        observed.monitors[0].work[3] -= 10;
        observed.taskbars[0].rect[1] += 5; // Auto-hide animation.
        observed.monitors[0].device.make_ascii_lowercase();
        assert_eq!(
            Environment::from_diagnostics(&desktop()),
            Environment::from_diagnostics(&observed)
        );
    }

    #[test]
    fn hotplug_window_recreation_and_monitor_reassignment_require_a_new_session() {
        let original = Environment::from_diagnostics(&desktop()).unwrap();
        for change in 0..8 {
            let mut observed = desktop();
            match change {
                0 => {
                    observed.monitors.pop();
                }
                1 => observed.taskbars[0].hwnd += 100,
                2 => observed.taskbars[0].process_id += 1,
                3 => observed.taskbars[0].thread_id += 1,
                4 => observed.taskbars[0].display_device = Some("DISPLAY3".into()),
                5 => observed.monitors[0].targets[0].id.push('2'),
                6 => observed.monitors[0].primary = false,
                _ => observed.monitors[0].rect[2] += 100,
            }
            assert_ne!(
                Environment::from_diagnostics(&observed).as_ref(),
                Some(&original),
                "change {change}"
            );
        }
    }

    #[test]
    fn incomplete_discovery_cannot_suppress_recovery() {
        let mut observed = desktop();
        observed.monitor_warnings.push("hotplug in progress".into());
        assert!(Environment::from_diagnostics(&observed).is_none());
        observed.monitor_warnings.clear();
        observed.taskbars[0].display_device = None;
        assert!(Environment::from_diagnostics(&observed).is_none());
    }
}
