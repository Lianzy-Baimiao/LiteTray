//! Reversible dependency-property writes shared by clock and tray models.
//! A snapshot retains bindings and distinguishes null from an unset value.
use crate::{
    xaml::{Object, check, factory},
    xaml_abi::*,
};
use std::{ffi::c_void, ptr::null_mut};
use windows_sys::core::IID_IUnknown;
pub(crate) enum Original {
    Unset,
    Local(Option<Object>),
    Binding(Object),
}
pub(crate) struct Property {
    target: Object,
    property: Object,
    pub(crate) original: Original,
    pub(crate) dirty: bool,
    written: Option<Object>,
}
impl Property {
    pub(crate) fn capture(target: &Object, property: Object) -> Result<Self, String> {
        unsafe {
            let dependency = target.query(&IID_IDependencyObject)?;
            // Normalize at the ABI boundary: COM identity can be a different
            // interface pointer, even while referring to this same property.
            let property = property.query(&IID_IDependencyProperty)?;
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
                        if value.try_query(&IID_IBindingExpressionBase)?.is_some() {
                            return Err("无法保存此 XAML 绑定，保留原属性".into());
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
    pub(crate) fn set(&mut self, value: &Object) -> Result<(), String> {
        self.set_nullable(Some(value))
    }
    pub(crate) fn set_nullable(&mut self, value: Option<&Object>) -> Result<(), String> {
        if !self.owns_current()? {
            return Err("XAML 属性已被其他组件修改".into());
        }
        let attempted = value.map(|v| v.query(&IID_IUnknown)).transpose()?;
        let previous = std::mem::replace(&mut self.written, attempted);
        let was_dirty = self.dirty;
        // A failed native setter may have produced side effects, so include it
        // in rollback even when HRESULT indicates a failure.
        self.dirty = true;
        unsafe {
            let result = check(
                (self.target.table::<IDependencyObject_Vtbl>().SetValue)(
                    self.target.raw(),
                    self.property.raw(),
                    value.map_or(null_mut(), Object::raw),
                ),
                "SetValue",
            );
            // Never adopt a reentrant writer's value as our own. Boxed text
            // and numbers may be canonicalized by XAML, so compare by value.
            if !self.owns_current()? {
                // A rejected update may leave our previous successful value
                // in place. Keep ownership of that value for group rollback;
                // otherwise an active clock could be left partially modified.
                self.written = previous;
                self.dirty = was_dirty;
                result?;
                return Err("XAML setter 返回时属性已被其他组件接管".into());
            }
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
    pub(crate) fn owns_current(&self) -> Result<bool, String> {
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
                let expression = Object::owned(binding)?.query(&IID_IBindingExpression)?;
                if !self.dirty
                    && let Original::Binding(original) = &self.original
                {
                    let mut parent = null_mut();
                    check(
                        (expression.table::<IBindingExpression_Vtbl>().ParentBinding)(
                            expression.raw(),
                            &mut parent,
                        ),
                        "ParentBinding ownership",
                    )?;
                    return same_object(&Object::owned(parent)?, original);
                }
                return Ok(false);
            }
        }
        let current = self.read_local()?;
        if self.dirty {
            return equal_optional(current.as_ref(), self.written.as_ref());
        }
        match &self.original {
            Original::Binding(_) => Ok(false),
            Original::Local(original) => equal_optional(current.as_ref(), original.as_ref()),
            Original::Unset => {
                let statics = factory(
                    "Windows.UI.Xaml.DependencyProperty",
                    &IID_IDependencyPropertyStatics,
                )?;
                let unset = unsafe {
                    property(
                        &statics,
                        statics
                            .table::<IDependencyPropertyStatics_Vtbl>()
                            .UnsetValue,
                    )?
                };
                equal_optional(current.as_ref(), Some(&unset))
            }
        }
    }
    pub(crate) fn restore(&mut self) -> Result<(), String> {
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
pub(crate) fn same_object(a: &Object, b: &Object) -> Result<bool, String> {
    Ok(a.query(&IID_IUnknown)?.raw() == b.query(&IID_IUnknown)?.raw())
}
pub(crate) fn equal_optional(a: Option<&Object>, b: Option<&Object>) -> Result<bool, String> {
    match (a, b) {
        (Some(a), Some(b)) => equal_value(a, b),
        (None, None) => Ok(true),
        _ => Ok(false),
    }
}
fn equal_value(a: &Object, b: &Object) -> Result<bool, String> {
    if same_object(a, b)? {
        return Ok(true);
    }
    if let (Some(a), Some(b)) = (
        crate::clock_style::alignment_value(a)?,
        crate::clock_style::alignment_value(b)?,
    ) {
        return Ok(a == b);
    }
    if let (Some(a), Some(b)) = (
        crate::visibility::enum_value(a)?,
        crate::visibility::enum_value(b)?,
    ) {
        return Ok(a == b);
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
            return a.same_string(&b);
        }
        if ak.0 == PropertyType::Double.0 {
            let mut av = 0.;
            let mut bv = 0.;
            check((at.GetDouble)(a.raw(), &mut av), "PropertyValue.Double")?;
            check((bt.GetDouble)(b.raw(), &mut bv), "PropertyValue.Double")?;
            return Ok(av == bv || av.is_nan() && bv.is_nan());
        }
        if ak.0 == PropertyType::Boolean.0 {
            let (mut av, mut bv) = (false, false);
            check((at.GetBoolean)(a.raw(), &mut av), "PropertyValue.Boolean")?;
            check((bt.GetBoolean)(b.raw(), &mut bv), "PropertyValue.Boolean")?;
            return Ok(av == bv);
        }
        Ok(false)
    }
}
type Getter = unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32;
pub(crate) fn property(factory: &Object, getter: Getter) -> Result<Object, String> {
    unsafe {
        let mut result = null_mut();
        check(
            getter(factory.raw(), &mut result),
            "DependencyProperty getter",
        )?;
        Object::owned(result)
    }
}
