//! Bridge persistent monitor preferences to a transient native target plan.
use crate::{config::TrayConfig, monitors};
use litetray_adapter_protocol::routing::{Options, Plan, Route};
pub fn plan(
    config: &TrayConfig,
    selection: &monitors::Selection,
    routes: &[Route],
) -> Result<Plan, String> {
    Plan::build(
        routes,
        &selection.devices,
        Options {
            clocks: config.clock_enabled,
            tray: config.enabled,
            icons: config.show_icons,
            control_center: config.show_control_center,
            copied_taskbars_only: config.copied_taskbars_only,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::MonitorScope,
        monitors::{Display, Snapshot, Target},
    };
    #[test]
    fn saved_physical_target_resolves_to_only_its_current_taskbar() {
        let config = TrayConfig {
            clock_enabled: true,
            enabled: true,
            scope: MonitorScope::Selected,
            selected_devices: vec![r"\\?\DISPLAY#B#instance".into()],
            ..TrayConfig::default()
        };
        let snapshot = Snapshot {
            displays: [("DISPLAY4", "A", true), ("DISPLAY9", "B", false)]
                .into_iter()
                .map(|(device, id, primary)| Display {
                    device: device.into(),
                    primary,
                    rect: [0, 0, 1, 1],
                    work: [0, 0, 1, 1],
                    targets: vec![Target {
                        id: format!(r"\\?\DISPLAY#{id}#instance"),
                        name: id.into(),
                    }],
                })
                .collect(),
            warnings: vec![],
        };
        let routes = [(1, false, "DISPLAY4"), (2, true, "DISPLAY9")]
            .into_iter()
            .map(|(window, secondary, device)| Route {
                window,
                secondary,
                device: device.into(),
                process_id: 3,
                thread_id: 4,
            })
            .collect::<Vec<_>>();
        let selection = monitors::resolve(&config, &snapshot);
        let plan = plan(&config, &selection, &routes).unwrap();
        assert_eq!(plan.source.unwrap().window, 1);
        assert_eq!(plan.targets.len(), 1);
        assert_eq!(plan.targets[0].route.window, 2);
    }
}
