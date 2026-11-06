//! Experimental resolver for the exact inspected Windows build. No broad
//! version ranges and no fallback from a failed source/root/window proof.
use crate::{
    inspect, wide,
    xaml::{self, Object},
    xaml_abi::*,
};
use litetray_adapter_protocol::{
    HostLayout,
    taskbar::{SHELL_HASH, TASKBAR_HASH},
};
use sha2::{Digest, Sha256};
use std::{mem::size_of, ptr::null_mut};
use windows_sys::{
    Win32::{
        Foundation::*,
        System::{LibraryLoader::*, SystemInformation::GetSystemDirectoryW},
    },
    core::GUID,
};
const IID_CHILD: GUID = GUID::from_u128(0xec97afd0_5c0d_581b_9508_074773c6f1e0);
pub(crate) struct Source {
    pub root: Object,
    pub source: Object,
    // Keep the private host alive while source and discovery are consumed.
    _host: inspect::SharedHost,
    _child: Object,
}
unsafe fn module(name: &str, hash: &str) -> Result<usize, String> {
    unsafe {
        let mut path = vec![0u16; 32768];
        let n = GetSystemDirectoryW(path.as_mut_ptr(), path.len() as u32) as usize;
        if n == 0 || n >= path.len() {
            return Err("无法读取系统目录".into());
        }
        let expected = std::path::PathBuf::from(String::from_utf16_lossy(&path[..n])).join(name);
        let image = std::fs::read(&expected).map_err(|e| e.to_string())?;
        if format!("{:x}", Sha256::digest(image)) != hash {
            return Err(format!("{name} 不是已核对版本，拒绝私有接口调用"));
        }
        let module = GetModuleHandleW(wide(name).as_ptr());
        if module.is_null() {
            return Err(format!("{name} 尚未加载"));
        }
        let n = GetModuleFileNameW(module, path.as_mut_ptr(), path.len() as u32) as usize;
        if n == 0
            || n >= path.len()
            || !String::from_utf16_lossy(&path[..n])
                .eq_ignore_ascii_case(&expected.to_string_lossy())
        {
            return Err("已加载系统模块路径不匹配".into());
        }
        let id = crate::loaded_identity(module as usize).ok_or("无法识别已加载系统模块")?;
        let (stamp, size, guid) = if name == "taskbar.dll" {
            (
                1989904251u32,
                3178496,
                vec![
                    121, 154, 182, 218, 224, 254, 240, 51, 216, 30, 2, 63, 191, 23, 43, 81,
                ],
            )
        } else {
            (
                3162946381u32,
                10113024,
                vec![
                    218, 243, 7, 149, 127, 212, 2, 95, 101, 105, 252, 80, 39, 60, 64, 215,
                ],
            )
        };
        if id["machine"] != 0x8664
            || id["timestamp"] != stamp
            || id["image_size"] != size
            || id["pdb_age"] != 1
            || id["pdb_guid"] != serde_json::json!(guid)
        {
            return Err("内存中系统模块身份不匹配".into());
        }
        Ok(module as usize)
    }
}
pub(crate) unsafe fn resolve(hwnd: HWND, layout: &HostLayout) -> Result<Source, String> {
    unsafe {
        let base = module("taskbar.dll", TASKBAR_HASH)?;
        let shell = module("twinui.pcshell.dll", SHELL_HASH)?;
        if inspect::validate_loaded(layout)? != base {
            return Err("任务栏模块不匹配".into());
        }
        let host = inspect::host(
            hwnd,
            crate::class_name(hwnd) == "Shell_SecondaryTrayWnd",
            base,
            layout,
        )?;
        // The exact TaskbarHost::HasFocus build inspected offline uses +0x38.
        // Never infer this from a new build or from FrameHeight's other field.
        let raw = inspect::pointer(host.value[0].checked_add(0x38).ok_or("宿主指针溢出")?)?;
        inspect::check_region(raw, size_of::<usize>(), 0, false)?;
        let table = inspect::pointer(raw)?;
        if table != shell + 7739040 {
            return Err("TaskbarHost 子宿主不是已核对的具体类型".into());
        }
        inspect::check_region(table, 6 * size_of::<usize>(), shell, false)?;
        let child = Object::query_borrowed(raw as *mut _, &IID_CHILD)?;
        if inspect::pointer(child.raw() as usize)? != table {
            return Err("子宿主接口身份变化".into());
        }
        let source = inspect::pointer(
            (child.raw() as usize)
                .checked_add(0x10)
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
            _host: host,
            _child: child,
        })
    }
}
