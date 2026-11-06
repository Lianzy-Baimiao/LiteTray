//! Real XAML model-sharing checks, exclusively inside the LiteTray process.
use crate::{
    clock::{boxed_text, load},
    preview::{InputFixture, ModelFixture},
    property::{Original, Property, equal_optional, same_object},
    session::{Session, State},
    tray::{Field, ModelLease, Pair, read},
    watchdog::Watchdog,
    xaml::{Object, check},
    xaml_abi::*,
};
use litetray_adapter_protocol::clock::{Frame, LEASE_MILLIS, Owner};
use std::ptr::null_mut;
use windows_collections::IVector;
use windows_core::{IInspectable, IUnknown, Interface};
use windows_sys::{
    Win32::{System::SystemInformation::GetTickCount64, UI::WindowsAndMessaging::*},
    core::IID_IUnknown,
};

const MARKUP: &str = r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
  <StackPanel x:Name="Parent" DataContext="Inherited context">
    <TextBlock x:Name="Context" Text="{Binding}"/>
    <ItemsControl x:Name="Items" ItemsSource="{Binding}"/>
    <ItemsControl x:Name="OtherItems" ItemsSource="{Binding}"/>
    <ContentControl x:Name="Content" Content="{Binding}" DataContext="Original content"/>
  </StackPanel>
  <ItemsControl x:Name="Collection" Visibility="Collapsed"><x:String>Alpha</x:String><x:String>Beta</x:String></ItemsControl>
  <ItemsControl x:Name="Replacement" Visibility="Collapsed"><x:String>Gamma</x:String></ItemsControl>
</Grid>"#;

fn require(value: bool, name: &str) -> Result<(), String> {
    if value {
        Ok(())
    } else {
        Err(format!("托盘模型检查失败：{name}"))
    }
}
fn model(element: &Object, field: Field) -> Result<Option<Object>, String> {
    read(element, &field.property()?)
}
fn set(element: &Object, field: Field, value: Option<&Object>) -> Result<(), String> {
    let element = element.query(&IID_IDependencyObject)?;
    let property = field.property()?;
    unsafe {
        check(
            (element.table::<IDependencyObject_Vtbl>().SetValue)(
                element.raw(),
                property.raw(),
                value.map_or(null_mut(), Object::raw),
            ),
            "External model change",
        )
    }
}
fn vector(element: &Object) -> Result<IVector<IInspectable>, String> {
    let element = element.query(&IID_IItemsControl)?;
    unsafe {
        let mut items = null_mut();
        check(
            (element.table::<IItemsControl_Vtbl>().Items)(element.raw(), &mut items),
            "Items collection",
        )?;
        let items = Object::owned(items)?;
        let raw = items.raw();
        IUnknown::from_raw_borrowed(&raw)
            .ok_or("缺少集合")?
            .cast()
            .map_err(|e| e.to_string())
    }
}
fn object<T: Interface>(value: &T) -> Result<Object, String> {
    unsafe { Object::query_borrowed(value.as_raw(), &IID_IUnknown) }
}
fn append(vector: &IVector<IInspectable>, value: &Object) -> Result<(), String> {
    let raw = value.raw();
    let inspectable: IInspectable = unsafe { IUnknown::from_raw_borrowed(&raw) }
        .ok_or("缺少模型")?
        .cast()
        .map_err(|e| e.to_string())?;
    vector.Append(&inspectable).map_err(|e| e.to_string())
}
fn count(element: &Object) -> Result<u32, String> {
    vector(element)?.Size().map_err(|e| e.to_string())
}
fn pair(source: &Object, target: &Object, fields: &[Field]) -> Result<Pair, String> {
    Ok(Pair {
        source: source.query(&IID_IUnknown)?,
        target: target.query(&IID_IUnknown)?,
        fields: fields.to_vec(),
    })
}
fn matches(element: &Object, field: Field, expected: Option<&Object>) -> Result<bool, String> {
    equal_optional(model(element, field)?.as_ref(), expected)
}
fn bind_from(source: &Object, target: &Object, field: Field) -> Result<(), String> {
    let property = field.property()?;
    let snapshot = Property::capture(source, field.property()?)?;
    let Original::Binding(binding) = snapshot.original else {
        return Err("检查来源缺少绑定".into());
    };
    let target = target.query(&IID_IFrameworkElement)?;
    unsafe {
        check(
            (target.table::<IFrameworkElement_Vtbl>().SetBinding)(
                target.raw(),
                property.raw(),
                binding.raw(),
            ),
            "External binding",
        )
    }
}

