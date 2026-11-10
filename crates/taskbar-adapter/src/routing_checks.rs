//! Real isolated XAML roots exercise the same routing and session preparation
//! used by the adapter. They are not Explorer taskbars or private Shell types.
use crate::{
    clock::boxed_text,
    discovery::View,
    preview::ModelFixture,
    property::equal_optional,
    routing::{self, BoundView},
    session::{Session, State},
    tray::{Field, read},
    visibility::{self, Target},
    watchdog::Watchdog,
    xaml::{Object, check},
    xaml_abi::*,
};
use litetray_adapter_protocol::{
    VERSION,
    clock::{Change, Command, Frame, Owner},
    routing::{Options, Plan, Route},
};
use windows_sys::Win32::System::{
    SystemInformation::GetTickCount64,
    Threading::{GetCurrentProcessId, GetCurrentThreadId},
};

pub(crate) const MARKUP: &str = r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
<StackPanel x:Name="SystemTrayFrameGrid" Width="520" Height="130">
  <ContentControl x:Name="NotificationAreaIcons" DataContext="Icons model" Content="Icons" Visibility="Collapsed"/>
  <ContentControl x:Name="NotifyIconStack" DataContext="Overflow model" Content="Overflow" Visibility="Collapsed"/>
  <ContentControl x:Name="MainStack" DataContext="System model" Content="System" Visibility="Collapsed"/>
  <ContentControl x:Name="NonActivatableStack" DataContext="Privacy model" Content="Privacy" Visibility="Collapsed"/>
  <ContentControl x:Name="ControlCenterButton" DataContext="Control center model" Content="Control center" Visibility="Collapsed"/>
  <Grid x:Name="UnusedSurface" Height="0"/>
  <ContentControl x:Name="NotificationCenterButton" MinHeight="80">
    <Grid><StackPanel x:Name="ClockText" Width="200" Height="60">
      <TextBlock x:Name="TimeInnerTextBlock" Text="Original time" Width="200" Height="30"/>
      <TextBlock x:Name="DateInnerTextBlock" Text="Original date" Width="200" Height="30"/>
    </StackPanel><Button x:Name="Bell" Content="Notifications" HorizontalAlignment="Right"/></Grid>
  </ContentControl>
