//! Source resolution uses built-in evidence, never guessed offsets or roots.
use crate::{
    inspect,
    xaml::{self, Object},
    xaml_abi::*,
};
use litetray_adapter_protocol::{
    HostLayout,
    compatibility::{self, Profile, SourceRecipe},
};
use std::{mem::size_of, ptr::null_mut};
use windows_sys::{Win32::Foundation::*, core::GUID};
const IID_CHILD: GUID = GUID::from_u128(0xec97afd0_5c0d_581b_9508_074773c6f1e0);
pub(crate) struct Source {
    pub root: Object,
    pub source: Object,
    pub profile: &'static Profile,
    // Keep the private host alive while source and discovery are consumed.
    _host: inspect::SharedHost,
    _child: Object,
}
pub(crate) unsafe fn resolve(hwnd: HWND, layout: &HostLayout) -> Result<Source, String> {
    unsafe {
        let profile = compatibility::profile_for_taskbar(&layout.module)?;
        if layout != &profile.host {
            return Err("宿主布局与内置兼容资料不一致".into());
        }
        let base = inspect::validate_loaded(layout)?;
        let shell = crate::compatibility::loaded(&profile.shell)?;
        let host = inspect::host(
            hwnd,
            crate::class_name(hwnd) == "Shell_SecondaryTrayWnd",
            base,
            layout,
        )?;
        // New recipes require an explicit implementation with their own ABI.
        let SourceRecipe::XamlExplorerHostChildV1 = profile.source.recipe;
        let raw = inspect::pointer(
            host.value[0]
                .checked_add(profile.source.host_child_offset as usize)
                .ok_or("宿主指针溢出")?,
        )?;
        inspect::check_region(raw, size_of::<usize>(), 0, false)?;
        let table = inspect::pointer(raw)?;
        if table != shell + profile.source.child_vtable_rva as usize {
            return Err("TaskbarHost 子宿主不是已核对的具体类型".into());
        }
        inspect::check_region(table, 6 * size_of::<usize>(), shell, false)?;
        let child = Object::query_borrowed(raw as *mut _, &IID_CHILD)?;
        if inspect::pointer(child.raw() as usize)? != table {
            return Err("子宿主接口身份变化".into());
        }
        let source = inspect::pointer(
            (child.raw() as usize)
                .checked_add(profile.source.child_source_offset as usize)
                .ok_or("source 指针溢出")?,
        )?;
        inspect::check_region(source, size_of::<usize>(), 0, false)?;
        let table = inspect::pointer(source)?;
        inspect::check_region(table, 3 * size_of::<usize>(), 0, false)?;
        inspect::check_region(inspect::pointer(table)?, 1, 0, true)?;
        let source = Object::query_borrowed(source as *mut _, &IID_IDesktopWindowXamlSource)?;
        let mut content = null_mut();
        xaml::check(
            (source.table::<IDesktopWindowXamlSource_Vtbl>().Content)(source.raw(), &mut content),
            "Taskbar source.Content",
        )?;
        let root = xaml::root_content(&Object::owned(content)?)?;
        // Binding::capture additionally proves its actual HWND and root belong
        // to this taskbar. No window-class/coordinate approximation is used.
        Ok(Source {
            root,
            source,
            profile,
            _host: host,
            _child: child,
        })
    }
}
