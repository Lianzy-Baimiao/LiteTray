//! Scoped XAML input handlers. Geometry is sampled on the owning UI thread;
//! publishing must be bounded and nonblocking. No Explorer discovery occurs.
use crate::{
    xaml::{Object, check},
    xaml_abi::*,
};
use litetray_adapter_protocol::{
    VERSION,
    activation::{Activation, Input, Rect},
    clock::Owner,
};
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr::null_mut,
    rc::Rc,
    sync::atomic::{AtomicU32, Ordering},
};
use windows_sys::{
    Win32::{
        Foundation::*, Graphics::Gdi::ClientToScreen, System::Threading::GetCurrentThreadId,
        UI::WindowsAndMessaging::*,
    },
    core::{GUID, IID_IUnknown, IUnknown_Vtbl},
};

const IID_TAPPED: GUID = GUID::from_u128(0x68d940cc_9ff0_49ce_b141_3f07ec477b97);
const IID_KEY: GUID = GUID::from_u128(0x7c63d2e5_7a0e_4e12_b96a_7715aa6ff1c8);
static LIVE_HANDLERS: AtomicU32 = AtomicU32::new(0);
#[repr(C)]
struct HandlerTable {
    base: IUnknown_Vtbl,
    invoke: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void) -> i32,
}
#[repr(C)]
struct Handler {
    table: &'static HandlerTable,
    refs: AtomicU32,
    kind: Input,
    context: Rc<Context>,
}
struct Context {
    thread: u32,
    active: Cell<bool>,
    sequence: Rc<Sequence>,
    clock_id: u64,
    owner: Owner,
    island: HWND,
    publish: Box<dyn Fn(Activation) -> bool>,
}
/// Shared by every clock in one session. The busy flag prevents nested native
/// events from publishing the same sequence while a callback pumps messages.
#[derive(Default)]
pub struct Sequence {
    value: Cell<u64>,
    busy: Cell<bool>,
}
impl Sequence {
    pub fn new() -> Rc<Self> {
        Rc::new(Self::default())
    }
}
struct DispatchGuard(Rc<Sequence>);
impl Drop for DispatchGuard {
    fn drop(&mut self) {
        self.0.busy.set(false);
    }
}
unsafe extern "system" fn query(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> i32 {
    unsafe {
        if out.is_null() || iid.is_null() {
            return E_POINTER;
        }
        *out = null_mut();
        let handler = &*(this as *const Handler);
        let expected = if handler.kind == Input::Pointer {
            IID_TAPPED
        } else {
            IID_KEY
        };
        if !guid_eq(*iid, IID_IUnknown) && !guid_eq(*iid, expected) {
            return E_NOINTERFACE;
        }
        *out = this;
        add_ref(this);
        S_OK
    }
}
unsafe extern "system" fn add_ref(this: *mut c_void) -> u32 {
    unsafe {
        (&*(this as *const Handler))
            .refs
            .fetch_add(1, Ordering::Relaxed)
            + 1
    }
}
unsafe extern "system" fn release(this: *mut c_void) -> u32 {
    unsafe {
        let remaining = (&*(this as *const Handler))
            .refs
            .fetch_sub(1, Ordering::AcqRel)
            - 1;
        if remaining == 0 {
            LIVE_HANDLERS.fetch_sub(1, Ordering::Relaxed);
            drop(Box::from_raw(this as *mut Handler));
        }
        remaining
    }
}
unsafe extern "system" fn invoke(this: *mut c_void, sender: *mut c_void, args: *mut c_void) -> i32 {
    unsafe {
        let handler = &*(this as *const Handler);
        // Non-agile delegate: no apartment or native object may cross threads.
        if GetCurrentThreadId() != handler.context.thread {
            return RPC_E_WRONG_THREAD;
        }
        let context = handler.context.clone();
        let kind = handler.kind;
        if !context.active.get() || sender.is_null() || args.is_null() {
            return S_OK;
        }
        if context.sequence.busy.replace(true) {
            return S_OK;
        }
        let _dispatch = DispatchGuard(context.sequence.clone());
        // Errors leave the original system input behavior available. Never
        // propagate a failed geometry query through the shell's event route.
        let _ = dispatch(&context, kind, sender, args);
        S_OK
    }
}
static TABLE: HandlerTable = HandlerTable {
    base: IUnknown_Vtbl {
        QueryInterface: query,
        AddRef: add_ref,
        Release: release,
    },
    invoke,
};
fn handler(context: Rc<Context>, kind: Input) -> Result<Object, String> {
    LIVE_HANDLERS.fetch_add(1, Ordering::Relaxed);
    unsafe {
        Object::owned(
            Box::into_raw(Box::new(Handler {
                table: &TABLE,
                refs: AtomicU32::new(1),
                kind,
                context,
            }))
            .cast(),
        )
    }
}
unsafe fn dispatch(
    context: &Context,
    kind: Input,
    sender: *mut c_void,
    args: *mut c_void,
) -> Result<(), String> {
    unsafe {
        let args = Object::query_borrowed(
            args,
            if kind == Input::Pointer {
                &IID_ITappedRoutedEventArgs
            } else {
                &IID_IKeyRoutedEventArgs
            },
        )?;
        let (handled, set_handled) = if kind == Input::Pointer {
            let table = args.table::<ITappedRoutedEventArgs_Vtbl>();
            (table.Handled, table.SetHandled)
        } else {
            let table = args.table::<IKeyRoutedEventArgs_Vtbl>();
            let mut key = VirtualKey(0);
            check((table.Key)(args.raw(), &mut key), "Key")?;
            if key.0 != VirtualKey::Enter.0 && key.0 != VirtualKey::Space.0 {
                return Ok(());
            }
            let mut status: CorePhysicalKeyStatus = std::mem::zeroed();
            check((table.KeyStatus)(args.raw(), &mut status), "KeyStatus")?;
            if status.WasKeyDown {
                return Ok(());
            }
            (table.Handled, table.SetHandled)
        };
        let mut already = false;
        check(handled(args.raw(), &mut already), "Handled")?;
        if already {
            return Ok(());
        }
        let sender = Object::query_borrowed(sender, &IID_IUIElement)?;
        let anchor = bounds(&sender, context.island)?;
        let sequence = context
            .sequence
            .value
            .get()
            .checked_add(1)
            .ok_or("点击序号已用尽")?;
        let event = Activation {
            version: VERSION,
            owner: context.owner.clone(),
            sequence,
            clock_id: context.clock_id,
            source_window: context.island as u64,
            anchor,
            input: kind,
        };
        event.validate()?;
        if (context.publish)(event) && context.active.get() {
            context.sequence.value.set(sequence);
            check(set_handled(args.raw(), true), "SetHandled")?;
        }
        Ok(())
    }
}
fn guid_eq(a: GUID, b: GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}
/// `island` is the verified native HWND whose client origin corresponds to the
/// XamlRoot. Passing the outer taskbar HWND without proving this is incorrect.
pub fn bounds(element: &Object, island: HWND) -> Result<Rect, String> {
    unsafe {
        if GetWindowThreadProcessId(island, null_mut()) != GetCurrentThreadId() {
            return Err("XAML 原生窗口不属于当前线程".into());
        }
        let ui = element.query(&IID_IUIElement)?;
        let fe = element.query(&IID_IFrameworkElement)?;
        let mut width = 0.;
        let mut height = 0.;
        check(
            (fe.table::<IFrameworkElement_Vtbl>().ActualWidth)(fe.raw(), &mut width),
            "ActualWidth",
        )?;
        check(
            (fe.table::<IFrameworkElement_Vtbl>().ActualHeight)(fe.raw(), &mut height),
            "ActualHeight",
        )?;
        let ui10 = element.query(&IID_IUIElement10)?;
        let mut root = null_mut();
        check(
            (ui10.table::<IUIElement10_Vtbl>().XamlRoot)(ui10.raw(), &mut root),
            "XamlRoot",
        )?;
        let root = Object::owned(root)?.query(&IID_IXamlRoot)?;
        let mut scale = 0.;
        check(
            (root.table::<IXamlRoot_Vtbl>().RasterizationScale)(root.raw(), &mut scale),
            "RasterizationScale",
        )?;
        let mut content = null_mut();
        check(
            (root.table::<IXamlRoot_Vtbl>().Content)(root.raw(), &mut content),
            "Content",
        )?;
        let content = Object::owned(content)?.query(&IID_IUIElement)?;
        let mut transform = null_mut();
        check(
            (ui.table::<IUIElement_Vtbl>().TransformToVisual)(
                ui.raw(),
                content.raw(),
                &mut transform,
            ),
            "TransformToVisual",
        )?;
        let transform = Object::owned(transform)?.query(&IID_IGeneralTransform)?;
        let mut rect: crate::xaml_abi::Rect = std::mem::zeroed();
        check(
            (transform.table::<IGeneralTransform_Vtbl>().TransformBounds)(
                transform.raw(),
                crate::xaml_abi::Rect {
                    X: 0.,
                    Y: 0.,
                    Width: width as f32,
                    Height: height as f32,
                },
                &mut rect,
            ),
            "TransformBounds",
        )?;
        let mut origin = POINT { x: 0, y: 0 };
        if ClientToScreen(island, &mut origin) == 0 {
            return Err("无法读取 XAML 屏幕坐标".into());
        }
        physical_bounds(rect, scale, origin)
    }
}
fn physical_bounds(rect: crate::xaml_abi::Rect, scale: f64, origin: POINT) -> Result<Rect, String> {
    if !scale.is_finite()
        || !(0.5..=8.).contains(&scale)
        || ![rect.X, rect.Y, rect.Width, rect.Height]
            .iter()
            .all(|v| v.is_finite())
        || rect.Width <= 0.
        || rect.Height <= 0.
    {
        return Err(format!(
            "XAML 点击几何信息无效：x={}, y={}, width={}, height={}, scale={scale}",
            rect.X, rect.Y, rect.Width, rect.Height
        ));
    }
    let values = [
        origin.x as f64 + rect.X as f64 * scale,
        origin.y as f64 + rect.Y as f64 * scale,
        origin.x as f64 + (rect.X as f64 + rect.Width as f64) * scale,
        origin.y as f64 + (rect.Y as f64 + rect.Height as f64) * scale,
    ];
    if values
        .iter()
        .any(|v| *v < i32::MIN as f64 || *v > i32::MAX as f64)
    {
        return Err("点击坐标溢出".into());
    }
    let bounds = Rect {
        left: values[0].floor() as i32,
        top: values[1].floor() as i32,
        right: values[2].ceil() as i32,
        bottom: values[3].ceil() as i32,
    };
    bounds.validate()?;
    Ok(bounds)
}
pub struct Subscription {
    element: Object,
    context: Rc<Context>,
    tap: Option<i64>,
    key: Option<i64>,
    hit_area: RefCell<Option<crate::input_surface::Surface>>,
}
impl Subscription {
    pub fn prepare(
        element: &Object,
        island: HWND,
        owner: Owner,
        clock_id: u64,
        sequence: Rc<Sequence>,
        publish: impl Fn(Activation) -> bool + 'static,
    ) -> Result<Self, String> {
        owner.validate()?;
        if clock_id == 0
            || unsafe { GetWindowThreadProcessId(island, null_mut()) }
                != unsafe { GetCurrentThreadId() }
        {
            return Err("点击订阅目标无效".into());
        }
        let context = Rc::new(Context {
            thread: unsafe { GetCurrentThreadId() },
            active: Cell::new(false),
            sequence,
            clock_id,
            owner,
            island,
            publish: Box::new(publish),
        });
        let mut subscription = Self {
            element: element.query(&IID_IUIElement)?,
            context,
            tap: None,
            key: None,
            hit_area: RefCell::new(crate::input_surface::Surface::prepare(element)?),
        };
        unsafe {
            let table = subscription.element.table::<IUIElement_Vtbl>();
            let tap = handler(subscription.context.clone(), Input::Pointer)?;
            let mut token = 0;
            check(
                (table.Tapped)(subscription.element.raw(), tap.raw(), &mut token),
                "Tapped subscription",
            )?;
            subscription.tap = Some(token);
            let key = handler(subscription.context.clone(), Input::Keyboard)?;
            check(
                (table.KeyDown)(subscription.element.raw(), key.raw(), &mut token),
                "KeyDown subscription",
            )?;
            subscription.key = Some(token);
        }
        Ok(subscription)
    }
    pub fn activate(&self) -> Result<(), String> {
        if self.tap.is_none() || self.key.is_none() {
            return Err("点击订阅已解除或不完整".into());
        }
        if let Some(surface) = self.hit_area.borrow_mut().as_mut() {
            surface.activate()?;
        }
        self.context.active.set(true);
        Ok(())
    }
    pub fn suspend(&self) {
        self.context.active.set(false);
    }
    pub fn detach(&mut self) -> Result<(), String> {
        self.suspend();
        let mut errors = Vec::new();
        unsafe {
            let table = self.element.table::<IUIElement_Vtbl>();
            if let Some(token) = self.tap {
                match check(
                    (table.RemoveTapped)(self.element.raw(), token),
                    "Remove tapped handler",
                ) {
                    Ok(()) => self.tap = None,
                    Err(e) => errors.push(e),
                }
            }
            if let Some(value) = self.key {
                match check(
                    (table.RemoveKeyDown)(self.element.raw(), value),
                    "Remove key handler",
                ) {
                    Ok(()) => self.key = None,
                    Err(e) => errors.push(e),
                }
            }
        }
        if let Some(surface) = self.hit_area.borrow_mut().as_mut()
            && let Err(e) = surface.restore()
        {
            errors.push(e);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

/// Retains the event delegate across teardown for isolated lifecycle checks.
pub(crate) struct Probe {
    handler: Object,
    element: Object,
    args: Object,
}
impl Probe {
    pub fn invoke(&self) -> Result<bool, String> {
        self.invoke_with_handled(false)
    }
    pub fn invoke_with_handled(&self, handled: bool) -> Result<bool, String> {
        unsafe {
            let table = self.args.table::<ITappedRoutedEventArgs_Vtbl>();
            check(
                (table.SetHandled)(self.args.raw(), handled),
                "Reset probe event",
            )?;
            check(
                (self.handler.table::<HandlerTable>().invoke)(
                    self.handler.raw(),
                    self.element.raw(),
                    self.args.raw(),
                ),
                "Invoke retained input delegate",
            )?;
            let mut handled = false;
            check(
                (table.Handled)(self.args.raw(), &mut handled),
                "Read probe event",
            )?;
            Ok(handled)
        }
    }
}
impl Subscription {
    pub(crate) fn probe(&self) -> Result<Probe, String> {
        Ok(Probe {
            handler: handler(self.context.clone(), Input::Pointer)?,
            element: self.element.query(&IID_IUIElement)?,
            args: crate::preview::activate("Windows.UI.Xaml.Input.TappedRoutedEventArgs")?
                .query(&IID_ITappedRoutedEventArgs)?,
        })
    }
}
impl Drop for Subscription {
    fn drop(&mut self) {
        if self.detach().is_err() {
            // A native object may retain our delegate after failed removal.
            // Keep its executable code mapped until process exit; the context
            // is already inactive and cannot publish any more events.
            unsafe {
                use windows_sys::Win32::System::LibraryLoader::*;
                let mut module = null_mut();
                GetModuleHandleExW(
                    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN,
                    invoke as *const u16,
                    &mut module,
                );
            }
        }
    }
}

/// Real XAML subscription lifetime and WinRT event-argument checks. This
/// deliberately invokes the delegate; actual OS input is verified in the UI.
pub(crate) fn self_test(
    element: &Object,
    island: HWND,
    owner: Owner,
) -> Result<Vec<&'static str>, String> {
    use std::cell::RefCell;
    let baseline = LIVE_HANDLERS.load(Ordering::Relaxed);
    let events = Rc::new(RefCell::new(Vec::new()));
    let collected = events.clone();
    let mut subscription = Subscription::prepare(
        element,
        island,
        owner.clone(),
        1,
        Sequence::new(),
        move |event| {
            collected.borrow_mut().push(event);
            true
        },
    )?;
    subscription.activate()?;
    let tap = handler(subscription.context.clone(), Input::Pointer)?;
    let args = crate::preview::activate("Windows.UI.Xaml.Input.TappedRoutedEventArgs")?
        .query(&IID_ITappedRoutedEventArgs)?;
    unsafe {
        check(
            (tap.table::<HandlerTable>().invoke)(tap.raw(), element.raw(), args.raw()),
            "Invoke tapped check",
        )?;
        let mut handled = false;
        check(
            (args.table::<ITappedRoutedEventArgs_Vtbl>().Handled)(args.raw(), &mut handled),
            "Handled check",
        )?;
        if !handled || events.borrow().len() != 1 {
            return Err("原生点击委托未发布或标记事件".into());
        }
        let event = events.borrow()[0].clone();
        if event.anchor != bounds(element, island)? || event.input != Input::Pointer {
            return Err("原生点击坐标不匹配".into());
        }
        subscription.detach()?;
        check(
            (args.table::<ITappedRoutedEventArgs_Vtbl>().SetHandled)(args.raw(), false),
            "Reset test args",
        )?;
        check(
            (tap.table::<HandlerTable>().invoke)(tap.raw(), element.raw(), args.raw()),
            "Invoke detached check",
        )?;
        if events.borrow().len() != 1 {
            return Err("已解除点击订阅仍发布事件".into());
        }
    }
    drop(tap);
    drop(subscription);
    let mut rejected =
        Subscription::prepare(element, island, owner.clone(), 1, Sequence::new(), |_| {
            false
        })?;
    rejected.activate()?;
    let tap = handler(rejected.context.clone(), Input::Pointer)?;
    unsafe {
        check(
            (args.table::<ITappedRoutedEventArgs_Vtbl>().SetHandled)(args.raw(), false),
            "Reset rejected event",
        )?;
        check(
            (tap.table::<HandlerTable>().invoke)(tap.raw(), element.raw(), args.raw()),
            "Invoke rejected event",
        )?;
        let mut handled = false;
        check(
            (args.table::<ITappedRoutedEventArgs_Vtbl>().Handled)(args.raw(), &mut handled),
            "Rejected Handled",
        )?;
        if handled || rejected.context.sequence.value.get() != 0 {
            return Err("发布失败不应吞掉输入或递增序号".into());
        }
    }
    rejected.detach()?;
    drop(tap);
    drop(rejected);
    for _ in 0..12 {
        let mut sub =
            Subscription::prepare(element, island, owner.clone(), 1, Sequence::new(), |_| {
                false
            })?;
        sub.activate()?;
        sub.detach()?;
    }
    if LIVE_HANDLERS.load(Ordering::Relaxed) != baseline {
        return Err("点击委托引用未释放".into());
    }
    Ok(vec![
        "xaml_input_delegate_activation_bounds",
        "xaml_input_detach_suppresses_callback",
        "xaml_input_repeated_subscription_cleanup",
        "xaml_input_rejected_delivery_preserves_default",
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_keep_negative_coordinates_and_round_outwards() {
        let r = physical_bounds(
            crate::xaml_abi::Rect {
                X: 10.5,
                Y: 2.,
                Width: 101.,
                Height: 30.,
            },
            1.25,
            POINT { x: -1920, y: -60 },
        )
        .unwrap();
        assert_eq!(
            r,
            Rect {
                left: -1907,
                top: -58,
                right: -1780,
                bottom: -20
            }
        );
        assert!(
            physical_bounds(
                crate::xaml_abi::Rect {
                    X: 0.,
                    Y: 0.,
                    Width: f32::NAN,
                    Height: 30.
                },
                1.,
                POINT { x: 0, y: 0 }
            )
            .is_err()
        );
    }
}
