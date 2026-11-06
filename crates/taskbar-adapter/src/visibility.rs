//! Visibility uses native enum boxes and the same binding-aware ownership
//! checks as text/models. No visual element is moved between XAML roots.
use crate::{
    clock::load,
    property::{Property, property, same_object},
    xaml::{Object, check, factory},
    xaml_abi::*,
};
use std::ptr::null_mut;

pub(crate) fn visibility_property() -> Result<Object, String> {
    let statics = factory("Windows.UI.Xaml.UIElement", &IID_IUIElementStatics)?;
    unsafe {
        property(
            &statics,
            statics.table::<IUIElementStatics_Vtbl>().VisibilityProperty,
        )
    }
}
// Standard WinRT parameterized interface signature for IReference<Visibility>.
// Do not compare arbitrary OtherType property values as integers.
const REFERENCE: windows_core::GUID =
    windows_core::GUID::from_signature(windows_core::imp::ConstBuffer::from_slice(
        b"pinterface({61c17706-2d65-11e0-9ae8-d48564015472};enum(Windows.UI.Xaml.Visibility;i4))",
    ));
const IID_REFERENCE: windows_sys::core::GUID =
    windows_sys::core::GUID::from_u128(REFERENCE.to_u128());
#[repr(C)]
struct ReferenceVtbl {
    inspectable: [usize; 6],
    value: unsafe extern "system" fn(*mut std::ffi::c_void, *mut i32) -> i32,
}
pub(crate) fn enum_value(object: &Object) -> Result<Option<i32>, String> {
    let Some(object) = object.try_query(&IID_REFERENCE)? else {
        return Ok(None);
    };
    let mut value = 0;
    unsafe {
        check(
            (object.table::<ReferenceVtbl>().value)(object.raw(), &mut value),
            "IReference<Visibility>.Value",
        )?;
    }
    if !(0..=1).contains(&value) {
        return Err("原生可见性枚举无效".into());
    }
    Ok(Some(value))
}
fn boxed(visible: bool, property: &Object) -> Result<Object, String> {
    // XAML supplies its own exact enum box, avoiding int-to-enum coercion.
    let node = load(if visible {
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Visibility="Visible"/>"#
    } else {
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Visibility="Collapsed"/>"#
    })?.query(&IID_IDependencyObject)?;
    unsafe {
        let mut value = null_mut();
        check(
            (node.table::<IDependencyObject_Vtbl>().ReadLocalValue)(
                node.raw(),
                property.raw(),
                &mut value,
            ),
            "Visibility enum box",
        )?;
        let value = Object::owned(value)?;
        if enum_value(&value)? != Some(if visible { 0 } else { 1 }) {
            return Err("可见性枚举装箱类型不匹配".into());
        }
        Ok(value)
    }
}
pub(crate) struct Target {
    pub element: Object,
    pub visible: bool,
}
pub(crate) struct VisibilityLease {
    values: Vec<(Property, Object)>,
    started: bool,
    closed: bool,
}
impl VisibilityLease {
    pub(crate) fn capture(targets: Vec<Target>) -> Result<Self, String> {
        if targets.is_empty() || targets.len() > 80 {
            return Err("可见性目标数量无效".into());
        }
        for (i, target) in targets.iter().enumerate() {
            target.element.query(&IID_IUIElement)?;
            for old in &targets[..i] {
                if same_object(&old.element, &target.element)? {
                    return Err("可见性目标重复".into());
                }
            }
        }
        let property = visibility_property()?;
        let visible = boxed(true, &property)?;
        let collapsed = boxed(false, &property)?;
        let mut values = Vec::with_capacity(targets.len());
        for target in targets {
            values.push((
                Property::capture(&target.element, property.clone())?,
                (if target.visible { &visible } else { &collapsed }).clone(),
            ));
        }
        Ok(Self {
            values,
            started: false,
            closed: false,
        })
    }
    pub(crate) fn start(&mut self) -> Result<(), String> {
        if self.started || self.closed {
            return Err("可见性会话已启动或关闭".into());
        }
        self.validate()?;
        self.started = true;
        for (property, value) in &mut self.values {
            property.set(value)?;
        }
        Ok(())
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        for (property, _) in &self.values {
            if !property.owns_current()? {
                return Err("托盘可见性已被其他组件修改".into());
            }
        }
        Ok(())
    }
    pub(crate) fn stop(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        for (property, _) in self.values.iter_mut().rev() {
            if let Err(error) = property.restore() {
                errors.push(error);
            }
        }
        if !errors.is_empty() {
            return Err(errors.join("；"));
        }
        self.closed = true;
        Ok(())
    }
}
impl Drop for VisibilityLease {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
