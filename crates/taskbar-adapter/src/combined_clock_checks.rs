//! One real TextBlock owns both displayed lines. These controls are local to
//! the existing isolated XAML process, not Explorer's private clock classes.
use super::*;
use litetray_adapter_protocol::clock::{Alignment, Frame, Style, Text};

fn require(value: bool, detail: &str) -> Result<(), String> {
    if value {
        Ok(())
    } else {
        Err(format!("单控件时钟隔离检查失败：{detail}"))
    }
}
fn text(node: &Object) -> Result<String, String> {
    let node = node.query(&IID_ITextBlock)?;
    unsafe { node.string(node.table::<ITextBlock_Vtbl>().Text, 4096) }
}
fn set_text(node: &Object, value: &str) -> Result<(), String> {
    let node = node.query(&IID_ITextBlock)?;
    let value = StringHandle::new(value)?;
    unsafe {
        check(
            (node.table::<ITextBlock_Vtbl>().SetText)(node.raw(), value.0),
            "Native combined text refresh",
        )
    }
}
fn font(node: &Object) -> Result<f64, String> {
    let node = node.query(&IID_ITextBlock)?;
    let mut value = 0.;
    unsafe {
        check(
            (node.table::<ITextBlock_Vtbl>().FontSize)(node.raw(), &mut value),
            "Combined font",
        )?;
    }
    Ok(value)
}
fn width(node: &Object) -> Result<f64, String> {
    let node = node.query(&IID_IFrameworkElement)?;
    let mut value = 0.;
    unsafe {
        check(
            (node.table::<IFrameworkElement_Vtbl>().Width)(node.raw(), &mut value),
            "Combined width",
        )?;
    }
    Ok(value)
}
fn appearance(node: &Object) -> Result<([u8; 4], i32), String> {
    let statics = factory("Windows.UI.Xaml.Controls.TextBlock", &IID_ITextBlockStatics)?;
    let foreground = unsafe {
        property(
            &statics,
            statics.table::<ITextBlockStatics_Vtbl>().ForegroundProperty,
        )?
    };
    let brush = crate::tray::read(node, &foreground)?
        .ok_or("missing combined brush")?
        .query(&IID_ISolidColorBrush)?;
    let mut color: Color = unsafe { std::mem::zeroed() };
    let text = node.query(&IID_ITextBlock)?;
    let mut alignment = TextAlignment::Left;
    unsafe {
        check(
            (brush.table::<ISolidColorBrush_Vtbl>().Color)(brush.raw(), &mut color),
            "Combined color",
        )?;
        check(
            (text.table::<ITextBlock_Vtbl>().TextAlignment)(text.raw(), &mut alignment),
            "Combined alignment",
        )?;
    }
    Ok(([color.A, color.R, color.G, color.B], alignment.0))
}
pub(super) fn run() -> Result<Vec<&'static str>, String> {
    let original = "Native time\nNative date";
    let node = load(
        r##"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Text="Native time&#10;Native date" FontSize="15" Foreground="#FF112233" TextAlignment="Center"/>"##,
    )?;
    let container = load(
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" ToolTipService.ToolTip="Native combined tip"/>"#,
    )?;
    let before_handlers = crate::clock_text::TextLayer::handler_count();
    require(
        ClockLease::capture(&node, &node, &container).is_err(),
        "split lease still rejects aliased rows",
    )?;
    let mut frame = Frame {
        text: Text {
            top: "12:34:56".into(),
            bottom: "Weather 22C".into(),
            tooltip: Some("Combined tooltip".into()),
        },
        style: Style {
            font_family: "Segoe UI".into(),
            font_size: 12,
            width: 240,
            compact_two_lines: false,
            top_color: Some("#FF0000".into()),
            bottom_color: Some("#00FF00".into()),
            alignment: Alignment::Center,
            top_style: Default::default(),
            bottom_style: Default::default(),
        },
    };
    frame.style.top_style.font_size = Some(13);
    frame.style.top_style.alignment = Some(Alignment::Right);
    frame.style.bottom_style.font_size = Some(22);
    frame.style.bottom_style.alignment = Some(Alignment::Left);
    {
        let mut lease = ClockLease::capture_combined(&node, &container, true)?;
        require(
            lease.properties.len() == 4
                && lease.native_text.as_ref().unwrap().len() == 1
                && lease.identities.len() == 2
                && text(&node)? == original,
            "one original and one refresh subscriber per native property",
        )?;
        lease.update(&frame, true)?;
        require(
            text(&node)? == "12:34:56\nWeather 22C"
                && font(&node)? == 13.
                && width(&container)? == 240.
                && appearance(&node)? == ([255, 255, 0, 0], TextAlignment::Right.0),
            "two displayed lines share top-row native appearance",
        )?;
        set_text(&node, "Native 12:35\nNative new date")?;
        require(
            text(&node)? == "12:34:56\nWeather 22C",
            "native refresh preserves active combined text",
        )?;
        lease.tick("12:35:01", "Weather 23C")?;
        require(
            text(&node)? == "12:35:01\nWeather 23C",
            "combined second update",
        )?;
        frame.text.top = "12:35:02".into();
        frame.text.bottom.clear();
        frame.style.width = 0;
        lease.update(&frame, true)?;
        require(
            text(&node)? == "12:35:02" && width(&container)?.is_nan(),
            "empty second line and automatic width",
        )?;
        lease.restore()?;
        require(
            text(&node)? == "Native 12:35\nNative new date"
                && font(&node)? == 15.
                && width(&container)?.is_nan()
                && appearance(&node)? == ([255, 17, 34, 51], TextAlignment::Center.0)
                && lease.clean(),
            "restore latest native text and original single style",
        )?;
        lease.restore()?;
    }
    require(
        crate::clock_text::TextLayer::handler_count() == before_handlers,
        "single native callback released",
    )?;
    {
        let mut lease = ClockLease::capture_combined(&node, &container, true)?;
        lease.update(&frame, true)?;
        set_text(&node, "Newest native text")?;
        let native = node.query(&IID_ITextBlock)?;
        unsafe {
            check(
                (native.table::<ITextBlock_Vtbl>().SetFontSize)(native.raw(), 19.),
                "External font writer",
            )?;
        }
        require(
            lease.tick("later", "later").is_err()
                && text(&node)? == "Newest native text"
                && font(&node)? == 19.
                && lease.clean(),
            "external style conflict stops and preserves external value",
        )?;
    }
    let bound = load(
        r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Text="{Binding}" DataContext="Original combined binding"/>"#,
    )?;
    {
        let mut lease = ClockLease::capture_combined(&bound, &container, false)?;
        require(
            matches!(lease.properties[0].original, Original::Binding(_)),
            "combined binding captured once",
        )?;
        lease.apply("Top", "Bottom", 12., "Segoe UI", 280.)?;
        lease.tick("", "Only bottom")?;
        require(
            text(&bound)? == "Only bottom",
            "empty first line keeps second text",
        )?;
    }
    let context = bound.query(&IID_IFrameworkElement)?;
    let new_value = boxed_text("Restored combined binding reacts")?;
    unsafe {
        check(
            (context.table::<IFrameworkElement_Vtbl>().SetDataContext)(
                context.raw(),
                new_value.raw(),
            ),
            "Combined restored binding update",
        )?;
    }
    require(
        text(&bound)? == "Restored combined binding reacts" && width(&container)?.is_nan(),
        "combined drop restores native binding",
    )?;
    require(
        crate::clock_text::TextLayer::handler_count() == before_handlers,
        "all combined callbacks released",
    )?;
    Ok(vec![
        "combined_clock_owns_each_native_property_once",
        "combined_clock_updates_both_lines_with_one_shared_style",
        "combined_clock_retains_custom_text_during_native_refresh",
        "combined_clock_restores_latest_native_text_and_original_style",
        "combined_clock_external_style_conflict_restores_without_overwriting_writer",
        "combined_clock_empty_lines_auto_width_and_binding_drop_restore",
    ])
}
