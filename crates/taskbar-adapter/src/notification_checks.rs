//! Own hidden native windows and real XAML sources exercise the extra parent
//! level without discovering Explorer or changing display configuration.
use crate::{
    island::{Binding, IID_NATIVE, Native, RestoreTarget},
    preview::activate,
    wide,
    xaml::{self, Object, check, factory},
    xaml_abi::*,
};
use std::{
    ffi::c_void,
    ptr::{null, null_mut},
    rc::Rc,
};
use windows_sys::{
    Win32::{Foundation::*, System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*},
    core::{GUID, IInspectable_Vtbl},
};

const CLOSABLE: GUID = GUID::from_u128(0x30d5a829_7fa4_4026_83bb_d75bae4ea99e);
#[repr(C)]
struct Closable {
    base: IInspectable_Vtbl,
    close: unsafe extern "system" fn(*mut c_void) -> i32,
}
fn close(value: &Object) {
    if let Ok(value) = value.query(&CLOSABLE) {
        unsafe {
            let _ = (value.table::<Closable>().close)(value.raw());
        }
    }
}
fn require(value: bool, detail: &str) -> Result<(), String> {
    if value {
        Ok(())
    } else {
        Err(format!("通知区宿主隔离检查失败：{detail}"))
    }
}
struct Windows {
    taskbar: HWND,
    parent: HWND,
    alternate: HWND,
    classes: Vec<Vec<u16>>,
}
impl Windows {
    fn new() -> Result<Self, String> {
        let mut own = Self {
            taskbar: null_mut(),
            parent: null_mut(),
            alternate: null_mut(),
            classes: Vec::new(),
        };
        unsafe {
            let instance = GetModuleHandleW(null());
            for class in ["Shell_TrayWnd", "TrayNotifyWnd"] {
                let class = wide(class);
                let definition = WNDCLASSW {
                    lpfnWndProc: Some(DefWindowProcW),
                    hInstance: instance,
                    lpszClassName: class.as_ptr(),
                    ..std::mem::zeroed()
                };
                require(
                    RegisterClassW(&definition) != 0,
                    "register own local fixture class",
                )?;
                own.classes.push(class);
            }
            own.taskbar = CreateWindowExW(
                0,
                own.classes[0].as_ptr(),
                wide("LiteTray isolated notification root").as_ptr(),
                WS_POPUP,
                0,
                0,
                200,
                100,
                null_mut(),
                null_mut(),
                instance,
                null(),
            );
            require(!own.taskbar.is_null(), "create own taskbar root")?;
            own.parent = CreateWindowExW(
                0,
                own.classes[1].as_ptr(),
                wide("LiteTray isolated notification parent").as_ptr(),
                WS_CHILD,
                0,
                0,
                200,
                100,
                own.taskbar,
                null_mut(),
                instance,
                null(),
            );
            require(!own.parent.is_null(), "create own notification parent")?;
            own.alternate = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("LiteTray isolated wrong parent").as_ptr(),
                WS_CHILD,
                0,
                0,
                200,
                100,
                own.taskbar,
                null_mut(),
                instance,
                null(),
            );
            require(!own.alternate.is_null(), "create own alternate parent")?;
        }
        Ok(own)
    }
}
impl Drop for Windows {
    fn drop(&mut self) {
        unsafe {
            if !self.taskbar.is_null() {
                DestroyWindow(self.taskbar);
            }
            for class in self.classes.iter().rev() {
                UnregisterClassW(class.as_ptr(), GetModuleHandleW(null()));
            }
        }
    }
}
struct Fixture {
    source: Option<Object>,
    manager: Object,
    windows: Windows,
}
impl Fixture {
    fn new() -> Result<Self, String> {
        let windows = Windows::new()?;
        let initializer = factory(
            "Windows.UI.Xaml.Hosting.WindowsXamlManager",
            &IID_IWindowsXamlManagerStatics,
        )?;
        let mut manager = null_mut();
        unsafe {
            check(
                (initializer
                    .table::<IWindowsXamlManagerStatics_Vtbl>()
                    .InitializeForCurrentThread)(initializer.raw(), &mut manager),
                "Initialize notification fixture",
            )?;
        }
        let mut fixture = Self {
            source: None,
            manager: unsafe { Object::owned(manager)? },
            windows,
        };
        let source = activate("Windows.UI.Xaml.Hosting.DesktopWindowXamlSource")?
            .query(&IID_IDesktopWindowXamlSource)?;
        fixture.source = Some(source);
        let native = fixture.source().query(&IID_NATIVE)?;
        unsafe {
            check(
                (native.table::<Native>().attach)(native.raw(), fixture.windows.parent),
                "Attach own notification source",
            )?;
        }
        let mut island = null_mut();
        unsafe {
            check(
                (native.table::<Native>().window)(native.raw(), &mut island),
                "Get own notification HWND",
            )?;
            require(
                SetWindowPos(
                    island,
                    null_mut(),
                    0,
                    0,
                    200,
                    100,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                ) != 0,
                "size own notification island",
            )?;
        }
        fixture.replace()?;
        Ok(fixture)
    }
    fn source(&self) -> &Object {
        self.source.as_ref().unwrap()
    }
    fn replace(&self) -> Result<Object, String> {
        let root = crate::clock::load(r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Width="160" Height="60"><TextBlock Text="Isolated notification"/></Grid>"#)?.query(&IID_IUIElement)?;
        unsafe {
            check(
                (self
                    .source()
                    .table::<IDesktopWindowXamlSource_Vtbl>()
                    .SetContent)(self.source().raw(), root.raw()),
                "Set own notification content",
            )?;
            let table = root.table::<IUIElement_Vtbl>();
            check(
                (table.Measure)(
                    root.raw(),
                    Size {
                        Width: 160.,
                        Height: 60.,
                    },
                ),
                "Measure notification fixture",
            )?;
            check(
                (table.Arrange)(
                    root.raw(),
                    Rect {
                        X: 0.,
                        Y: 0.,
                        Width: 160.,
                        Height: 60.,
                    },
                ),
                "Arrange notification fixture",
            )?;
            check(
                (table.UpdateLayout)(root.raw()),
                "Layout notification fixture",
            )?;
        }
        Ok(root)
    }
    fn binding(&self) -> Result<Rc<Binding>, String> {
        let mut root = null_mut();
        unsafe {
            check(
                (self
                    .source()
                    .table::<IDesktopWindowXamlSource_Vtbl>()
                    .Content)(self.source().raw(), &mut root),
                "Read notification fixture root",
            )?;
            let root = Object::owned(root)?;
            Binding::capture_notification(
                self.windows.taskbar,
                self.windows.parent,
                self.source(),
                &xaml::root_content(&root)?,
            )
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(source) = self.source.take() {
            close(&source);
        }
        close(&self.manager);
    }
}
struct Reparent(HWND, HWND);
impl Drop for Reparent {
    fn drop(&mut self) {
        unsafe {
            SetParent(self.0, self.1);
        }
    }
}

pub(crate) fn run() -> Result<Vec<&'static str>, String> {
    let fixture = Fixture::new()?;
    let binding = fixture.binding()?;
    binding.validate()?;
    require(
        binding.taskbar() == fixture.windows.taskbar
            && binding.restore_target()? == RestoreTarget::Current,
        "three-window binding retained",
    )?;
    let different = crate::clock::load(
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"/>"#,
    )?;
    require(
        Binding::capture_notification(
            fixture.windows.taskbar,
            fixture.windows.parent,
            fixture.source(),
            &different,
        )
        .is_err(),
        "foreign root rejected",
    )?;
    require(
        Binding::capture_notification(
            fixture.windows.taskbar,
            fixture.windows.alternate,
            fixture.source(),
            &different,
        )
        .is_err(),
        "wrong notification class rejected",
    )?;
    require(
        Binding::capture_notification(
            fixture.windows.alternate,
            fixture.windows.parent,
            fixture.source(),
            &different,
        )
        .is_err(),
        "wrong taskbar parent rejected",
    )?;
    for (window, parent) in [
        (binding.window(), fixture.windows.alternate),
        (fixture.windows.parent, fixture.windows.alternate),
    ] {
        let previous = unsafe { SetParent(window, parent) };
        require(!previous.is_null(), "reparent own fixture window")?;
        let restore = Reparent(window, previous);
        require(
            binding.validate().is_err() && binding.restore_target().is_err(),
            "both direct links required",
        )?;
        drop(restore);
        binding.validate()?;
    }
    fixture.replace()?;
    require(
        binding.validate().is_err() && binding.restore_target()? == RestoreTarget::Retired,
        "replacement root retires original binding",
    )?;
    let mut binding = fixture.binding()?;
    Rc::get_mut(&mut binding)
        .unwrap()
        .invalidate_notification_monitor_for_test();
    require(
        binding.validate().is_err(),
        "active binding rejects changed monitor snapshot",
    )?;
    require(
        binding.restore_target()? == RestoreTarget::Current,
        "same source still permits restoration after monitor change",
    )?;
    close(fixture.source());
    require(
        binding.validate().is_err() && binding.restore_target()? == RestoreTarget::Retired,
        "closed native source retires binding",
    )?;
    Ok(vec![
        "notification_island_owns_exact_three_window_chain",
        "notification_island_rejects_foreign_root_and_wrong_parents",
        "notification_island_rechecks_both_direct_parent_links",
        "notification_island_retires_replaced_root_and_closed_source",
        "notification_monitor_change_blocks_writes_but_allows_current_source_restore",
    ])
}
