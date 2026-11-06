//! Real XAML property and session checks for optional appearance overrides.
use crate::{
    clock::{ClockLease, boxed_text, load},
    property::{property, same_object},
    tray::read,
    xaml::{Object, check, factory},
    xaml_abi::*,
};
use litetray_adapter_protocol::clock::{
    Alignment, Change, Command, Frame, LineStyle, Owner, Style, Text,
};
use std::ptr::null_mut;
fn require(value: bool, label: &str) -> Result<(), String> {
    if value {
        Ok(())
    } else {
        Err(format!("时钟样式隔离检查失败：{label}"))
    }
}
fn foreground(node: &Object) -> Result<Object, String> {
    let statics = factory("Windows.UI.Xaml.Controls.TextBlock", &IID_ITextBlockStatics)?;
    let prop = unsafe {
        property(
            &statics,
            statics.table::<ITextBlockStatics_Vtbl>().ForegroundProperty,
        )?
    };
    read(node, &prop)?.ok_or("缺少画刷".into())
}
fn color(node: &Object) -> Result<[u8; 4], String> {
    let brush = foreground(node)?.query(&IID_ISolidColorBrush)?;
    let mut color: Color = unsafe { std::mem::zeroed() };
    unsafe {
        check(
            (brush.table::<ISolidColorBrush_Vtbl>().Color)(brush.raw(), &mut color),
            "Read brush color",
        )?;
    }
    Ok([color.A, color.R, color.G, color.B])
}
fn alignment(node: &Object) -> Result<i32, String> {
    let node = node.query(&IID_ITextBlock)?;
    let mut value = TextAlignment::Left;
    unsafe {
        check(
            (node.table::<ITextBlock_Vtbl>().TextAlignment)(node.raw(), &mut value),
            "Read text alignment",
        )?;
    }
    Ok(value.0)
}
fn tip_property() -> Result<Object, String> {
    let statics = factory(
        "Windows.UI.Xaml.Controls.ToolTipService",
        &IID_IToolTipServiceStatics,
    )?;
    unsafe {
        property(
            &statics,
            statics
                .table::<IToolTipServiceStatics_Vtbl>()
                .ToolTipProperty,
        )
    }
}
fn tip(node: &Object) -> Result<String, String> {
    let value = read(node, &tip_property()?)?
        .ok_or("missing tooltip")?
        .query(&IID_IPropertyValue)?;
    unsafe { value.string(value.table::<IPropertyValue_Vtbl>().GetString, 4096) }
}
fn text(node: &Object) -> Result<String, String> {
    let node = node.query(&IID_ITextBlock)?;
    unsafe { node.string(node.table::<ITextBlock_Vtbl>().Text, 4096) }
}
fn context(node: &Object, value: &Object) -> Result<(), String> {
    let node = node.query(&IID_IFrameworkElement)?;
    unsafe {
        check(
            (node.table::<IFrameworkElement_Vtbl>().SetDataContext)(node.raw(), value.raw()),
            "Change bound style source",
        )
    }
}
pub(crate) fn run() -> Result<Vec<&'static str>, String> {
    let time = load(
        r##"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Text="Original time" Foreground="{Binding}" TextAlignment="Left"/>"##,
    )?;
    let date = load(
        r##"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Text="Original date" Foreground="#FF112233"/>"##,
    )?;
    let container = load(
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" ToolTipService.ToolTip="{Binding}" DataContext="Original tip"/>"#,
    )?;
    let original = load(
        r##"<SolidColorBrush xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Color="#FF445566"/>"##,
    )?;
    context(&time, &original)?;
    let mut frame = Frame {
        text: Text {
            top: "12:30".into(),
            bottom: "天气晴".into(),
            tooltip: Some("<Button Content=\"literal\"/>\n天气 80%".into()),
        },
        style: Style {
            font_family: "Segoe UI".into(),
            font_size: 12,
            width: 280,
            top_color: Some("#FF0000".into()),
            bottom_color: Some("#8000FF00".into()),
            alignment: Alignment::Right,
            top_style: Default::default(),
            bottom_style: Default::default(),
        },
    };
    let mut lease = ClockLease::capture(&time, &date, &container)?;
    lease.update(&frame, true)?;
    require(
        color(&time)? == [255, 255, 0, 0]
            && color(&date)? == [128, 0, 255, 0]
            && alignment(&time)? == TextAlignment::Right.0
            && alignment(&date)? == TextAlignment::Right.0
            && tip(&container)? == frame.text.tooltip.as_deref().unwrap(),
        "independent colors, exact enum and literal tooltip",
    )?;
    let mut checks = vec!["clock_style_colors_alignment_and_literal_tooltip"];
    let brush = foreground(&time)?;
    frame.text.tooltip = Some("Next second".into());
    lease.update(&frame, false)?;
    require(
        same_object(&brush, &foreground(&time)?)? && tip(&container)? == "Next second",
        "tooltip tick does not recreate brush",
    )?;
    checks.push("clock_style_tooltip_tick_preserves_brush_identity");
    let mut invalid = frame.clone();
    invalid.style.top_color = Some("Red\"/><Button/>".into());
    require(
        lease.update(&invalid, true).is_err()
            && color(&time)? == [255, 255, 0, 0]
            && tip(&container)? == "Next second",
        "invalid style leaves active frame intact",
    )?;
    checks.push("clock_style_invalid_color_rejected_before_changes");
    frame.style.top_color = None;
    frame.style.bottom_color = None;
    frame.style.alignment = Alignment::Native;
    frame.text.tooltip = None;
    lease.update(&frame, true)?;
    require(
        color(&time)? == [255, 68, 85, 102]
            && color(&date)? == [255, 17, 34, 51]
            && alignment(&time)? == TextAlignment::Left.0
            && tip(&container)? == "Original tip",
        "native restores original values and binding",
    )?;
    let new_original = load(
        r##"<SolidColorBrush xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Color="#FFABCDEF"/>"##,
    )?;
    context(&time, &new_original)?;
    context(&container, &boxed_text("Bound tip changed")?)?;
    lease.update(&frame, false)?;
    require(
        color(&time)? == [255, 171, 205, 239] && tip(&container)? == "Bound tip changed",
        "native keeps binding reactive",
    )?;
    frame.style.top_color = Some("#010203".into());
    frame.text.tooltip = Some("temporary".into());
    lease.update(&frame, true)?;
    lease.restore()?;
    require(
        color(&time)? == [255, 171, 205, 239]
            && tip(&container)? == "Bound tip changed"
            && text(&time)? == "Original time"
            && lease.clean(),
        "reenable captures current original and stop restores all",
    )?;
    checks.push("clock_style_native_restore_binding_and_reenable");
    // Once restored to Native, external replacement is valid and must become
    // the next override's original; stale capture must not resurrect a binding.
    let mut lease = ClockLease::capture(&time, &date, &container)?;
    lease.update(&frame, true)?;
    frame.text.tooltip = None;
    lease.update(&frame, false)?;
    let dep = container.query(&IID_IDependencyObject)?;
    unsafe {
        check(
            (dep.table::<IDependencyObject_Vtbl>().SetValue)(
                dep.raw(),
                tip_property()?.raw(),
                boxed_text("New native local tip")?.raw(),
            ),
            "Replace restored tooltip binding",
        )?;
    }
    lease.update(&frame, false)?;
    frame.text.tooltip = Some("temporary again".into());
    lease.update(&frame, false)?;
    lease.restore()?;
    context(&container, &boxed_text("Old binding must stay detached")?)?;
    require(
        tip(&container)? == "New native local tip",
        "reenabling captures replacement local value after Native",
    )?;
    checks.push("clock_style_native_external_replacement_becomes_new_original");
    let owner = Owner {
        nonce: 777,
        process_id: unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() },
        receiver: 1,
    };
    let mut session = crate::session::Session::new(
        owner.clone(),
        vec![crate::session::Target {
            id: 1,
            clock: ClockLease::capture(&time, &date, &container)?,
        }],
        100,
    )?;
    session.start(frame.clone(), 100)?;
    let container_dep = container.query(&IID_IDependencyObject)?;
    unsafe {
        check(
            (container_dep.table::<IDependencyObject_Vtbl>().SetValue)(
                container_dep.raw(),
                tip_property()?.raw(),
                boxed_text("External tip")?.raw(),
            ),
            "External tooltip writer",
        )?;
    }
    let command = litetray_adapter_protocol::clock::Command {
        version: litetray_adapter_protocol::VERSION,
        owner,
        sequence: 1,
        change: litetray_adapter_protocol::clock::Change::Tick {
            text: frame.text.clone(),
        },
    };
    require(
        session
            .receive(
                &serde_json::to_vec(&command).map_err(|e| e.to_string())?,
                101,
            )
            .is_err()
            && tip(&container)? == "External tip"
            && text(&time)? == "Original time"
            && color(&time)? == [255, 171, 205, 239]
            && session.state() == crate::session::State::Closed,
        "conflict preserves external tooltip and restores other properties",
    )?;
    checks.push("clock_style_external_tooltip_conflict_restores_owned_properties");
    {
        let mut lease = ClockLease::capture(&time, &date, &container)?;
        lease.update(&frame, true)?;
    }
    require(
        tip(&container)? == "External tip" && color(&time)? == [255, 171, 205, 239],
        "drop restores optional style",
    )?;
    checks.push("clock_style_drop_restores_optional_properties");
    // Verify unset is restored, not replaced with an effective default value.
    let statics = factory("Windows.UI.Xaml.Controls.TextBlock", &IID_ITextBlockStatics)?;
    let align = unsafe {
        property(
            &statics,
            statics
                .table::<ITextBlockStatics_Vtbl>()
                .TextAlignmentProperty,
        )?
    };
    let dep = date.query(&IID_IDependencyObject)?;
    let mut raw = null_mut();
    unsafe {
        check(
            (dep.table::<IDependencyObject_Vtbl>().ReadLocalValue)(
                dep.raw(),
                align.raw(),
                &mut raw,
            ),
            "Read restored unset alignment",
        )?;
    }
    let unset = unsafe { Object::owned(raw)? };
    let dp = factory(
        "Windows.UI.Xaml.DependencyProperty",
        &IID_IDependencyPropertyStatics,
    )?;
    let expected = unsafe {
        property(
            &dp,
            dp.table::<IDependencyPropertyStatics_Vtbl>().UnsetValue,
        )?
    };
    require(
        same_object(&unset, &expected)?,
        "unset alignment remains unset",
    )?;
    checks.push("clock_style_unset_alignment_restored");
    checks.extend(line_styles()?);
    Ok(checks)
}

