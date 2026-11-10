//! Reversible clock properties. Capture every original value before writing
//! any target. Keep bindings as bindings and unset values as unset values.
use crate::{
    property::{Original, Property, property, same_object},
    xaml::{Object, StringHandle, check, factory},
    xaml_abi::*,
};
use std::{borrow::Cow, ptr::null_mut};
use windows_sys::core::IID_IUnknown;
#[path = "combined_clock_checks.rs"]
mod combined_checks;

pub(crate) fn boxed_text(text: &str) -> Result<Object, String> {
    unsafe {
        let text = StringHandle::new(text)?;
        let factory = factory(
            "Windows.Foundation.PropertyValue",
            &IID_IPropertyValueStatics,
        )?;
        let mut value = null_mut();
        check(
            (factory.table::<IPropertyValueStatics_Vtbl>().CreateString)(
                factory.raw(),
                text.0,
                &mut value,
            ),
            "PropertyValue.CreateString",
        )?;
        Object::owned(value)
    }
}
fn boxed_number(number: f64) -> Result<Object, String> {
    unsafe {
        let factory = factory(
            "Windows.Foundation.PropertyValue",
            &IID_IPropertyValueStatics,
        )?;
        let mut value = null_mut();
        check(
            (factory.table::<IPropertyValueStatics_Vtbl>().CreateDouble)(
                factory.raw(),
                number,
                &mut value,
            ),
            "PropertyValue.CreateDouble",
        )?;
        Object::owned(value)
    }
}
fn font_family(name: &str) -> Result<Object, String> {
    unsafe {
        let name = StringHandle::new(name)?;
        let factory = factory("Windows.UI.Xaml.Media.FontFamily", &IID_IFontFamilyFactory)?;
        let mut inner = null_mut();
        let mut value = null_mut();
        let hr = (factory
            .table::<IFontFamilyFactory_Vtbl>()
            .CreateInstanceWithName)(
            factory.raw(), name.0, null_mut(), &mut inner, &mut value
        );
        if !inner.is_null() {
            drop(Object::owned(inner)?);
        }
        check(hr, "FontFamily.CreateInstanceWithName")?;
        Object::owned(value)
    }
}

