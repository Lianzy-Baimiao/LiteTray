//! Native input stays in Explorer. Scope coordinate conversion to the island
//! dispatching real input and reattach shared system menus before that input.
use crate::{
    hooks::{self, Entry},
    island::Binding,
    property::{Property, same_object},
    tray::{Field, read},
    tray_menu::Menus,
    xaml::{self, Object, check, factory},
    xaml_abi::*,
};
use litetray_adapter_protocol::{compatibility::TrayAbi, topology::Role};
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr::null_mut,
    rc::Rc,
    sync::{
        OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::{
        ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    },
    System::{LibraryLoader::*, SystemInformation::GetTickCount64},
    UI::{
        Controls::{WM_MOUSEHOVER, WM_MOUSELEAVE},
        WindowsAndMessaging::*,
    },
};

#[derive(Clone, Copy)]
struct PopupAnchor {
    bounds: RECT,
    work: RECT,
    tick: u64,
    popup: usize,
}
static OVERFLOW: std::sync::Mutex<Option<PopupAnchor>> = std::sync::Mutex::new(None);
pub(crate) struct Surface {
    pub role: Role,
    pub element: Object,
    pub items: Option<Object>,
}
pub(crate) struct Host {
    pub taskbar: HWND,
    pub secondary: bool,
    pub binding: Rc<Binding>,
    pub root: Object,
    pub surfaces: Vec<Surface>,
}
pub(crate) struct Runtime {
    hosts: Vec<Host>,
    active: Cell<bool>,
    started: Cell<bool>,
    restored: Cell<bool>,
    busy: Cell<bool>,
    owners: RefCell<Vec<(Role, usize)>>,
    last: Cell<Option<(usize, u64)>>,
}
thread_local! {
    static RUNTIME: RefCell<Option<Rc<Runtime>>> = const { RefCell::new(None) };
    static DISPATCH: Cell<Option<usize>> = const { Cell::new(None) };
    static RAW_DISPATCH: Cell<(u64, u32)> = const { Cell::new((0, 0)) };
    static TRACE: RefCell<std::collections::VecDeque<serde_json::Value>> = const { RefCell::new(std::collections::VecDeque::new()) };
    static MENUS: RefCell<Menus> = RefCell::new(Menus::default());
    static MENU_BUSY: Cell<bool> = const { Cell::new(false) };
}
fn trace(kind: &str, details: serde_json::Value) {
    TRACE.with(|values| {
        if let Ok(mut values) = values.try_borrow_mut() {
            if values.len() == 32 { values.pop_front(); }
            let (window, message) = RAW_DISPATCH.with(Cell::get);
            values.push_back(serde_json::json!({"kind":kind,"message":message,"window":window,"details":details}));
        }
    });
}
pub(crate) fn diagnostics() -> serde_json::Value {
    serde_json::json!({"active":runtime().is_some(),"recent":TRACE.with(|values| values.borrow().clone())})
}
fn runtime() -> Option<Rc<Runtime>> {
    RUNTIME
        .with(|state| state.borrow().clone())
        .filter(|r| r.active.get())
}
impl Runtime {
    pub fn new(hosts: Vec<Host>) -> Rc<Self> {
        Rc::new(Self {
            hosts,
            active: Cell::new(false),
            started: Cell::new(false),
            restored: Cell::new(false),
            busy: Cell::new(false),
            owners: RefCell::new(vec![]),
            last: Cell::new(None),
        })
    }
    pub fn start(self: &Rc<Self>) -> Result<(), String> {
        for host in &self.hosts {
            host.binding.validate()?;
        }
        hooks::activate()?;
        RUNTIME.with(|state| *state.borrow_mut() = Some(self.clone()));
        self.active.set(true);
        self.started.set(true);
        Ok(())
    }
    pub fn stop(&self) -> Result<(), String> {
        if !self.started.get() || self.restored.get() {
            return Ok(());
        }
        self.active.set(false);
        hooks::suspend()?;
        self.last.set(None);
        if let Ok(mut overflow) = OVERFLOW.lock() {
            *overflow = None;
        }
        RUNTIME.with(|state| {
            state.borrow_mut().take();
        });
        MENUS.with(|menus| menus.borrow_mut().restore())?;
        // Attaching the primary last restores the shell's per-item owner.
        if let Some(primary) = self.hosts.iter().find(|h| !h.secondary) {
            primary.binding.validate()?;
            for surface in &primary.surfaces {
                if let Some(items) = &surface.items {
                    reattach(items)?;
                }
            }
        }
        self.restored.set(true);
        Ok(())
    }
    fn host(&self, message: &MSG) -> Option<usize> {
        if !is_input(message.message) {
            return None;
        }
        self.hosts.iter().position(|host| unsafe {
            !message.hwnd.is_null() && GetAncestor(message.hwnd, GA_ROOT) == host.taskbar
        })
    }
    fn arm(&self, index: usize, point: POINT) -> Result<(), String> {
        // Recreating item containers while a button is held can consume the
        // press/drag. Transfer ownership on hover, before the press arrives.
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON,
        };
        if [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON]
            .into_iter()
            .any(|key| unsafe { GetAsyncKeyState(key as i32) < 0 })
        {
            return Ok(());
        }
        let host = &self.hosts[index];
        host.binding.validate()?;
        for surface in &host.surfaces {
            if !matches!(
                surface.role,
                Role::SystemIcons | Role::PrivacyIndicators | Role::ControlCenter
            ) {
                continue;
            }
            let Ok(bounds) = crate::input::bounds(&surface.element, host.binding.window()) else {
                continue;
            };
            if point.x < bounds.left
                || point.x >= bounds.right
                || point.y < bounds.top
                || point.y >= bounds.bottom
            {
                continue;
            }
            if self.owners.borrow().contains(&(surface.role, index)) {
                return Ok(());
            }
            if let Some(items) = &surface.items {
                reattach(items)?;
                let mut owners = self.owners.borrow_mut();
                owners.retain(|(role, _)| *role != surface.role);
                owners.push((surface.role, index));
            }
            break;
        }
        Ok(())
    }
    fn arm_overflow(&self, index: usize, message: &MSG) {
        if !matches!(
            message.message,
            WM_LBUTTONDOWN | WM_LBUTTONUP | WM_POINTERDOWN | WM_POINTERUP | WM_KEYDOWN
        ) {
            return;
        }
        let host = &self.hosts[index];
        let Some(surface) = host.surfaces.iter().find(|s| s.role == Role::HiddenIcons) else {
            return;
        };
        let Ok(rect) = crate::input::bounds(&surface.element, host.binding.window()) else {
            return;
        };
        if message.pt.x < rect.left
            || message.pt.x >= rect.right
            || message.pt.y < rect.top
            || message.pt.y >= rect.bottom
        {
            return;
        }
        unsafe {
            let monitor = MonitorFromWindow(host.taskbar, MONITOR_DEFAULTTONEAREST);
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info) == 0 {
                return;
            }
            if let Ok(mut anchor) = OVERFLOW.lock() {
                *anchor = Some(PopupAnchor {
                    bounds: RECT {
                        left: rect.left,
                        top: rect.top,
                        right: rect.right,
                        bottom: rect.bottom,
                    },
                    work: info.rcWork,
                    tick: GetTickCount64(),
                    popup: 0,
                });
            }
        }
    }
    fn preferred(&self) -> Option<usize> {
        let mut point = POINT { x: 0, y: 0 };
        if unsafe { GetCursorPos(&mut point) } != 0 {
            for (index, host) in self.hosts.iter().enumerate() {
                let mut rect: RECT = unsafe { std::mem::zeroed() };
                if unsafe { GetWindowRect(host.taskbar, &mut rect) } != 0
                    && point.x >= rect.left
                    && point.x < rect.right
                    && point.y >= rect.top
                    && point.y < rect.bottom
                {
                    return Some(index);
                }
            }
        }
        self.last
            .get()
            .filter(|(_, tick)| unsafe { GetTickCount64() }.saturating_sub(*tick) < 5000)
            .map(|(i, _)| i)
    }
    fn point(&self, index: usize, point: Point) -> Result<Point, String> {
        let host = &self.hosts[index];
        host.binding.validate()?;
        let root = xaml::root(&host.root)?;
        let mut scale = 0.;
        let mut origin = POINT { x: 0, y: 0 };
        unsafe {
            check(
                (root.table::<IXamlRoot_Vtbl>().RasterizationScale)(root.raw(), &mut scale),
                "Tray scale",
            )?;
            if ClientToScreen(host.binding.window(), &mut origin) == 0 {
                return Err("无法读取托盘原点".into());
            }
        }
        if !scale.is_finite() || scale <= 0. || !point.X.is_finite() || !point.Y.is_finite() {
            return Err("托盘坐标无效".into());
        }
        Ok(Point {
            X: (origin.x as f64 + point.X as f64 * scale).round() as f32,
            Y: (origin.y as f64 + point.Y as f64 * scale).round() as f32,
        })
    }
    fn rect(&self, element: &Object) -> Result<RECT, String> {
        let root = xaml::root_content(element)?;
        let owner = self
            .hosts
            .iter()
            .position(|h| same_object(&h.root, &root).unwrap_or(false))
            .ok_or("非托盘元素")?;
        let selected = self.preferred().unwrap_or(owner);
        let host = &self.hosts[selected];
        host.binding.validate()?;
        let target = if owner == selected {
            element.clone()
        } else {
            let field = Field::DataContext.property()?;
            let model = read(element, &field)?.ok_or("托盘元素没有模型")?;
            let class = element.class_name()?;
            let mut twin = None;
            xaml::walk(host.root.clone(), |candidate, info| {
                if info.class_name == class
                    && let Some(other) = read(candidate, &field)?
                    && same_object(&other, &model)?
                {
                    twin = Some(candidate.clone());
                }
                Ok(())
            })?;
            twin.ok_or("目标屏幕没有对应图标")?
        };
        let rect = crate::input::bounds(&target, host.binding.window())?;
        Ok(RECT {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        })
    }
}
fn reattach(element: &Object) -> Result<(), String> {
    if element.try_query(&IID_IItemsControl)?.is_none() {
        return Ok(());
    }
    let property = Field::ItemsSource.property()?;
    if read(element, &property)?.is_none() {
        return Ok(());
    }
    let mut lease = Property::capture(element, property)?;
    lease.set_nullable(None)?;
    lease.restore()?;
    let ui = element.query(&IID_IUIElement)?;
    unsafe {
        check(
            (ui.table::<IUIElement_Vtbl>().UpdateLayout)(ui.raw()),
            "Reattach tray layout",
        )
    }
}
fn is_input(message: u32) -> bool {
    (WM_MOUSEFIRST..=WM_MOUSELAST).contains(&message)
        || (0x0238..=0x0257).contains(&message)
        || (WM_KEYFIRST..=WM_KEYLAST).contains(&message)
        || matches!(
            message,
            WM_CONTEXTMENU
                | WM_TIMER
                | WM_MOUSEHOVER
                | WM_MOUSELEAVE
                | WM_NCMOUSEHOVER
                | WM_NCMOUSELEAVE
        )
}
pub(crate) fn check_hover_routes(hosts: Vec<Host>) -> Result<&'static str, String> {
    let runtime = Runtime::new(hosts);
    for (index, host) in runtime.hosts.iter().enumerate() {
        for event in [WM_MOUSEMOVE, WM_TIMER, WM_MOUSEHOVER, WM_MOUSELEAVE] {
            let message = MSG {
                hwnd: host.binding.window(),
                message: event,
                ..unsafe { std::mem::zeroed() }
            };
            if runtime.host(&message) != Some(index) {
                return Err(format!(
                    "原生延迟悬停路由丢失：message=0x{event:X}, target={index}"
                ));
            }
            let mapped = runtime.point(index, Point { X: 0., Y: 0. })?;
            let mut origin = POINT { x: 0, y: 0 };
            unsafe {
                if ClientToScreen(host.binding.window(), &mut origin) == 0 {
                    return Err("悬停夹具坐标无效".into());
                }
            }
            if mapped.X != origin.x as f32 || mapped.Y != origin.y as f32 {
                return Err("延迟悬停未映射到所属屏幕".into());
            }
        }
    }
    let unowned = MSG {
        message: WM_TIMER,
        ..unsafe { std::mem::zeroed() }
    };
    if runtime.host(&unowned).is_some() {
        return Err("无窗口定时器被误认成托盘悬停".into());
    }
    Ok("native_island_delayed_hover_timer_and_leave_keep_screen_coordinates")
}
static DISPATCH_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static POINT_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static BOUNDS_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static RECT_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static POSITION_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static MENU_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static MENU_OPTIONS_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static FLYOUT_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static FLYOUT_OPTIONS_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
unsafe extern "system" fn dispatch(message: *const MSG) -> LRESULT {
    unsafe {
        let original: unsafe extern "system" fn(*const MSG) -> LRESULT =
            std::mem::transmute(DISPATCH_ORIGINAL.load(Ordering::Acquire));
        let state = runtime();
        let selected = state
            .as_ref()
            .and_then(|r| message.as_ref().and_then(|m| r.host(m)));
        if let (Some(state), Some(index), Some(message)) = (&state, selected, message.as_ref())
            && !state.busy.replace(true)
        {
            let _ = state.arm(index, message.pt);
            state.arm_overflow(index, message);
            state.busy.set(false);
        }
        let previous = DISPATCH.with(|value| value.replace(selected));
        let raw = RAW_DISPATCH.with(|value| {
            value.replace(
                message
                    .as_ref()
                    .map(|m| (m.hwnd as u64, m.message))
                    .unwrap_or_default(),
            )
        });
        let result = original(message);
        RAW_DISPATCH.with(|value| value.set(raw));
        DISPATCH.with(|value| value.set(previous));
        result
    }
}
unsafe extern "system" fn position(
    hwnd: HWND,
    after: HWND,
    mut x: i32,
    mut y: i32,
    cx: i32,
    cy: i32,
    mut flags: u32,
) -> i32 {
    unsafe {
        let original: unsafe extern "system" fn(HWND, HWND, i32, i32, i32, i32, u32) -> i32 =
            std::mem::transmute(POSITION_ORIGINAL.load(Ordering::Acquire));
        if crate::class_name(hwnd) == "TopLevelWindowForOverflowXamlIsland" {
            let anchor = OVERFLOW.lock().ok().and_then(|state| *state);
            if let Some(mut anchor) = anchor {
                if flags & SWP_HIDEWINDOW != 0 {
                    if let Ok(mut state) = OVERFLOW.lock() {
                        *state = None;
                    }
                } else if (anchor.popup == hwnd as usize && IsWindowVisible(hwnd) != 0)
                    || GetTickCount64().saturating_sub(anchor.tick) < 2000
                {
                    let mut current: RECT = std::mem::zeroed();
                    if GetWindowRect(hwnd, &mut current) != 0 {
                        let width = if flags & SWP_NOSIZE != 0 {
                            current.right - current.left
                        } else {
                            cx
                        };
                        let height = if flags & SWP_NOSIZE != 0 {
                            current.bottom - current.top
                        } else {
                            cy
                        };
                        if let Some((left, top)) =
                            popup_position(anchor.bounds, anchor.work, width, height)
                        {
                            x = left;
                            y = top;
                            flags &= !SWP_NOMOVE;
                            anchor.popup = hwnd as usize;
                            if let Ok(mut state) = OVERFLOW.lock() {
                                *state = Some(anchor);
                            }
                        }
                    }
                }
            }
        }
        original(hwnd, after, x, y, cx, cy, flags)
    }
}
fn popup_position(anchor: RECT, work: RECT, width: i32, height: i32) -> Option<(i32, i32)> {
    if width <= 0 || height <= 0 || work.right <= work.left || work.bottom <= work.top {
        return None;
    }
    let x = (anchor.left + (anchor.right - anchor.left - width) / 2)
        .clamp(work.left, (work.right - width).max(work.left));
    let y = if anchor.top >= work.bottom {
        anchor.top - height - 8
    } else if anchor.bottom <= work.top {
        anchor.bottom + 8
    } else {
        anchor.top - height
    };
    Some((x, y.clamp(work.top, (work.bottom - height).max(work.top))))
}
unsafe extern "system" fn invocation(
    this: *mut c_void,
    result: *mut Point,
    point: *const Point,
) -> *mut Point {
    unsafe {
        let original: unsafe extern "system" fn(
            *mut c_void,
            *mut Point,
            *const Point,
        ) -> *mut Point = std::mem::transmute(POINT_ORIGINAL.load(Ordering::Acquire));
        let returned = original(this, result, point);
        let native = returned.as_ref().map(|p| (p.X, p.Y));
        if let (Some(state), Some(index), Some(point), Some(result)) = (
            runtime(),
            DISPATCH.with(Cell::get),
            point.as_ref(),
            returned.as_mut(),
        ) {
            state.last.set(Some((index, GetTickCount64())));
            if state.hosts[index].secondary
                && let Ok(mapped) = state.point(index, *point)
            {
                *result = mapped;
            }
        }
        trace(
            "invocation",
            serde_json::json!({"route":DISPATCH.with(Cell::get), "native":native, "returned":returned.as_ref().map(|p| (p.X, p.Y))}),
        );
        returned
    }
}
unsafe extern "system" fn icon_bounds(
    this: *mut c_void,
    result: *mut Rect,
    rect: *const Rect,
) -> *mut Rect {
    unsafe {
        let original: unsafe extern "system" fn(*mut c_void, *mut Rect, *const Rect) -> *mut Rect =
            std::mem::transmute(BOUNDS_ORIGINAL.load(Ordering::Acquire));
        let returned = original(this, result, rect);
        let native = returned.as_ref().map(|r| (r.X, r.Y, r.Width, r.Height));
        if let (Some(state), Some(index), Some(rect), Some(result)) = (
            runtime(),
            DISPATCH.with(Cell::get),
            rect.as_ref(),
            returned.as_mut(),
        ) {
            state.last.set(Some((index, GetTickCount64())));
            if state.hosts[index].secondary
                && let (Ok(a), Ok(b)) = (
                    state.point(
                        index,
                        Point {
                            X: rect.X,
                            Y: rect.Y,
                        },
                    ),
                    state.point(
                        index,
                        Point {
                            X: rect.X + rect.Width,
                            Y: rect.Y + rect.Height,
                        },
                    ),
                )
            {
                *result = Rect {
                    X: a.X,
                    Y: a.Y,
                    Width: b.X - a.X,
                    Height: b.Y - a.Y,
                };
            }
        }
        trace(
            "bounds",
            serde_json::json!({"route":DISPATCH.with(Cell::get), "native":native, "returned":returned.as_ref().map(|r| (r.X, r.Y, r.Width, r.Height))}),
        );
        returned
    }
}
// MSVC x64 C++ free functions returning a 16-byte RECT use a hidden first
// result pointer. The original consumes its by-value FrameworkElement temp.
unsafe extern "system" fn screen_rect(
    result: *mut RECT,
    element: *const *mut c_void,
    hwnd: HWND,
) -> *mut RECT {
    unsafe {
        let retained = element
            .as_ref()
            .and_then(|raw| Object::query_borrowed(*raw, &IID_IFrameworkElement).ok());
        let original: unsafe extern "system" fn(*mut RECT, *const *mut c_void, HWND) -> *mut RECT =
            std::mem::transmute(RECT_ORIGINAL.load(Ordering::Acquire));
        let returned = original(result, element, hwnd);
        finish_screen_rect(returned, retained)
    }
}
// DragDropManager::ScreenRectForElement uses a hidden second result pointer
// and borrows the FrameworkElement wrapper. Some native callers leave `this`
// unspecified, so it must be forwarded without validation or dereferencing.
unsafe extern "system" fn screen_rect_borrowed(
    opaque_this: *mut c_void,
    result: *mut RECT,
    element: *const *mut c_void,
    hwnd: HWND,
) -> *mut RECT {
    unsafe {
        let retained = element
            .as_ref()
            .and_then(|raw| Object::query_borrowed(*raw, &IID_IFrameworkElement).ok());
        let original: unsafe extern "system" fn(
            *mut c_void,
            *mut RECT,
            *const *mut c_void,
            HWND,
        ) -> *mut RECT = std::mem::transmute(RECT_ORIGINAL.load(Ordering::Acquire));
        let returned = original(opaque_this, result, element, hwnd);
        finish_screen_rect(returned, retained)
    }
}
unsafe fn finish_screen_rect(returned: *mut RECT, retained: Option<Object>) -> *mut RECT {
    unsafe {
        let native = returned
            .as_ref()
            .map(|r| (r.left, r.top, r.right, r.bottom));
        if let (Some(state), Some(element), Some(result)) = (runtime(), retained, returned.as_mut())
            && let Ok(corrected) = state.rect(&element)
        {
            *result = corrected;
        }
        trace(
            "query_rect",
            serde_json::json!({"native":native, "returned":returned.as_ref().map(|r| (r.left, r.top, r.right, r.bottom))}),
        );
        returned
    }
}
fn screen_rect_entry(abi: TrayAbi, target: usize) -> Entry {
    let (detour, arguments) = match abi {
        TrayAbi::SystemTrayCoordinatesX64V1 => (screen_rect as *const () as usize, 3),
        TrayAbi::SystemTrayDragDropBorrowedCoordinatesX64V1 => {
            (screen_rect_borrowed as *const () as usize, 4)
        }
    };
    Entry {
        target,
        detour,
        original: &RECT_ORIGINAL,
        arguments,
    }
}
fn shared_target(element: &Object) -> Result<bool, String> {
    let helper = factory(
        "Windows.UI.Xaml.Media.VisualTreeHelper",
        &IID_IVisualTreeHelperStatics,
    )?;
    let mut node = element.clone();
    for _ in 0..32 {
        if let Ok(fe) = node.query(&IID_IFrameworkElement) {
            let name = unsafe { fe.string(fe.table::<IFrameworkElement_Vtbl>().Name, 128)? };
            if ["MainStack", "NonActivatableStack", "ControlCenterButton"].contains(&name.as_str())
            {
                return Ok(true);
            }
        }
        let mut parent = null_mut();
        // WinRT parameters require the declared ABI interface, not merely a
        // different interface on the same COM object. Passing FrameworkElement
        // here terminates XAML with FAST_FAIL_GUARD_ICALL_CHECK_FAILURE (10).
        let dependency = node.query(&IID_IDependencyObject)?;
        unsafe {
            check(
                (helper.table::<IVisualTreeHelperStatics_Vtbl>().GetParent)(
                    helper.raw(),
                    dependency.raw(),
                    &mut parent,
                ),
                "Shared menu parent",
            )?;
        }
        if parent.is_null() {
            return Ok(false);
        }
        node = unsafe { Object::owned(parent)? };
    }
    Ok(false)
}
pub(crate) fn check_context_target() -> Result<Vec<&'static str>, String> {
    // A normal taskbar context menu enters the same ancestry guard, starting
    // from IFrameworkElement rather than IDependencyObject.
    let root = crate::clock::load(
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"/>"#,
    )?;
    if shared_target(&root.query(&IID_IFrameworkElement)?)? {
        return Err("普通任务栏菜单目标被误识别为共享托盘菜单".into());
    }
    Ok(vec![
        "native_context_menu_framework_element_ancestry_does_not_crash",
    ])
}
pub(crate) fn check_context_ancestry(
    target: &Object,
    shared: bool,
) -> Result<&'static str, String> {
    for interface in [
        IID_IFrameworkElement,
        windows_sys::core::IID_IUnknown,
        IID_IDependencyObject,
    ] {
        if shared_target(&target.query(&interface)?)? != shared {
            return Err("菜单祖先分类或接口规范化失败".into());
        }
        // Also exercise the general tree reader with each caller interface.
        xaml::walk(target.query(&interface)?, |_, _| Ok(()))?;
    }
    Ok(if shared {
        "native_shared_menu_ancestor_accepts_framework_dependency_and_identity_interfaces"
    } else {
        "native_ordinary_taskbar_menu_nested_target_is_not_redirected"
    })
}
unsafe fn show(
    original: &AtomicUsize,
    this: *mut c_void,
    target: *mut c_void,
    options: Option<*mut c_void>,
) -> i32 {
    unsafe {
        let call = || {
            if let Some(options) = options {
                let original: unsafe extern "system" fn(
                    *mut c_void,
                    *mut c_void,
                    *mut c_void,
                ) -> i32 = std::mem::transmute(original.load(Ordering::Acquire));
                original(this, target, options)
            } else {
                let original: unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32 =
                    std::mem::transmute(original.load(Ordering::Acquire));
                original(this, target)
            }
        };
        let Ok(element) = Object::query_borrowed(target, &IID_IFrameworkElement) else {
            return call();
        };
        if !shared_target(&element).unwrap_or(false) {
            return call();
        }
        if MENU_BUSY.with(|busy| busy.replace(true)) {
            return 0;
        }
        let prepared = (|| {
            let menu = Object::query_borrowed(this, &IID_IFlyoutBase)?;
            let root = xaml::root(&element)?;
            MENUS.with(|menus| menus.borrow_mut().prepare(&menu, &root))
        })();
        MENU_BUSY.with(|busy| busy.set(false));
        match prepared {
            Ok(None) => call(),
            Ok(Some(proxy)) => {
                let base = match proxy.query(&IID_IFlyoutBase) {
                    Ok(base) => base,
                    Err(_) => return 0,
                };
                (base.table::<IFlyoutBase_Vtbl>().ShowAt)(base.raw(), element.raw())
            }
            // Never let a known cross-island menu reach the crashing native call.
            Err(_) => 0,
        }
    }
}
unsafe extern "system" fn menu(this: *mut c_void, target: *mut c_void) -> i32 {
    unsafe { show(&MENU_ORIGINAL, this, target, None) }
}
unsafe extern "system" fn menu_options(
    this: *mut c_void,
    target: *mut c_void,
    options: *mut c_void,
) -> i32 {
    unsafe { show(&MENU_OPTIONS_ORIGINAL, this, target, Some(options)) }
}
unsafe extern "system" fn flyout(this: *mut c_void, target: *mut c_void) -> i32 {
    unsafe { show(&FLYOUT_ORIGINAL, this, target, None) }
}
unsafe extern "system" fn flyout_options(
    this: *mut c_void,
    target: *mut c_void,
    options: *mut c_void,
) -> i32 {
    unsafe { show(&FLYOUT_OPTIONS_ORIGINAL, this, target, Some(options)) }
}
pub(crate) fn install(
    tray: &litetray_adapter_protocol::compatibility::ResolvedTray,
) -> Result<(), String> {
    // Every new session must still match the same disk and loaded image.
    // Only the first installation checks unpatched function prefixes below.
    unsafe {
        crate::compatibility::loaded(&tray.module)?;
    }
    static INSTALLED: OnceLock<Result<String, String>> = OnceLock::new();
    let installed = INSTALLED.get_or_init(|| unsafe {
        let addresses = crate::tray_symbols::resolve(tray)?;
        let user32 = GetModuleHandleW(crate::wide("user32.dll").as_ptr());
        let dispatch_address = GetProcAddress(user32, c"DispatchMessageW".as_ptr().cast()).ok_or("DispatchMessageW 不可用")? as usize;
        let mut entries = vec![
            Entry { target: dispatch_address, detour: dispatch as *const () as usize, original: &DISPATCH_ORIGINAL, arguments: 1 },
            Entry { target: addresses[0], detour: invocation as *const () as usize, original: &POINT_ORIGINAL, arguments: 3 },
            Entry { target: addresses[1], detour: icon_bounds as *const () as usize, original: &BOUNDS_ORIGINAL, arguments: 3 },
            screen_rect_entry(tray.abi, addresses[2]),
            Entry { target: GetProcAddress(user32, c"SetWindowPos".as_ptr().cast()).ok_or("SetWindowPos 不可用")? as usize, detour: position as *const () as usize, original: &POSITION_ORIGINAL, arguments: 7 },
        ];
        for (markup, regular, options, old, old_options) in [
            (r#"<MenuFlyout xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"/>"#, menu as *const () as usize, menu_options as *const () as usize, &MENU_ORIGINAL, &MENU_OPTIONS_ORIGINAL),
            (r#"<Flyout xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"/>"#, flyout as *const () as usize, flyout_options as *const () as usize, &FLYOUT_ORIGINAL, &FLYOUT_OPTIONS_ORIGINAL),
        ] {
            let object = crate::clock::load(markup)?;
            let base = object.query(&IID_IFlyoutBase)?;
            let base5 = object.query(&IID_IFlyoutBase5)?;
            entries.push(Entry { target: base.table::<IFlyoutBase_Vtbl>().ShowAt as usize, detour: regular, original: old, arguments: 2 });
            entries.push(Entry { target: base5.table::<IFlyoutBase5_Vtbl>().ShowAt as usize, detour: options, original: old_options, arguments: 3 });
        }
            hooks::install(&entries, 5)?;
            Ok(tray.module.sha256.clone())
    }).as_ref().map_err(Clone::clone)?;
    if *installed != tray.module.sha256 {
        return Err("托盘钩子已绑定其他兼容资料，拒绝混用布局".into());
    }
    Ok(())
}
#[cfg(test)]
#[path = "tray_interaction_checks.rs"]
mod checks;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overflow_anchors_to_clicked_monitor_and_clamps_to_its_work_area() {
        let anchor = RECT {
            left: 6000,
            top: 1540,
            right: 6040,
            bottom: 1600,
        };
        let work = RECT {
            left: 3840,
            top: 0,
            right: 6400,
            bottom: 1540,
        };
        assert_eq!(popup_position(anchor, work, 300, 240), Some((5870, 1292)));
        let anchor = RECT {
            left: -50,
            top: 1040,
            right: -10,
            bottom: 1080,
        };
        let work = RECT {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1040,
        };
        assert_eq!(popup_position(anchor, work, 300, 240), Some((-300, 792)));
        assert_eq!(popup_position(anchor, work, 0, 240), None);
    }
}
