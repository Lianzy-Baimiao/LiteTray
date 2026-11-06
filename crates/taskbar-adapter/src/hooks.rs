//! Process-lifetime trampolines. Callers pin this DLL before installation.
//! Inactive sessions pass through; trampolines are never freed under a caller.
use minhook::MH_STATUS;
use std::{
    ffi::c_void,
    sync::atomic::{AtomicUsize, Ordering},
};
static TRANSIENT: std::sync::OnceLock<Vec<usize>> = std::sync::OnceLock::new();

unsafe extern "system" {
    fn MH_Initialize() -> MH_STATUS;
    fn MH_CreateHook(
        target: *mut c_void,
        detour: *mut c_void,
        original: *mut *mut c_void,
    ) -> MH_STATUS;
    fn MH_EnableHook(target: *mut c_void) -> MH_STATUS;
    fn MH_DisableHook(target: *mut c_void) -> MH_STATUS;
}
pub(crate) struct Entry {
    pub target: usize,
    pub detour: usize,
    pub original: &'static AtomicUsize,
    pub arguments: usize,
}
pub(crate) unsafe fn install(entries: &[Entry], transient_count: usize) -> Result<(), String> {
    unsafe {
        // Use the fallible C initialization API; the Rust wrapper panics on
        // allocation failure, which is inappropriate inside Explorer.
        let status = MH_Initialize();
        if status != MH_STATUS::MH_OK && status != MH_STATUS::MH_ERROR_ALREADY_INITIALIZED {
            return Err(format!("初始化托盘钩子失败：{status:?}"));
        }
        let mut created = Vec::<&Entry>::new();
        for entry in entries {
            if entry.target == 0 {
                return Err("托盘钩子入口为空".into());
            }
            if let Some(previous) = created.iter().find(|e| e.target == entry.target) {
                if previous.arguments != entry.arguments {
                    return Err("重复钩子 ABI 不一致".into());
                }
                entry
                    .original
                    .store(previous.original.load(Ordering::Acquire), Ordering::Release);
                continue;
            }
            let mut original = std::ptr::null_mut();
            let status = MH_CreateHook(entry.target as _, entry.detour as _, &mut original);
            if status != MH_STATUS::MH_OK {
                return Err(format!("准备托盘钩子失败：{status:?}"));
            }
            entry.original.store(original as usize, Ordering::Release);
            created.push(entry);
        }
        let mut enabled = Vec::new();
        for entry in created.into_iter().skip(transient_count) {
            let status = MH_EnableHook(entry.target as _);
            if status != MH_STATUS::MH_OK {
                for target in enabled {
                    let _ = MH_DisableHook(target as _);
                }
                return Err(format!("启用托盘钩子失败：{status:?}"));
            }
            enabled.push(entry.target);
        }
        TRANSIENT
            .set(
                entries
                    .iter()
                    .take(transient_count)
                    .map(|entry| entry.target)
                    .collect(),
            )
            .map_err(|_| "重复注册托盘钩子")?;
        Ok(())
    }
}
pub(crate) fn activate() -> Result<(), String> {
    let mut enabled = Vec::new();
    for &target in TRANSIENT.get().ok_or("托盘钩子尚未准备")? {
        let status = unsafe { MH_EnableHook(target as _) };
        if status != MH_STATUS::MH_OK && status != MH_STATUS::MH_ERROR_ENABLED {
            for address in enabled {
                unsafe {
                    let _ = MH_DisableHook(address as _);
                }
            }
            return Err(format!("启用托盘输入钩子失败：{status:?}"));
        }
        enabled.push(target);
    }
    Ok(())
}
pub(crate) fn suspend() -> Result<(), String> {
    for &target in TRANSIENT.get().ok_or("托盘钩子尚未准备")? {
        let status = unsafe { MH_DisableHook(target as _) };
        if status != MH_STATUS::MH_OK && status != MH_STATUS::MH_ERROR_DISABLED {
            return Err(format!("恢复托盘输入钩子失败：{status:?}"));
        }
    }
    Ok(())
}
