//! Replay pointer + gesture events through the production delegates on a real
//! hidden XAML element. The point fixture supplies only OS event properties;
//! publishing, hit testing, geometry and Handled are the production paths.
use super::*;
use windows_sys::core::{HSTRING, IID_IInspectable, IInspectable_Vtbl};

#[derive(Clone, Copy)]
enum NodeKind {
    Args,
    Point,
    Properties,
}
struct Event {
    update: PointerUpdateKind,
    handled: Cell<bool>,
}
#[repr(C)]
struct Node {
    table: *const c_void,
    refs: Cell<u32>,
    kind: NodeKind,
    event: Rc<Event>,
}
unsafe extern "system" fn query_node(
    this: *mut c_void,
    iid: *const GUID,
    out: *mut *mut c_void,
) -> i32 {
    unsafe {
        if iid.is_null() || out.is_null() {
            return E_POINTER;
        }
        *out = null_mut();
        let node = &*(this as *const Node);
        let expected = match node.kind {
            NodeKind::Args => IID_IPointerRoutedEventArgs,
            NodeKind::Point => IID_IPointerPoint,
            NodeKind::Properties => IID_IPointerPointProperties,
        };
        if !guid_eq(*iid, expected)
            && !guid_eq(*iid, IID_IUnknown)
            && !guid_eq(*iid, IID_IInspectable)
        {
            return E_NOINTERFACE;
        }
        *out = this;
        retain_node(this);
        S_OK
    }
}
unsafe extern "system" fn retain_node(this: *mut c_void) -> u32 {
    unsafe {
        let n = &*(this as *const Node);
        let refs = n.refs.get() + 1;
        n.refs.set(refs);
        refs
    }
}
unsafe extern "system" fn release_node(this: *mut c_void) -> u32 {
    unsafe {
        let n = &*(this as *const Node);
        let refs = n.refs.get() - 1;
        n.refs.set(refs);
        if refs == 0 {
            drop(Box::from_raw(this as *mut Node));
        }
        refs
    }
}
unsafe extern "system" fn iids(_: *mut c_void, count: *mut u32, out: *mut *mut GUID) -> i32 {
    unsafe {
        *count = 0;
        *out = null_mut();
    }
    S_OK
}
unsafe extern "system" fn name(_: *mut c_void, out: *mut HSTRING) -> i32 {
    unsafe {
        *out = null_mut();
    }
    S_OK
}
unsafe extern "system" fn trust(_: *mut c_void, out: *mut i32) -> i32 {
    unsafe {
        *out = 0;
    }
    S_OK
}
const BASE: IInspectable_Vtbl = IInspectable_Vtbl {
    base: IUnknown_Vtbl {
        QueryInterface: query_node,
        AddRef: retain_node,
        Release: release_node,
    },
    GetIids: iids,
    GetRuntimeClassName: name,
    GetTrustLevel: trust,
};
unsafe extern "system" fn handled(this: *mut c_void, out: *mut bool) -> i32 {
    unsafe {
        *out = (&*(this as *const Node)).event.handled.get();
    }
    S_OK
}
unsafe extern "system" fn set_handled(this: *mut c_void, value: bool) -> i32 {
    unsafe {
        (&*(this as *const Node)).event.handled.set(value);
    }
    S_OK
}
unsafe extern "system" fn current(this: *mut c_void, _: *mut c_void, out: *mut *mut c_void) -> i32 {
    unsafe {
        *out = node(NodeKind::Point, (*(this as *const Node)).event.clone());
    }
    S_OK
}
unsafe extern "system" fn position(_: *mut c_void, out: *mut Point) -> i32 {
    unsafe {
        *out = Point { X: 1., Y: 1. };
    }
    S_OK
}
unsafe extern "system" fn properties(this: *mut c_void, out: *mut *mut c_void) -> i32 {
    unsafe {
        *out = node(NodeKind::Properties, (*(this as *const Node)).event.clone());
    }
    S_OK
}
unsafe extern "system" fn update(this: *mut c_void, out: *mut PointerUpdateKind) -> i32 {
    unsafe {
        *out = (&*(this as *const Node)).event.update;
    }
    S_OK
}
#[repr(C)]
struct ArgsTable {
    base: IInspectable_Vtbl,
    pointer: usize,
    modifiers: usize,
    handled: unsafe extern "system" fn(*mut c_void, *mut bool) -> i32,
    set_handled: unsafe extern "system" fn(*mut c_void, bool) -> i32,
    current: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> i32,
    intermediate: usize,
}
#[repr(C)]
struct PointTable {
    base: IInspectable_Vtbl,
    device: usize,
    position: unsafe extern "system" fn(*mut c_void, *mut Point) -> i32,
    unused: [usize; 5],
    properties: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32,
}
#[repr(C)]
struct PropertiesTable {
    base: IInspectable_Vtbl,
    unused: [usize; 21],
    update: unsafe extern "system" fn(*mut c_void, *mut PointerUpdateKind) -> i32,
    trailing: [usize; 2],
}
static ARGS: ArgsTable = ArgsTable {
    base: BASE,
    pointer: 0,
    modifiers: 0,
    handled,
    set_handled,
    current,
    intermediate: 0,
};
static POINT: PointTable = PointTable {
    base: BASE,
    device: 0,
    position,
    unused: [0; 5],
    properties,
};
static PROPERTIES: PropertiesTable = PropertiesTable {
    base: BASE,
    unused: [0; 21],
    update,
    trailing: [0; 2],
};
fn node(kind: NodeKind, event: Rc<Event>) -> *mut c_void {
    let table = match kind {
        NodeKind::Args => &ARGS as *const _ as _,
        NodeKind::Point => &POINT as *const _ as _,
        NodeKind::Properties => &PROPERTIES as *const _ as _,
    };
    Box::into_raw(Box::new(Node {
        table,
        refs: Cell::new(1),
        kind,
        event,
    }))
    .cast()
}
fn pointer(
    sub: &Subscription,
    element: &Object,
    kind: Input,
    update: PointerUpdateKind,
) -> Result<bool, String> {
    let event = Rc::new(Event {
        update,
        handled: Cell::new(false),
    });
    let args = unsafe { Object::owned(node(NodeKind::Args, event.clone()))? };
    let delegate = handler(sub.context.clone(), kind)?;
    unsafe {
        check(
            (delegate.table::<HandlerTable>().invoke)(delegate.raw(), element.raw(), args.raw()),
            "Replay pointer",
        )?;
    }
    Ok(event.handled.get())
}
pub(super) fn run(element: &Object, island: HWND, owner: Owner) -> Result<(), String> {
    use std::mem::{offset_of, size_of};
    assert_eq!(
        size_of::<ArgsTable>(),
        size_of::<IPointerRoutedEventArgs_Vtbl>()
    );
    assert_eq!(
        offset_of!(ArgsTable, current),
        offset_of!(IPointerRoutedEventArgs_Vtbl, GetCurrentPoint)
    );
    assert_eq!(size_of::<PointTable>(), size_of::<IPointerPoint_Vtbl>());
    assert_eq!(
        offset_of!(PointTable, properties),
        offset_of!(IPointerPoint_Vtbl, Properties)
    );
    assert_eq!(
        size_of::<PropertiesTable>(),
        size_of::<IPointerPointProperties_Vtbl>()
    );
    assert_eq!(
        offset_of!(PropertiesTable, update),
        offset_of!(IPointerPointProperties_Vtbl, PointerUpdateKind)
    );
    for calendar in [false, true] {
        let events = Rc::new(RefCell::new(Vec::new()));
        let seen = events.clone();
        let accepting = Rc::new(Cell::new(true));
        let accepted = accepting.clone();
        let sub = Subscription::prepare_with_options(
            element,
            island,
            owner.clone(),
            1,
            Sequence::new(),
            Options {
                calendar,
                volume_scroll: true,
                volume_mute: true,
            },
            move |event| {
                if !accepted.get() {
                    return false;
                }
                seen.borrow_mut().push(event.input);
                true
            },
        )?;
        sub.activate()?;
        let tap = sub.probe()?;
        for _ in 0..10 {
            events.borrow_mut().clear();
            if !pointer(
                &sub,
                element,
                Input::VolumeMute,
                PointerUpdateKind::MiddleButtonPressed,
            )? || *events.borrow() != vec![Input::VolumeMute]
            {
                return Err("中键回归：静音按下未被处理".into());
            }
            // A handled press and any subsequent gesture events must produce
            // only one mute command, including native-calendar mode.
            for _ in 0..3 {
                if !tap.invoke()? || *events.borrow() != vec![Input::VolumeMute] {
                    return Err(format!(
                        "中键回归：静音后又展开日历，事件={:?}",
                        events.borrow()
                    ));
                }
            }
            // New primary input must open normally even if a prior middle
            // gesture was cancelled and never generated Tapped.
            for primary in [
                PointerUpdateKind::LeftButtonPressed,
                PointerUpdateKind::Other,
            ] {
                events.borrow_mut().clear();
                pointer(
                    &sub,
                    element,
                    Input::VolumeMute,
                    PointerUpdateKind::MiddleButtonPressed,
                )?;
                if pointer(&sub, element, Input::VolumeMute, primary)? {
                    return Err("中键回归：左键／触摸按下被错误消费".into());
                }
                if tap.invoke()? != calendar {
                    return Err("中键回归：下一次正常点击失效".into());
                }
                let expected = if calendar {
                    vec![Input::VolumeMute, Input::Pointer]
                } else {
                    vec![Input::VolumeMute]
                };
                if *events.borrow() != expected {
                    return Err("中键回归：后续主点击事件不匹配".into());
                }
            }
        }
        // Rejected delivery must not claim a mute occurred or consume native
        // behavior. A retained delegate after suspension remains inert.
        events.borrow_mut().clear();
        accepting.set(false);
        if pointer(
            &sub,
            element,
            Input::VolumeMute,
            PointerUpdateKind::MiddleButtonPressed,
        )? || tap.invoke()?
            || !events.borrow().is_empty()
        {
            return Err("中键回归：发送失败仍吞掉点击".into());
        }
        accepting.set(true);
        pointer(
            &sub,
            element,
            Input::VolumeMute,
            PointerUpdateKind::MiddleButtonPressed,
        )?;
        sub.suspend();
        if tap.invoke()? {
            return Err("中键回归：停用后仍消费点击".into());
        }
        sub.activate()?;
        // Tray model refresh temporarily suspends and resumes the same input
        // subscription. It must not turn a held middle gesture into a click.
        if !tap.invoke()? {
            return Err("中键回归：托盘刷新后中键继续交给系统日历".into());
        }
        if *events.borrow() != vec![Input::VolumeMute] {
            return Err("中键回归：托盘刷新后中键又展开日历".into());
        }
        pointer(
            &sub,
            element,
            Input::VolumeMute,
            PointerUpdateKind::LeftButtonPressed,
        )?;
        if tap.invoke()? != calendar {
            return Err("中键回归：托盘刷新后左键失效".into());
        }
    }
    Ok(())
}
