//! Structural tray selection from a bounded, depth-first native panel snapshot.
//! Names alone are insufficient: roles must be direct children of the panel,
//! and clocks must be within one visible clock surface. No addresses or writes.
use crate::ElementInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    HiddenIcons,
    NotificationIcons,
    SystemIcons,
    PrivacyIndicators,
    SecondaryClock,
    ControlCenter,
    NotificationCenter,
    ShowDesktop,
}
impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Self::HiddenIcons => "NotifyIconStack",
            Self::NotificationIcons => "NotificationAreaIcons",
            Self::SystemIcons => "MainStack",
            Self::PrivacyIndicators => "NonActivatableStack",
            Self::SecondaryClock => "SecondaryClockStack",
            Self::ControlCenter => "ControlCenterButton",
            Self::NotificationCenter => "NotificationCenterButton",
            Self::ShowDesktop => "ShowDesktopStack",
        }
    }
    pub const ALL: [Self; 8] = [
        Self::HiddenIcons,
        Self::NotificationIcons,
        Self::SystemIcons,
        Self::PrivacyIndicators,
        Self::SecondaryClock,
        Self::ControlCenter,
        Self::NotificationCenter,
        Self::ShowDesktop,
    ];
    pub fn model(self) -> bool {
        matches!(
            self,
            Self::HiddenIcons
                | Self::NotificationIcons
                | Self::SystemIcons
                | Self::PrivacyIndicators
                | Self::ControlCenter
                | Self::ShowDesktop
        )
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Surface {
    pub role: Role,
    pub node: usize,
    pub items: Option<usize>,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clock {
    pub surface: Role,
    pub time: usize,
    pub date: usize,
    /// Smallest layout-only subtree containing exactly the two text elements.
    /// This does NOT authorize intercepting its parent notification button.
    pub container: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topology {
    pub surfaces: Vec<Surface>,
    pub clock: Option<Clock>,
    pub clock_error: Option<String>,
}
/// Indices always refer to this report's own panel-scoped snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub elements: Vec<ElementInfo>,
    pub selection: Option<Topology>,
    pub error: Option<String>,
}
struct Tree<'a> {
    nodes: &'a [ElementInfo],
    parents: Vec<Option<usize>>,
    ends: Vec<usize>,
}
impl<'a> Tree<'a> {
    fn new(nodes: &'a [ElementInfo], truncated: bool) -> Result<Self, String> {
        if truncated {
            return Err("托盘树检查被截断，不能根据部分结果接管".into());
        }
        if nodes.is_empty() || nodes.len() > 320 || nodes[0].depth != 0 {
            return Err("托盘树根或元素数量无效".into());
        }
        if nodes[0].name != "SystemTrayFrameGrid" || !layout(&nodes[0].class_name) {
            return Err("托盘容器名称或类型不匹配".into());
        }
        let mut stack = Vec::new();
        let mut parents = vec![None; nodes.len()];
        let mut ends = vec![nodes.len(); nodes.len()];
        for (index, node) in nodes.iter().enumerate() {
            let depth = node.depth as usize;
            if depth > 40
                || depth > stack.len()
                || index != 0 && depth == 0
                || node.name.len() > 512
                || node.class_name.len() > 1024
                || [node.width, node.height]
                    .into_iter()
                    .flatten()
                    .any(|v| !v.is_finite() || v < 0.)
            {
                return Err("托盘树深度、名称或尺寸无效".into());
            }
            while stack.len() > depth {
                ends[stack.pop().expect("nonempty stack")] = index;
            }
            parents[index] = stack.last().copied();
            stack.push(index);
        }
        Ok(Self {
            nodes,
            parents,
            ends,
        })
    }
    fn inside(&self, node: usize, ancestor: usize) -> bool {
        node >= ancestor && node < self.ends[ancestor]
    }
    fn visible(&self, node: usize) -> Result<bool, String> {
        let mut cursor = Some(node);
        let mut unknown = false;
        while let Some(index) = cursor {
            match self.nodes[index].visible {
                Some(false) => return Ok(false),
                None => unknown = true,
                Some(true) => {}
            }
            cursor = self.parents[index];
        }
        if unknown {
            Err("时钟祖先可见性未知".into())
        } else {
            Ok(true)
        }
    }
    fn clock(&self, surfaces: &[Surface]) -> Result<Clock, String> {
        let mut candidates = Vec::new();
        for surface in surfaces
            .iter()
            .filter(|s| matches!(s.role, Role::SecondaryClock | Role::NotificationCenter))
        {
            let mut time = Vec::new();
            let mut date = Vec::new();
            if !self.visible(surface.node)? {
                continue;
            }
            for i in surface.node + 1..self.ends[surface.node] {
                let node = &self.nodes[i];
                if !matches!(
                    node.name.as_str(),
                    "TimeInnerTextBlock" | "DateInnerTextBlock"
                ) {
                    continue;
                }
                // Compact Shell clocks retain a collapsed date TextBlock in
                // this exact two-text layout. It is still part of the clock,
                // although it has no arranged size on a compact cold start.
                let collapsed_date = node.name == "DateInnerTextBlock"
                    && node.visible == Some(false)
                    && self.parents[i].is_some_and(|p| {
                        self.nodes[p].name == "DateTimeStackPanel"
                            && self.nodes[p].class_name == "Windows.UI.Xaml.Controls.StackPanel"
                    })
                    && self.visible(self.parents[i].expect("checked above"))?;
                if !self.visible(i)? && !collapsed_date {
                    continue;
                }
                if node.class_name != "Windows.UI.Xaml.Controls.TextBlock" {
                    return Err("时钟文本控件类型不匹配".into());
                }
                if node.name == "TimeInnerTextBlock" {
                    time.push(i);
                } else {
                    date.push(i);
                }
            }
            if time.is_empty() && date.is_empty() {
                continue;
            }
            if time.len() != 1 || date.len() != 1 {
                return Err("同一时钟区域的时间或日期缺失、重复".into());
            }
            let (time, date) = (time[0], date[0]);
            let mut parent = self.parents[time].ok_or("时钟缺少容器")?;
            while !self.inside(date, parent) {
                parent = self.parents[parent].ok_or("时钟文本没有共同容器")?;
            }
            if parent == surface.node {
                return Err("无法将时钟文字容器与整个系统按钮分离".into());
            }
            for (i, node) in self
                .nodes
                .iter()
                .enumerate()
                .take(self.ends[parent])
                .skip(parent)
            {
                if i != time && i != date && !layout(&node.class_name) {
                    return Err("时钟文字容器还包含其他交互或未知控件".into());
                }
            }
            if [parent, time, date]
                .into_iter()
                .filter(|&i| i != date || self.nodes[date].visible != Some(false))
                .any(|i| {
                    self.nodes[i].width.is_none_or(|v| v <= 0.)
                        || self.nodes[i].height.is_none_or(|v| v <= 0.)
                })
            {
                return Err("时钟布局尚未就绪".into());
            }
            candidates.push(Clock {
                surface: surface.role,
                time,
                date,
                container: parent,
            });
        }
        if candidates.len() != 1 {
            return Err("未找到唯一可见的原生时钟".into());
        }
        Ok(candidates.remove(0))
    }
}
fn layout(class: &str) -> bool {
    matches!(
        class,
        "Windows.UI.Xaml.Controls.Grid" | "Windows.UI.Xaml.Controls.StackPanel"
    )
}
pub fn analyze_panel(nodes: &[ElementInfo], truncated: bool) -> Result<Topology, String> {
    let tree = Tree::new(nodes, truncated)?;
    let mut surfaces = Vec::new();
    for role in Role::ALL {
        let indices = nodes
            .iter()
            .enumerate()
            .filter(|(i, n)| tree.parents[*i] == Some(0) && n.name == role.name())
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        if indices.len() > 1 {
            return Err(format!("托盘直接子控件重复：{}", role.name()));
        }
        let Some(&node) = indices.first() else {
            continue;
        };
        let lists = (node + 1..tree.ends[node])
            .filter(|&i| nodes[i].class_name == "SystemTray.StackListView")
            .collect::<Vec<_>>();
        let error = if lists.len() > 1 {
            Some("同一托盘区域存在多个原生列表，拒绝猜测".into())
        } else if lists
            .first()
            .is_some_and(|&i| nodes[i].depth - nodes[node].depth > 6)
        {
            Some("原生托盘列表超出已知层级".into())
        } else {
            None
        };
        let items = if error.is_none() {
            lists.first().copied()
        } else {
            None
        };
        surfaces.push(Surface {
            role,
            node,
            items,
            error,
        });
    }
    if surfaces.is_empty() {
        return Err("托盘容器不包含已知区域".into());
    }
    let (clock, clock_error) = match tree.clock(&surfaces) {
        Ok(clock) => (Some(clock), None),
        Err(error) => (None, Some(error)),
    };
    Ok(Topology {
        surfaces,
        clock,
        clock_error,
    })
}
/// The input boundary is separate from the text/formatting boundary.
pub fn clock_input_node(nodes: &[ElementInfo], clock: &Clock) -> Result<usize, String> {
    let path = clock_boundary_path(nodes, clock)?;
    let boundary = *path.last().expect("clock container present");
    if nodes[boundary].class_name == "SystemTray.IconView" {
        // Handle padded clicks before the native DateTimeIconContent and
        // IconView class handlers run.
        if let Some(&content) = path
            .iter()
            .take_while(|&&content| nodes[content].class_name != "SystemTray.DateTimeIconContent")
            .find(|&&content| {
                layout(&nodes[content].class_name)
                    && nodes[content].width == nodes[boundary].width
                    && nodes[content].height == nodes[boundary].height
            })
        {
            return Ok(content);
        }
    }
    Ok(clock.container)
}

/// Tooltips may be attached to the clock's outer IconView rather than its
/// text layout. Include that ancestor only when its whole subtree belongs to
/// this clock; never reach a notification button shared with a bell/action.
pub fn clock_tooltip_nodes(nodes: &[ElementInfo], clock: &Clock) -> Result<Vec<usize>, String> {
    let mut targets = clock_boundary_path(nodes, clock)?;
    targets.extend([clock.time, clock.date]);
    targets.sort_unstable();
    targets.dedup();
    Ok(targets)
}

fn clock_boundary_path(nodes: &[ElementInfo], clock: &Clock) -> Result<Vec<usize>, String> {
    let tree = Tree::new(nodes, false)?;
    if [clock.container, clock.time, clock.date]
        .iter()
        .any(|&i| i >= nodes.len())
    {
        return Err("时钟输入索引无效".into());
    }
    let mut path = vec![clock.container];
    let mut cursor = tree.parents[clock.container];
    let mut datetime = false;
    while let Some(index) = cursor {
        let class = nodes[index].class_name.as_str();
        datetime |= class == "SystemTray.DateTimeIconContent";
        if class == "SystemTray.IconView" {
            if datetime
                && tree.visible(index)?
                && (index..tree.ends[index]).all(|i| {
                    i == clock.time
                        || i == clock.date
                        || layout(&nodes[i].class_name)
                        || matches!(
                            nodes[i].class_name.as_str(),
                            "SystemTray.IconView" | "SystemTray.DateTimeIconContent"
                        )
                })
            {
                path.push(index);
                return Ok(path);
            }
            break;
        }
        if !layout(class) && class != "SystemTray.DateTimeIconContent" {
            break;
        }
        path.push(index);
        cursor = tree.parents[index];
    }
    Ok(vec![clock.container])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(name: &str, class: &str, depth: u32) -> ElementInfo {
        ElementInfo {
            name: name.into(),
            class_name: format!("Windows.UI.Xaml.Controls.{class}"),
            depth,
            width: Some(100.),
            height: Some(20.),
            text: None,
            visible: Some(true),
        }
    }
    #[test]
    fn captured_clock_padding_belongs_to_input_but_notification_bell_does_not() {
        let mut nodes = vec![
            node("SystemTrayFrameGrid", "StackPanel", 0),
            node("NotificationCenterButton", "ContentControl", 1),
            node("SystemTrayIcon", "Grid", 2),
            node("ContainerGrid", "Grid", 3),
            node("ContentGrid", "Grid", 4),
            node("", "Grid", 5),
            node("ContainerGrid", "Grid", 6),
            node("DateTimeStackPanel", "StackPanel", 7),
            node("TimeInnerTextBlock", "TextBlock", 8),
            node("DateInnerTextBlock", "TextBlock", 8),
            node("Bell", "Button", 2),
        ];
        nodes[2].class_name = "SystemTray.IconView".into();
        nodes[5].class_name = "SystemTray.DateTimeIconContent".into();
        for n in &mut nodes[2..7] {
            n.width = Some(120.);
            n.height = Some(48.);
        }
        nodes[7].width = Some(112.8);
        nodes[7].height = Some(32.);
        let clock = analyze_panel(&nodes, false).unwrap().clock.unwrap();
        assert_eq!(clock.container, 7);
        // Replay a bubbling tap from the padded content to IconView. XAML
        // invokes a control's class handler before its instance handlers.
        // Subscribing on IconView with handledEventsToo observes the action
        // after the native calendar has already opened.
        let input = clock_input_node(&nodes, &clock).unwrap();
        let mut handled = false;
        let mut native = 0;
        let mut custom = 0;
        for index in (2..=clock.container).rev() {
            if matches!(index, 2 | 5) && !handled {
                native += 1;
                handled = true;
            }
            if index == input && !handled {
                custom += 1;
                handled = true;
            }
        }
        assert_eq!(
            (custom, native),
            (1, 0),
            "one click must open only our calendar"
        );
        assert_eq!(
            clock_input_node(&nodes, &clock).unwrap(),
            6,
            "top/bottom padding must activate the calendar"
        );
        assert_eq!(
            clock_tooltip_nodes(&nodes, &clock).unwrap(),
            (2..=9).collect::<Vec<_>>(),
            "clock tooltip includes its exclusive IconView and excludes the bell"
        );
        nodes.insert(10, node("OtherAction", "Button", 3));
        let clock = analyze_panel(&nodes, false).unwrap().clock.unwrap();
        assert_eq!(
            clock_input_node(&nodes, &clock).unwrap(),
            7,
            "unknown actions prevent expansion"
        );
        assert_eq!(
            clock_tooltip_nodes(&nodes, &clock).unwrap(),
            vec![7, 8, 9],
            "unknown actions prevent suppressing a shared ancestor tooltip"
        );
    }
    fn fixture() -> Vec<ElementInfo> {
        vec![
            node("SystemTrayFrameGrid", "Grid", 0),
            node("NotificationAreaIcons", "ItemsControl", 1),
            node("NotificationCenterButton", "ContentControl", 1),
            node("ClockText", "StackPanel", 2),
            node("TimeInnerTextBlock", "TextBlock", 3),
            node("DateInnerTextBlock", "TextBlock", 3),
            node("Bell", "Button", 2),
            node("ShowDesktopStack", "Grid", 1),
        ]
    }
    #[test]
    fn confines_clock_to_its_text_container_and_roles_to_direct_children() {
        let mut nodes = fixture();
        nodes.insert(2, node("ControlCenterButton", "Grid", 2));
        let topology = analyze_panel(&nodes, false).unwrap();
        assert!(
            !topology
                .surfaces
                .iter()
                .any(|s| s.role == Role::ControlCenter)
        );
        let clock = topology.clock.unwrap();
        assert_eq!(nodes[clock.container].name, "ClockText");
        assert_eq!(nodes[clock.time].name, "TimeInnerTextBlock");
    }
    #[test]
    fn rejects_partial_and_malformed_snapshots() {
        let mut nodes = fixture();
        assert!(analyze_panel(&nodes, true).is_err());
        nodes[3].depth = 8;
        assert!(analyze_panel(&nodes, false).is_err());
        let mut nodes = fixture();
        nodes[1].depth = 0;
        assert!(analyze_panel(&nodes, false).is_err());
        let mut nodes = fixture();
        nodes[0].name = "PretendSystemTrayFrameGrid".into();
        assert!(analyze_panel(&nodes, false).is_err());
    }
    #[test]
    fn compact_taskbar_keeps_the_native_clock_when_shell_collapses_only_the_date() {
        let mut nodes = fixture();
        nodes[0].height = Some(32.);
        nodes[2].height = Some(32.);
        nodes[3].name = "DateTimeStackPanel".into();
        nodes[3].height = Some(16.);
        nodes[5].visible = Some(false);
        // A collapsed TextBlock can retain its previous arrangement, or have
        // no layout at all when Explorer starts directly in compact mode.
        for size in [16., 0.] {
            nodes[5].width = Some(size);
            nodes[5].height = Some(size);
            let topology = analyze_panel(&nodes, false).unwrap();
            assert!(
                topology.clock.is_some(),
                "small taskbar loses the custom clock: {:?}",
                topology.clock_error
            );
            let input = clock_input_node(&nodes, topology.clock.as_ref().unwrap()).unwrap();
            assert_ne!(nodes[input].name, "Bell");
        }
        nodes[4].visible = Some(false);
        assert!(analyze_panel(&nodes, false).unwrap().clock.is_none());
        nodes[4].visible = Some(true);
        nodes[3].visible = Some(false);
        assert!(analyze_panel(&nodes, false).unwrap().clock.is_none());
        nodes[3].visible = Some(true);
        nodes[3].name = "UnknownClockLayout".into();
        assert!(analyze_panel(&nodes, false).unwrap().clock.is_none());
    }
    #[test]
    fn clock_failure_does_not_hide_other_surface_diagnostics() {
        for name in ["DateInnerTextBlock", "TimeInnerTextBlock"] {
            let mut nodes = fixture();
            nodes[4].name = name.into();
            nodes[5].name = name.into();
            let result = analyze_panel(&nodes, false).unwrap();
            assert!(result.clock.is_none());
            assert!(result.clock_error.is_some());
            assert_eq!(result.surfaces.len(), 3);
        }
        let mut nodes = fixture();
        nodes[5].height = Some(0.);
        assert!(analyze_panel(&nodes, false).unwrap().clock.is_none());
        nodes[5].height = Some(20.);
        nodes[3].visible = None;
        assert!(analyze_panel(&nodes, false).unwrap().clock.is_none());
    }
    #[test]
    fn handles_secondary_clock_without_hiding_ambiguity() {
        let mut nodes = fixture();
        let mut secondary = nodes[2..6].to_vec();
        secondary[0].name = "SecondaryClockStack".into();
        nodes.extend(secondary);
        assert!(analyze_panel(&nodes, false).unwrap().clock.is_none());
        nodes[2].visible = Some(false);
        assert_eq!(
            analyze_panel(&nodes, false).unwrap().clock.unwrap().surface,
            Role::SecondaryClock
        );
    }
    #[test]
    fn rejects_duplicate_roles_and_unsafe_clock_containers() {
        let mut nodes = fixture();
        nodes.push(node("NotificationAreaIcons", "ItemsControl", 1));
        assert!(analyze_panel(&nodes, false).is_err());
        let mut nodes = fixture();
        nodes[6].depth = 3;
        assert!(analyze_panel(&nodes, false).unwrap().clock.is_none());
        let mut nodes = fixture();
        nodes.remove(3);
        nodes[3].depth = 2;
        nodes[4].depth = 2;
        assert!(analyze_panel(&nodes, false).unwrap().clock.is_none());
    }
    #[test]
    fn duplicate_inner_lists_disable_that_surface() {
        let mut nodes = fixture();
        for _ in 0..2 {
            let mut list = node("IconStack", "ItemsControl", 2);
            list.class_name = "SystemTray.StackListView".into();
            nodes.insert(2, list);
        }
        let topology = analyze_panel(&nodes, false).unwrap();
        let icons = topology
            .surfaces
            .iter()
            .find(|s| s.role == Role::NotificationIcons)
            .unwrap();
        assert!(icons.error.is_some() && icons.items.is_none());
        assert!(topology.clock.is_some());
    }
}