pub(crate) fn run(owner: Owner, frame: Frame) -> Result<Vec<&'static str>, String> {
    let source = ModelFixture::new(owner.clone(), frame.clone(), MARKUP)?;
    let target = ModelFixture::new(owner.clone(), frame.clone(), MARKUP)?;
    let context = source.find("Context")?;
    let items = source.find("Items")?;
    let content = source.find("Content")?;
    let target_context = target.find("Context")?;
    let target_items = target.find("Items")?;
    let target_other = target.find("OtherItems")?;
    let target_content = target.find("Content")?;
    // Populate DataContext explicitly so this fixture tests ItemsSource's
    // binding without depending on XamlReader's ElementName scope resolution.
    let source_collection = source.find("Collection")?;
    let target_collection = target.find("Collection")?;
    let source_vector = vector(&source_collection)?;
    let target_vector = vector(&target_collection)?;
    let source_model = object(&source_vector)?;
    let target_model = object(&target_vector)?;
    set(&items, Field::DataContext, Some(&source_model))?;
    set(&target_items, Field::DataContext, Some(&target_model))?;
    set(&target_other, Field::DataContext, Some(&target_model))?;
    let original_items = model(&target_items, Field::ItemsSource)?.ok_or("集合模型未建立")?;
    let original_other_items =
        model(&target_other, Field::ItemsSource)?.ok_or("第二集合模型未建立")?;
    let original_content = model(&target_content, Field::Content)?.ok_or("内容绑定未生效")?;
    let original_context =
        model(&target_context, Field::DataContext)?.ok_or("继承 DataContext 未生效")?;
    let source_items = model(&items, Field::ItemsSource)?.ok_or("来源集合模型未建立")?;
    let source_context = boxed_text("Primary context")?;
    let source_content = boxed_text("Primary content")?;
    set(&context, Field::DataContext, Some(&source_context))?;
    set(&content, Field::DataContext, Some(&source_content))?;
    let pairs = || -> Result<Vec<Pair>, String> {
        Ok(vec![
            pair(&context, &target_context, &[Field::DataContext])?,
            pair(&items, &target_items, &[Field::ItemsSource])?,
            pair(&items, &target_other, &[Field::ItemsSource])?,
            pair(&content, &target_content, &[Field::Content])?,
        ])
    };
    let restored = || -> Result<bool, String> {
        Ok(
            matches(&target_context, Field::DataContext, Some(&original_context))?
                && matches(&target_items, Field::ItemsSource, Some(&original_items))?
                && matches(
                    &target_other,
                    Field::ItemsSource,
                    Some(&original_other_items),
                )?
                && matches(&target_content, Field::Content, Some(&original_content))?,
        )
    };
    let mut checks = Vec::new();
    let mut lease = ModelLease::capture(pairs()?)?;
    require(restored()?, "capture does not write")?;
    lease.start()?;
    require(
        matches(&target_context, Field::DataContext, Some(&source_context))?
            && matches(&target_content, Field::Content, Some(&source_content))?
            && same_object(
                &model(&target_items, Field::ItemsSource)?.ok_or("missing target model")?,
                &source_items,
            )?
            && matches(&target_other, Field::ItemsSource, Some(&source_items))?
            && matches(&items, Field::ItemsSource, Some(&source_items))?,
        "share one native model without changing source",
    )?;
    checks.push("tray_native_models_across_islands");
    append(&source_vector, &boxed_text("Added after start")?)?;
    target.layout()?;
    require(
        count(&target_items)? == 3 && count(&target_other)? == 3,
        "collection notification propagates without refresh",
    )?;
    source_vector.RemoveAt(0).map_err(|e| e.to_string())?;
    require(
        count(&target_items)? == 2 && count(&target_other)? == 2,
        "collection removal propagates",
    )?;
    checks.push("tray_live_collection_add_remove");
    let replacement = vector(&source.find("Replacement")?)?;
    let replacement_object = object(&replacement)?;
    set(&items, Field::ItemsSource, Some(&replacement_object))?;
    let next_context = boxed_text("Replacement context")?;
    set(&context, Field::DataContext, Some(&next_context))?;
    set(&content, Field::Content, Some(&next_context))?;
    lease.refresh()?;
    require(
        count(&target_items)? == 1
            && count(&target_other)? == 1
            && matches(&target_context, Field::DataContext, Some(&next_context))?
            && matches(&target_content, Field::Content, Some(&next_context))?,
        "replace model references",
    )?;
    // The detached source must no longer update either replica.
    append(&source_vector, &boxed_text("Old collection only")?)?;
    require(
        count(&target_items)? == 1 && count(&target_other)? == 1,
        "old collection detached",
    )?;
    checks.push("tray_source_replacement_detaches_old_collection");
    for (element, field) in [
        (&items, Field::ItemsSource),
        (&context, Field::DataContext),
        (&content, Field::Content),
    ] {
        set(element, field, None)?;
    }
    lease.refresh()?;
    require(
        count(&target_items)? == 0
            && count(&target_other)? == 0
            && model(&target_context, Field::DataContext)?.is_none()
            && model(&target_content, Field::Content)?.is_none(),
        "null source clears stale replicas",
    )?;
    checks.push("tray_null_source_clears_replicas");
    lease.stop()?;
    lease.stop()?;
    require(
        restored()? && lease.refresh().is_err() && lease.start().is_err(),
        "restore and reject stale calls",
    )?;
    let new_original = boxed_text("Original binding is live")?;
    set(
        &target.find("Parent")?,
        Field::DataContext,
        Some(&new_original),
    )?;
    set(&target_content, Field::DataContext, Some(&new_original))?;
    append(&vector(&target.find("Collection")?)?, &new_original)?;
    require(
        matches(&target_context, Field::DataContext, Some(&new_original))?
            && matches(&target_content, Field::Content, Some(&new_original))?
            && count(&target_items)? == 3
            && count(&target_other)? == 3,
        "restore inherited context and live original bindings",
    )?;
    checks.push("tray_restore_bindings_inheritance_and_closed_state");
    set(&target_items, Field::DataContext, Some(&replacement_object))?;
    require(
        count(&target_items)? == 1
            && matches(&target_items, Field::ItemsSource, Some(&replacement_object))?,
        "restored ItemsSource binding follows replacement DataContext",
    )?;
    set(&target_items, Field::DataContext, Some(&target_model))?;
    set(
        &target.find("Parent")?,
        Field::DataContext,
        Some(&original_context),
    )?;
    set(&target_content, Field::DataContext, Some(&original_content))?;
    set(&context, Field::DataContext, Some(&source_context))?;
    set(&items, Field::ItemsSource, Some(&source_items))?;
    set(&content, Field::Content, Some(&source_content))?;
    {
        let mut lease = ModelLease::capture(pairs()?)?;
        lease.start()?;
    }
    require(restored()?, "drop restores group")?;
    checks.push("tray_drop_restores_group");
    let external = boxed_text("External content")?;
    let mut lease = ModelLease::capture(pairs()?)?;
    set(&target_content, Field::Content, Some(&external))?;
    require(
        lease.start().is_err()
            && matches(&target_content, Field::Content, Some(&external))?
            && matches(&target_context, Field::DataContext, Some(&original_context))?
            && matches(&target_items, Field::ItemsSource, Some(&original_items))?,
        "first write refuses stale snapshot",
    )?;
    checks.push("tray_capture_to_start_conflict_preserved");
    let long_a = boxed_text(&format!("{}a", "x".repeat(9000)))?;
    let long_b = boxed_text(&format!("{}b", "x".repeat(9000)))?;
    set(&target_content, Field::Content, Some(&long_a))?;
    let mut lease = ModelLease::capture(pairs()?)?;
    set(&target_content, Field::Content, Some(&long_b))?;
    require(
        lease.start().is_err() && matches(&target_content, Field::Content, Some(&long_b))?,
        "different long strings must not share truncated ownership",
    )?;
    let binding_source = load(
        r#"<ContentControl xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Content="{Binding}"/>"#,
    )?;
    bind_from(&binding_source, &target_content, Field::Content)?;
    let mut lease = ModelLease::capture(pairs()?)?;
    lease.start()?;
    set(&target_content, Field::DataContext, Some(&source_content))?;
    bind_from(&binding_source, &target_content, Field::Content)?;
    require(
        lease.refresh().is_err(),
        "same visible content with new external binding conflicts",
    )?;
    set(&target_content, Field::DataContext, Some(&external))?;
    require(
        matches(&target_content, Field::Content, Some(&external))?
            && matches(&target_items, Field::ItemsSource, Some(&original_items))?
            && matches(&target_context, Field::DataContext, Some(&original_context))?,
        "external binding survives group restore",
    )?;
    checks.push("tray_external_binding_preserved_and_group_restored");
    set(&target_content, Field::DataContext, Some(&original_content))?;
    let visual = load(
        r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Text="Visual stays in source"/>"#,
    )?;
    let mut lease = ModelLease::capture(pairs()?)?;
    lease.start()?;
    set(&content, Field::Content, Some(&visual))?;
    require(
        lease.refresh().is_err()
            && restored()?
            && matches(&content, Field::Content, Some(&visual))?,
        "late UIElement content stops before sharing",
    )?;
    checks.push("tray_visual_content_rejected_and_source_preserved");
    set(&content, Field::Content, Some(&source_content))?;
    set(&context, Field::DataContext, None)?;
    let mut lease = ModelLease::capture(pairs()?)?;
    require(
        lease.start().is_err() && restored()?,
        "initial missing source is not ready",
    )?;
    set(&context, Field::DataContext, Some(&source_context))?;
    checks.push("tray_empty_initial_source_preserves_target");
    let visuals = load(
        r#"<ItemsControl xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"><TextBlock Text="Do not share"/></ItemsControl>"#,
    )?;
    set(
        &items,
        Field::ItemsSource,
        Some(&object(&vector(&visuals)?)?),
    )?;
    let mut lease = ModelLease::capture(pairs()?)?;
    require(
        lease.start().is_err() && restored()?,
        "visual items rejected",
    )?;
    checks.push("tray_collection_of_visuals_rejected");
    let oversized = load(
        r#"<ItemsControl xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"/>"#,
    )?;
    let oversized_vector = vector(&oversized)?;
    let entry = boxed_text("bounded")?;
    for _ in 0..513 {
        append(&oversized_vector, &entry)?;
    }
    set(
        &items,
        Field::ItemsSource,
        Some(&object(&oversized_vector)?),
    )?;
    let mut lease = ModelLease::capture(pairs()?)?;
    require(
        lease.start().is_err() && restored()?,
        "bounded collection enumeration",
    )?;
    set(&items, Field::ItemsSource, Some(&source_items))?;
    checks.push("tray_oversized_collection_rejected");
    require(
        ModelLease::capture(vec![pair(&items, &items, &[Field::ItemsSource])?]).is_err()
            && ModelLease::capture(vec![
                pair(&items, &target_items, &[Field::ItemsSource])?,
                pair(&target_items, &items, &[Field::ItemsSource])?,
            ])
            .is_err()
            && ModelLease::capture(vec![
                pair(&items, &target_items, &[Field::ItemsSource])?,
                pair(&items, &target_items, &[Field::ItemsSource])?,
            ])
            .is_err()
            && ModelLease::capture(vec![pair(
                &items,
                &target_items,
                &[Field::ItemsSource, Field::ItemsSource],
            )?])
            .is_err()
            && ModelLease::capture(vec![pair(
                &context,
                &target_context,
                &[Field::ItemsSource],
            )?])
            .is_err(),
        "reject overlaps duplicates and wrong interfaces",
    )?;
    checks.push("tray_invalid_targets_rejected");

    // Updating DataContext re-evaluates an existing Content binding before
    // Content itself is written. Its binding identity must still be accepted.
    let both_source = load(
        r#"<ContentControl xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" DataContext="Shared" Content="{Binding}"/>"#,
    )?;
    let both_target = load(
        r#"<ContentControl xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" DataContext="Own" Content="{Binding}"/>"#,
    )?;
    let mut both = ModelLease::capture(vec![pair(
        &both_source,
        &both_target,
        &[Field::Content, Field::DataContext],
    )?])?;
    both.start()?;
    require(
        matches(&both_target, Field::Content, Some(&boxed_text("Shared")?))?,
        "data context and bound content share together",
    )?;
    both.stop()?;
    set(
        &both_target,
        Field::DataContext,
        Some(&boxed_text("Restored binding")?),
    )?;
    require(
        matches(
            &both_target,
            Field::Content,
            Some(&boxed_text("Restored binding")?),
        )?,
        "combined fields restore binding",
    )?;
    let null_target = load(
        r#"<ContentControl xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"/>"#,
    )?;
    set(&null_target, Field::Content, None)?;
    let mut null_lease =
        ModelLease::capture(vec![pair(&both_source, &null_target, &[Field::Content])?])?;
    null_lease.start()?;
    null_lease.stop()?;
    require(
        matches!(
            Property::capture(&null_target, Field::Content.property()?)?.original,
            Original::Local(None)
        ),
        "restore explicit null as null, not unset",
    )?;
    checks.push("tray_combined_fields_and_explicit_null_restore");

    let clock = InputFixture::new(owner.clone(), frame.clone())?;
    let new_session = || {
        Session::new(owner.clone(), vec![clock.capture(1)?], unsafe {
            GetTickCount64()
        })
    };
    let mut session = new_session()?;
    session.prepare_tray(pairs()?)?;
    require(
        session.prepare_tray(pairs()?).is_err(),
        "cannot replace snapshot",
    )?;
    let watchdog = Watchdog::attach(session)?;
    watchdog.prepare_inputs(vec![clock.input(1)?], |_| true)?;
    let input = watchdog.input_probe(1)?;
    watchdog.start(frame.clone())?;
    clock.layout()?;
    let geometry = clock.input(1)?;
    crate::input::bounds(&geometry.element, geometry.island)?;
    require(input.invoke()?, "input active before tray refresh")?;
    // Deliver the native timer to the existing endpoint: the same handler as
    // its one-second timer, without adding another long timeout to this suite.
    set(&items, Field::ItemsSource, Some(&replacement_object))?;
    unsafe {
        SendMessageW(
            watchdog.endpoint().ok_or("missing watchdog")?,
            WM_TIMER,
            1,
            0,
        );
    }
    require(count(&target_items)? == 1, "watchdog refreshes models")?;
    require(
        watchdog.state() == State::Active,
        "watchdog remains active after refresh",
    )?;
    require(input.invoke()?, "input resumes after tray refresh")?;
    set(&target_content, Field::Content, Some(&external))?;
    unsafe {
        SendMessageW(
            watchdog.endpoint().ok_or("missing watchdog")?,
            WM_TIMER,
            1,
            0,
        );
    }
    require(
        watchdog.state() == State::Closed
            && clock.restored()?
            && !input.invoke()?
            && matches(&target_content, Field::Content, Some(&external))?
            && matches(&target_items, Field::ItemsSource, Some(&original_items))?,
        "model conflict stops clocks and input",
    )?;
    checks.push("tray_watchdog_refresh_and_atomic_session_stop");
    drop(watchdog);
    drop(input);
    bind_from(&binding_source, &target_content, Field::Content)?;
    set(&items, Field::ItemsSource, Some(&source_items))?;
    let mut session = new_session()?;
    session.prepare_tray(pairs()?)?;
    let now = unsafe { GetTickCount64() };
    session.start(frame.clone(), now)?;
    session.poll(now + LEASE_MILLIS, true)?;
    require(
        session.state() == State::Closed && clock.restored()? && restored()?,
        "lease expiry restores both features",
    )?;
    checks.push("tray_session_expiry_restores_clock_and_models");
    let mut session = new_session()?;
    session.prepare_tray(pairs()?)?;
    set(&context, Field::DataContext, None)?;
    require(
        session.start(frame, unsafe { GetTickCount64() }).is_err()
            && session.state() == State::Closed
            && clock.restored()?
            && restored()?,
        "model start failure rolls back clock start",
    )?;
    checks.push("tray_failed_start_rolls_back_clock");
    let mut session = new_session()?;
    let clock_element = clock.input(1)?.element;
    require(
        session
            .prepare_tray(vec![pair(&context, &clock_element, &[Field::DataContext])?])
            .is_err(),
        "clock/tray overlap rejected before capture",
    )?;
    checks.push("tray_clock_overlap_rejected");
    Ok(checks)
}
