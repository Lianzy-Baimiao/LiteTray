//! Real controls for the shared panel reader; not a claimed reproduction of
//! Explorer's private SystemTray runtime classes or shell menu behavior.
use crate::{
    clock::{boxed_text, load},
    discovery::View,
    preview::ModelFixture,
    property::equal_optional,
    session::{Session, State},
    tray::{Field, read},
    xaml::{Object, StringHandle, check},
    xaml_abi::*,
};
use litetray_adapter_protocol::{
    clock::{Frame, Owner},
    topology::Role,
};
use std::ptr::null_mut;
use windows_collections::IVector;
use windows_core::{IInspectable, IUnknown, Interface};
use windows_sys::core::IID_IUnknown;

const MARKUP: &str = r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
<StackPanel x:Name="SystemTrayFrameGrid" Width="520" Height="130">
  <ItemsControl x:Name="NotificationAreaIcons" ItemsSource="{Binding}" Visibility="Collapsed"/>
  <ContentControl x:Name="NotifyIconStack" DataContext="Hidden icons" Content="{Binding}" Visibility="Collapsed"/>
  <ContentControl x:Name="ControlCenterButton" DataContext="Network and volume" Content="{Binding}" Visibility="Collapsed"/>
  <ContentControl x:Name="NotificationCenterButton" MinHeight="80">
    <Grid><StackPanel x:Name="ClockText" Width="200" Height="60">
      <TextBlock x:Name="TimeInnerTextBlock" Text="Original time" Width="200" Height="30"/>
      <TextBlock x:Name="DateInnerTextBlock" Text="Original date" Width="200" Height="30"/>
    </StackPanel><Button x:Name="Bell" Content="Notification entry" HorizontalAlignment="Right"/></Grid>
  </ContentControl>
  <Grid x:Name="ShowDesktopStack" Height="10"/>
