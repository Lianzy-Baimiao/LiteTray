//! Persistent monitor preferences resolved against a fresh desktop snapshot.
//! GDI names and handles are runtime routing data, never saved identities.
use crate::config::{MonitorScope, TrayConfig};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Display {
    pub device: String,
    pub primary: bool,
    pub rect: [i32; 4],
    pub work: [i32; 4],
    pub targets: Vec<Target>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub displays: Vec<Display>,
    pub warnings: Vec<String>,
}
/// Calendar placement depends on the desktop/work areas and their physical
/// targets, not display enumeration order or a refreshed friendly name. This
/// comparison is only for UI cache invalidation, never taskbar ABI validation.
pub fn same_panel_layout(previous: &Snapshot, current: &Snapshot) -> bool {
    if previous.displays.is_empty() || current.displays.is_empty() {
        return false;
    }
    let layout = |snapshot: &Snapshot| {
        let mut areas = snapshot
            .displays
            .iter()
            .map(|display| {
                let mut targets = display
                    .targets
                    .iter()
                    .map(|target| target.id.to_ascii_lowercase())
                    .collect::<Vec<_>>();
                targets.sort();
                (
                    display.device.to_ascii_lowercase(),
                    display.primary,
                    display.rect,
                    display.work,
                    targets,
                )
            })
            .collect::<Vec<_>>();
        areas.sort();
        areas
    };
    layout(previous) == layout(current)
}
#[derive(Debug, Clone, Serialize)]
pub struct Selection {
    /// GDI source names to resolve to current taskbars, never persisted.
    pub devices: Vec<String>,
    pub pending: Vec<String>,
    pub warnings: Vec<String>,
}
pub fn stable_id(id: &str) -> bool {
    id.len() <= 1024
        && id.to_ascii_lowercase().starts_with(r"\\?\display#")
        && id.split('#').count() >= 3
        && !id.chars().any(char::is_control)
}
pub fn same_id(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}
pub fn validate_preferences(ids: &[String]) -> Result<(), String> {
    if ids.len() > 64 {
        return Err("选择的显示器数量超过 64 项".into());
    }
    for (i, id) in ids.iter().enumerate() {
        if id.trim().is_empty() || id.len() > 1024 || id.chars().any(char::is_control) {
            return Err("显示器标识为空、过长或包含控制字符".into());
        }
        if ids[..i].iter().any(|old| same_id(old, id)) {
            return Err("显示器选择不能重复".into());
        }
    }
    Ok(())
}
pub fn resolve(config: &TrayConfig, snapshot: &Snapshot) -> Selection {
    let mut selection = Selection {
        devices: Vec::new(),
        pending: Vec::new(),
        warnings: snapshot.warnings.clone(),
    };
    if let Err(error) = validate_preferences(&config.selected_devices) {
        selection.warnings.push(error);
        return selection;
    }
    match config.scope {
        MonitorScope::All => {
            selection.devices = snapshot.displays.iter().map(|m| m.device.clone()).collect()
        }
        MonitorScope::Primary => {
            let primary = snapshot
                .displays
                .iter()
                .filter(|m| m.primary)
                .collect::<Vec<_>>();
            if primary.len() == 1 {
                selection.devices.push(primary[0].device.clone());
            } else {
                selection.warnings.push("当前未能确定唯一的主显示器".into());
            }
        }
        MonitorScope::Selected => {
            for id in &config.selected_devices {
                let count = snapshot
                    .displays
                    .iter()
                    .filter(|m| m.targets.iter().any(|t| same_id(&t.id, id)))
                    .count();
                if !stable_id(id) || count != 1 {
                    selection.pending.push(id.clone());
                    if !stable_id(id) {
                        selection
                            .warnings
                            .push(format!("旧显示器编号需要重新选择：{id}"));
                    } else if count > 1 {
                        selection
                            .warnings
                            .push("同一设备标识对应多个桌面区域，暂不选择该显示器".into());
                    }
                }
            }
            for display in &snapshot.displays {
                let selected = display
                    .targets
                    .iter()
                    .filter(|target| {
                        config
                            .selected_devices
                            .iter()
                            .any(|id| same_id(id, &target.id))
                            && !selection.pending.iter().any(|id| same_id(id, &target.id))
                    })
                    .count();
                if selected == 0 {
                    continue;
                }
                if selected != display.targets.len() {
                    selection.warnings.push(format!(
                        "复制显示共用一个任务栏，请同时选择同组屏幕：{}",
                        display
                            .targets
                            .iter()
                            .map(|t| t.name.as_str())
                            .collect::<Vec<_>>()
                            .join(" / ")
                    ));
                    continue;
                }
                selection.devices.push(display.device.clone());
            }
        }
    }
    selection.devices.sort_by_key(|s| s.to_ascii_lowercase());
    selection.devices.dedup_by(|a, b| same_id(a, b));
    selection
}
#[derive(Clone)]
pub struct Choice {
    pub id: Option<String>,
    pub label: String,
    pub selected: bool,
}
pub fn choices(snapshot: &Snapshot, saved: &[String]) -> Vec<Choice> {
    let mut rows = Vec::new();
    for display in &snapshot.displays {
        let width = i64::from(display.rect[2]) - i64::from(display.rect[0]);
        let height = i64::from(display.rect[3]) - i64::from(display.rect[1]);
        let device = display
            .device
            .strip_prefix(r"\\.\")
            .unwrap_or(&display.device);
        let suffix = format!(
            "{device}{} · {width} × {height}{}",
            if display.primary { " · 主屏" } else { "" },
            if display.targets.len() > 1 {
                " · 复制显示组"
            } else {
                ""
            }
        );
        if display.targets.is_empty() {
            rows.push(Choice {
                id: None,
                label: format!("{suffix} · 无法取得稳定标识，请刷新"),
                selected: false,
            });
        }
        for target in &display.targets {
            if rows
                .iter()
                .any(|r| r.id.as_ref().is_some_and(|id| same_id(id, &target.id)))
            {
                continue;
            }
            rows.push(Choice {
                id: Some(target.id.clone()),
                label: format!("{} · {suffix}", target.name),
                selected: saved.iter().any(|id| same_id(id, &target.id)),
            });
        }
    }
    for id in saved {
        if rows
            .iter()
            .any(|r| r.id.as_ref().is_some_and(|old| same_id(old, id)))
        {
            continue;
        }
        let label = if stable_id(id) {
            format!(
                "未连接的已保存显示器 · {}",
                id.split('#').nth(1).unwrap_or("未知设备")
            )
        } else {
            format!("旧编号 {id} · 请重新勾选当前屏幕")
        };
        rows.push(Choice {
            id: Some(id.clone()),
            label,
            selected: true,
        });
    }
    rows
}
/// Preserve IDs exactly when a page is merely opened or refreshed; no silent
/// migration from a legacy number and no loss of disconnected selections.
pub fn collect_choices(
    rows: &[Choice],
    selected: &[bool],
    previous: &[String],
) -> Result<Vec<String>, String> {
    if rows.len() != selected.len() {
        return Err("显示器列表已变化，请刷新后重试".into());
    }
    let mut ids = Vec::new();
    for (row, selected) in rows.iter().zip(selected) {
        if !selected {
            continue;
        }
        let id = row
            .id
            .as_ref()
            .ok_or("此屏幕暂时没有稳定标识，请刷新显示器列表")?;
        if !ids.iter().any(|old: &String| same_id(old, id)) {
            ids.push(id.clone());
        }
    }
    validate_preferences(&ids)?;
    let mut ordered = previous
        .iter()
        .filter(|old| ids.iter().any(|id| same_id(old, id)))
        .cloned()
        .collect::<Vec<_>>();
    for id in ids {
        if !ordered.iter().any(|old| same_id(old, &id)) {
            ordered.push(id);
        }
    }
    Ok(ordered)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(name: &str) -> String {
        format!(r"\\?\DISPLAY#{name}#instance#class")
    }
    fn display(device: &str, primary: bool, names: &[&str]) -> Display {
        Display {
            device: device.into(),
            primary,
            rect: [-1920, 0, 0, 1080],
            work: [-1920, 0, 0, 1040],
            targets: names
                .iter()
                .map(|n| Target {
                    id: id(n),
                    name: n.to_string(),
                })
                .collect(),
        }
    }
    fn config(ids: &[&str]) -> TrayConfig {
        TrayConfig {
            scope: MonitorScope::Selected,
            selected_devices: ids.iter().map(|s| id(s)).collect(),
            ..TrayConfig::default()
        }
    }
    #[test]
    fn stable_identity_survives_source_renumbering_and_primary_change() {
        let snapshot = Snapshot {
            displays: vec![
                display("DISPLAY9", false, &["A"]),
                display("DISPLAY1", true, &["B"]),
            ],
            warnings: vec![],
        };
        assert_eq!(resolve(&config(&["A"]), &snapshot).devices, ["DISPLAY9"]);
        let mut c = config(&["A"]);
        c.scope = MonitorScope::Primary;
        assert_eq!(resolve(&c, &snapshot).devices, ["DISPLAY1"]);
        c.scope = MonitorScope::All;
        assert_eq!(resolve(&c, &snapshot).devices.len(), 2);
        let previous = vec![id("b").to_ascii_uppercase(), id("A")];
        let rows = choices(&snapshot, &previous);
        assert_eq!(
            collect_choices(
                &rows,
                &rows.iter().map(|r| r.selected).collect::<Vec<_>>(),
                &previous
            )
            .unwrap(),
            previous
        );
    }
    #[test]
    fn legacy_and_disconnected_preferences_are_preserved_without_guessing() {
        let snapshot = Snapshot {
            displays: vec![display(r"\\.\DISPLAY1", true, &["B"])],
            warnings: vec![],
        };
        let mut c = config(&["A"]);
        c.selected_devices.push(r"\\.\DISPLAY1".into());
        let result = resolve(&c, &snapshot);
        assert!(result.devices.is_empty());
        assert_eq!(result.pending.len(), 2);
        let rows = choices(&snapshot, &c.selected_devices);
        assert_eq!(
            collect_choices(
                &rows,
                &rows.iter().map(|r| r.selected).collect::<Vec<_>>(),
                &c.selected_devices
            )
            .unwrap(),
            c.selected_devices
        );
    }
    #[test]
    fn cloned_screens_require_the_whole_shared_desktop_surface() {
        let snapshot = Snapshot {
            displays: vec![display("DISPLAY1", true, &["A", "B"])],
            warnings: vec![],
        };
        assert!(resolve(&config(&["A"]), &snapshot).devices.is_empty());
        assert_eq!(
            resolve(&config(&["A", "B"]), &snapshot).devices,
            ["DISPLAY1"]
        );
    }
    #[test]
    fn ambiguous_identity_is_not_assigned_to_either_screen() {
        let snapshot = Snapshot {
            displays: vec![
                display("DISPLAY1", true, &["A"]),
                display("DISPLAY2", false, &["A"]),
            ],
            warnings: vec![],
        };
        assert!(resolve(&config(&["A"]), &snapshot).devices.is_empty());
    }
    #[test]
    fn selection_rejects_duplicate_invalid_and_unidentified_rows() {
        assert!(validate_preferences(&[id("A"), id("a")]).is_err());
        assert!(validate_preferences(&["x\0y".into()]).is_err());
        let rows = choices(
            &Snapshot {
                displays: vec![display("DISPLAY1", true, &[])],
                warnings: vec![],
            },
            &[],
        );
        assert!(collect_choices(&rows, &[true], &[]).is_err());
        assert!(collect_choices(&rows, &[], &[]).is_err());
    }
}
