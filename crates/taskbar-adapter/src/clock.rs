//! Reversible clock properties. Capture every original value before writing
//! any target. Keep bindings as bindings and unset values as unset values.
use crate::{
    property::{Original, Property, property, same_object},
    xaml::{Object, StringHandle, check, factory},
    xaml_abi::*,
};
use std::ptr::null_mut;
use windows_sys::core::IID_IUnknown;

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

/// Holds references to the exact two native TextBlocks and their original
/// properties. Must be restored and dropped on the same XAML thread.
pub(crate) struct ClockLease {
    properties: Vec<Property>,
    live: bool,
    identities: [usize; 3],
    last_text: Option<(String, String)>,
    decorations: crate::clock_style::Decorations,
    native_text: Option<[crate::clock_text::TextLayer; 2]>,
}
impl ClockLease {
    pub(crate) fn capture_native(
        time: &Object,
        date: &Object,
        container: &Object,
    ) -> Result<Self, String> {
        let mut lease = Self::capture(time, date, container)?;
        lease.native_text = Some([
            crate::clock_text::TextLayer::prepare(time)?,
            crate::clock_text::TextLayer::prepare(date)?,
        ]);
        Ok(lease)
    }
    pub fn capture(time: &Object, date: &Object, container: &Object) -> Result<Self, String> {
        unsafe {
            if same_object(time, date)? {
                return Err("时间和日期元素不能相同".into());
            }
            time.query(&IID_ITextBlock)?;
            date.query(&IID_ITextBlock)?;
            let text = factory("Windows.UI.Xaml.Controls.TextBlock", &IID_ITextBlockStatics)?;
            let fe = factory(
                "Windows.UI.Xaml.FrameworkElement",
                &IID_IFrameworkElementStatics,
            )?;
            let text_table = text.table::<ITextBlockStatics_Vtbl>();
            let mut properties = Vec::with_capacity(7);
            for (node, statics, getter) in [
                (time, &text, text_table.TextProperty),
                (date, &text, text_table.TextProperty),
                (time, &text, text_table.FontSizeProperty),
                (date, &text, text_table.FontSizeProperty),
                (time, &text, text_table.FontFamilyProperty),
                (date, &text, text_table.FontFamilyProperty),
                (
                    container,
                    &fe,
                    fe.table::<IFrameworkElementStatics_Vtbl>().WidthProperty,
                ),
            ] {
                properties.push(Property::capture(node, property(statics, getter)?)?);
            }
            Ok(Self {
                properties,
                live: false,
                identities: [
                    time.query(&IID_IUnknown)?.raw() as usize,
                    date.query(&IID_IUnknown)?.raw() as usize,
                    container.query(&IID_IUnknown)?.raw() as usize,
                ],
                last_text: None,
                decorations: crate::clock_style::Decorations::capture(time, date, container)?,
                native_text: None,
            })
        }
    }
    pub(crate) fn update(
        &mut self,
        frame: &litetray_adapter_protocol::clock::Frame,
        style_changed: bool,
    ) -> Result<(), String> {
        frame.validate()?;
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
        }) || !(80.0..=600.0).contains(&width)
            || top.len() > 4096
            || bottom.len() > 4096
            || top.contains('\0')
            || bottom.contains('\0')
        {
            return Err("时钟显示参数无效".into());
        }
        // Construct every value before writing. Failure here leaves the active
        // clock untouched and keeps the original rollback snapshot.
        let top_value = boxed_text(top)?;
        let bottom_value = boxed_text(bottom)?;
        let top_size = boxed_number(fonts[0].0)?;
        let bottom_size = boxed_number(fonts[1].0)?;
        let top_family = font_family(fonts[0].1)?;
        let bottom_family = font_family(fonts[1].1)?;
        let width = boxed_number(width)?;
        self.write(&[
            &top_value,
            &bottom_value,
            &top_size,
            &bottom_size,
            &top_family,
            &bottom_family,
            &width,
        ])?;
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
            .skip(if self.native_text.is_some() { 2 } else { 0 })
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
            layers[1].set(bottom)?;
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
        for property in self
            .properties
            .iter()
            .skip(if self.native_text.is_some() { 2 } else { 0 })
        {
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
        checks.extend(crate::lifecycle_checks::run()?);
        checks.extend(crate::clock_style_checks::run()?);
        checks.extend(crate::clock_text_checks::run()?);
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
