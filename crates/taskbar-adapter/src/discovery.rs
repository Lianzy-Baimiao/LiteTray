//! Taskbar-scoped role discovery. Native references never leave this STA.
//! Production entry requires the exact known frame; isolated checks enter
//! the shared panel reader using a real Grid/StackPanel they created locally.
use crate::{
    clock::ClockLease,
    property::same_object,
    session::Target,
    tray::{Field, Pair, read},
    xaml::{self, Object, check, factory},
    xaml_abi::*,
};
use litetray_adapter_protocol::topology::{self, Report, Role, Surface, Topology};
use std::ptr::null_mut;
use windows_sys::core::IID_IUnknown;

pub(crate) struct View {
    native_clock: bool,
    panel: Object,
    ancestors: Vec<Object>,
    objects: Vec<Object>,
    pub report: Report,
}
/// Identity snapshot retained by an active session. Model collection changes
/// inside a known surface are allowed; replacing the surface or moving it to
/// another root invalidates the lease before the next native write.
pub(crate) struct LeaseGuard {
    panel: Object,
    surfaces: Vec<(Role, Object)>,
    root: Object,
    scale: f64,
    nodes: Vec<GuardNode>,
    clock_tree: Vec<GuardNode>,
}
struct GuardNode {
    object: Object,
    parent: Option<Object>,
    name: String,
    class_name: String,
}
impl View {
    pub(crate) fn native(&self) -> bool {
        self.native_clock
    }
    pub(crate) fn tray_surface(&self, role: Role) -> Result<(Object, Option<Object>), String> {
        let fresh = self.revalidate()?;
        let selected = surface(fresh.selection()?, role)?;
        Ok((
            fresh.objects[selected.node].clone(),
            selected.items.map(|i| fresh.objects[i].clone()),
        ))
    }
    pub(crate) fn root_content(&self) -> Result<Object, String> {
        xaml::root_content(&self.panel)
    }
    pub(crate) fn lease_guard(&self, roles: &[Role], clock: bool) -> Result<LeaseGuard, String> {
        let fresh = self.revalidate()?;
        let selection = fresh.selection()?;
        let mut indices = vec![0usize];
        let mut guarded_roles = roles.to_vec();
        if clock {
            let role = selection.clock.as_ref().ok_or("时钟检查目标缺失")?.surface;
            if !guarded_roles.contains(&role) {
                guarded_roles.push(role);
            }
        }
        let mut surfaces = Vec::new();
        for role in guarded_roles {
            let surface = surface(selection, role)?;
            indices.push(surface.node);
            indices.extend(surface.items);
            surfaces.push((role, fresh.objects[surface.node].clone()));
        }
        if clock {
            let clock = selection.clock.as_ref().ok_or("时钟检查目标缺失")?;
            indices.extend([clock.time, clock.date, clock.container]);
        }
        indices.sort_unstable();
        indices.dedup();
        let mut nodes = Vec::new();
        for index in indices {
            for object in ancestry(&fresh.objects[index])? {
                if !nodes
                    .iter()
                    .any(|n: &GuardNode| n.object.raw() == object.raw())
                {
                    nodes.push(GuardNode::capture(&object)?);
                }
            }
        }
        let clock_tree = if clock {
            let clock = selection.clock.as_ref().ok_or("时钟检查目标缺失")?;
            let input = litetray_adapter_protocol::topology::clock_input_node(
                &fresh.report.elements,
                clock,
            )?;
            clock_nodes(&fresh.objects[input])?
        } else {
            vec![]
        };
        let root = xaml::root(&fresh.panel)?;
        let scale = root_scale(&root)?;
        Ok(LeaseGuard {
            panel: fresh.panel.clone(),
            surfaces,
            root,
            scale,
            nodes,
            clock_tree,
        })
    }
    /// Only this entry is used for a native TaskbarHost root. It deliberately
    /// has no fallback to recursive substring/name matching.
    pub(crate) fn discover(root: &Object) -> Result<Self, String> {
        let litetray_adapter_protocol::compatibility::Topology::SystemTrayFrameV1 =
            litetray_adapter_protocol::compatibility::builtin()?.topology;
        let frames = children(root)?
            .into_iter()
            .filter_map(|node| match node.class_name() {
                Ok(class) if class == "SystemTray.SystemTrayFrame" => Some(Ok(node)),
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .collect::<Result<Vec<_>, _>>()?;
        if frames.len() != 1 {
            return Err("未找到唯一的 SystemTray.SystemTrayFrame 直接子控件".into());
        }
        let mut panels = Vec::new();
        for node in children(&frames[0])? {
            if name(&node)? == "SystemTrayFrameGrid" {
                panels.push(node);
            }
        }
        if panels.len() != 1 {
            return Err("未找到唯一的 SystemTrayFrameGrid 直接子控件".into());
        }
        let panel = panels.pop().expect("exactly one panel");
        if !same_object(&xaml::root_content(&panel)?, root)? {
            return Err("托盘容器不属于给定的 XamlRoot".into());
        }
        let mut view = Self::from_panel(&panel)?;
        view.native_clock = true;
        Ok(view)
    }
    pub(crate) fn from_panel(panel: &Object) -> Result<Self, String> {
        let ancestors = ancestry(panel)?;
        let mut objects = Vec::new();
        let mut tree = xaml::walk(panel.query(&IID_IDependencyObject)?, |node, _| {
            objects.push(node.query(&IID_IUnknown)?);
            Ok(())
        })?;
        // The panel-scoped report omits its outer ancestors. Fold their
        // visibility into the panel root so a collapsed frame cannot expose a
        // seemingly visible clock whose last arranged dimensions remain set.
        if let Some(root) = tree.elements.first_mut() {
            for ancestor in ancestors.iter().skip(1) {
                match (root.visible, xaml::visibility(ancestor)?) {
                    (Some(false), _) | (_, Some(false)) => root.visible = Some(false),
                    (None, _) | (_, None) => root.visible = None,
                    _ => {}
                }
            }
        }
        let (selection, error) = match topology::analyze_panel(&tree.elements, tree.truncated) {
            Ok(selection) => (Some(selection), None),
            Err(error) => (None, Some(error)),
        };
        Ok(Self {
            native_clock: false,
            panel: panel.query(&IID_IUnknown)?,
            ancestors,
            objects,
            report: Report {
                elements: tree.elements,
                selection,
                error,
            },
        })
    }
    fn selection(&self) -> Result<&Topology, String> {
        self.report
            .selection
            .as_ref()
            .ok_or_else(|| self.report.error.clone().unwrap_or("托盘结构不可用".into()))
    }
    /// Reject stale snapshots even if a replacement has identical names and
    /// visible text. The caller must rediscover and capture new originals.
    fn revalidate(&self) -> Result<Self, String> {
        self.selection()?;
        let mut fresh = Self::from_panel(&self.panel)?;
        fresh.native_clock = self.native_clock;
        fresh.selection()?;
        if self.ancestors.len() != fresh.ancestors.len() {
            return Err("托盘容器已迁移到其他父级或 XamlRoot".into());
        }
        for (old, new) in self.ancestors.iter().zip(&fresh.ancestors) {
            if !same_object(old, new)? {
                return Err("托盘容器已迁移到其他父级或 XamlRoot".into());
            }
        }
        if self.objects.len() != fresh.objects.len() {
            return Err("托盘树已变化，需要重新识别".into());
        }
        for (i, (old, new)) in self.objects.iter().zip(&fresh.objects).enumerate() {
            let before = &self.report.elements[i];
            let after = &fresh.report.elements[i];
            if !same_object(old, new)?
                || before.name != after.name
                || before.class_name != after.class_name
                || before.depth != after.depth
            {
                return Err("托盘控件身份或层级已变化，需要重新识别".into());
            }
        }
        Ok(fresh)
    }
    pub(crate) fn clock_target(&self, id: u64) -> Result<Target, String> {
        if id == 0 {
            return Err("时钟目标 ID 无效".into());
        }
        let fresh = self.revalidate()?;
        let selection = fresh.selection()?;
        let clock = selection.clock.as_ref().ok_or_else(|| {
            selection
                .clock_error
                .clone()
                .unwrap_or("原生时钟未就绪".into())
        })?;
        let tooltip_target = topology::clock_input_node(&fresh.report.elements, clock)?;
        let tooltip_nodes = topology::clock_tooltip_nodes(&fresh.report.elements, clock)?
            .into_iter()
            .map(|index| fresh.objects[index].clone())
            .collect::<Vec<_>>();
        Ok(Target {
            id,
            clock: (if fresh.native_clock {
                ClockLease::capture_native
            } else {
                ClockLease::capture
            })(
                &fresh.objects[clock.time],
                &fresh.objects[clock.date],
                &fresh.objects[clock.container],
            )?
            .with_tooltip_targets(&fresh.objects[tooltip_target], &tooltip_nodes)?,
        })
    }
    pub(crate) fn clock_input_element(&self) -> Result<Object, String> {
        let fresh = self.revalidate()?;
        let clock = fresh
            .selection()?
            .clock
            .as_ref()
            .ok_or("独立时钟点击区域不可用")?;
        let input =
            litetray_adapter_protocol::topology::clock_input_node(&fresh.report.elements, clock)?;
        fresh.objects[input].query(&IID_IUIElement)
    }
    pub(crate) fn visibility_target(
        &self,
        role: Role,
        visible: bool,
    ) -> Result<crate::visibility::Target, String> {
        if !role.model() {
            return Err("此区域不能作为托盘可见性目标".into());
        }
        let fresh = self.revalidate()?;
        let surface = surface(fresh.selection()?, role)?;
        Ok(crate::visibility::Target {
            element: fresh.objects[surface.node].query(&IID_IUnknown)?,
            visible,
        })
    }
    /// Derive fields from native interfaces and populated nonvisual models.
    /// All requested roles must yield usable pairs or the whole plan fails.
    /// This only prepares data sharing: visibility, menus and shell callbacks
    /// need their own verified integration before production activation.
    pub(crate) fn model_pairs_to(
        &self,
        target: &Self,
        roles: &[Role],
    ) -> Result<Vec<Pair>, String> {
        if roles.is_empty()
            || roles.len() > 6
            || roles
                .iter()
                .enumerate()
                .any(|(i, r)| !r.model() || roles[..i].contains(r))
        {
            return Err("托盘模型区域选择无效或重复".into());
        }
        if same_object(
            &xaml::root_content(&self.panel)?,
            &xaml::root_content(&target.panel)?,
        )? {
            return Err("托盘模型来源和副本必须属于不同 XAML 根".into());
        }
        let source = self.revalidate()?;
        let target = target.revalidate()?;
        let mut pairs = Vec::new();
        for &role in roles {
            let a = surface(source.selection()?, role)?;
            let b = surface(target.selection()?, role)?;
            let start = pairs.len();
            if let Some(mut pair) = model_pair(&source.objects[a.node], &target.objects[b.node])? {
                if self.native_clock {
                    pair.fields.extend([
                        Field::Width,
                        Field::MinWidth,
                        Field::Opacity,
                        Field::HitTest,
                        Field::Visibility,
                    ]);
                }
                pairs.push(pair);
            }
            match (a.items, b.items) {
                (Some(a), Some(b)) => {
                    if let Some(mut pair) = model_pair(&source.objects[a], &target.objects[b])? {
                        if self.native_clock {
                            pair.fields.extend([
                                Field::Width,
                                Field::MinWidth,
                                Field::Opacity,
                                Field::HitTest,
                                Field::Visibility,
                            ]);
                        }
                        pairs.push(pair);
                    }
                }
                (None, None) => {}
                _ => return Err(format!("{} 的主副屏内部列表结构不一致", role.name())),
            }
            if pairs.len() == start {
                return Err(format!("{} 的原生数据来源尚未就绪", role.name()));
            }
        }
        Ok(pairs)
    }
}
impl LeaseGuard {
    pub(crate) fn validate(&self) -> Result<(), String> {
        // Recheck retained edges, not every generated icon/item descendant.
        // Text, size and owned visibility are deliberately not structural.
        let direct = children(&self.panel)?
            .into_iter()
            .map(|object| Ok((name(&object)?, object)))
            .collect::<Result<Vec<_>, String>>()?;
        for (role, object) in &self.surfaces {
            let matches = direct
                .iter()
                .filter(|(name, _)| name == role.name())
                .collect::<Vec<_>>();
            if matches.len() != 1 || !same_object(object, &matches[0].1)? {
                return Err("托盘区域已替换或出现重复角色".into());
            }
        }
        for node in &self.nodes {
            if !same_object(&xaml::root(&node.object)?, &self.root)? {
                return Err("托盘控件已脱离原 XamlRoot".into());
            }
            node.validate()?;
        }
        if root_scale(&self.root)? != self.scale {
            return Err("XAML 缩放已变化，需要重新识别".into());
        }
        if let Some(container) = self.clock_tree.first() {
            let current = clock_nodes(&container.object)?;
            if current.len() != self.clock_tree.len() {
                return Err("时钟输入区域结构已变化".into());
            }
            for (old, new) in self.clock_tree.iter().zip(current) {
                if !same_object(&old.object, &new.object)? {
                    return Err("时钟输入区域控件已替换".into());
                }
                old.validate()?;
            }
        }
        Ok(())
    }
}
fn root_scale(root: &Object) -> Result<f64, String> {
    let root = root.query(&IID_IXamlRoot)?;
    let mut scale = 0.;
    unsafe {
        check(
            (root.table::<IXamlRoot_Vtbl>().RasterizationScale)(root.raw(), &mut scale),
            "Guard XAML scale",
        )?;
    }
    if !scale.is_finite() || !(0.5..=8.).contains(&scale) {
        return Err("XAML 缩放不可用".into());
    }
    Ok(scale)
}
impl GuardNode {
    fn capture(object: &Object) -> Result<Self, String> {
        Ok(Self {
            object: object.query(&IID_IUnknown)?,
            parent: parent(object)?,
            name: name(object)?,
            class_name: object.class_name()?,
        })
    }
    fn validate(&self) -> Result<(), String> {
        if name(&self.object)? != self.name || self.object.class_name()? != self.class_name {
            return Err("托盘控件名称或类型已变化".into());
        }
        let current = parent(&self.object)?;
        let same = match (&self.parent, &current) {
            (None, None) => true,
            (Some(a), Some(b)) => same_object(a, b)?,
            _ => false,
        };
        if !same {
            return Err("托盘控件父级已变化".into());
        }
        Ok(())
    }
}
fn parent(object: &Object) -> Result<Option<Object>, String> {
    let helper = factory(
        "Windows.UI.Xaml.Media.VisualTreeHelper",
        &IID_IVisualTreeHelperStatics,
    )?;
    let node = object.query(&IID_IDependencyObject)?;
    let mut value = null_mut();
    unsafe {
        check(
            (helper.table::<IVisualTreeHelperStatics_Vtbl>().GetParent)(
                helper.raw(),
                node.raw(),
                &mut value,
            ),
            "Guard parent",
        )?;
        if value.is_null() {
            Ok(None)
        } else {
            Ok(Some(Object::owned(value)?))
        }
    }
}
fn clock_nodes(container: &Object) -> Result<Vec<GuardNode>, String> {
    let mut nodes: Vec<GuardNode> = Vec::new();
    let mut pending = vec![(container.clone(), 0)];
    while let Some((node, depth)) = pending.pop() {
        if nodes.len() >= 80 || depth > 40 {
            return Err("时钟区域结构超限".into());
        }
        for old in &nodes {
            if same_object(&old.object, &node)? {
                return Err("时钟区域重复控件".into());
            }
        }
        nodes.push(GuardNode::capture(&node)?);
        for child in children(&node)?.into_iter().rev() {
            pending.push((child, depth + 1));
        }
    }
    Ok(nodes)
}
fn ancestry(panel: &Object) -> Result<Vec<Object>, String> {
    let root = xaml::root_content(panel)?;
    let helper = factory(
        "Windows.UI.Xaml.Media.VisualTreeHelper",
        &IID_IVisualTreeHelperStatics,
    )?;
    let mut result = vec![panel.query(&IID_IUnknown)?];
    unsafe {
        loop {
            let current = result.last().expect("panel present");
            if same_object(current, &root)? {
                return Ok(result);
            }
            if result.len() >= 41 {
                return Err("托盘容器祖先层级超限".into());
            }
            let mut parent = null_mut();
            let dependency = current.query(&IID_IDependencyObject)?;
            check(
                (helper.table::<IVisualTreeHelperStatics_Vtbl>().GetParent)(
                    helper.raw(),
                    dependency.raw(),
                    &mut parent,
                ),
                "Tray visual parent",
            )?;
            if parent.is_null() {
                return Err("托盘容器已脱离原生 XAML 根".into());
            }
            let parent = Object::owned(parent)?;
            for existing in &result {
                if same_object(existing, &parent)? {
                    return Err("托盘祖先出现循环引用".into());
                }
            }
            result.push(parent);
        }
    }
}
fn surface(topology: &Topology, role: Role) -> Result<&Surface, String> {
    let surface = topology
        .surfaces
        .iter()
        .find(|s| s.role == role)
        .ok_or_else(|| format!("缺少托盘区域：{}", role.name()))?;
    if let Some(error) = &surface.error {
        return Err(format!("{}：{error}", role.name()));
    }
    Ok(surface)
}
fn model_pair(source: &Object, target: &Object) -> Result<Option<Pair>, String> {
    let mut fields = Vec::new();
    for (field, iid) in [
        (Field::DataContext, &IID_IFrameworkElement),
        (Field::ItemsSource, &IID_IItemsControl),
        (Field::Content, &IID_IContentControl),
    ] {
        if source.try_query(iid)?.is_none() {
            continue;
        }
        let Some(value) = read(source, &field.property()?)? else {
            continue;
        };
        if value.try_query(&IID_IUIElement)?.is_some() {
            continue;
        }
        if target.try_query(iid)?.is_none() {
            return Err("目标控件缺少来源模型所需接口".into());
        }
        fields.push(field);
    }
    if fields.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Pair {
            source: source.query(&IID_IUnknown)?,
            target: target.query(&IID_IUnknown)?,
            fields,
        }))
    }
}
fn name(object: &Object) -> Result<String, String> {
    let Some(framework) = object.try_query(&IID_IFrameworkElement)? else {
        return Ok(String::new());
    };
    unsafe { framework.string(framework.table::<IFrameworkElement_Vtbl>().Name, 128) }
}
fn children(object: &Object) -> Result<Vec<Object>, String> {
    let object = object.query(&IID_IDependencyObject)?;
    let helper = factory(
        "Windows.UI.Xaml.Media.VisualTreeHelper",
        &IID_IVisualTreeHelperStatics,
    )?;
    unsafe {
        let mut count = 0;
        let table = helper.table::<IVisualTreeHelperStatics_Vtbl>();
        check(
            (table.GetChildrenCount)(helper.raw(), object.raw(), &mut count),
            "Taskbar children",
        )?;
        if !(0..=512).contains(&count) {
            return Err("任务栏子控件数量异常".into());
        }
        let mut result = Vec::with_capacity(count as usize);
        for index in 0..count {
            let mut value = null_mut();
            check(
                (table.GetChild)(helper.raw(), object.raw(), index, &mut value),
                "Taskbar child",
            )?;
            result.push(Object::owned(value)?);
        }
        Ok(result)
    }
}
