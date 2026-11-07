//! Consume a monitor-scoped plan on its owning UI thread. The caller supplies
//! views captured from verified roots; this module performs no discovery,
//! injection or hook installation. Retain both trees for the whole session.
use crate::{discovery::View, island::Binding, session::Session, xaml::Object};
use litetray_adapter_protocol::{
    clock::Owner,
    routing::{Plan, Route},
};
use std::rc::Rc;
use windows_sys::Win32::{
    Foundation::HWND,
    System::Threading::{GetCurrentProcessId, GetCurrentThreadId},
    UI::WindowsAndMessaging::GetWindowThreadProcessId,
};

/// Construction requires the native source to prove the HWND/root relation.
pub(crate) struct BoundView<'a> {
    route: Route,
    view: &'a View,
    binding: Rc<Binding>,
}
impl<'a> BoundView<'a> {
    pub(crate) fn interaction_host(&self) -> Result<crate::tray_interaction::Host, String> {
        let mut surfaces = Vec::new();
        for role in [
            litetray_adapter_protocol::topology::Role::HiddenIcons,
            litetray_adapter_protocol::topology::Role::NotificationIcons,
            litetray_adapter_protocol::topology::Role::SystemIcons,
            litetray_adapter_protocol::topology::Role::PrivacyIndicators,
            litetray_adapter_protocol::topology::Role::ControlCenter,
        ] {
            let (element, items) = self.view.tray_surface(role)?;
            let items = items.or_else(|| {
                element
                    .try_query(&crate::xaml_abi::IID_IItemsControl)
                    .ok()
                    .flatten()
            });
            surfaces.push(crate::tray_interaction::Surface {
                role,
                element,
                items,
            });
        }
        Ok(crate::tray_interaction::Host {
            taskbar: self.route.window as HWND,
            secondary: self.route.secondary,
            binding: self.binding.clone(),
            root: self.view.root_content()?,
            surfaces,
        })
    }
    pub(crate) fn input_window(&self) -> u64 {
        self.binding.window() as u64
    }
    pub(crate) fn capture(route: Route, view: &'a View, source: &Object) -> Result<Self, String> {
        check_window(&route)?;
        let binding = Binding::capture(route.window as HWND, source, &view.root_content()?)?;
        Ok(Self {
            route,
            view,
            binding,
        })
    }
}
fn find<'a>(route: &Route, views: &'a [BoundView<'a>]) -> Result<&'a BoundView<'a>, String> {
    let matches = views
        .iter()
        .filter(|b| b.route.window == route.window)
        .collect::<Vec<_>>();
    if matches.len() != 1 || &matches[0].route != route {
        return Err("任务栏绑定缺失、重复或已变化".into());
    }
    check_window(route)?;
    matches[0]
        .binding
        .matches_root(&matches[0].view.root_content()?)?;
    Ok(matches[0])
}
fn check_window(route: &Route) -> Result<(), String> {
    unsafe {
        let mut pid = 0;
        let tid = GetWindowThreadProcessId(route.window as HWND, &mut pid);
        if pid != route.process_id
            || tid != route.thread_id
            || pid != GetCurrentProcessId()
            || tid != GetCurrentThreadId()
        {
            return Err("任务栏窗口已失效或不属于当前原生线程".into());
        }
    }
    Ok(())
}
pub(crate) fn prepare(
    plan: &Plan,
    views: &[BoundView<'_>],
    owner: Owner,
    now: u64,
) -> Result<Session, String> {
    if plan.targets.is_empty() {
        return Err("当前范围没有需要接管的组件".into());
    }
    let source = plan
        .source
        .as_ref()
        .map(|route| find(route, views))
        .transpose()?;
    let mut clocks = Vec::new();
    let mut pairs = Vec::new();
    let mut visibility = Vec::new();
    let mut guards = Vec::new();
    let mut bindings = Vec::new();
    if let Some(source) = source {
        let mut roles = Vec::new();
        for role in plan.targets.iter().flat_map(|t| &t.models) {
            if !roles.contains(role) {
                roles.push(*role);
            }
        }
        guards.push(source.view.lease_guard(&roles, false)?);
        bindings.push(source.binding.clone());
    }
    for target in &plan.targets {
        let bound = find(&target.route, views)?;
        let view = bound.view;
        let mut roles = target.models.clone();
        for change in &target.visibility {
            if !roles.contains(&change.role) {
                roles.push(change.role);
            }
        }
        guards.push(view.lease_guard(&roles, target.clock)?);
        if plan
            .source
            .as_ref()
            .is_none_or(|r| r.window != target.route.window)
        {
            bindings.push(bound.binding.clone());
        }
        if target.clock {
            clocks.push(view.clock_target(target.route.window)?);
        }
        if !target.models.is_empty() {
            pairs.extend(
                source
                    .ok_or("缺少原生托盘来源")?
                    .view
                    .model_pairs_to(view, &target.models)?,
            );
        }
        for change in &target.visibility {
            if !view.native() || !target.models.contains(&change.role) {
                visibility.push(view.visibility_target(change.role, change.visible)?);
            }
        }
    }
    // Everything is captured before Session starts any native property write.
    let mut session = Session::new(owner, clocks, now)?;
    if !pairs.is_empty() {
        session.prepare_tray(pairs)?;
    }
    if !visibility.is_empty() {
        session.prepare_visibility(visibility)?;
    }
    session.prepare_guards(guards, bindings)?;
    Ok(session)
}

/// Only the custom-calendar path uses this entry. Callers that retain the
/// system clock behavior use prepare/Watchdog::attach without subscriptions.
/// The returned watchdog is still Prepared; no input is consumed before start.
pub(crate) fn attach_interactive(
    plan: &Plan,
    views: &[BoundView<'_>],
    owner: Owner,
    now: u64,
    publish: impl Fn(litetray_adapter_protocol::activation::Activation) -> bool + 'static,
) -> Result<crate::watchdog::Watchdog, String> {
    attach_with_options(plan, views, owner, now, Default::default(), publish)
}
pub(crate) fn attach_with_options(
    plan: &Plan,
    views: &[BoundView<'_>],
    owner: Owner,
    now: u64,
    options: litetray_adapter_protocol::activation::Options,
    publish: impl Fn(litetray_adapter_protocol::activation::Activation) -> bool + 'static,
) -> Result<crate::watchdog::Watchdog, String> {
    let session = prepare(plan, views, owner, now)?;
    let watchdog = crate::watchdog::Watchdog::attach(session)?;
    let mut inputs = Vec::new();
    for target in plan.targets.iter().filter(|target| target.clock) {
        let bound = find(&target.route, views)?;
        inputs.push(
            bound
                .binding
                .input(target.route.window, bound.view.clock_input_element()?)?,
        );
    }
    if !inputs.is_empty() {
        watchdog.prepare_inputs_with_options(inputs, options, publish)?;
    }
    Ok(watchdog)
}
