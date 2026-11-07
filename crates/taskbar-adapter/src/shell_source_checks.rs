//! Version-pinned experiment in freshly created, private Shell XAML hosts.
//! Only Host::create can construct a host; no borrowed Shell object, external
//! HWND or process is accepted. This is not a production TaskbarHost resolver.
use crate::{
    clock::load,
    discovery::View,
    island::{Binding, IID_NATIVE, Native},
    property::{equal_optional, same_object},
    routing::{self, BoundView},
    session::State,
    tray::{Field, read},
    wide,
    xaml::{self, Object, StringHandle, check, factory},
    xaml_abi::*,
};
use litetray_adapter_protocol::{
    clock::{Frame, Owner, Style, Text},
    compatibility::builtin,
    routing::{Options, Plan, Route},
};
use std::{
    cell::RefCell,
    ffi::c_void,
    ptr::{null, null_mut},
    rc::Rc,
};
use windows_sys::{
    Win32::{
        Foundation::*,
        System::{LibraryLoader::*, SystemInformation::GetTickCount64, Threading::*},
        UI::WindowsAndMessaging::*,
    },
    core::{GUID, HSTRING, IInspectable_Vtbl},
};
windows_link::link!("combase.dll" "system" fn RoInitialize(kind:u32)->i32);
windows_link::link!("combase.dll" "system" fn RoUninitialize());
const IID_FACTORY: GUID = GUID::from_u128(0xff119559_d636_5e6b_b169_390efeb2cf6e);
const IID_CHILD: GUID = GUID::from_u128(0xec97afd0_5c0d_581b_9508_074773c6f1e0);
const IID_CLOSABLE: GUID = GUID::from_u128(0x30d5a829_7fa4_4026_83bb_d75bae4ea99e);
const CLASS: &str = "Windows.Internal.Shell.XamlExplorerHost.XamlExplorerHostChildWindow";
#[repr(C)]
struct Factory {
    base: IInspectable_Vtbl,
    create: unsafe extern "system" fn(*mut c_void, u64, HSTRING, *mut *mut c_void) -> i32,
}
#[repr(C)]
struct Child {
    base: IInspectable_Vtbl,
    bounds: usize,
    set_bounds: unsafe extern "system" fn(*mut c_void, Rect) -> i32,
    content: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32,
    set_content: unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32,
    other_methods: [usize; 9],
    is_visible: unsafe extern "system" fn(*mut c_void, *mut u8) -> i32,
    set_is_visible: unsafe extern "system" fn(*mut c_void, u8) -> i32,
}
#[repr(C)]
struct Closable {
    base: IInspectable_Vtbl,
    close: unsafe extern "system" fn(*mut c_void) -> i32,
}
struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            RoUninitialize();
        }
    }
}
struct Parent(HWND);
impl Drop for Parent {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.0);
        }
    }
}
struct Module(HMODULE);
impl Drop for Module {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.0);
        }
    }
}
fn require(condition: bool, label: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(format!("私有宿主隔离检查失败：{label}"))
    }
}
fn pointer(address: usize) -> Result<usize, String> {
    let bytes = unsafe { crate::read_memory(address, 8) }.ok_or("读取本进程夹具失败")?;
    Ok(u64::from_le_bytes(bytes.try_into().map_err(|_| "无效指针长度")?) as usize)
}
fn inspected_module() -> Result<Module, String> {
    require(
        cfg!(target_arch = "x86_64"),
        "only inspected x64 ABI is supported",
    )?;
    let profile = builtin()?;
    let path = crate::compatibility::verified_file(&profile.shell)?;
    let module = Module(unsafe {
        LoadLibraryExW(
            wide(&path.to_string_lossy()).as_ptr(),
            null_mut(),
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    });
    require(!module.0.is_null(), "load inspected system module")?;
    require(
        unsafe { crate::compatibility::loaded(&profile.shell)? } == module.0 as usize,
        "loaded image and PDB identity",
    )?;
    Ok(module)
}
struct Host {
    // Release the source before closing the host, and the host before parent.
    source: Option<Object>,
    child: Object,
    closable: Object,
    closed: bool,
    parent: Parent,
    island: HWND,
}
impl Host {
    fn create(module: &Module, factory: &Object) -> Result<Self, String> {
        unsafe {
            let parent = Parent(CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("LiteTray isolated Shell host").as_ptr(),
                WS_OVERLAPPEDWINDOW,
                100,
                100,
                600,
                180,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            ));
            require(!parent.0.is_null(), "create own parent")?;
            let name = StringHandle::new("LiteTray isolated Shell host")?;
            let mut child = null_mut();
            check(
                (factory.table::<Factory>().create)(
                    factory.raw(),
                    parent.0 as u64,
                    name.0,
                    &mut child,
                ),
                "Create own Shell host",
            )?;
            let child = Object::owned(child)?.query(&IID_CHILD)?;
            let closable = child.query(&IID_CLOSABLE)?;
            let mut host = Self {
                source: None,
                child,
                closable,
                closed: false,
                parent,
                island: null_mut(),
            };
            require(
                pointer(host.child.raw() as usize)?
                    == module.0 as usize + builtin()?.source.child_vtable_rva as usize,
                "inspected concrete child vtable",
            )?;
            // Exact image + exact interface vtable + own freshly created host.
            // Offline getter chain: ABI this - 0x10, then member + 0x20.
            let source = pointer(
                host.child.raw() as usize + builtin()?.source.child_source_offset as usize,
            )?;
            let source = Object::query_borrowed(source as *mut _, &IID_IDesktopWindowXamlSource)?;
            let native = source.query(&IID_NATIVE)?;
            check(
                (native.table::<Native>().window)(native.raw(), &mut host.island),
                "Get own Shell island HWND",
            )?;
            let mut pid = 0;
            require(
                GetWindowThreadProcessId(host.island, &mut pid) == GetCurrentThreadId()
                    && pid == GetCurrentProcessId()
                    && GetParent(host.island) == host.parent.0
                    && IsChild(host.parent.0, host.island) != 0,
                "own source parent and UI thread",
            )?;
            host.source = Some(source);
            check(
                (host.child.table::<Child>().set_bounds)(
                    host.child.raw(),
                    Rect {
                        X: 0.,
                        Y: 0.,
                        Width: 520.,
                        Height: 130.,
                    },
                ),
                "Set own private host bounds",
            )?;
            check(
                (host.child.table::<Child>().set_is_visible)(host.child.raw(), 1),
                "Make own child visible within hidden parent",
            )?;
            Ok(host)
        }
    }
    fn source(&self) -> Result<&Object, String> {
        self.source.as_ref().ok_or("夹具 source 已关闭".into())
    }
    fn set_content(&self, root: &Object) -> Result<(), String> {
        let root = root.query(&IID_IUIElement)?;
        unsafe {
            check(
                (self.child.table::<Child>().set_content)(self.child.raw(), root.raw()),
                "Set own Shell XamlContent",
            )?;
            let mut returned = null_mut();
            check(
                (self.child.table::<Child>().content)(self.child.raw(), &mut returned),
                "Get own Shell XamlContent",
            )?;
            require(
                same_object(&root, &Object::owned(returned)?)?,
                "private getter returns assigned content",
            )?;
        }
        layout(&root)
    }
    fn close(&mut self) -> Result<(), String> {
        if !self.closed {
            unsafe {
                check(
                    (self.closable.table::<Closable>().close)(self.closable.raw()),
                    "Close own Shell host",
                )?;
            }
            self.closed = true;
        }
        Ok(())
    }
    fn route(&self, secondary: bool, device: &str) -> Route {
        Route {
            window: self.parent.0 as u64,
            process_id: unsafe { GetCurrentProcessId() },
            thread_id: unsafe { GetCurrentThreadId() },
            secondary,
            device: device.into(),
        }
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.source.take();
        let _ = self.close();
    }
}
fn layout(root: &Object) -> Result<(), String> {
    let root = root.query(&IID_IUIElement)?;
    unsafe {
        let table = root.table::<IUIElement_Vtbl>();
        check(
            (table.Measure)(
                root.raw(),
                Size {
                    Width: 520.,
                    Height: 130.,
                },
            ),
            "Measure own Shell content",
        )?;
        check(
            (table.Arrange)(
                root.raw(),
                Rect {
                    X: 0.,
                    Y: 0.,
                    Width: 520.,
                    Height: 130.,
                },
            ),
            "Arrange own Shell content",
        )?;
        check((table.UpdateLayout)(root.raw()), "Layout own Shell content")
    }
}
fn find(root: &Object, name: &str) -> Result<Object, String> {
    let root = root.query(&IID_IFrameworkElement)?;
    let name = StringHandle::new(name)?;
    unsafe {
        let mut value = null_mut();
        check(
            (root.table::<IFrameworkElement_Vtbl>().FindName)(root.raw(), name.0, &mut value),
            "Find own Shell fixture element",
        )?;
        Object::owned(value)
    }
}
fn text(node: &Object) -> Result<String, String> {
    let node = node.query(&IID_ITextBlock)?;
    unsafe { node.string(node.table::<ITextBlock_Vtbl>().Text, 4096) }
}
fn wait_for_attachment(roots: &[&Object]) -> Result<(), String> {
    let helper = factory(
        "Windows.UI.Xaml.Media.VisualTreeHelper",
        &IID_IVisualTreeHelperStatics,
    )?;
    // Initial visual attachment is asynchronous in the private host. Test its
    // actual parent relation and clock layout, not a fixed sleep or relaxed guard.
    let deadline = unsafe { GetTickCount64() } + 2000;
    loop {
        let mut ready = true;
        for root in roots {
            layout(root)?;
            let panel = find(root, "SystemTrayFrameGrid")?.query(&IID_IDependencyObject)?;
            let mut parent = null_mut();
            unsafe {
                check(
                    (helper.table::<IVisualTreeHelperStatics_Vtbl>().GetParent)(
                        helper.raw(),
                        panel.raw(),
                        &mut parent,
                    ),
                    "Check private host attachment",
                )?;
            }
            if parent.is_null() {
                ready = false;
            } else {
                require(
                    same_object(&unsafe { Object::owned(parent)? }, root)?,
                    "fixture panel parent is assigned root",
                )?;
                let view = View::from_panel(&panel)?;
                if view
                    .report
                    .selection
                    .as_ref()
                    .is_none_or(|s| s.clock.is_none())
                {
                    ready = false;
                }
            }
        }
        if ready {
            return Ok(());
        }
        if unsafe { GetTickCount64() } >= deadline {
            return Err("私有宿主可视树初始化超时".into());
        }
        unsafe {
            let mut message = std::mem::zeroed();
            for _ in 0..256 {
                if PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) == 0 {
                    break;
                }
                if message.message == WM_QUIT {
                    return Err("私有宿主检查线程提前退出".into());
                }
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            MsgWaitForMultipleObjectsEx(0, null(), 10, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
        }
    }
}
pub(crate) fn run() -> Result<serde_json::Value, String> {
    // Start on a fresh STA. The private host creates its XAML application;
    // initializing WindowsXamlManager first makes this build fail in creation.
    unsafe {
        check(RoInitialize(0), "Initialize isolated Shell STA")?;
    }
    let _apartment = Apartment;
    let module = inspected_module()?;
    let factory = factory(CLASS, &IID_FACTORY)?;
    require(
        pointer(factory.raw() as usize)?
            == module.0 as usize + builtin()?.source.factory_vtable_rva as usize,
        "inspected concrete factory vtable",
    )?;
    let first = Host::create(&module, &factory)?;
    let mut second = Host::create(&module, &factory)?;
    let source_root = load(crate::routing_checks::MARKUP)?;
    let target_root = load(
        &crate::routing_checks::MARKUP.replace("Control center model", "Target original model"),
    )?;
    first.set_content(&source_root)?;
    second.set_content(&target_root)?;
    wait_for_attachment(&[&source_root, &target_root])?;
    let a = Binding::capture(
        first.parent.0,
        first.source()?,
        &xaml::root_content(&source_root)?,
    )?;
    let b = Binding::capture(
        second.parent.0,
        second.source()?,
        &xaml::root_content(&target_root)?,
    )?;
    require(
        Binding::capture(
            first.parent.0,
            second.source()?,
            &xaml::root_content(&target_root)?,
        )
        .is_err()
            && Binding::capture(
                first.parent.0,
                first.source()?,
                &xaml::root_content(&target_root)?,
            )
            .is_err(),
        "swapped own host/root rejected",
    )?;
    let mut checks = vec![
        "shell_own_private_host_public_source_and_root_verified",
        "shell_own_host_swapped_source_and_root_rejected",
    ];
    let source_view = View::from_panel(&find(&source_root, "SystemTrayFrameGrid")?)?;
    let target_view = View::from_panel(&find(&target_root, "SystemTrayFrameGrid")?)?;
    let routes = [
        first.route(false, "ISOLATED-A"),
        second.route(true, "ISOLATED-B"),
    ];
    let plan = Plan::build(
        &routes,
        &["ISOLATED-B".into()],
        Options {
            clocks: true,
            tray: true,
            icons: true,
            control_center: true,
            copied_taskbars_only: false,
        },
    )?;
    let views = [
        BoundView::capture(routes[0].clone(), &source_view, first.source()?)?,
        BoundView::capture(routes[1].clone(), &target_view, second.source()?)?,
    ];
    let owner = Owner {
        process_id: unsafe { GetCurrentProcessId() },
        receiver: first.parent.0 as u64,
        nonce: 0x5348454c4c,
    };
    let frame = Frame {
        text: Text {
            top: "12:34:56".into(),
            bottom: "天气晴 22°C".into(),
            tooltip: Some("Own Shell host tooltip".into()),
        },
        style: Style {
            font_size: 12,
            font_family: "Segoe UI".into(),
            width: 240,
            compact_two_lines: false,
            top_color: Some("#79C0FF".into()),
            bottom_color: Some("#FFCC88".into()),
            alignment: litetray_adapter_protocol::clock::Alignment::Center,
            top_style: Default::default(),
            bottom_style: Default::default(),
        },
    };
    let events = Rc::new(RefCell::new(Vec::new()));
    let received = events.clone();
    let watchdog = routing::attach_interactive(
        &plan,
        &views,
        owner.clone(),
        unsafe { GetTickCount64() },
        move |event| {
            received.borrow_mut().push(event);
            true
        },
    )?;
    let probe = watchdog.input_probe(routes[1].window)?;
    require(
        !probe.invoke()? && events.borrow().is_empty(),
        "prepared input preserves default handling",
    )?;
    let target_time = find(&target_root, "TimeInnerTextBlock")?;
    let target_model = find(&target_root, "ControlCenterButton")?;
    let source_model = read(
        &find(&source_root, "ControlCenterButton")?,
        &Field::DataContext.property()?,
    )?;
    let original_model = read(&target_model, &Field::DataContext.property()?)?;
    require(
        !equal_optional(original_model.as_ref(), source_model.as_ref())?,
        "model sources differ before sharing",
    )?;
    watchdog.start(frame.clone())?;
    layout(&target_root)?;
    require(
        text(&target_time)? == frame.text.top
            && text(&find(&source_root, "TimeInnerTextBlock")?)? == "Original time"
            && equal_optional(
                read(&target_model, &Field::DataContext.property()?)?.as_ref(),
                source_model.as_ref(),
            )?,
        "selected Shell host clock and model update",
    )?;
    require(probe.invoke()?, "active Shell host input published")?;
    {
        let events = events.borrow();
        require(
            events.len() == 1
                && events[0].source_window == second.island as u64
                && events[0].clock_id == routes[1].window
                && events[0].owner == owner,
            "activation source matches private host source",
        )?;
        events[0].anchor.validate()?;
    }
    checks.push("shell_own_host_routed_clock_model_and_input_apply");
    watchdog.stop()?;
    require(
        !probe.invoke()?
            && watchdog.state() == State::Closed
            && text(&target_time)? == "Original time"
            && equal_optional(
                read(&target_model, &Field::DataContext.property()?)?.as_ref(),
                original_model.as_ref(),
            )?,
        "restore own Shell content and detach input",
    )?;
    checks.push("shell_own_host_stop_restores_clock_model_and_input");
    drop(probe);
    drop(watchdog);
    drop(views);
    // Rediscover after layout; old complete snapshots must never be reused.
    let source_view = View::from_panel(&find(&source_root, "SystemTrayFrameGrid")?)?;
    let target_view = View::from_panel(&find(&target_root, "SystemTrayFrameGrid")?)?;
    let views = [
        BoundView::capture(routes[0].clone(), &source_view, first.source()?)?,
        BoundView::capture(routes[1].clone(), &target_view, second.source()?)?,
    ];
    let watchdog =
        routing::attach_interactive(&plan, &views, owner, unsafe { GetTickCount64() }, |_| true)?;
    let probe = watchdog.input_probe(routes[1].window)?;
    watchdog.start(frame)?;
    let replacement = load(crate::routing_checks::MARKUP)?;
    second.set_content(&replacement)?;
    require(
        b.validate().is_err()
            && !probe.invoke()?
            && watchdog.state() == State::Closed
            && text(&target_time)? == "Original time",
        "replaced private content stops old session before input",
    )?;
    checks.push("shell_own_host_content_replacement_restores_and_rejects_input");
    a.validate()?;
    let fresh = Binding::capture(
        second.parent.0,
        second.source()?,
        &xaml::root_content(&replacement)?,
    )?;
    fresh.validate()?;
    second.close()?;
    require(
        fresh.validate().is_err(),
        "closed private host invalidates binding",
    )?;
    checks.push("shell_own_host_close_invalidates_public_source_binding");
    Ok(
        serde_json::json!({ "passed": true, "environment": "isolated_private_shell_host_in_app_process", "explorer_accessed": false,
        "module_sha256": builtin()?.shell.sha256, "checks": checks, "production_taskbar_verified": false, "input_method": "retained_native_delegate" }),
    )
}
