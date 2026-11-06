//! Reversible clock properties. Capture every original value before writing
//! any target. Keep bindings as bindings and unset values as unset values.
use crate::{
    xaml::{Object, StringHandle, check, factory},
    xaml_abi::*,
};
use std::{ffi::c_void, ptr::null_mut};
use windows_sys::core::IID_IUnknown;

enum Original {
    Unset,
    Local(Option<Object>),
    Binding(Object),
}
struct Property {
    target: Object,
    property: Object,
    original: Original,
    dirty: bool,
    written: Option<Object>,
}
impl Property {
    fn capture(target: &Object, property: Object) -> Result<Self, String> {
        unsafe {
            let dependency = target.query(&IID_IDependencyObject)?;
            let framework = target.query(&IID_IFrameworkElement2)?;
            let mut expression = null_mut();
            check(
                (framework
                    .table::<IFrameworkElement2_Vtbl>()
                    .GetBindingExpression)(
                    framework.raw(), property.raw(), &mut expression
                ),
                "GetBindingExpression",
            )?;
            let original = if !expression.is_null() {
                let expression = Object::owned(expression)?.query(&IID_IBindingExpression)?;
                let mut binding = null_mut();
                check(
                    (expression.table::<IBindingExpression_Vtbl>().ParentBinding)(
                        expression.raw(),
                        &mut binding,
                    ),
                    "ParentBinding",
                )?;
                Original::Binding(Object::owned(binding)?)
            } else {
                let mut value = null_mut();
                check(
                    (dependency.table::<IDependencyObject_Vtbl>().ReadLocalValue)(
                        dependency.raw(),
                        property.raw(),
                        &mut value,
                    ),
                    "ReadLocalValue",
                )?;
                if value.is_null() {
                    Original::Local(None)
                } else {
                    let value = Object::owned(value)?;
                    let statics = factory(
                        "Windows.UI.Xaml.DependencyProperty",
                        &IID_IDependencyPropertyStatics,
                    )?;
                    let mut unset = null_mut();
                    check(
                        (statics
                            .table::<IDependencyPropertyStatics_Vtbl>()
                            .UnsetValue)(statics.raw(), &mut unset),
                        "DependencyProperty.UnsetValue",
                    )?;
                    let unset = Object::owned(unset)?;
                    if same_object(&value, &unset)? {
                        Original::Unset
                    } else {
                        // A compiled/unsupported binding must never be replaced
                        // as an ordinary value that cannot recreate its source.
                        if value.query(&IID_IBindingExpressionBase).is_ok() {
                            return Err("无法保存此 XAML 绑定，保留原时钟".into());
                        }
                        Original::Local(Some(value))
                    }
                }
            };
            Ok(Self {
                target: dependency,
                property,
                original,
                dirty: false,
                written: None,
            })
        }
    }
    fn set(&mut self, value: &Object) -> Result<(), String> {
        if self.dirty && !self.owns_current()? {
            return Err("时钟属性已被其他组件修改".into());
        }
        self.written = Some(value.query(&IID_IUnknown)?);
        // A failed native setter may have produced side effects, so include it
        // in rollback even when HRESULT indicates a failure.
        self.dirty = true;
        unsafe {
            let result = check(
                (self.target.table::<IDependencyObject_Vtbl>().SetValue)(
                    self.target.raw(),
                    self.property.raw(),
                    value.raw(),
                ),
                "SetValue",
            );
            // XAML may canonicalize a boxed value. Read its actual local value
            // after the setter so ownership compares the native representation.
            self.written = self.read_local()?;
            result
        }
    }
    fn read_local(&self) -> Result<Option<Object>, String> {
        unsafe {
            let mut value = null_mut();
            check(
                (self.target.table::<IDependencyObject_Vtbl>().ReadLocalValue)(
                    self.target.raw(),
                    self.property.raw(),
                    &mut value,
                ),
                "ReadLocalValue ownership",
            )?;
            if value.is_null() {
                Ok(None)
            } else {
                Ok(Some(Object::owned(value)?))
            }
        }
    }
    fn owns_current(&self) -> Result<bool, String> {
        if !self.dirty {
            return Ok(true);
        }
        unsafe {
            let framework = self.target.query(&IID_IFrameworkElement2)?;
            let mut binding = null_mut();
            check(
                (framework
                    .table::<IFrameworkElement2_Vtbl>()
                    .GetBindingExpression)(
                    framework.raw(), self.property.raw(), &mut binding
                ),
                "GetBindingExpression ownership",
            )?;
            if !binding.is_null() {
                drop(Object::owned(binding)?);
                return Ok(false);
            }
        }
        match (self.read_local()?, &self.written) {
            (Some(current), Some(written)) => equal_value(&current, written),
            (None, None) => Ok(true),
            _ => Ok(false),
        }
    }
    fn restore(&mut self) -> Result<(), String> {
        if !self.dirty {
            return Ok(());
        }
        if !self.owns_current()? {
            // The external writer now owns this property. Preserve that value
            // and restore only the other properties still owned by LiteTray.
            self.dirty = false;
            self.written = None;
            return Ok(());
        }
        unsafe {
            let table = self.target.table::<IDependencyObject_Vtbl>();
            match &self.original {
                Original::Unset => check(
                    (table.ClearValue)(self.target.raw(), self.property.raw()),
                    "ClearValue restore",
                )?,
                Original::Local(value) => check(
                    (table.SetValue)(
                        self.target.raw(),
                        self.property.raw(),
                        value.as_ref().map_or(null_mut(), Object::raw),
                    ),
                    "SetValue restore",
                )?,
                Original::Binding(binding) => {
                    let framework = self.target.query(&IID_IFrameworkElement)?;
                    check(
                        (framework.table::<IFrameworkElement_Vtbl>().SetBinding)(
                            framework.raw(),
                            self.property.raw(),
                            binding.raw(),
                        ),
                        "SetBinding restore",
                    )?;
                }
            }
            self.dirty = false;
            self.written = None;
        }
        Ok(())
    }
}
fn same_object(a: &Object, b: &Object) -> Result<bool, String> {
    Ok(a.query(&IID_IUnknown)?.raw() == b.query(&IID_IUnknown)?.raw())
}
fn equal_value(a: &Object, b: &Object) -> Result<bool, String> {
    if same_object(a, b)? {
        return Ok(true);
    }
    let (Ok(a), Ok(b)) = (a.query(&IID_IPropertyValue), b.query(&IID_IPropertyValue)) else {
        return Ok(false);
    };
    unsafe {
        let (at, bt) = (
            a.table::<IPropertyValue_Vtbl>(),
            b.table::<IPropertyValue_Vtbl>(),
        );
        let mut ak = PropertyType::Empty;
        let mut bk = PropertyType::Empty;
        check((at.Type)(a.raw(), &mut ak), "PropertyValue.Type")?;
        check((bt.Type)(b.raw(), &mut bk), "PropertyValue.Type")?;
        if ak.0 != bk.0 {
            return Ok(false);
        }
        if ak.0 == PropertyType::String.0 {
            return Ok(a.string(at.GetString, 8192)? == b.string(bt.GetString, 8192)?);
        }
        if ak.0 == PropertyType::Double.0 {
            let mut av = 0.;
            let mut bv = 0.;
            check((at.GetDouble)(a.raw(), &mut av), "PropertyValue.Double")?;
            check((bt.GetDouble)(b.raw(), &mut bv), "PropertyValue.Double")?;
            return Ok(av == bv || av.is_nan() && bv.is_nan());
        }
        Ok(false)
    }
}
type Getter = unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32;
fn property(factory: &Object, getter: Getter) -> Result<Object, String> {
    unsafe {
        let mut result = null_mut();
        check(
            getter(factory.raw(), &mut result),
            "DependencyProperty getter",
        )?;
        Object::owned(result)
    }
}
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
}
impl ClockLease {
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
            })
        }
    }
    pub fn apply(
        &mut self,
        top: &str,
        bottom: &str,
        size: f64,
        family: &str,
        width: f64,
    ) -> Result<(), String> {
        if !(8.0..=32.0).contains(&size)
            || !(80.0..=600.0).contains(&width)
            || family.trim().is_empty()
            || family.len() > 128
            || top.len() > 4096
            || bottom.len() > 4096
        {
            return Err("时钟显示参数无效".into());
        }
        // Construct every value before writing. Failure here leaves the active
        // clock untouched and keeps the original rollback snapshot.
        let top_value = boxed_text(top)?;
        let bottom_value = boxed_text(bottom)?;
        let size = boxed_number(size)?;
        let family = font_family(family)?;
        let width = boxed_number(width)?;
        self.write(&[
            &top_value,
            &bottom_value,
            &size,
            &size,
            &family,
            &family,
            &width,
        ])?;
        self.live = true;
        self.last_text = Some((top.to_owned(), bottom.to_owned()));
        Ok(())
    }
    fn write(&mut self, values: &[&Object]) -> Result<(), String> {
        self.verify_ownership()?;
        for (i, value) in values.iter().enumerate() {
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
    pub fn clean(&self) -> bool {
        self.properties.iter().all(|p| !p.dirty)
    }
    fn verify_ownership(&mut self) -> Result<(), String> {
        let mut failure = None;
        for property in &self.properties {
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
        let size = boxed_number(18.)?;
        let invalid = boxed_number(4.)?;
        require(
            lease
                .write(&[&temporary, &temporary, &size, &size, &invalid])
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
        let mut checks = vec![
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
        ];
        checks.extend(crate::lifecycle_checks::run()?);
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
