//! A transparent fill makes the passive clock layout receive padded clicks
//! before they bubble to IconView. No new controls, layout or shell hooks.
use crate::{
    property::{Property, property},
    xaml::{Object, factory},
    xaml_abi::*,
};

pub(crate) struct Surface {
    background: Property,
    brush: Object,
}
impl Surface {
    pub(crate) fn prepare(element: &Object) -> Result<Option<Self>, String> {
        if element.try_query(&IID_IPanel)?.is_none() {
            return Ok(None);
        }
        let statics = factory("Windows.UI.Xaml.Controls.Panel", &IID_IPanelStatics)?;
        let background = unsafe {
            property(
                &statics,
                statics.table::<IPanelStatics_Vtbl>().BackgroundProperty,
            )?
        };
        // Existing brushes already participate in hit testing. Preserve them.
        if crate::tray::read(element, &background)?.is_some() {
            return Ok(None);
        }
        Ok(Some(Self {
            background: Property::capture(element, background)?,
            brush: crate::preview::activate("Windows.UI.Xaml.Media.SolidColorBrush")?,
        }))
    }
    pub(crate) fn activate(&mut self) -> Result<(), String> {
        self.background.set(&self.brush)
    }
    pub(crate) fn restore(&mut self) -> Result<(), String> {
        self.background.restore()
    }
}

pub(crate) fn self_test(
    owner: litetray_adapter_protocol::clock::Owner,
    frame: litetray_adapter_protocol::clock::Frame,
) -> Result<Vec<&'static str>, String> {
    use crate::{
        input::{Sequence, Subscription},
        preview::ModelFixture,
        xaml::check,
    };
    use std::{cell::Cell, rc::Rc};
    let fixture = ModelFixture::new(
        owner.clone(),
        frame,
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"><Grid x:Name="Clock" Width="160" Height="64"><StackPanel VerticalAlignment="Center" HorizontalAlignment="Center"><TextBlock Text="12:34"/><TextBlock Text="2026/11/07"/></StackPanel></Grid></Grid>"#,
    )?;
    let input = fixture.input("Clock", 1)?;
    let statics = factory("Windows.UI.Xaml.Controls.Panel", &IID_IPanelStatics)?;
    let background = unsafe {
        property(
            &statics,
            statics.table::<IPanelStatics_Vtbl>().BackgroundProperty,
        )?
    };
    let read = || crate::tray::read(&input.element, &background);
    let events = Rc::new(Cell::new(0));
    let seen = events.clone();
    let mut sub = Subscription::prepare(
        &input.element,
        input.island,
        owner,
        1,
        Sequence::new(),
        move |_| {
            seen.set(seen.get() + 1);
            true
        },
    )?;
    if read()?.is_some() {
        return Err("准备点击接管不应改写背景".into());
    }
    sub.activate()?;
    let brush = read()?
        .ok_or("完整点击区域缺少透明背景")?
        .query(&IID_ISolidColorBrush)?;
    let mut color: Color = unsafe { std::mem::zeroed() };
    unsafe {
        check(
            (brush.table::<ISolidColorBrush_Vtbl>().Color)(brush.raw(), &mut color),
            "Clock input brush",
        )?;
    }
    let probe = sub.probe()?;
    let handled = probe.invoke()?;
    let native_actions = usize::from(!handled);
    if color.A != 0 || events.get() != 1 || native_actions != 0 {
        return Err("透明内容区应先处理点击，阻止祖先日历动作".into());
    }
    probe.invoke_with_handled(true)?;
    if events.get() != 1 {
        return Err("已处理事件不应再次打开日历".into());
    }
    sub.detach()?;
    if read()?.is_some() || probe.invoke()? {
        return Err("退出后未恢复原始背景和系统点击".into());
    }
    Ok(vec![
        "clock_passive_hit_surface_is_transparent",
        "clock_input_surface_restores_background",
        "clock_handled_event_never_opens_second_calendar",
    ])
}
