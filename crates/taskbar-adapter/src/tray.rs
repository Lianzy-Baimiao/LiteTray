//! Share native tray data, never visual elements, within the owning XAML STA.
//! Discovery/routing is deliberately outside this transaction. Its caller must
//! identify matching source/target roles and keep both visual trees alive.
use crate::{
    property::{Property, equal_optional, property},
    xaml::{Object, check, factory},
    xaml_abi::*,
};
use std::ptr::null_mut;
use windows_collections::IIterable;
use windows_core::{IInspectable, IUnknown, Interface};
use windows_sys::core::IID_IUnknown;

const MAX_PAIRS: usize = 64;
const MAX_ITEMS: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Field {
    DataContext,
    ItemsSource,
    Content,
    Visibility,
    Width,
    MinWidth,
    Opacity,
    HitTest,
}
impl Field {
    pub(crate) fn property(self) -> Result<Object, String> {
        unsafe {
            let (name, iid) = match self {
                Self::DataContext | Self::Width | Self::MinWidth => (
                    "Windows.UI.Xaml.FrameworkElement",
                    &IID_IFrameworkElementStatics,
                ),
                Self::ItemsSource => (
                    "Windows.UI.Xaml.Controls.ItemsControl",
                    &IID_IItemsControlStatics,
                ),
                Self::Content => (
                    "Windows.UI.Xaml.Controls.ContentControl",
                    &IID_IContentControlStatics,
                ),
                Self::Visibility | Self::Opacity | Self::HitTest => {
                    ("Windows.UI.Xaml.UIElement", &IID_IUIElementStatics)
                }
            };
            let statics = factory(name, iid)?;
            let getter = match self {
                Self::DataContext => {
                    statics
                        .table::<IFrameworkElementStatics_Vtbl>()
                        .DataContextProperty
                }
                Self::ItemsSource => {
                    statics
                        .table::<IItemsControlStatics_Vtbl>()
                        .ItemsSourceProperty
                }
                Self::Content => {
                    statics
                        .table::<IContentControlStatics_Vtbl>()
                        .ContentProperty
                }
                Self::Visibility => statics.table::<IUIElementStatics_Vtbl>().VisibilityProperty,
                Self::Opacity => statics.table::<IUIElementStatics_Vtbl>().OpacityProperty,
                Self::HitTest => {
                    statics
                        .table::<IUIElementStatics_Vtbl>()
                        .IsHitTestVisibleProperty
                }
                Self::Width => {
                    statics
                        .table::<IFrameworkElementStatics_Vtbl>()
                        .WidthProperty
                }
                Self::MinWidth => {
                    statics
                        .table::<IFrameworkElementStatics_Vtbl>()
                        .MinWidthProperty
                }
            };
            property(&statics, getter)
        }
    }
    fn check_target(self, element: &Object) -> Result<(), String> {
        element.query(match self {
            Self::DataContext => &IID_IFrameworkElement,
            Self::ItemsSource => &IID_IItemsControl,
            Self::Content => &IID_IContentControl,
            Self::Width | Self::MinWidth => &IID_IFrameworkElement,
            Self::Visibility | Self::Opacity | Self::HitTest => &IID_IUIElement,
        })?;
        Ok(())
    }
}
pub(crate) struct Pair {
    pub source: Object,
    pub target: Object,
    pub fields: Vec<Field>,
}
struct Source {
    identity: usize,
    element: Object,
    field: Field,
    property: Object,
}
struct Target {
    source: usize,
    property: Property,
    applied: bool,
    last: Option<Object>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Prepared,
    Active,
    Restoring,
    Closed,
}
pub(crate) struct ModelLease {
    sources: Vec<Source>,
    targets: Vec<Target>,
    state: State,
}
impl ModelLease {
    /// Capture all originals without mutating either tree. Reject duplicates
    /// and source/target overlap across the entire batch, including cycles.
    pub(crate) fn capture(pairs: Vec<Pair>) -> Result<Self, String> {
        if pairs.is_empty() || pairs.len() > MAX_PAIRS {
            return Err("托盘模型目标数量无效".into());
        }
        let mut identities = Vec::with_capacity(pairs.len());
        for pair in &pairs {
            let source = pair.source.query(&IID_IUnknown)?.raw() as usize;
            let target = pair.target.query(&IID_IUnknown)?.raw() as usize;
            if pair.fields.is_empty()
                || pair.fields.len() > 8
                || pair
                    .fields
                    .iter()
                    .enumerate()
                    .any(|(i, f)| pair.fields[..i].contains(f))
                || identities.iter().any(|(_, old)| *old == target)
            {
                return Err("托盘模型字段或目标重复".into());
            }
            identities.push((source, target));
        }
        if identities
            .iter()
            .any(|(source, _)| identities.iter().any(|(_, target)| source == target))
        {
            return Err("托盘模型来源与目标重叠".into());
        }
        let mut lease = Self {
            sources: Vec::new(),
            targets: Vec::new(),
            state: State::Prepared,
        };
        for (pair, (identity, _)) in pairs.into_iter().zip(identities) {
            // Set inherited data before properties which may bind to it.
            for field in [
                Field::DataContext,
                Field::ItemsSource,
                Field::Content,
                Field::Width,
                Field::MinWidth,
                Field::Opacity,
                Field::HitTest,
                Field::Visibility,
            ] {
                if !pair.fields.contains(&field) {
                    continue;
                }
                field.check_target(&pair.source)?;
                field.check_target(&pair.target)?;
                let source = if let Some(index) = lease
                    .sources
                    .iter()
                    .position(|s| s.identity == identity && s.field == field)
                {
                    index
                } else {
                    lease.sources.push(Source {
                        identity,
                        element: pair.source.query(&IID_IDependencyObject)?,
                        field,
                        property: field.property()?,
                    });
                    lease.sources.len() - 1
                };
                lease.targets.push(Target {
                    source,
                    property: Property::capture(&pair.target, field.property()?)?,
                    applied: false,
                    last: None,
                });
            }
        }
        Ok(lease)
    }
    pub(crate) fn start(&mut self) -> Result<(), String> {
        self.start_with_rollback(|| {})
    }
    /// The session must suspend its interaction before a failed write restores
    /// native properties, whose setters can synchronously enter shell callbacks.
    pub(crate) fn start_with_rollback(
        &mut self,
        before_restore: impl FnOnce(),
    ) -> Result<(), String> {
        if self.state != State::Prepared {
            return Err("托盘模型会话已启动或已关闭".into());
        }
        self.update(true, before_restore)?;
        self.state = State::Active;
        Ok(())
    }
    pub(crate) fn refresh(&mut self) -> Result<(), String> {
        self.refresh_with_rollback(|| {})
    }
    pub(crate) fn refresh_with_rollback(
        &mut self,
        before_restore: impl FnOnce(),
    ) -> Result<(), String> {
        if self.state != State::Active {
            return Err("托盘模型会话不再接受更新".into());
        }
        self.update(false, before_restore)
    }
    fn update(&mut self, initial: bool, before_restore: impl FnOnce()) -> Result<(), String> {
        let result = self.write(initial);
        if let Err(error) = result {
            before_restore();
            return match self.stop() {
                Ok(()) => Err(error),
                Err(restore) => Err(format!("{error}；托盘恢复未完成：{restore}")),
            };
        }
        Ok(())
    }
    fn write(&mut self, initial: bool) -> Result<(), String> {
        // Each source property is read/validated exactly once per batch, even
        // when several monitors share it. Keep these model references alive
        // until every target has consumed the same snapshot.
        let mut values = Vec::with_capacity(self.sources.len());
        for source in &self.sources {
            let value = read(&source.element, &source.property)?;
            if initial && value.is_none() {
                return Err("选定的托盘模型来源尚未就绪".into());
            }
            if let Some(value) = &value {
                validate(source.field, value)?;
            }
            values.push(value);
        }
        for target in &self.targets {
            if !target.property.owns_current()? {
                return Err("托盘属性已被其他组件修改，停止同步".into());
            }
        }
        for target in &mut self.targets {
            let value = values[target.source].as_ref();
            if target.applied && equal_optional(value, target.last.as_ref())? {
                continue;
            }
            target.property.set_nullable(value)?;
            target.last = value.map(|v| v.query(&IID_IUnknown)).transpose()?;
            target.applied = true;
        }
        Ok(())
    }
    pub(crate) fn stop(&mut self) -> Result<(), String> {
        if self.state == State::Closed {
            return Ok(());
        }
        self.state = State::Restoring;
        let mut errors = Vec::new();
        for target in self.targets.iter_mut().rev() {
            if let Err(error) = target.property.restore() {
                errors.push(error);
            }
            target.last = None;
            target.applied = false;
        }
        if errors.is_empty() {
            self.state = State::Closed;
            Ok(())
        } else {
            Err(errors.join("；"))
        }
    }
}
impl Drop for ModelLease {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
pub(crate) fn read(element: &Object, property: &Object) -> Result<Option<Object>, String> {
    let element = element.query(&IID_IDependencyObject)?;
    unsafe {
        let mut value = null_mut();
        check(
            (element.table::<IDependencyObject_Vtbl>().GetValue)(
                element.raw(),
                property.raw(),
                &mut value,
            ),
            "Read tray model",
        )?;
        if value.is_null() {
            Ok(None)
        } else {
            Ok(Some(Object::owned(value)?))
        }
    }
}
fn validate(field: Field, value: &Object) -> Result<(), String> {
    if value.try_query(&IID_IUIElement)?.is_some() {
        return Err("不能跨 XAML 树共享可视控件".into());
    }
    if field == Field::ItemsSource {
        let raw = value.raw();
        let unknown = unsafe { IUnknown::from_raw_borrowed(&raw) }.ok_or("托盘集合为空")?;
        let iterable = unknown
            .cast::<IIterable<IInspectable>>()
            .map_err(|e| format!("托盘集合接口不受支持：{e}"))?;
        let enumerate = || -> Result<(), String> {
            let iterator = iterable.First().map_err(|e| e.to_string())?;
            let mut count = 0;
            while iterator.HasCurrent().map_err(|e| e.to_string())? {
                if count >= MAX_ITEMS {
                    return Err(format!("集合超过 {MAX_ITEMS} 项"));
                }
                let item = iterator.Current().map_err(|e| e.to_string())?;
                // This is a bounded snapshot check, not an interceptor for
                // arbitrary later collection mutations by third parties.
                let object = unsafe { Object::query_borrowed(item.as_raw(), &IID_IUnknown) }?;
                if object.try_query(&IID_IUIElement)?.is_some() {
                    return Err("集合包含不能跨树共享的可视控件".into());
                }
                count += 1;
                iterator.MoveNext().map_err(|e| e.to_string())?;
            }
            Ok(())
        };
        enumerate().map_err(|e| format!("托盘集合检查失败：{e}"))?;
    }
    Ok(())
}
pub(crate) fn check_surface_sync() -> Result<Vec<&'static str>, String> {
    let source = crate::clock::load(
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Width="Auto" MinWidth="0" Opacity="1" IsHitTestVisible="True" Visibility="Visible"/>"#,
    )?;
    let target = crate::clock::load(
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Width="0" MinWidth="5" Opacity="0" IsHitTestVisible="False" Visibility="Collapsed"/>"#,
    )?;
    let fields = vec![
        Field::Width,
        Field::MinWidth,
        Field::Opacity,
        Field::HitTest,
        Field::Visibility,
    ];
    let originals = fields
        .iter()
        .map(|field| read(&target, &field.property()?))
        .collect::<Result<Vec<_>, String>>()?;
    let mut lease = ModelLease::capture(vec![Pair {
        source: source.clone(),
        target: target.clone(),
        fields: fields.clone(),
    }])?;
    lease.start()?;
    let matches_source = || -> Result<bool, String> {
        for field in &fields {
            let property = field.property()?;
            if !equal_optional(
                read(&source, &property)?.as_ref(),
                read(&target, &property)?.as_ref(),
            )? {
                return Ok(false);
            }
        }
        Ok(true)
    };
    if !matches_source()? {
        return Err("副屏托盘尺寸、透明度或命中测试不同步".into());
    }
    let ui = source.query(&IID_IUIElement)?;
    unsafe {
        check(
            (ui.table::<IUIElement_Vtbl>().SetVisibility)(ui.raw(), Visibility::Collapsed),
            "Source indicator hides",
        )?;
    }
    lease.refresh()?;
    if !matches_source()? {
        return Err("副屏未跟随原生指示器隐藏".into());
    }
    lease.stop()?;
    for (field, original) in fields.iter().zip(&originals) {
        if !equal_optional(
            original.as_ref(),
            read(&target, &field.property()?)?.as_ref(),
        )? {
            return Err("副屏托盘原生布局未恢复".into());
        }
    }
    Ok(vec![
        "tray_surface_auto_width_opacity_hit_test_and_visibility_sync",
        "tray_source_visibility_refresh",
        "tray_surface_originals_restored",
    ])
}
