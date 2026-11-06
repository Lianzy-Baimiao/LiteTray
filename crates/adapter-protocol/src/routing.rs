//! Resolve an already selected set of desktop devices to explicit taskbars.
//! Handles are a single-session snapshot, never persistent monitor identities.
use crate::{clock::MAX_CLOCKS, topology::Role};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub window: u64,
    pub process_id: u32,
    pub thread_id: u32,
    pub secondary: bool,
    pub device: String,
}
#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub clocks: bool,
    pub tray: bool,
    pub icons: bool,
    pub control_center: bool,
    pub copied_taskbars_only: bool,
}
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Visibility {
    pub role: Role,
    pub visible: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct Target {
    pub route: Route,
    pub clock: bool,
    pub models: Vec<Role>,
    pub visibility: Vec<Visibility>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub source: Option<Route>,
    pub targets: Vec<Target>,
}
const ICONS: [Role; 4] = [
    Role::HiddenIcons,
    Role::NotificationIcons,
    Role::SystemIcons,
    Role::PrivacyIndicators,
];
fn device_valid(device: &str) -> bool {
    !device.trim().is_empty() && device.len() <= 128 && !device.chars().any(char::is_control)
}
impl Plan {
    pub fn build(routes: &[Route], selected: &[String], options: Options) -> Result<Self, String> {
        let mut plan = Self {
            source: None,
            targets: vec![],
        };
        if !options.clocks && !options.tray || selected.is_empty() {
            return Ok(plan);
        }
        if routes.len() > 64 || selected.len() > MAX_CLOCKS {
            return Err("任务栏或选定桌面区域数量超限".into());
        }
        for (i, route) in routes.iter().enumerate() {
            if route.window == 0
                || route.process_id == 0
                || route.thread_id == 0
                || !device_valid(&route.device)
                || routes[..i].iter().any(|r| {
                    r.window == route.window || r.device.eq_ignore_ascii_case(&route.device)
                })
            {
                return Err("任务栏窗口或显示器映射不唯一、无效".into());
            }
        }
        for (i, device) in selected.iter().enumerate() {
            if !device_valid(device) || selected[..i].iter().any(|d| d.eq_ignore_ascii_case(device))
            {
                return Err("选定桌面区域无效或重复".into());
            }
            let route = routes
                .iter()
                .find(|r| r.device.eq_ignore_ascii_case(device))
                .ok_or_else(|| format!("选定屏幕尚无任务栏：{device}"))?;
            let mut target = Target {
                route: route.clone(),
                clock: options.clocks,
                models: vec![],
                visibility: vec![],
            };
            if options.tray && (route.secondary || !options.copied_taskbars_only) {
                for role in ICONS.into_iter().chain([Role::ControlCenter]) {
                    let visible = if role == Role::ControlCenter {
                        options.control_center
                    } else {
                        options.icons
                    };
                    if route.secondary {
                        if visible {
                            target.models.push(role);
                        }
                        target.visibility.push(Visibility { role, visible });
                    } else if !visible {
                        // Preserve the primary shell's natural visibility for
                        // enabled regions, including empty/privacy indicators.
                        target.visibility.push(Visibility {
                            role,
                            visible: false,
                        });
                    }
                }
            }
            if target.clock || !target.models.is_empty() || !target.visibility.is_empty() {
                plan.targets.push(target);
            }
        }
        if plan.targets.iter().any(|t| !t.models.is_empty()) {
            let primary = routes.iter().filter(|r| !r.secondary).collect::<Vec<_>>();
            if primary.len() != 1 {
                return Err("无法确定唯一的原生托盘来源任务栏".into());
            }
            plan.source = Some(primary[0].clone());
        }
        // COM model sharing is restricted to a single owning UI thread.
        // A future multi-thread adapter must establish separate sessions.
        if let Some(first) = plan.targets.first() {
            for route in plan
                .targets
                .iter()
                .map(|t| &t.route)
                .chain(plan.source.iter())
            {
                if route.process_id != first.route.process_id
                    || route.thread_id != first.route.thread_id
                {
                    return Err("选定任务栏不属于同一个原生 UI 线程，需要独立会话适配".into());
                }
            }
        }
        plan.targets.sort_by_key(|t| t.route.window);
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn route(window: u64, secondary: bool, device: &str) -> Route {
        Route {
            window,
            process_id: 10,
            thread_id: 20,
            secondary,
            device: device.into(),
        }
    }
    fn options() -> Options {
        Options {
            clocks: true,
            tray: true,
            icons: true,
            control_center: true,
            copied_taskbars_only: false,
        }
    }
    #[test]
    fn secondary_selection_uses_explicit_primary_without_modifying_its_clock() {
        let routes = [route(2, true, "DISPLAY9"), route(1, false, "DISPLAY4")];
        let plan = Plan::build(&routes, &["display9".into()], options()).unwrap();
        assert_eq!(plan.source.unwrap().window, 1);
        assert_eq!(plan.targets.len(), 1);
        assert_eq!(plan.targets[0].route.window, 2);
        assert!(plan.targets[0].clock);
        assert_eq!(plan.targets[0].models.len(), 5);
    }
    #[test]
    fn independent_clock_and_tray_modes_keep_scope_and_primary_rules() {
        let routes = [route(1, false, "A"), route(2, true, "B")];
        let mut o = options();
        o.tray = false;
        let p = Plan::build(&routes, &["A".into()], o).unwrap();
        assert!(
            p.source.is_none()
                && p.targets[0].visibility.is_empty()
                && p.targets[0].models.is_empty()
        );
        o.tray = true;
        o.clocks = false;
        o.icons = false;
        o.copied_taskbars_only = true;
        let p = Plan::build(&routes, &["A".into(), "B".into()], o).unwrap();
        assert_eq!(p.targets.len(), 1);
        assert!(!p.targets[0].clock);
        assert_eq!(p.targets[0].models, [Role::ControlCenter]);
        assert_eq!(
            p.targets[0]
                .visibility
                .iter()
                .filter(|v| !v.visible)
                .count(),
            4
        );
        o.copied_taskbars_only = false;
        let p = Plan::build(&routes, &["A".into()], o).unwrap();
        assert!(p.source.is_none());
        assert_eq!(p.targets[0].visibility.len(), 4);
        o.icons = true;
        assert!(
            Plan::build(&routes, &["A".into()], o)
                .unwrap()
                .targets
                .is_empty()
        );
    }
    #[test]
    fn ambiguous_missing_and_cross_thread_routes_fail_before_capture() {
        let primary = route(1, false, "A");
        let secondary = route(2, true, "B");
        assert!(Plan::build(std::slice::from_ref(&primary), &["B".into()], options()).is_err());
        assert!(
            Plan::build(
                &[primary.clone(), route(2, true, "a")],
                &["A".into()],
                options()
            )
            .is_err()
        );
        assert!(Plan::build(std::slice::from_ref(&secondary), &["B".into()], options()).is_err());
        assert!(
            Plan::build(
                &[primary.clone(), route(3, false, "C"), secondary.clone()],
                &["B".into()],
                options()
            )
            .is_err()
        );
        let mut other = secondary;
        other.thread_id += 1;
        assert!(Plan::build(&[primary, other], &["B".into()], options()).is_err());
    }
    #[test]
    fn no_selected_monitor_and_disabled_features_produce_no_work() {
        assert!(Plan::build(&[], &[], options()).unwrap().targets.is_empty());
        let mut o = options();
        o.clocks = false;
        o.tray = false;
        assert!(
            Plan::build(&[], &["missing".into()], o)
                .unwrap()
                .targets
                .is_empty()
        );
    }
}