fn font_object(node: &Object) -> Result<Object, String> {
    let statics = factory("Windows.UI.Xaml.Controls.TextBlock", &IID_ITextBlockStatics)?;
    let prop = unsafe {
        property(
            &statics,
            statics.table::<ITextBlockStatics_Vtbl>().FontFamilyProperty,
        )?
    };
    read(node, &prop)?.ok_or("missing font family".into())
}
fn font(node: &Object) -> Result<(String, f64), String> {
    let family = font_object(node)?.query(&IID_IFontFamily)?;
    let node = node.query(&IID_ITextBlock)?;
    unsafe {
        let source = family.string(family.table::<IFontFamily_Vtbl>().Source, 128)?;
        let mut size = 0.;
        check(
            (node.table::<ITextBlock_Vtbl>().FontSize)(node.raw(), &mut size),
            "Read font size",
        )?;
        Ok((source, size))
    }
}
fn line_styles() -> Result<Vec<&'static str>, String> {
    let time = load(
        r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Text="Original time" FontFamily="{Binding FontFamily}" FontSize="{Binding FontSize}" TextAlignment="{Binding TextAlignment}"/>"#,
    )?;
    let date = load(
        r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Text="Original date" FontFamily="Times New Roman" FontSize="16" TextAlignment="Right"/>"#,
    )?;
    let container =
        load(r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"/>"#)?;
    let source = load(
        r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" FontFamily="Segoe UI" FontSize="11" TextAlignment="Left"/>"#,
    )?;
    context(&time, &source)?;
    require(
        font(&time)? == ("Segoe UI".into(), 11.),
        "initial font bindings",
    )?;
    let owner = Owner {
        nonce: 778,
        process_id: unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() },
        receiver: 1,
    };
    let mut session = crate::session::Session::new(
        owner.clone(),
        vec![crate::session::Target {
            id: 1,
            clock: ClockLease::capture(&time, &date, &container)?,
        }],
        100,
    )?;
    let mut frame = Frame {
        text: Text {
            top: "12:30".into(),
            bottom: "天气".into(),
            tooltip: None,
        },
        style: Style {
            font_family: "Courier New".into(),
            font_size: 12,
            width: 280,
            top_color: None,
            bottom_color: None,
            alignment: Alignment::Right,
            top_style: LineStyle {
                font_family: Some("Consolas".into()),
                font_size: Some(18),
                alignment: Some(Alignment::Center),
            },
            bottom_style: LineStyle {
                font_family: Some("Georgia".into()),
                font_size: Some(10),
                alignment: Some(Alignment::Left),
            },
        },
    };
    session.start(frame.clone(), 100)?;
    require(
        font(&time)? == ("Consolas".into(), 18.)
            && font(&date)? == ("Georgia".into(), 10.)
            && alignment(&time)? == TextAlignment::Center.0
            && alignment(&date)? == TextAlignment::Left.0,
        "independent fonts sizes and alignment",
    )?;
    let mut checks = vec!["clock_line_styles_distinct_native_properties"];
    let send = |session: &mut crate::session::Session, sequence, change| {
        let command = Command {
            version: litetray_adapter_protocol::VERSION,
            owner: owner.clone(),
            sequence,
            change,
        };
        session.receive(
            &serde_json::to_vec(&command).map_err(|e| e.to_string())?,
            100 + sequence,
        )
    };
    let before = [font_object(&time)?, font_object(&date)?];
    frame.text.top = "12:31".into();
    send(
        &mut session,
        1,
        Change::Tick {
            text: frame.text.clone(),
        },
    )?;
    require(
        text(&time)? == "12:31"
            && same_object(&before[0], &font_object(&time)?)?
            && same_object(&before[1], &font_object(&date)?)?,
        "tick preserves line fonts",
    )?;
    checks.push("clock_line_styles_tick_preserves_font_objects");
    frame.style.top_style = LineStyle {
        alignment: Some(Alignment::Native),
        ..Default::default()
    };
    frame.style.bottom_style = LineStyle::default();
    // Only per-line fields changed; Session must detect this as a style update.
    send(
        &mut session,
        2,
        Change::Update {
            frame: frame.clone(),
        },
    )?;
    require(
        font(&time)? == ("Courier New".into(), 12.)
            && font(&date)? == ("Courier New".into(), 12.)
            && alignment(&time)? == TextAlignment::Left.0
            && alignment(&date)? == TextAlignment::Right.0,
        "cleared line values inherit common and explicit native restores binding",
    )?;
    checks.push("clock_line_styles_update_inheritance_and_native_alignment");
    let next = load(
        r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" FontFamily="Arial" FontSize="15" TextAlignment="Center"/>"#,
    )?;
    context(&time, &next)?;
    let mut invalid = frame.clone();
    invalid.style.bottom_style.font_size = Some(33);
    require(
        send(&mut session, 3, Change::Update { frame: invalid }).is_err()
            && session.state() == crate::session::State::Active
            && font(&time)? == ("Courier New".into(), 12.)
            && font(&date)? == ("Courier New".into(), 12.)
            && alignment(&time)? == TextAlignment::Center.0,
        "invalid single line does not mutate active frame or detach native alignment",
    )?;
    checks.push("clock_line_styles_invalid_update_keeps_active_frame");
    send(&mut session, 3, Change::Stop)?;
    require(
        session.state() == crate::session::State::Closed
            && font(&time)? == ("Arial".into(), 15.)
            && font(&date)? == ("Times New Roman".into(), 16.)
            && text(&time)? == "Original time"
            && text(&date)? == "Original date",
        "stop restores current binding values and original local fonts",
    )?;
    context(&time, &source)?;
    require(
        font(&time)? == ("Segoe UI".into(), 11.) && alignment(&time)? == TextAlignment::Left.0,
        "font and alignment bindings remain reactive after stop",
    )?;
    checks.push("clock_line_styles_stop_restores_reactive_bindings_and_local_fonts");
    Ok(checks)
}
