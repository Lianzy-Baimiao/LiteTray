//! Optional appearance overrides own properties only while enabled. Returning
//! to Native restores the exact binding/local/unset snapshot and releases it.
use crate::{
    clock::{boxed_text, load},
    property::{Property, property},
    tray::read,
    xaml::{Object, check, factory},
    xaml_abi::*,
};
use litetray_adapter_protocol::clock::{Alignment, Frame};

struct Override {
    target: Object,
    property: Object,
    lease: Option<Property>,
}
impl Override {
    fn new(target: &Object, property: Object) -> Self {
        Self {
            target: target.clone(),
            property,
            lease: None,
        }
    }
    fn set(&mut self, value: Option<&Object>) -> Result<(), String> {
        match value {
            Some(value) => {
                if self.lease.is_none() {
                    self.lease = Some(Property::capture(&self.target, self.property.clone())?);
                }
                self.lease.as_mut().expect("captured").set(value)
            }
            None => {
                if let Some(lease) = &mut self.lease {
                    lease.restore()?;
                }
                self.lease = None;
                Ok(())
            }
        }
    }
    fn validate(&self) -> Result<(), String> {
        if let Some(lease) = &self.lease
            && !lease.owns_current()?
        {
            return Err("时钟颜色、对齐或提示已被其他组件修改".into());
        }
        Ok(())
    }
}
pub(crate) struct Decorations {
    values: [Override; 5],
    last: Option<Applied>,
}
#[derive(PartialEq)]
struct Applied {
    top: Option<String>,
    bottom: Option<String>,
    alignment: Alignment,
    tooltip: Option<String>,
}
pub(crate) struct Prepared {
    next: Applied,
    pending: Vec<(usize, Option<Object>)>,
}
impl Decorations {
    pub(crate) fn capture(
        time: &Object,
        date: &Object,
        container: &Object,
    ) -> Result<Self, String> {
        let text = factory("Windows.UI.Xaml.Controls.TextBlock", &IID_ITextBlockStatics)?;
        let tip = factory(
            "Windows.UI.Xaml.Controls.ToolTipService",
            &IID_IToolTipServiceStatics,
        )?;
        let (color, alignment, tooltip) = unsafe {
            (
                property(
                    &text,
                    text.table::<ITextBlockStatics_Vtbl>().ForegroundProperty,
                )?,
                property(
                    &text,
                    text.table::<ITextBlockStatics_Vtbl>().TextAlignmentProperty,
                )?,
                property(
                    &tip,
                    tip.table::<IToolTipServiceStatics_Vtbl>().ToolTipProperty,
                )?,
            )
        };
        Ok(Self {
            values: [
                Override::new(time, color.clone()),
                Override::new(date, color),
                Override::new(time, alignment.clone()),
                Override::new(date, alignment),
                Override::new(container, tooltip),
            ],
            last: None,
        })
    }
    pub(crate) fn prepare(&mut self, frame: &Frame) -> Result<Option<Prepared>, String> {
        self.validate()?;
        let next = Applied {
            top: frame.style.top_color.clone(),
            bottom: frame.style.bottom_color.clone(),
            alignment: frame.style.alignment,
            tooltip: frame.text.tooltip.clone(),
        };
        if self.last.as_ref() == Some(&next) {
            return Ok(None);
        }
        // All strings reaching XamlReader are validated color literals or enum
        // constants. Tooltip user text is always a boxed string, never markup.
        let changed = if let Some(old) = &self.last {
            [
                old.top != next.top,
                old.bottom != next.bottom,
                old.alignment != next.alignment,
                old.alignment != next.alignment,
                old.tooltip != next.tooltip,
            ]
        } else {
            [true; 5]
        };
        let mut pending = Vec::new();
        for (i, changed) in changed.into_iter().enumerate() {
            if !changed {
                continue;
            }
            let value = match i {
                0 => frame.style.top_color.as_deref().map(brush).transpose()?,
                1 => frame.style.bottom_color.as_deref().map(brush).transpose()?,
                2 | 3 => alignment_box(frame.style.alignment, &self.values[i].property)?,
                _ => frame.text.tooltip.as_deref().map(boxed_text).transpose()?,
            };
            pending.push((i, value));
        }
        // Capture every newly enabled override before the first setter. A
        // synchronous callback cannot change a later original unnoticed.
        for (i, value) in &pending {
            let target = &mut self.values[*i];
            if value.is_some() && target.lease.is_none() {
                target.lease = Some(Property::capture(&target.target, target.property.clone())?);
            }
        }
        self.validate()?;
        Ok(Some(Prepared { next, pending }))
    }
    pub(crate) fn apply(&mut self, prepared: Option<Prepared>) -> Result<(), String> {
        self.validate()?;
        let Some(Prepared { next, pending }) = prepared else {
            return Ok(());
        };
        for (i, value) in pending {
            self.values[i].set(value.as_ref())?;
        }
        self.last = Some(next);
        Ok(())
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        for value in &self.values {
            value.validate()?;
        }
        Ok(())
    }
    pub(crate) fn restore(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        for value in self.values.iter_mut().rev() {
            if let Err(error) = value.set(None) {
                errors.push(error);
            }
        }
        self.last = None;
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("；"))
        }
    }
    pub(crate) fn clean(&self) -> bool {
        self.values
            .iter()
            .all(|value| value.lease.as_ref().is_none_or(|lease| !lease.dirty))
    }
}
impl Drop for Decorations {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
fn brush(color: &str) -> Result<Object, String> {
    litetray_adapter_protocol::clock::validate_color(color)?;
    load(&format!(
        r#"<SolidColorBrush xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Color="{color}"/>"#
    ))
}
fn alignment_box(alignment: Alignment, property: &Object) -> Result<Option<Object>, String> {
    let name = match alignment {
        Alignment::Native => return Ok(None),
        Alignment::Left => "Left",
        Alignment::Center => "Center",
        Alignment::Right => "Right",
    };
    let node = load(&format!(
        r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" TextAlignment="{name}"/>"#
    ))?;
    read(&node, property)
}
const REFERENCE: windows_core::GUID = windows_core::GUID::from_signature(windows_core::imp::ConstBuffer::from_slice(
    b"pinterface({61c17706-2d65-11e0-9ae8-d48564015472};enum(Windows.UI.Xaml.TextAlignment;i4))",
));
const IID_REFERENCE: windows_sys::core::GUID =
    windows_sys::core::GUID::from_u128(REFERENCE.to_u128());
#[repr(C)]
struct ReferenceVtbl {
    inspectable: [usize; 6],
    value: unsafe extern "system" fn(*mut std::ffi::c_void, *mut i32) -> i32,
}
pub(crate) fn alignment_value(object: &Object) -> Result<Option<i32>, String> {
    let Some(object) = object.try_query(&IID_REFERENCE)? else {
        return Ok(None);
    };
    let mut value = 0;
    unsafe {
        check(
            (object.table::<ReferenceVtbl>().value)(object.raw(), &mut value),
            "IReference<TextAlignment>.Value",
        )?;
    }
    if !(0..=4).contains(&value) {
        return Err("原生文本对齐枚举无效".into());
    }
    Ok(Some(value))
}