/// Holds references to the selected native TextBlocks and their original
/// properties. Must be restored and dropped on the same XAML thread.
pub(crate) struct ClockLease {
    properties: Vec<Property>,
    live: bool,
    identities: Vec<usize>,
    last_text: Option<(String, String)>,
    decorations: crate::clock_style::Decorations,
    native_text: Option<Vec<crate::clock_text::TextLayer>>,
    native_date: Option<Object>,
    compact_two_lines: bool,
    combined: bool,
}
impl ClockLease {
    pub(crate) fn capture_native(
        time: &Object,
        date: &Object,
        container: &Object,
    ) -> Result<Self, String> {
        let mut lease = Self::capture(time, date, container)?;
        lease.native_text = Some(vec![
            crate::clock_text::TextLayer::prepare(time)?,
            crate::clock_text::TextLayer::prepare(date)?,
        ]);
        lease.native_date = Some(date.clone());
        Ok(lease)
    }
    pub fn capture(time: &Object, date: &Object, container: &Object) -> Result<Self, String> {
        if same_object(time, date)? {
            return Err("时间和日期元素不能相同".into());
        }
        Self::capture_rows(&[time, date], container)
    }
    /// The caller must have selected the exact single-TextBlock topology.
    /// One property snapshot and one native refresh subscription own its text.
    pub(crate) fn capture_combined(
        text: &Object,
        container: &Object,
        native: bool,
    ) -> Result<Self, String> {
        let mut lease = Self::capture_rows(&[text], container)?;
        if native {
            lease.native_text = Some(vec![crate::clock_text::TextLayer::prepare(text)?]);
        }
        Ok(lease)
    }
    fn capture_rows(rows: &[&Object], container: &Object) -> Result<Self, String> {
        unsafe {
            for row in rows {
                if same_object(row, container)? {
                    return Err("时钟文字与布局容器不能相同".into());
                }
                row.query(&IID_ITextBlock)?;
            }
            let text = factory("Windows.UI.Xaml.Controls.TextBlock", &IID_ITextBlockStatics)?;
            let fe = factory(
                "Windows.UI.Xaml.FrameworkElement",
                &IID_IFrameworkElementStatics,
            )?;
            let text_table = text.table::<ITextBlockStatics_Vtbl>();
            let mut properties = Vec::with_capacity(7);
            for getter in [
                text_table.TextProperty,
                text_table.FontSizeProperty,
                text_table.FontFamilyProperty,
            ] {
                let property = property(&text, getter)?;
                for row in rows {
                    properties.push(Property::capture(row, property.clone())?);
                }
            }
            properties.push(Property::capture(
                container,
                property(
                    &fe,
                    fe.table::<IFrameworkElementStatics_Vtbl>().WidthProperty,
                )?,
            )?);
            let mut identities = rows
                .iter()
                .map(|row| Ok(row.query(&IID_IUnknown)?.raw() as usize))
                .collect::<Result<Vec<_>, String>>()?;
            identities.push(container.query(&IID_IUnknown)?.raw() as usize);
            Ok(Self {
                properties,
                live: false,
                identities,
                last_text: None,
                decorations: if rows.len() == 1 {
                    crate::clock_style::Decorations::capture_combined(rows[0], container)?
                } else {
                    crate::clock_style::Decorations::capture(rows[0], rows[1], container)?
                },
                native_text: None,
                native_date: None,
                compact_two_lines: false,
                combined: rows.len() == 1,
            })
        }
    }
    fn row_count(&self) -> usize {
        if self.combined { 1 } else { 2 }
    }
    pub(crate) fn with_tooltip_targets(
        mut self,
        primary: &Object,
        targets: &[Object],
    ) -> Result<Self, String> {
        self.decorations.configure_tooltips(primary, targets)?;
        for target in targets.iter().chain(std::iter::once(primary)) {
            let identity = target.query(&IID_IUnknown)?.raw() as usize;
            if !self.identities.contains(&identity) {
                self.identities.push(identity);
            }
        }
        Ok(self)
    }
    pub(crate) fn update(
        &mut self,
        frame: &litetray_adapter_protocol::clock::Frame,
        style_changed: bool,
    ) -> Result<(), String> {
        frame.validate()?;
        self.compact_two_lines = frame.style.compact_two_lines;
        let result = (|| {
            let prepared = self.decorations.prepare(frame)?;
            if style_changed {
                let style = &frame.style;
                self.apply_lines(
                    &frame.text.top,
                    &frame.text.bottom,
                    [&style.top_style, &style.bottom_style].map(|line| {
                        (
                            line.font_size.unwrap_or(style.font_size) as f64,
                            line.font_family.as_deref().unwrap_or(&style.font_family),
                        )
                    }),
                    style.width as f64,
                )?;
            } else {
                self.tick(&frame.text.top, &frame.text.bottom)?;
            }
            self.decorations.apply(prepared)
        })();
        if let Err(error) = result {
            return match self.restore() {
                Ok(()) => Err(error),
                Err(restore) => Err(format!("{error}；恢复未完成：{restore}")),
            };
        }
        Ok(())
    }
    pub fn apply(
        &mut self,
        top: &str,
        bottom: &str,
        size: f64,
        family: &str,
        width: f64,
    ) -> Result<(), String> {
        self.apply_lines(top, bottom, [(size, family); 2], width)
    }
    fn apply_lines(
        &mut self,
        top: &str,
        bottom: &str,
        fonts: [(f64, &str); 2],
        width: f64,
    ) -> Result<(), String> {
        if fonts.iter().any(|(size, family)| {
            !(8.0..=32.0).contains(size)
                || litetray_adapter_protocol::clock::validate_font_family(family).is_err()
        }) || width != 0.0 && !(80.0..=600.0).contains(&width)
            || top.len() > 4096
            || bottom.len() > 4096
            || top.contains('\0')
            || bottom.contains('\0')
        {
            return Err("时钟显示参数无效".into());
        }
        // Construct every value before writing. Failure here leaves the active
        // clock untouched and keeps the original rollback snapshot.
        let displayed_top = self.display_top(top, bottom)?;
        let top = displayed_top.as_ref();
        let row_count = self.row_count();
        let mut values = [top, bottom][..row_count]
            .iter()
            .map(|text| boxed_text(text))
            .collect::<Result<Vec<_>, _>>()?;
        for (size, _) in &fonts[..row_count] {
            values.push(boxed_number(*size)?);
        }
        for (_, family) in &fonts[..row_count] {
            values.push(font_family(family)?);
        }
        // FrameworkElement.Width = NaN is native Auto. Text changes then
        // invalidate layout themselves; no width polling or text estimates.
        values.push(boxed_number(if width == 0.0 { f64::NAN } else { width })?);
        self.write(&values.iter().collect::<Vec<_>>())?;
        if let Err(error) = self.write_native_text(top, bottom) {
            let restore = self.restore();
            return restore.and(Err(error));
        }
        self.live = true;
        self.last_text = Some((top.to_owned(), bottom.to_owned()));
        Ok(())
    }
    fn write(&mut self, values: &[&Object]) -> Result<(), String> {
        self.verify_ownership()?;
        for (i, value) in values
            .iter()
            .enumerate()
            .skip(if self.native_text.is_some() {
                self.row_count()
            } else {
                0
            })
        {
            if let Err(error) = self.properties[i].set(value) {
                return match self.restore() {
                    Ok(()) => Err(error),
                    Err(restore) => Err(format!("{error}；恢复未完成：{restore}")),
                };
            }
        }
        Ok(())
    }
    pub fn tick(&mut self, top: &str, bottom: &str) -> Result<(), String> {
        if !self.live {
            return Err("时钟尚未接管".into());
        }
        if top.len() > 4096 || bottom.len() > 4096 {
            return Err("时钟文本过长".into());
        }
        self.verify_ownership()?;
        let displayed_top = self.display_top(top, bottom)?;
        let top = displayed_top.as_ref();
        if self
            .last_text
            .as_ref()
            .is_some_and(|old| old.0 == top && old.1 == bottom)
        {
            return Ok(());
        }
        if self.native_text.is_some() {
            if let Err(error) = self.write_native_text(top, bottom) {
                let restore = self.restore();
                return restore.and(Err(error));
            }
            self.last_text = Some((top.to_owned(), bottom.to_owned()));
            return Ok(());
        }
        let values = [boxed_text(top)?, boxed_text(bottom)?];
        for (i, (text, value)) in [(top, &values[0]), (bottom, &values[1])]
            .into_iter()
            .take(self.row_count())
            .enumerate()
        {
            let same = self
                .last_text
                .as_ref()
                .is_some_and(|old| if i == 0 { old.0 == text } else { old.1 == text });
            if !same && let Err(error) = self.properties[i].set(value) {
                return match self.restore() {
                    Ok(()) => Err(error),
                    Err(restore) => Err(format!("{error}；恢复未完成：{restore}")),
                };
            }
        }
        self.last_text = Some((top.to_owned(), bottom.to_owned()));
        Ok(())
    }
    fn display_top<'a>(&self, top: &'a str, bottom: &str) -> Result<Cow<'a, str>, String> {
        if self.combined {
            return Ok(Cow::Owned(
                [top, bottom]
                    .into_iter()
                    .filter(|line| !line.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ));
        }
        let Some(date) = &self.native_date else {
            return Ok(Cow::Borrowed(top));
        };
        match crate::xaml::visibility(date)? {
            Some(true) => Ok(Cow::Borrowed(top)),
            Some(false) => {
                // Shell deliberately collapses the date in compact taskbars.
                // Keep its layout/visibility untouched and show both configured
                // lines in the remaining text row. Read this on each tick so a
                // size change works even when the formatted text is unchanged.
                let lines = [top.trim(), bottom.trim()];
                Ok(Cow::Owned(
                    lines
                        .into_iter()
                        .filter(|line| !line.is_empty())
                        .map(|line| line.replace(['\r', '\n'], " "))
                        .collect::<Vec<_>>()
                        .join(if self.compact_two_lines { "\n" } else { " · " }),
                ))
            }
            None => Err("原生日期行可见性尚未就绪".into()),
        }
    }
    pub fn restore(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        if let Some(layers) = &mut self.native_text {
            for layer in layers {
                if let Err(error) = layer.stop() {
                    errors.push(error);
                }
            }
        }
        if let Err(error) = self.decorations.restore() {
            errors.push(error);
        }
        for property in self.properties.iter_mut().rev() {
            if let Err(error) = property.restore() {
                errors.push(error);
            }
        }
        self.live = false;
        self.last_text = None;
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("；"))
        }
    }
    pub fn overlaps(&self, other: &Self) -> bool {
        self.identities
            .iter()
            .any(|id| other.identities.contains(id))
    }
    pub(crate) fn references(&self, object: &Object) -> Result<bool, String> {
        Ok(self
            .identities
            .contains(&(object.query(&IID_IUnknown)?.raw() as usize)))
    }
    pub fn clean(&self) -> bool {
        self.properties.iter().all(|p| !p.dirty)
            && self.decorations.clean()
            && self
                .native_text
                .as_ref()
                .is_none_or(|layers| layers.iter().all(|layer| layer.clean()))
    }
    fn write_native_text(&mut self, top: &str, bottom: &str) -> Result<(), String> {
        if let Some(layers) = &mut self.native_text {
            layers[0].set(top)?;
            if let Some(date) = layers.get_mut(1) {
                date.set(bottom)?;
            }
        }
        Ok(())
    }
    pub(crate) fn verify_ownership(&mut self) -> Result<(), String> {
        let mut failure = self.decorations.validate().err();
        if let Some(layers) = &self.native_text {
            for layer in layers {
                if let Err(error) = layer.validate() {
                    failure = Some(error);
                }
            }
        }
        for property in self.properties.iter().skip(if self.native_text.is_some() {
            self.row_count()
        } else {
            0
        }) {
            match property.owns_current() {
                Ok(true) => {}
                Ok(false) => {
                    failure = Some("时钟属性已被其他组件修改，停止接管".to_string());
                    break;
                }
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        if let Some(error) = failure {
            return match self.restore() {
                Ok(()) => Err(error),
                Err(restore) => Err(format!("{error}；{restore}")),
            };
        }
        Ok(())
    }
}
impl Drop for ClockLease {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

/// Runs only inside the calling process on an STA thread, using fresh XAML
/// controls. Does not find Explorer, install hooks or touch a taskbar.
pub(crate) fn self_test() -> Result<serde_json::Value, String> {
    windows_link::link!("combase.dll" "system" fn RoInitialize(kind:u32)->i32);
    windows_link::link!("combase.dll" "system" fn RoUninitialize());
    struct Runtime;
    impl Drop for Runtime {
        fn drop(&mut self) {
            unsafe {
                RoUninitialize();
            }
        }
    }
    unsafe {
        check(RoInitialize(0), "RoInitialize STA")?;
    }
    let _runtime = Runtime;
    unsafe {
        let factory = factory(
            "Windows.UI.Xaml.Hosting.WindowsXamlManager",
            &IID_IWindowsXamlManagerStatics,
        )?;
        let mut manager = null_mut();
        check(
            (factory
                .table::<IWindowsXamlManagerStatics_Vtbl>()
                .InitializeForCurrentThread)(factory.raw(), &mut manager),
            "WindowsXamlManager.InitializeForCurrentThread",
        )?;
        let _manager = Object::owned(manager)?;
        crate::routing_checks::check_visibility_scalar()?;
        let time = load(
            r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Text="{Binding}" DataContext="Original time"/>"#,
        )?;
        let date = load(
            r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Text="Original date" FontSize="15"/>"#,
        )?;
        let container =
            load(r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"/>"#)?;
        let width = |node: &Object| -> Result<f64, String> {
            let fe = node.query(&IID_IFrameworkElement)?;
            let mut value = 0.;
            check(
                (fe.table::<IFrameworkElement_Vtbl>().Width)(fe.raw(), &mut value),
                "Width",
            )?;
            Ok(value)
        };
        let text = |node: &Object| -> Result<String, String> {
            let tb = node.query(&IID_ITextBlock)?;
            tb.string(tb.table::<ITextBlock_Vtbl>().Text, 256)
        };
        let font = |node: &Object| -> Result<f64, String> {
            let tb = node.query(&IID_ITextBlock)?;
            let mut value = 0.;
            check(
                (tb.table::<ITextBlock_Vtbl>().FontSize)(tb.raw(), &mut value),
                "FontSize",
            )?;
            Ok(value)
        };
        let require = |value: bool, label: &str| -> Result<(), String> {
            if value {
                Ok(())
            } else {
                Err(format!("隔离 XAML 检查失败：{label}"))
            }
        };
        let mut lease = ClockLease::capture(&time, &date, &container)?;
        require(
            matches!(lease.properties[0].original, Original::Binding(_)),
            "capture binding",
        )?;
        require(
            matches!(lease.properties[1].original, Original::Local(_)),
            "capture local text",
        )?;
        require(
            matches!(lease.properties[6].original, Original::Unset),
            "capture unset width",
        )?;
        lease.apply("12:34:56", "天气晴 22°C", 12., "Segoe UI", 280.)?;
        require(
            text(&time)? == "12:34:56" && text(&date)? == "天气晴 22°C",
            "apply text",
        )?;
        require(
            width(&container)? == 280. && font(&date)? == 12.,
            "apply style",
        )?;
        lease.tick("12:34:57", "天气晴 23°C")?;
        require(text(&time)? == "12:34:57", "tick text")?;
        require(
            lease
                .apply("invalid", "", f64::NAN, "Segoe UI", 280.)
                .is_err()
                && text(&time)? == "12:34:57",
            "invalid settings leave live state intact",
        )?;
        lease.apply("12:34:57", "天气晴 23°C", 12., "Segoe UI", 0.)?;
        require(width(&container)?.is_nan(), "automatic native Width")?;
        lease.tick("12:34:58", "Longer weather text")?;
        lease.verify_ownership()?;
        require(width(&container)?.is_nan(), "automatic Width survives tick")?;
        lease.restore()?;
        require(
            text(&date)? == "Original date" && font(&date)? == 15.,
            "restore local values",
        )?;
        require(width(&container)?.is_nan(), "restore unset width")?;
        let fe = time.query(&IID_IFrameworkElement)?;
        let source = boxed_text("Restored binding updates")?;
        check(
            (fe.table::<IFrameworkElement_Vtbl>().SetDataContext)(fe.raw(), source.raw()),
            "SetDataContext",
        )?;
        require(
            text(&time)? == "Restored binding updates",
            "restored binding reacts to source",
        )?;
        lease.restore()?;
        drop(lease);
        {
            let mut lease = ClockLease::capture(&time, &date, &container)?;
            lease.apply("temporary", "temporary", 13., "Segoe UI", 300.)?;
        }
        require(
            text(&time)? == "Restored binding updates"
                && text(&date)? == "Original date"
                && width(&container)?.is_nan(),
            "drop restores properties",
        )?;
        // Fail the fifth setter with a real type mismatch after four writes.
        // The transaction must restore both text values and both font sizes.
        let mut lease = ClockLease::capture(&time, &date, &container)?;
        let temporary = boxed_text("partial write")?;
        let top_size = boxed_number(18.)?;
        let bottom_size = boxed_number(10.)?;
        let invalid = boxed_number(4.)?;
        require(
            lease
                .write(&[&temporary, &temporary, &top_size, &bottom_size, &invalid])
                .is_err(),
            "native setter failure",
        )?;
        require(
            text(&time)? == "Restored binding updates"
                && text(&date)? == "Original date"
                && font(&date)? == 15.
                && width(&container)?.is_nan(),
            "partial failure rollback",
        )?;
        require(
            !lease.live && lease.properties.iter().all(|p| !p.dirty),
            "rollback leaves no pending writes",
        )?;
        drop(lease);
        let mut lease = ClockLease::capture(&time, &date, &container)?;
        lease.apply_lines(
            "already active",
            "already active",
            [(13., "Consolas"), (9., "Arial")],
            280.,
        )?;
        require(
            lease
                .write(&[&temporary, &temporary, &top_size, &bottom_size, &invalid])
                .is_err(),
            "active setter failure",
        )?;
        require(
            text(&time)? == "Restored binding updates"
                && text(&date)? == "Original date"
                && font(&date)? == 15.
                && width(&container)?.is_nan()
                && lease.clean(),
            "active failure restores previous owned values",
        )?;
        drop(lease);
        let mut checks = vec![
            "visibility_scalar_native_property_capture_apply_restore",
            "binding_capture",
            "local_value_capture",
            "unset_capture",
            "text_apply",
            "style_apply",
            "tick",
            "invalid_settings",
            "native_auto_width_and_tick_ownership",
            "local_restore",
            "unset_restore",
            "binding_source_update",
            "idempotent_restore",
            "drop_restore",
            "native_setter_failure",
            "partial_failure_rollback",
            "no_pending_writes",
            "active_native_setter_failure_restores_previous_values",
        ];
        checks.extend(crate::tray_interaction::check_context_target()?);
        checks.extend(crate::lifecycle_checks::run()?);
        checks.extend(crate::clock_style_checks::run()?);
        checks.extend(crate::clock_text_checks::run()?);
        checks.extend(combined_checks::run()?);
        checks.extend(crate::tray_menu::self_test()?);
        checks.extend(crate::tray::check_surface_sync()?);
        checks.extend(crate::tray_interaction::check_recovery()?);
        Ok(
            serde_json::json!({"passed":true,"environment":"isolated_xaml_in_host_process","explorer_accessed":false,"checks":checks}),
        )
    }
}
pub(crate) fn load(markup: &str) -> Result<Object, String> {
    unsafe {
        let reader = factory("Windows.UI.Xaml.Markup.XamlReader", &IID_IXamlReaderStatics)?;
        let markup = StringHandle::new(markup)?;
        let mut object = null_mut();
        check(
            (reader.table::<IXamlReaderStatics_Vtbl>().Load)(reader.raw(), markup.0, &mut object),
            "XamlReader.Load",
        )?;
        Object::owned(object)
    }
}
