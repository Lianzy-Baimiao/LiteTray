//! Optional appearance overrides own properties only while enabled. Tooltip
//! leases stay active until the clock stops: an empty template hides the
//! native tooltip, while a nonempty template shows the user's literal text.
use crate::{
    clock::{boxed_text, load},
    property::{Property, property, same_object},
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
    fn capture(&mut self) -> Result<(), String> {
        if self.lease.is_none() {
            self.lease = Some(Property::capture(&self.target, self.property.clone())?);
        }
        Ok(())
    }
    fn set_tooltip(&mut self, value: Option<&Object>) -> Result<(), String> {
        self.capture()?;
        self.lease.as_mut().expect("captured").set_nullable(value)
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
    suppressed_tooltips: Vec<Override>,
    last: Option<Applied>,
}
#[derive(PartialEq)]
struct Applied {
    top: Option<String>,
    bottom: Option<String>,
    alignment: [Alignment; 2],
    tooltip: Option<String>,
}
pub(crate) struct Prepared {
    next: Applied,
    pending: Vec<(usize, Option<Object>)>,
    suppress_native: bool,
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
            suppressed_tooltips: Vec::new(),
            last: None,
        })
    }
    pub(crate) fn configure_tooltips(
        &mut self,
        primary: &Object,
        targets: &[Object],
    ) -> Result<(), String> {
        if self.last.is_some() || !self.clean() {
            return Err("不能更换已接管时钟的提示目标".into());
        }
        let property = self.values[4].property.clone();
        let mut suppressed: Vec<Override> = Vec::new();
        for target in targets {
            if same_object(primary, target)? {
                continue;
            }
            for item in &suppressed {
                if same_object(&item.target, target)? {
                    return Err("时钟提示目标重复".into());
                }
            }
            suppressed.push(Override::new(target, property.clone()));
        }
        self.values[4] = Override::new(primary, property);
        self.suppressed_tooltips = suppressed;
        Ok(())
    }
    pub(crate) fn prepare(&mut self, frame: &Frame) -> Result<Option<Prepared>, String> {
        self.validate()?;
        let next = Applied {
            top: frame.style.top_color.clone(),
            bottom: frame.style.bottom_color.clone(),
            alignment: [
                frame
                    .style
                    .top_style
                    .alignment
                    .unwrap_or(frame.style.alignment),
                frame
                    .style
                    .bottom_style
                    .alignment
                    .unwrap_or(frame.style.alignment),
            ],
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
                old.alignment[0] != next.alignment[0],
                old.alignment[1] != next.alignment[1],
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
                2 | 3 => alignment_box(next.alignment[i - 2], &self.values[i].property)?,
                _ => frame.text.tooltip.as_deref().map(boxed_text).transpose()?,
            };
            pending.push((i, value));
        }
        // Capture every newly enabled override before the first setter. A
        // synchronous callback cannot change a later original unnoticed.
        for (i, value) in &pending {
            let target = &mut self.values[*i];
            if value.is_some() || *i == 4 {
                target.capture()?;
            }
        }
        let suppress_native = self.last.is_none();
        if suppress_native {
            for target in &mut self.suppressed_tooltips {
                target.capture()?;
            }
        }
        self.validate()?;
        Ok(Some(Prepared {
            next,
            pending,
            suppress_native,
        }))
    }
    pub(crate) fn apply(&mut self, prepared: Option<Prepared>) -> Result<(), String> {
        self.validate()?;
        let Some(Prepared {
            next,
            pending,
            suppress_native,
        }) = prepared
        else {
            return Ok(());
        };
        if suppress_native {
            for target in &mut self.suppressed_tooltips {
                target.set_tooltip(None)?;
            }
        }
        for (i, value) in pending {
            if i == 4 {
                self.values[i].set_tooltip(value.as_ref())?;
            } else {
                self.values[i].set(value.as_ref())?;
            }
        }
        self.last = Some(next);
        Ok(())
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        for value in self.values.iter().chain(&self.suppressed_tooltips) {
            value.validate()?;
        }
        Ok(())
    }
    pub(crate) fn restore(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        for value in self
            .values
            .iter_mut()
            .chain(&mut self.suppressed_tooltips)
            .rev()
        {
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
            .chain(&self.suppressed_tooltips)
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
