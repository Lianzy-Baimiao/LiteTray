//! Consume a monitor-scoped plan on its owning UI thread. The caller supplies
//! views captured from verified roots; this module performs no discovery,
//! injection or hook installation. Retain both trees for the whole session.
use crate::{discovery::View, session::Session};
use litetray_adapter_protocol::{
    clock::Owner,
    routing::{Plan, Route},
};
use windows_sys::Win32::{
    Foundation::HWND,
    System::Threading::{GetCurrentProcessId, GetCurrentThreadId},
    UI::WindowsAndMessaging::GetWindowThreadProcessId,
};

/// The caller must obtain this View from this window's verified XAML root.
/// Window identity alone cannot prove the root/window relationship.
pub(crate) struct BoundView<'a> {
    pub route: Route,
    pub view: &'a View,
}
fn find<'a>(route: &Route, views: &'a [BoundView<'a>]) -> Result<&'a View, String> {
    let matches = views
        .iter()
        .filter(|b| b.route.window == route.window)
        .collect::<Vec<_>>();
    if matches.len() != 1 || &matches[0].route != route {
        return Err("任务栏绑定缺失、重复或已变化".into());
    }
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
    Ok(matches[0].view)
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
    let mut windows = Vec::new();
    if let Some(source) = source {
        let mut roles = Vec::new();
        for role in plan.targets.iter().flat_map(|t| &t.models) {
            if !roles.contains(role) {
                roles.push(*role);
            }
        }
        guards.push(source.lease_guard(&roles, false)?);
        windows.push(crate::window_guard::WindowGuard::capture(
            plan.source.as_ref().ok_or("缺少来源窗口")?.window as HWND,
        )?);
    }
    for target in &plan.targets {
        let view = find(&target.route, views)?;
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
            windows.push(crate::window_guard::WindowGuard::capture(
                target.route.window as HWND,
            )?);
        }
        if target.clock {
            clocks.push(view.clock_target(target.route.window)?);
        }
        if !target.models.is_empty() {
            pairs.extend(
                source
                    .ok_or("缺少原生托盘来源")?
                    .model_pairs_to(view, &target.models)?,
            );
        }
        for change in &target.visibility {
            visibility.push(view.visibility_target(change.role, change.visible)?);
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
    session.prepare_guards(guards, windows)?;
    Ok(session)
}