</StackPanel></Grid>"#;
fn require(value: bool, label: &str) -> Result<(), String> {
    if value {
        Ok(())
    } else {
        Err(format!("范围会话隔离检查失败：{label}"))
    }
}
fn text(node: &Object) -> Result<String, String> {
    let node = node.query(&IID_ITextBlock)?;
    unsafe { node.string(node.table::<ITextBlock_Vtbl>().Text, 4096) }
}
fn visible(node: &Object) -> Result<bool, String> {
    let node = node.query(&IID_IUIElement)?;
    let mut value = Visibility::Collapsed;
    unsafe {
        check(
            (node.table::<IUIElement_Vtbl>().Visibility)(node.raw(), &mut value),
            "Check visibility",
        )?;
    }
    Ok(value.0 == Visibility::Visible.0)
}
fn set_visible(node: &Object, visible: bool) -> Result<(), String> {
    let node = node.query(&IID_IUIElement)?;
    unsafe {
        check(
            (node.table::<IUIElement_Vtbl>().SetVisibility)(
                node.raw(),
                if visible {
                    Visibility::Visible
                } else {
                    Visibility::Collapsed
                },
            ),
            "External visibility",
        )
    }
}
fn set_context(node: &Object, value: &str) -> Result<(), String> {
    let node = node.query(&IID_IFrameworkElement)?;
    unsafe {
        check(
            (node.table::<IFrameworkElement_Vtbl>().SetDataContext)(
                node.raw(),
                boxed_text(value)?.raw(),
            ),
            "Check DataContext",
        )
    }
}
fn rename(node: &Object, value: &str) -> Result<(), String> {
    let node = node.query(&IID_IFrameworkElement)?;
    let value = crate::xaml::StringHandle::new(value)?;
    unsafe {
        check(
            (node.table::<IFrameworkElement_Vtbl>().SetName)(node.raw(), value.0),
            "Rename route fixture",
        )
    }
}
pub(crate) fn check_visibility_scalar() -> Result<(), String> {
    let node = crate::clock::load(
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Visibility="Collapsed"/>"#,
    )?;
    let identity = visibility::visibility_property()?.query(&windows_sys::core::IID_IUnknown)?;
    let _capture = crate::property::Property::capture(&node, identity)?;
    let mut lease = visibility::VisibilityLease::capture(vec![Target {
        element: node.query(&windows_sys::core::IID_IUnknown)?,
        visible: true,
    }])?;
    lease.start()?;
    require(visible(&node)?, "single native enum apply")?;
    lease.stop()?;
    require(!visible(&node)?, "single native enum restore")
}
fn route(fixture: &ModelFixture, secondary: bool, device: &str) -> Route {
    Route {
        window: fixture.window() as u64,
        process_id: unsafe { GetCurrentProcessId() },
        thread_id: unsafe { GetCurrentThreadId() },
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
pub(crate) fn run(owner: Owner, frame: Frame) -> Result<Vec<&'static str>, String> {
    let source = ModelFixture::new(owner.clone(), frame.clone(), MARKUP)?;
    let target = ModelFixture::new(owner.clone(), frame.clone(), MARKUP)?;
    let untouched = ModelFixture::new(owner.clone(), frame.clone(), MARKUP)?;
    let source_control = source.find("ControlCenterButton")?;
    let target_control = target.find("ControlCenterButton")?;
    set_context(&source_control, "Source control model")?;
    set_context(&target_control, "Target original model")?;
    let original = read(&target_control, &Field::DataContext.property()?)?;
    let source_model = read(&source_control, &Field::DataContext.property()?)?;
    let a = View::from_panel(&source.find("SystemTrayFrameGrid")?)?;
    let b = View::from_panel(&target.find("SystemTrayFrameGrid")?)?;
    let c = View::from_panel(&untouched.find("SystemTrayFrameGrid")?)?;
    let routes = [
        route(&source, false, "A"),
        route(&target, true, "B"),
        route(&untouched, true, "C"),
    ];
    let views = [
        BoundView::capture(routes[0].clone(), &a, source.source()?)?,
        BoundView::capture(routes[1].clone(), &b, target.source()?)?,
        BoundView::capture(routes[2].clone(), &c, untouched.source()?)?,
    ];
    let hover_check = crate::tray_interaction::check_hover_routes(
        views
            .iter()
            .map(BoundView::interaction_host)
            .collect::<Result<Vec<_>, _>>()?,
    )?;
    let menu_fixture = ModelFixture::new(
        owner.clone(),
        frame.clone(),
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"><StackPanel><Grid x:Name="MainStack"><StackPanel><TextBlock x:Name="SharedMenuTarget" Text="Shared"/></StackPanel></Grid><Grid><TextBlock x:Name="OrdinaryMenuTarget" Text="Ordinary"/></Grid></StackPanel></Grid>"#,
    )?;
    let shared_menu_check = crate::tray_interaction::check_context_ancestry(
        &menu_fixture.find("SharedMenuTarget")?,
        true,
    )?;
    let ordinary_menu_check = crate::tray_interaction::check_context_ancestry(
        &menu_fixture.find("OrdinaryMenuTarget")?,
        false,
    )?;
    let plan = Plan::build(&routes, &["B".into()], options())?;
    let now = unsafe { GetTickCount64() };
    let mut session = routing::prepare(&plan, &views, owner.clone(), now)?;
    let time = target.find("TimeInnerTextBlock")?;
    require(
        text(&time)? == "Original time" && !visible(&target_control)?,
        "prepare performs no writes",
    )?;
    session.start(frame.clone(), now)?;
    require(
        text(&time)? == frame.text.top
            && visible(&target_control)?
            && text(&source.find("TimeInnerTextBlock")?)? == "Original time"
            && text(&untouched.find("TimeInnerTextBlock")?)? == "Original time"
            && !visible(&untouched.find("ControlCenterButton")?)?
            && equal_optional(
                read(&target_control, &Field::DataContext.property()?)?.as_ref(),
                source_model.as_ref(),
            )?,
        "only selected replica receives clock models and visibility",
    )?;
    session.stop()?;
    require(
        text(&time)? == "Original time"
            && !visible(&target_control)?
            && equal_optional(
                read(&target_control, &Field::DataContext.property()?)?.as_ref(),
                original.as_ref(),
            )?,
        "stop restores selected screen",
    )?;
    let mut checks = vec![
        "routing_selected_replica_native_apply_restore_and_unselected_preservation",
        hover_check,
        shared_menu_check,
        ordinary_menu_check,
    ];
    require(
        routing::prepare(&plan, &views[..1], owner.clone(), now).is_err(),
        "missing target binding rejected",
    )?;
    let mut changed = routes[1].clone();
    changed.device = "changed".into();
    let mismatched = [
        BoundView::capture(routes[0].clone(), &a, source.source()?)?,
        BoundView::capture(changed, &b, target.source()?)?,
    ];
    require(
        routing::prepare(&plan, &mismatched, owner.clone(), now).is_err()
            && text(&time)? == "Original time",
        "changed mapping rejected before writes",
    )?;
    checks.push("routing_missing_or_stale_window_binding_rejected");

    // Tray can now run without claiming the clock. Watchdog still owns lease
    // validation and restores both models and visibility on explicit stop.
    let mut o = options();
    o.clocks = false;
    o.icons = false;
    let plan = Plan::build(&routes, &["B".into()], o)?;
    let session = routing::prepare(&plan, &views, owner.clone(), unsafe { GetTickCount64() })?;
    let watchdog = Watchdog::attach(session)?;
    watchdog.start_tray()?;
    require(
        text(&time)? == "Original time"
            && visible(&target_control)?
            && !visible(&target.find("NotificationAreaIcons")?)?,
        "tray-only scope leaves clock and hidden regions alone",
    )?;
    let encode = |sequence, change| {
        serde_json::to_vec(&Command {
            version: VERSION,
            owner: owner.clone(),
            sequence,
            change,
        })
        .map_err(|e| e.to_string())
    };
    require(
        watchdog
            .receive(&encode(
                1,
                Change::Tick {
                    text: frame.text.clone(),
                },
            )?)
            .is_err(),
        "tray-only session refuses clock tick",
    )?;
    require(
        watchdog
            .receive(&encode(
                1,
                Change::Update {
                    frame: frame.clone(),
                },
            )?)
            .is_err(),
        "tray-only session refuses clock style",
    )?;
    watchdog.receive(&encode(1, Change::Heartbeat)?)?;
    watchdog.receive(&encode(2, Change::Stop)?)?;
    require(
        !visible(&target_control)?
            && text(&time)? == "Original time"
            && equal_optional(
                read(&target_control, &Field::DataContext.property()?)?.as_ref(),
                original.as_ref(),
            )?,
        "tray-only stop restores all owned properties",
    )?;
    checks.push("routing_tray_only_watchdog_commands_and_restore");

    let plan = Plan::build(&routes, &["B".into()], options())?;
    let mut session = routing::prepare(&plan, &views, owner.clone(), now)?;
    session.start(frame.clone(), now)?;
    rename(&target_control, "ReplacedControlCenterButton")?;
    require(
        session.poll(now + 1, true).is_err()
            && session.state() == State::Closed
            && text(&time)? == "Original time",
        "runtime replacement stops and restores the old lease",
    )?;
    rename(&target_control, "ControlCenterButton")?;
    checks.push("routing_runtime_surface_replacement_stops_and_restores");

    // A conflict after capture fails the whole start and rolls back clocks
    // and models already written, while preserving the competing visibility.
    let plan = Plan::build(&routes, &["B".into()], options())?;
    let mut session = routing::prepare(&plan, &views, owner.clone(), now)?;
    set_visible(&target_control, true)?;
    require(
        session.start(frame.clone(), now).is_err()
            && session.state() == State::Closed
            && visible(&target_control)?
            && text(&time)? == "Original time"
            && equal_optional(
                read(&target_control, &Field::DataContext.property()?)?.as_ref(),
                original.as_ref(),
            )?,
        "visibility conflict rolls back whole start",
    )?;
    set_visible(&target_control, false)?;
    checks.push("routing_visibility_start_conflict_rolls_back_clock_and_models");
    let mut session = routing::prepare(&plan, &views, owner.clone(), now)?;
    session.start(frame.clone(), now)?;
    set_visible(&target_control, false)?;
    require(
        session.poll(now + 1, true).is_err()
            && session.state() == State::Closed
            && !visible(&target_control)?
            && text(&time)? == "Original time",
        "active visibility conflict stops whole session",
    )?;
    checks.push("routing_active_visibility_conflict_stops_session");

    let mut empty = Session::new(owner.clone(), vec![], now)?;
    require(
        empty.start_tray(now).is_err() && empty.start(frame.clone(), now).is_err(),
        "empty session cannot activate",
    )?;
    checks.push("routing_empty_session_cannot_activate");
    checks.extend(binding_checks(owner.clone(), frame.clone())?);
    checks.extend(interactive_routing_checks(owner.clone(), frame.clone())?);
    checks.extend(runtime_guard_checks(owner.clone(), frame.clone())?);
    checks.extend(crate::window_guard::self_test()?);
    checks.extend(crate::notification_checks::run()?);
    checks.extend(visibility_checks(owner, frame)?);
    Ok(checks)
}
struct GuardFixture {
    source_view: View,
    target_view: View,
    routes: [Route; 2],
    source: ModelFixture,
    target: ModelFixture,
}
fn binding_checks(owner: Owner, frame: Frame) -> Result<Vec<&'static str>, String> {
    use crate::island::Binding;
    use windows_sys::Win32::{Foundation::HWND, UI::WindowsAndMessaging::*};
    let mut checks = Vec::new();
    {
        let fixture = GuardFixture::new(owner.clone(), frame.clone())?;
        let identity = fixture
            .source
            .source()?
            .query(&windows_sys::core::IID_IUnknown)?;
        let binding = Binding::capture(
            fixture.source.window(),
            &identity,
            &fixture.source_view.root_content()?,
        )?;
        binding.validate()?;
        require(
            binding.input(1, fixture.target.find("ClockText")?).is_err(),
            "foreign input element rejected",
        )?;
        checks.push("island_binding_normalizes_source_interface_and_verifies_root");
        require(
            BoundView::capture(
                fixture.routes[0].clone(),
                &fixture.source_view,
                fixture.target.source()?,
            )
            .is_err()
                && BoundView::capture(
                    fixture.routes[0].clone(),
                    &fixture.target_view,
                    fixture.source.source()?,
                )
                .is_err()
                && BoundView::capture(
                    fixture.routes[0].clone(),
                    &fixture.source_view,
                    &fixture.source.find("ClockText")?,
                )
                .is_err(),
            "wrong window, root and source interface rejected",
        )?;
        checks.push("island_binding_rejects_swapped_windows_roots_and_non_sources");
    }
    {
        let fixture = GuardFixture::new(owner.clone(), frame.clone())?;
        let mut session = fixture.session(owner.clone())?;
        let now = unsafe { GetTickCount64() };
        session.start(frame.clone(), now)?;
        let replacement = crate::clock::load(MARKUP)?;
        fixture.target.replace_content(&replacement)?;
        require(
            session.poll(now + 1, true).is_err()
                && session.state() == State::Closed
                && text(&fixture.target.find("TimeInnerTextBlock")?)? == "Original time",
            "source content replacement stops and restores captured tree",
        )?;
        checks.push("island_binding_content_replacement_restores_session");
    }
    {
        let fixture = GuardFixture::new(owner.clone(), frame.clone())?;
        let mut session = fixture.session(owner.clone())?;
        let now = unsafe { GetTickCount64() };
        session.start(frame.clone(), now)?;
        let island = fixture.target.input("ClockText", 1)?.island;
        let old_parent = unsafe { SetParent(island, fixture.source.window()) };
        require(old_parent == fixture.target.window(), "reparent own island")?;
        struct RestoreParent(HWND, HWND);
        impl Drop for RestoreParent {
            fn drop(&mut self) {
                unsafe {
                    SetParent(self.0, self.1);
                }
            }
        }
        let _restore = RestoreParent(island, old_parent);
        require(
            session.poll(now + 1, true).is_err()
                && session.state() == State::Closed
                && text(&fixture.target.find("TimeInnerTextBlock")?)? == "Original time",
            "same-thread reparented island stops before renewal",
        )?;
        checks.push("island_binding_reparented_native_window_stops_session");
    }
    {
        let fixture = GuardFixture::new(owner, frame)?;
        let binding = Binding::capture(
            fixture.source.window(),
            fixture.source.source()?,
            &fixture.source_view.root_content()?,
        )?;
        fixture.source.close_source()?;
        require(binding.validate().is_err(), "closed native source rejected")?;
        checks.push("island_binding_closed_source_cannot_be_reused");
    }
    Ok(checks)
}
impl GuardFixture {
    fn new(owner: Owner, frame: Frame) -> Result<Self, String> {
        let source = ModelFixture::new(owner.clone(), frame.clone(), MARKUP)?;
        let target = ModelFixture::new(owner, frame, MARKUP)?;
        let source_view = View::from_panel(&source.find("SystemTrayFrameGrid")?)?;
        let target_view = View::from_panel(&target.find("SystemTrayFrameGrid")?)?;
        let routes = [route(&source, false, "A"), route(&target, true, "B")];
        Ok(Self {
            source_view,
            target_view,
            routes,
            source,
            target,
        })
    }
    fn session(&self, owner: Owner) -> Result<Session, String> {
        let plan = Plan::build(&self.routes, &["B".into()], options())?;
        routing::prepare(
            &plan,
            &[
                BoundView::capture(
                    self.routes[0].clone(),
                    &self.source_view,
                    self.source.source()?,
                )?,
                BoundView::capture(
                    self.routes[1].clone(),
                    &self.target_view,
                    self.target.source()?,
                )?,
            ],
            owner,
            unsafe { GetTickCount64() },
        )
    }
}
fn set_content(node: &Object, value: &Object) -> Result<(), String> {
    let node = node.query(&IID_IContentControl)?;
    unsafe {
        check(
            (node.table::<IContentControl_Vtbl>().SetContent)(node.raw(), value.raw()),
            "Set guard fixture Content",
        )
    }
}
fn interactive_routing_checks(owner: Owner, frame: Frame) -> Result<Vec<&'static str>, String> {
    use std::{cell::RefCell, rc::Rc};
    use windows_sys::Win32::{Foundation::HWND, UI::WindowsAndMessaging::*};
    let fixture = GuardFixture::new(owner.clone(), frame.clone())?;
    let plan = Plan::build(&fixture.routes, &["B".into()], options())?;
    let views = [
        BoundView::capture(
            fixture.routes[0].clone(),
            &fixture.source_view,
            fixture.source.source()?,
        )?,
        BoundView::capture(
            fixture.routes[1].clone(),
            &fixture.target_view,
            fixture.target.source()?,
        )?,
    ];
    let events = Rc::new(RefCell::new(Vec::new()));
    let received = events.clone();
    let watchdog = routing::attach_interactive(
        &plan,
        &views,
        owner.clone(),
        unsafe { GetTickCount64() },
        move |event| {
            received.borrow_mut().push(event);
            true
        },
    )
    .map_err(|e| format!("initial interactive attach: {e}"))?;
    let probe = watchdog.input_probe(fixture.routes[1].window)?;
    require(
        !probe.invoke()?
            && events.borrow().is_empty()
            && watchdog.input_probe(fixture.routes[0].window).is_err(),
        "only selected clock is prepared and inactive",
    )?;
    watchdog.start(frame.clone())?;
    fixture.target.layout()?;
    require(
        probe.invoke()?,
        "verified selected clock publishes native activation",
    )?;
    {
        let received = events.borrow();
        let event = received.first().ok_or("missing verified activation")?;
        require(
            received.len() == 1
                && event.clock_id == fixture.routes[1].window
                && event.owner == owner
                && event.sequence == 1
                && unsafe { GetParent(event.source_window as HWND) } == fixture.target.window(),
            "activation uses exact source-provided island and selected route ID",
        )?;
        event.anchor.validate()?;
    }
    let mut checks = vec!["routing_verified_source_input_selects_only_clock_target"];

    let island = fixture.target.input("ClockText", 1)?.island;
    let old_parent = unsafe { SetParent(island, fixture.source.window()) };
    require(
        old_parent == fixture.target.window(),
        "reparent interactive island",
    )?;
    struct RestoreParent(HWND, HWND);
    impl Drop for RestoreParent {
        fn drop(&mut self) {
            unsafe {
                SetParent(self.0, self.1);
            }
        }
    }
    let restore = RestoreParent(island, old_parent);
    require(
        !probe.invoke()?
            && events.borrow().len() == 1
            && watchdog.state() == State::Closed
            && text(&fixture.target.find("TimeInnerTextBlock")?)? == "Original time",
        "stale source-window relation stops before publishing input",
    )?;
    drop(restore);
    drop(probe);
    drop(watchdog);
    checks.push("routing_verified_source_reparenting_blocks_input_and_restores");

    // Make the stale snapshot deterministic even on builds that instantiate
    // every template before the initial discovery rather than on first input.
    rename(&fixture.target.find("UnusedSurface")?, "AfterLayout")?;
    require(
        routing::attach_interactive(
            &plan,
            &views,
            owner.clone(),
            unsafe { GetTickCount64() },
            |_| false,
        )
        .is_err(),
        "layout changed the complete preparation snapshot; old view cannot restart",
    )?;
    let refreshed_source = View::from_panel(&fixture.source.find("SystemTrayFrameGrid")?)?;
    let refreshed_target = View::from_panel(&fixture.target.find("SystemTrayFrameGrid")?)?;
    let refreshed_views = [
        BoundView::capture(
            fixture.routes[0].clone(),
            &refreshed_source,
            fixture.source.source()?,
        )?,
        BoundView::capture(
            fixture.routes[1].clone(),
            &refreshed_target,
            fixture.target.source()?,
        )?,
    ];
    let watchdog = routing::attach_interactive(
        &plan,
        &refreshed_views,
        owner,
        unsafe { GetTickCount64() },
        |_| false,
    )
    .map_err(|e| format!("interactive reattach after layout and restoration: {e}"))?;
    checks.push("routing_restart_after_layout_requires_fresh_discovery");
    watchdog.start(frame)?;
    fixture.target.layout()?;
    let probe = watchdog.input_probe(fixture.routes[1].window)?;
    require(
        !probe.invoke()? && watchdog.state() == State::Active,
        "host rejection preserves default clock behavior",
    )?;
    watchdog.stop()?;
    require(
        !probe.invoke()? && text(&fixture.target.find("TimeInnerTextBlock")?)? == "Original time",
        "stopped route releases input and restores",
    )?;
    checks.push("routing_verified_input_rejection_and_stop_preserve_default");
    Ok(checks)
}
fn runtime_guard_checks(owner: Owner, frame: Frame) -> Result<Vec<&'static str>, String> {
    let mut checks = Vec::new();
    {
        use windows_core::{IInspectable, IUnknown, Interface};
        let fixture = ModelFixture::new(
            owner.clone(),
            frame.clone(),
            r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"><StackPanel x:Name="SystemTrayFrameGrid"><ItemsControl x:Name="NotificationAreaIcons"><x:String>Original</x:String></ItemsControl></StackPanel></Grid>"#,
        )?;
        let view = View::from_panel(&fixture.find("SystemTrayFrameGrid")?)?;
        let guard = view.lease_guard(
            &[litetray_adapter_protocol::topology::Role::NotificationIcons],
            false,
        )?;
        let items = fixture
            .find("NotificationAreaIcons")?
            .query(&IID_IItemsControl)?;
        let mut raw = std::ptr::null_mut();
        unsafe {
            check(
                (items.table::<IItemsControl_Vtbl>().Items)(items.raw(), &mut raw),
                "Guard test Items",
            )?;
        }
        let object = unsafe { Object::owned(raw) }?;
        let object_raw = object.raw();
        let vector: windows_collections::IVector<IInspectable> =
            unsafe { IUnknown::from_raw_borrowed(&object_raw) }
                .ok_or("Missing items")?
                .cast()
                .map_err(|e| e.to_string())?;
        let value = boxed_text("Added icon model")?;
        let value_raw = value.raw();
        let inspectable: IInspectable = unsafe { IUnknown::from_raw_borrowed(&value_raw) }
            .ok_or("Missing item")?
            .cast()
            .map_err(|e| e.to_string())?;
        vector.Append(&inspectable).map_err(|e| e.to_string())?;
        fixture.layout()?;
        guard.validate()?;
        require(
            vector.Size().map_err(|e| e.to_string())? == 2,
            "native item added",
        )?;
        vector.RemoveAt(0).map_err(|e| e.to_string())?;
        fixture.layout()?;
        guard.validate()?;
        checks.push("guard_native_collection_add_remove_preserves_session_structure");
    }
    let fixture = GuardFixture::new(owner.clone(), frame.clone())?;
    let control = fixture.target.find("ControlCenterButton")?;
    let time = fixture.target.find("TimeInnerTextBlock")?;
    let mut session = fixture.session(owner.clone())?;
    rename(&control, "ChangedBeforeStart")?;
    require(
        session
            .start(frame.clone(), unsafe { GetTickCount64() })
            .is_err()
            && session.state() == State::Closed
            && text(&time)? == "Original time"
            && !visible(&control)?,
        "stale structure rejected before first write",
    )?;
    rename(&control, "ControlCenterButton")?;
    checks.push("guard_structure_change_before_start_prevents_writes");

    let mut session = fixture.session(owner.clone())?;
    session.start(frame.clone(), unsafe { GetTickCount64() })?;
    // Real template child replacement below a model surface must not kill a
    // clock lease; collection/model refresh can naturally generate new visuals.
    let bell = fixture.target.find("Bell")?;
    set_content(
        &bell,
        &crate::clock::load(
            r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"><TextBlock Text="New"/><TextBlock Text="notification content"/></StackPanel>"#,
        )?,
    )?;
    fixture.target.layout()?;
    session.poll(unsafe { GetTickCount64() }, true)?;
    require(
        session.state() == State::Active && text(&time)? == frame.text.top,
        "unrelated generated visual changes do not invalidate guard",
    )?;
    session.stop()?;
    checks.push("guard_unrelated_template_children_can_change");

    let fixture = GuardFixture::new(owner.clone(), frame.clone())?;
    let time = fixture.target.find("TimeInnerTextBlock")?;
    let source_control = fixture.source.find("ControlCenterButton")?;
    let session = fixture.session(owner.clone())?;
    let watchdog = Watchdog::attach(session)?;
    watchdog.start(frame.clone())?;
    rename(&source_control, "ChangedSource")?;
    let command = serde_json::to_vec(&Command {
        version: VERSION,
        owner: owner.clone(),
        sequence: 1,
        change: Change::Heartbeat,
    })
    .map_err(|e| e.to_string())?;
    require(
        watchdog.receive(&command).is_err()
            && watchdog.state() == State::Closed
            && text(&time)? == "Original time",
        "source invalidation rejects heartbeat and restores selected target",
    )?;
    rename(&source_control, "ControlCenterButton")?;
    checks.push("guard_source_change_rejects_heartbeat_and_restores");

    let mut session = fixture.session(owner.clone())?;
    session.start(frame.clone(), unsafe { GetTickCount64() })?;
    rename(
        &fixture.target.find("UnusedSurface")?,
        "ControlCenterButton",
    )?;
    require(
        session.poll(unsafe { GetTickCount64() }, true).is_err()
            && session.state() == State::Closed
            && text(&time)? == "Original time",
        "new duplicate role invalidates active target even when old object remains",
    )?;
    checks.push("guard_new_duplicate_role_invalidates_active_session");

    let fixture = GuardFixture::new(owner.clone(), frame.clone())?;
    let session = fixture.session(owner.clone())?;
    let watchdog = Watchdog::attach(session)?;
    let published = std::rc::Rc::new(std::cell::Cell::new(0));
    let count = published.clone();
    watchdog.prepare_inputs(
        vec![
            fixture
                .target
                .input("ClockText", fixture.routes[1].window)?,
        ],
        move |_| {
            count.set(count.get() + 1);
            true
        },
    )?;
    watchdog.start(frame.clone())?;
    fixture.target.layout()?;
    let probe = watchdog.input_probe(fixture.routes[1].window)?;
    require(
        probe.invoke()? && published.get() == 1,
        "valid input still publishes",
    )?;
    rename(
        &fixture.source.find("ControlCenterButton")?,
        "SourceChangedBeforeClick",
    )?;
    require(
        !probe.invoke()?
            && published.get() == 1
            && watchdog.state() == State::Closed
            && text(&fixture.target.find("TimeInnerTextBlock")?)? == "Original time",
        "input-time guard stops stale session before publishing and preserves default input",
    )?;
    checks.push("guard_stale_input_stops_before_publish");

    let fixture = GuardFixture::new(owner.clone(), frame.clone())?;
    let old_time = fixture.target.find("TimeInnerTextBlock")?;
    let mut session = fixture.session(owner.clone())?;
    session.start(frame.clone(), unsafe { GetTickCount64() })?;
    let replacement = crate::clock::load(
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"><StackPanel x:Name="ClockText" Width="200" Height="60"><TextBlock x:Name="TimeInnerTextBlock" Text="Replacement time" Width="200" Height="30"/><TextBlock x:Name="DateInnerTextBlock" Text="Replacement date" Width="200" Height="30"/></StackPanel></Grid>"#,
    )?;
    set_content(
        &fixture.target.find("NotificationCenterButton")?,
        &replacement,
    )?;
    fixture.target.layout()?;
    require(
        session.poll(unsafe { GetTickCount64() }, true).is_err()
            && session.state() == State::Closed
            && text(&old_time)? == "Original time",
        "same-name replacement stops old session and restores retained original",
    )?;
    let fresh = View::from_panel(&fixture.target.find("SystemTrayFrameGrid")?)?;
    let mut new_session = Session::new(owner.clone(), vec![fresh.clock_target(1)?], unsafe {
        GetTickCount64()
    })?;
    new_session.start(frame.clone(), unsafe { GetTickCount64() })?;
    new_session.stop()?;
    let root = replacement.query(&IID_IFrameworkElement)?;
    let name = crate::xaml::StringHandle::new("TimeInnerTextBlock")?;
    let mut node = std::ptr::null_mut();
    unsafe {
        check(
            (root.table::<IFrameworkElement_Vtbl>().FindName)(root.raw(), name.0, &mut node),
            "Replacement clock lookup",
        )?;
    }
    require(
        text(&unsafe { Object::owned(node) }?)? == "Replacement time",
        "fresh session restores new object's own value",
    )?;
    checks.push("guard_same_name_replacement_requires_fresh_originals");

    let fixture = GuardFixture::new(owner.clone(), frame.clone())?;
    let time = fixture.target.find("TimeInnerTextBlock")?;
    let session = fixture.session(owner)?;
    let watchdog = Watchdog::attach(session)?;
    watchdog.start(frame)?;
    fixture.target.detach()?;
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW(
            watchdog.endpoint().ok_or("Missing watchdog")?,
            windows_sys::Win32::UI::WindowsAndMessaging::WM_TIMER,
            1,
            0,
        );
    }
    require(
        watchdog.state() == State::Closed
            && watchdog.error().is_some()
            && text(&time)? == "Original time",
        "watchdog timer restores detached target root",
    )?;
    checks.push("guard_native_watchdog_restores_detached_root");
    Ok(checks)
}
fn visibility_checks(owner: Owner, frame: Frame) -> Result<Vec<&'static str>, String> {
    let fixture = ModelFixture::new(
        owner.clone(),
        frame,
        r#"<StackPanel xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
          <Grid x:Name="Bound" DataContext="Collapsed" Visibility="{Binding}"/>
          <Grid x:Name="Local" Visibility="Collapsed"/><Grid x:Name="Unset"/>
        </StackPanel>"#,
    )?;
    let bound = fixture.find("Bound")?;
    let local = fixture.find("Local")?;
    let unset = fixture.find("Unset")?;
    let targets = || -> Result<Vec<Target>, String> {
        [&bound, &local, &unset]
            .into_iter()
            .map(|n| {
                Ok(Target {
                    element: n.query(&windows_sys::core::IID_IUnknown)?,
                    visible: true,
                })
            })
            .collect()
    };
    let mut lease = visibility::VisibilityLease::capture(targets()?)?;
    lease.start()?;
    require(
        visible(&bound)? && visible(&local)? && visible(&unset)?,
        "enum visibility writes",
    )?;
    lease.stop()?;
    require(
        !visible(&bound)? && !visible(&local)? && visible(&unset)?,
        "binding local unset restored",
    )?;
    set_context(&bound, "Visible")?;
    require(
        visible(&bound)?,
        "original visibility binding still updates",
    )?;
    set_context(&bound, "Collapsed")?;
    require(!visible(&bound)?, "binding toggles after restore")?;
    let original = crate::property::Property::capture(&unset, visibility::visibility_property()?)?;
    require(
        matches!(original.original, crate::property::Original::Unset),
        "unset restored as unset",
    )?;
    let mut checks = vec!["visibility_native_enum_binding_local_unset_restore"];
    {
        let mut lease = visibility::VisibilityLease::capture(targets()?)?;
        lease.start()?;
    }
    require(
        !visible(&bound)? && !visible(&local)? && visible(&unset)?,
        "drop restores native visibility",
    )?;
    checks.push("visibility_drop_restores_native_properties");
    let mut targets = targets()?;
    targets.push(Target {
        element: bound.query(&windows_sys::core::IID_IUnknown)?,
        visible: false,
    });
    require(
        visibility::VisibilityLease::capture(targets).is_err(),
        "duplicate visibility rejected",
    )?;
    checks.push("visibility_duplicate_targets_rejected");
    let mut session = Session::new(owner, vec![], 100)?;
    session.prepare_visibility(vec![Target {
        element: bound.query(&windows_sys::core::IID_IUnknown)?,
        visible: true,
    }])?;
    session.start_tray(100)?;
    session.poll(15100, true)?;
    require(
        session.state() == State::Closed && !visible(&bound)?,
        "visibility-only lease expiry restores binding",
    )?;
    checks.push("visibility_only_session_expiry_restores_binding");
    Ok(checks)
}
