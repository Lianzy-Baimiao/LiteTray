//! A source, content root and HWND must come from one live XAML island.
//! Coordinates or a window class name are not evidence of this relationship.
use crate::{
    property::same_object,
    window_guard::WindowGuard,
    xaml::{self, Object, check},
    xaml_abi::*,
};
use std::{ffi::c_void, ptr::null_mut, rc::Rc};
use windows_sys::{
    Win32::{Foundation::HWND, System::Threading::*, UI::WindowsAndMessaging::*},
    core::{GUID, IUnknown_Vtbl},
};

// SDK windows.ui.xaml.hosting.desktopwindowxamlsource.h, also used by preview.
pub(crate) const IID_NATIVE: GUID = GUID::from_u128(0x3cbcf1bf_2f76_4e9c_96ab_e84b37972554);
#[repr(C)]
pub(crate) struct Native {
    pub base: IUnknown_Vtbl,
    pub attach: unsafe extern "system" fn(*mut c_void, HWND) -> i32,
    pub window: unsafe extern "system" fn(*mut c_void, *mut HWND) -> i32,
}

pub(crate) struct Binding {
    source: Object,
    native: Object,
    content: Object,
    root: Object,
    root_content: Object,
    parent: HWND,
    window: HWND,
    parent_guard: WindowGuard,
    window_guard: WindowGuard,
}
fn local_pair(parent: HWND, window: HWND) -> Result<(), String> {
    unsafe {
        for handle in [parent, window] {
            let mut pid = 0;
            if handle.is_null()
                || GetWindowThreadProcessId(handle, &mut pid) != GetCurrentThreadId()
                || pid != GetCurrentProcessId()
            {
                return Err("XAML 绑定窗口不属于当前进程和 UI 线程".into());
            }
        }
        if parent == window || GetParent(window) != parent || IsChild(parent, window) == 0 {
            return Err("XAML source 的原生窗口不属于给定父窗口".into());
        }
    }
    Ok(())
}
fn content(source: &Object) -> Result<Object, String> {
    unsafe {
        let mut value = null_mut();
        check(
            (source.table::<IDesktopWindowXamlSource_Vtbl>().Content)(source.raw(), &mut value),
            "Get island content",
        )?;
        Object::owned(value)
    }
}
fn window(native: &Object) -> Result<HWND, String> {
    unsafe {
        let mut value = null_mut();
        check(
            (native.table::<Native>().window)(native.raw(), &mut value),
            "Get island HWND",
        )?;
        Ok(value)
    }
}
impl Binding {
    /// Only accepts an actual DesktopWindowXamlSource and the root content
    /// already used for discovery. A caller cannot supply an arbitrary HWND.
    pub(crate) fn capture(
        parent: HWND,
        source: &Object,
        expected_root_content: &Object,
    ) -> Result<Rc<Self>, String> {
        let source = source.query(&IID_IDesktopWindowXamlSource)?;
        let native = source.query(&IID_NATIVE)?;
        let window = window(&native)?;
        local_pair(parent, window)?;
        let content = content(&source)?;
        let root = xaml::root(&content)?;
        let root_content = xaml::root_content(&content)?;
        if !same_object(&root_content, expected_root_content)? {
            return Err("XAML source 内容根与识别结果不匹配".into());
        }
        let binding = Rc::new(Self {
            source,
            native,
            content,
            root,
            root_content,
            parent,
            window,
            parent_guard: WindowGuard::capture(parent)?,
            window_guard: WindowGuard::capture(window)?,
        });
        binding.validate()?;
        Ok(binding)
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        self.validate_windows()?;
        let current = content(&self.source)?;
        if !same_object(&current, &self.content)?
            || !same_object(&xaml::root(&current)?, &self.root)?
            || !same_object(&xaml::root_content(&current)?, &self.root_content)?
        {
            return Err("XAML source 内容或根已变化，需要重新建立绑定".into());
        }
        // COM reads can pump messages. Recheck the native relation before
        // returning a successful proof to the writer/input publisher.
        self.validate_windows()
    }
    fn validate_windows(&self) -> Result<(), String> {
        self.parent_guard.validate()?;
        self.window_guard.validate()?;
        if window(&self.native)? != self.window {
            return Err("XAML source 的原生窗口已变化".into());
        }
        local_pair(self.parent, self.window)
    }
    pub(crate) fn matches_root(&self, expected: &Object) -> Result<(), String> {
        self.validate()?;
        if !same_object(&self.root_content, expected)? {
            return Err("托盘识别结果不再属于当前窗口绑定".into());
        }
        Ok(())
    }
    pub(crate) fn input(
        &self,
        id: u64,
        element: Object,
    ) -> Result<crate::session::InputTarget, String> {
        self.validate()?;
        if !same_object(&xaml::root(&element)?, &self.root)?
            || !same_object(&xaml::root_content(&element)?, &self.root_content)?
        {
            return Err("时钟点击控件不属于已验证的 XAML source".into());
        }
        Ok(crate::session::InputTarget {
            id,
            element,
            island: self.window,
        })
    }
}
