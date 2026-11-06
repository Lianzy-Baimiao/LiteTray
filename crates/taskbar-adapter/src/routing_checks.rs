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

const MARKUP: &str = r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
<StackPanel x:Name="SystemTrayFrameGrid" Width="520" Height="130">
  <ContentControl x:Name="NotificationAreaIcons" DataContext="Icons model" Content="Icons" Visibility="Collapsed"/>
  <ContentControl x:Name="NotifyIconStack" DataContext="Overflow model" Content="Overflow" Visibility="Collapsed"/>
  <ContentControl x:Name="MainStack" DataContext="System model" Content="System" Visibility="Collapsed"/>
  <ContentControl x:Name="NonActivatableStack" DataContext="Privacy model" Content="Privacy" Visibility="Collapsed"/>
  <ContentControl x:Name="ControlCenterButton" DataContext="Control center model" Content="Control center" Visibility="Collapsed"/>
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
        BoundView {
            route: routes[0].clone(),
            view: &a,
        },
        BoundView {
            route: routes[1].clone(),
            view: &b,
        },
        BoundView {
            route: routes[2].clone(),
            view: &c,
        },
    ];
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
    let mut checks =
        vec!["routing_selected_replica_native_apply_restore_and_unselected_preservation"];
    require(
        routing::prepare(&plan, &views[..1], owner.clone(), now).is_err(),
        "missing target binding rejected",
    )?;
    let mut changed = routes[1].clone();
    changed.device = "changed".into();
    let mismatched = [
        BoundView {
            route: routes[0].clone(),
            view: &a,
        },
        BoundView {
            route: changed,
            view: &b,
        },
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
    checks.extend(visibility_checks(owner, frame)?);
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
