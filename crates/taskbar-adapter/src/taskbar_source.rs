//! Source resolution uses built-in evidence, never guessed offsets or roots.
use crate::{
    inspect,
    xaml::{self, Object},
    xaml_abi::*,
};
use litetray_adapter_protocol::compatibility::{Resolved, SourceRecipe};
use std::{mem::size_of, ptr::null_mut};
use windows_sys::Win32::System::Memory::*;
use windows_sys::{Win32::Foundation::*, core::GUID};
fn child_iid(recipe: SourceRecipe) -> GUID {
    // These are distinct private interfaces, despite their identical names.
    // The reviewed early QI dispatch and GetIids both identify eef8f632;
    // slot count alone cannot identify an IID. Select only after the module and
    // concrete vtable have matched the corresponding implementation recipe.
    match recipe {
        SourceRecipe::XamlExplorerHostChildV1 => {
            GUID::from_u128(0xec97afd0_5c0d_581b_9508_074773c6f1e0)
        }
        SourceRecipe::XamlExplorerHostChildXfgV1 => {
            GUID::from_u128(0xeef8f632_6c0d_5fcb_b6a1_354819e51f3b)
        }
    }
}
pub(crate) struct Source {
    pub root: Object,
    pub source: Object,
    // Keep the private host alive while source and discovery are consumed.
    _host: inspect::SharedHost,
    _child: Object,
}
unsafe fn source_image_region(
    address: usize,
    length: usize,
    shell: usize,
    executable: bool,
) -> Result<(), String> {
    unsafe {
        inspect::check_region(address, length, shell, executable)?;
        let mut info: MEMORY_BASIC_INFORMATION = std::mem::zeroed();
        if VirtualQuery(
            address as *const _,
            &mut info,
            size_of::<MEMORY_BASIC_INFORMATION>(),
        ) == 0
            || info.Type != MEM_IMAGE
            || if executable {
                !matches!(info.Protect, PAGE_EXECUTE | PAGE_EXECUTE_READ)
            } else {
                !matches!(
                    info.Protect,
                    PAGE_READONLY | PAGE_READWRITE | PAGE_WRITECOPY
                )
            }
        {
            return Err("子宿主虚表或函数的映像页权限已变化".into());
        }
        Ok(())
    }
}
unsafe fn validate_child_vtable(plan: &Resolved, shell: usize, table: usize) -> Result<(), String> {
    unsafe {
        if table
            != shell
                .checked_add(plan.source.child_vtable_rva as usize)
                .ok_or("虚表地址溢出")?
        {
            return Err("TaskbarHost 子宿主不是已核对的具体类型".into());
        }
        match plan.source.recipe {
            SourceRecipe::XamlExplorerHostChildV1 => {
                inspect::check_region(table, 6 * size_of::<usize>(), shell, false)?;
            }
            SourceRecipe::XamlExplorerHostChildXfgV1 => {
                let entries = plan
                    .source
                    .child_vtable_entries
                    .as_ref()
                    .ok_or("缺少子宿主虚表证据")?;
                let length = entries.len() * size_of::<usize>();
                source_image_region(table, length, shell, false)?;
                let actual = crate::read_memory(table, length).ok_or("无法读取子宿主虚表")?;
                plan.source.verify_child_vtable(shell as u64, &actual)?;
                // The image stores this table in writable data. Validate the
                // entire reviewed 13-slot prefix and owned code before QI;
                // merely matching the table address would not be sufficient.
                for entry in entries {
                    let address = shell
                        .checked_add(entry.rva as usize)
                        .ok_or("虚表函数地址溢出")?;
                    source_image_region(address, 16, shell, true)?;
                    if crate::read_memory(address, 16).ok_or("无法读取子宿主函数入口")?
                        != entry.prologue
                    {
                        return Err("子宿主函数入口已变化或被其他模块改写".into());
                    }
                }
            }
        }
        Ok(())
    }
}
pub(crate) unsafe fn resolve(hwnd: HWND, plan: &Resolved) -> Result<Source, String> {
    unsafe {
        let base = inspect::validate_resolved(plan)?;
        let shell = crate::compatibility::loaded(&plan.shell)?;
        let host = inspect::host(
            hwnd,
            crate::class_name(hwnd) == "Shell_SecondaryTrayWnd",
            base,
            &plan.host,
        )?;
        let raw = inspect::pointer(
            host.value[0]
                .checked_add(plan.source.host_child_offset as usize)
                .ok_or("宿主指针溢出")?,
        )?;
        inspect::check_region(raw, size_of::<usize>(), 0, false)?;
        let table = inspect::pointer(raw)?;
        validate_child_vtable(plan, shell, table)?;
        let child = Object::query_borrowed(raw as *mut _, &child_iid(plan.source.recipe))
            .map_err(|error| format!("Taskbar child interface: {error}"))?;
        if inspect::pointer(child.raw() as usize)? != table {
            return Err("子宿主接口身份变化".into());
        }
        let source = inspect::pointer(
            (child.raw() as usize)
                .checked_add(plan.source.child_source_offset as usize)
                .ok_or("source 指针溢出")?,
        )?;
        inspect::check_region(source, size_of::<usize>(), 0, false)?;
        let table = inspect::pointer(source)?;
        inspect::check_region(table, 3 * size_of::<usize>(), 0, false)?;
        inspect::check_region(inspect::pointer(table)?, 1, 0, true)?;
        let source = Object::query_borrowed(source as *mut _, &IID_IDesktopWindowXamlSource)
            .map_err(|error| format!("Taskbar DesktopWindowXamlSource interface: {error}"))?;
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
            _host: host,
            _child: child,
        })
    }
}