</StackPanel>
<ItemsControl x:Name="Collection" Visibility="Collapsed"><x:String>One</x:String><x:String>Two</x:String></ItemsControl>
</Grid>"#;
fn require(value: bool, label: &str) -> Result<(), String> {
    if value {
        Ok(())
    } else {
        Err(format!("托盘识别隔离检查失败：{label}"))
    }
}
fn set(element: &Object, field: Field, value: &Object) -> Result<(), String> {
    let element = element.query(&IID_IDependencyObject)?;
    unsafe {
        check(
            (element.table::<IDependencyObject_Vtbl>().SetValue)(
                element.raw(),
                field.property()?.raw(),
                value.raw(),
            ),
            "Set discovery fixture model",
        )
    }
}
fn rename(element: &Object, value: &str) -> Result<(), String> {
    let element = element.query(&IID_IFrameworkElement)?;
    let value = StringHandle::new(value)?;
    unsafe {
        check(
            (element.table::<IFrameworkElement_Vtbl>().SetName)(element.raw(), value.0),
            "Rename fixture node",
        )
    }
}
fn collection(fixture: &ModelFixture) -> Result<IVector<IInspectable>, String> {
    let node = fixture.find("Collection")?.query(&IID_IItemsControl)?;
    unsafe {
        let mut items = null_mut();
        check(
            (node.table::<IItemsControl_Vtbl>().Items)(node.raw(), &mut items),
            "Discovery fixture Items",
        )?;
        let items = Object::owned(items)?;
        let raw = items.raw();
        IUnknown::from_raw_borrowed(&raw)
            .ok_or("缺少集合")?
            .cast()
            .map_err(|e| e.to_string())
    }
}
fn prepare(fixture: &ModelFixture) -> Result<Object, String> {
    let vector = collection(fixture)?;
    let object = unsafe { Object::query_borrowed(vector.as_raw(), &IID_IUnknown) }?;
    set(
        &fixture.find("NotificationAreaIcons")?,
        Field::DataContext,
        &object,
    )?;
    fixture.layout()?;
    Ok(object)
}
fn text(node: &Object) -> Result<String, String> {
    let node = node.query(&IID_ITextBlock)?;
    unsafe { node.string(node.table::<ITextBlock_Vtbl>().Text, 4096) }
}
pub(crate) fn run(owner: Owner, frame: Frame) -> Result<Vec<&'static str>, String> {
    let source = ModelFixture::new(owner.clone(), frame.clone(), MARKUP)?;
    let target = ModelFixture::new(owner.clone(), frame.clone(), MARKUP)?;
    let source_model = prepare(&source)?;
    let target_model = prepare(&target)?;
    let source_panel = source.find("SystemTrayFrameGrid")?;
    let target_panel = target.find("SystemTrayFrameGrid")?;
    let a = View::from_panel(&source_panel)?;
    let b = View::from_panel(&target_panel)?;
    let clock = a
        .report
        .selection
        .as_ref()
        .and_then(|v| v.clock.as_ref())
        .ok_or_else(|| format!("夹具时钟未识别：{:?}", a.report))?;
    require(
        a.report.elements[clock.container].name == "ClockText"
            && a.report.elements[clock.time].name == "TimeInnerTextBlock"
            && a.report.elements[clock.date].name == "DateInnerTextBlock",
        "choose clock texts separately from notification bell",
    )?;
    require(
        View::discover(
            &crate::xaml::root_content(&source_panel)?,
            litetray_adapter_protocol::compatibility::Topology::SystemTrayFrameV1,
        )
        .is_err(),
        "fixture does not impersonate production frame",
    )?;
    let mut checks = vec![
        "discovery_scoped_native_panel_and_separate_clock_container",
        "discovery_requires_exact_production_frame",
    ];
    let roles = [
        Role::NotificationIcons,
        Role::HiddenIcons,
        Role::ControlCenter,
    ];
    let pairs = a.model_pairs_to(&b, &roles)?;
    require(pairs.len() == 3, "one plan per matching surface")?;
    let mut session = Session::new(
        owner.clone(),
        vec![a.clock_target(1)?, b.clock_target(2)?],
        100,
    )?;
    session.prepare_tray(pairs)?;
    let source_time = source.find("TimeInnerTextBlock")?;
    let target_time = target.find("TimeInnerTextBlock")?;
    require(
        text(&source_time)? == "Original time" && text(&target_time)? == "Original time",
        "discovery and capture do not write",
    )?;
    session.start(frame.clone(), 100)?;
    let target_icons = target.find("NotificationAreaIcons")?;
    require(
        text(&source_time)? == frame.text.top
            && text(&target_time)? == frame.text.top
            && equal_optional(
                read(&target_icons, &Field::ItemsSource.property()?)?.as_ref(),
                Some(&source_model),
            )?,
        "discovered nodes drive actual clock and model transaction",
    )?;
    session.stop()?;
    require(
        session.state() == State::Closed
            && text(&source_time)? == "Original time"
            && text(&target_time)? == "Original time"
            && equal_optional(
                read(&target_icons, &Field::ItemsSource.property()?)?.as_ref(),
                Some(&target_model),
            )?,
        "discovered targets restore native originals",
    )?;
    checks.push("discovery_to_session_native_apply_and_restore");
    require(
        a.model_pairs_to(&a, &roles).is_err()
            && a.model_pairs_to(&b, &[Role::NotificationCenter]).is_err()
            && a.model_pairs_to(&b, &[Role::NotificationIcons, Role::NotificationIcons])
                .is_err()
            && a.model_pairs_to(&b, &[Role::SystemIcons]).is_err(),
        "reject same root unsupported duplicate and absent roles",
    )?;
    checks.push("discovery_model_scope_validation");
    let outer = crate::xaml::root_content(&source_panel)?.query(&IID_IUIElement)?;
    unsafe {
        check(
            (outer.table::<IUIElement_Vtbl>().SetVisibility)(outer.raw(), Visibility::Collapsed),
            "Collapse outer fixture root",
        )?;
    }
    require(
        a.clock_target(1).is_err(),
        "collapsed ancestor outside panel suppresses clock",
    )?;
    unsafe {
        check(
            (outer.table::<IUIElement_Vtbl>().SetVisibility)(outer.raw(), Visibility::Visible),
            "Restore outer fixture root",
        )?;
    }
    source.layout()?;
    checks.push("discovery_outer_ancestor_visibility");
    let hidden = target.find("NotifyIconStack")?;
    rename(&hidden, "RenamedByAnotherComponent")?;
    require(
        b.clock_target(1).is_err() && a.model_pairs_to(&b, &roles).is_err(),
        "stale name snapshot rejected before any capture",
    )?;
    rename(&hidden, "NotifyIconStack")?;
    let control = target.find("ControlCenterButton")?;
    rename(&control, "NotificationAreaIcons")?;
    let duplicate = View::from_panel(&target_panel)?;
    require(
        duplicate.report.selection.is_none()
            && duplicate.report.error.is_some()
            && duplicate.clock_target(1).is_err(),
        "duplicate direct surface prevents activation",
    )?;
    rename(&control, "ControlCenterButton")?;
    rename(&target_icons, "TemporarilyUnselectedIcons")?;
    rename(&control, "NotificationAreaIcons")?;
    let mismatch = View::from_panel(&target_panel)?;
    require(
        a.model_pairs_to(&mismatch, &[Role::NotificationIcons])
            .is_err()
            && equal_optional(
                read(&target_icons, &Field::ItemsSource.property()?)?.as_ref(),
                Some(&target_model),
            )?,
        "same role name with wrong native interface fails before writes",
    )?;
    rename(&control, "ControlCenterButton")?;
    rename(&target_icons, "NotificationAreaIcons")?;
    checks.push("discovery_stale_names_and_duplicate_surfaces_rejected");
    // Replace the clock subtree with fresh objects carrying identical names.
    // Pointer identity must invalidate the old view even when text matches.
    let old = View::from_panel(&target_panel)?;
    let replacement = load(
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"><StackPanel x:Name="ClockText" Width="200" Height="60"><TextBlock x:Name="TimeInnerTextBlock" Text="Original time" Width="200" Height="30"/><TextBlock x:Name="DateInnerTextBlock" Text="Original date" Width="200" Height="30"/></StackPanel><Button x:Name="Bell" Content="Notification entry" HorizontalAlignment="Right"/></Grid>"#,
    )?;
    set(
        &target.find("NotificationCenterButton")?,
        Field::Content,
        &replacement,
    )?;
    target.layout()?;
    require(
        old.clock_target(1).is_err(),
        "identical names with new native identities rejected",
    )?;
    let fresh = View::from_panel(&target_panel)?;
    let mut session = Session::new(owner, vec![fresh.clock_target(1)?], 100)?;
    session.start(frame, 100)?;
    session.stop()?;
    checks.push("discovery_replaced_objects_require_fresh_capture");
    // A visually identical non-model Content never becomes a copied element.
    let source_control = source.find("ControlCenterButton")?;
    set(
        &source_control,
        Field::Content,
        &load(
            r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Text="visual"/>"#,
        )?,
    )?;
    set(
        &source_control,
        Field::DataContext,
        &boxed_text("Model stays shareable")?,
    )?;
    source.layout()?;
    let a = View::from_panel(&source_panel)?;
    let pairs = a.model_pairs_to(&fresh, &[Role::ControlCenter])?;
    require(
        pairs.len() == 1 && pairs[0].fields == [Field::DataContext],
        "skip visual content while retaining its separate data model",
    )?;
    checks.push("discovery_never_plans_visual_content_sharing");
    target.detach()?;
    require(
        fresh.clock_target(1).is_err() && a.model_pairs_to(&fresh, &[Role::ControlCenter]).is_err(),
        "detached XamlRoot cannot reuse retained objects",
    )?;
    checks.push("discovery_detached_root_rejected");
    Ok(checks)
}
