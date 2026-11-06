//! Simulate native clock refreshes using real XAML setters and bindings.
//! No Explorer access or process hook is used by these checks.
use crate::{
    clock::{ClockLease, boxed_text, load},
    xaml::{Object, StringHandle, check},
    xaml_abi::*,
};
fn text(node: &Object) -> Result<String, String> {
    let node = node.query(&IID_ITextBlock)?;
    unsafe { node.string(node.table::<ITextBlock_Vtbl>().Text, 4096) }
}
fn require(value: bool, label: &str) -> Result<(), String> {
    if value {
        Ok(())
    } else {
        Err(format!("原生时钟刷新检查失败：{label}"))
    }
}
pub(crate) fn run() -> Result<Vec<&'static str>, String> {
    let baseline = crate::clock_text::TextLayer::handler_count();
    let time = load(
        r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Text="System time"/>"#,
    )?;
    let date = load(
        r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Text="{Binding}" DataContext="System date"/>"#,
    )?;
    let container =
        load(r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"/>"#)?;
    let mut lease = ClockLease::capture_native(&time, &date, &container)?;
    require(
        text(&time)? == "System time" && text(&date)? == "System date",
        "prepare leaves originals visible",
    )?;
    lease.apply("12:34:56", "多云 24°C", 12., "Segoe UI", 240.)?;
    require(
        text(&time)? == "12:34:56" && text(&date)? == "多云 24°C",
        "initial custom frame",
    )?;
    let tb = time.query(&IID_ITextBlock)?;
    let fe = date.query(&IID_IFrameworkElement)?;
    for n in 0..120 {
        unsafe {
            let value = StringHandle::new(&format!("System time {n}"))?;
            check(
                (tb.table::<ITextBlock_Vtbl>().SetText)(tb.raw(), value.0),
                "simulate native time refresh",
            )?;
            check(
                (fe.table::<IFrameworkElement_Vtbl>().SetDataContext)(
                    fe.raw(),
                    boxed_text(&format!("System date {n}"))?.raw(),
                ),
                "simulate native date binding refresh",
            )?;
        }
        require(
            text(&time)? == "12:34:56" && text(&date)? == "多云 24°C",
            &format!(
                "system refresh cannot overwrite displayed custom text at {n}: {:?} / {:?}",
                text(&time)?,
                text(&date)?
            ),
        )?;
    }
    lease.tick("12:35:00", "雨 23°C")?;
    require(
        text(&time)? == "12:35:00" && text(&date)? == "雨 23°C",
        "custom tick after native refresh",
    )?;
    lease.restore()?;
    require(
        text(&time)? == "System time 119" && text(&date)? == "System date 119" && lease.clean(),
        "stop reveals latest native values and live binding",
    )?;
    lease.restore()?;
    unsafe {
        check(
            (fe.table::<IFrameworkElement_Vtbl>().SetDataContext)(
                fe.raw(),
                boxed_text("After stop")?.raw(),
            ),
            "binding after stop",
        )?;
    }
    require(
        text(&date)? == "After stop",
        "binding survives repeated stop",
    )?;
    {
        let mut again = ClockLease::capture_native(&time, &date, &container)?;
        again.apply(
            "Second session",
            "Second session date",
            13.,
            "Segoe UI",
            260.,
        )?;
    }
    require(
        text(&time)? == "System time 119" && text(&date)? == "After stop",
        "drop restores native base values",
    )?;
    drop(lease);
    require(
        crate::clock_text::TextLayer::handler_count() == baseline,
        "all refresh callbacks released",
    )?;
    Ok(vec![
        "native_clock_refresh_text_priority",
        "native_clock_refresh_binding_priority",
        "native_clock_stop_reveals_latest_values",
        "native_clock_drop_releases_text_override",
    ])
}
