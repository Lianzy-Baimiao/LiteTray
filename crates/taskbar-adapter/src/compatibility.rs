//! Revalidate built-in evidence against the file AND the already loaded image.
//! Does not load modules or invoke any private entry point.
use litetray_adapter_protocol::compatibility::Module;
use sha2::{Digest, Sha256};
use std::{io::Read, path::PathBuf};
use windows_sys::Win32::System::{LibraryLoader::*, SystemInformation::GetWindowsDirectoryW};

pub(crate) fn verified_file(expected: &Module) -> Result<PathBuf, String> {
    let mut path = vec![0u16; 32768];
    let length = unsafe { GetWindowsDirectoryW(path.as_mut_ptr(), path.len() as u32) } as usize;
    if length == 0 || length >= path.len() {
        return Err("无法取得 Windows 目录".into());
    }
    let path = PathBuf::from(String::from_utf16_lossy(&path[..length]))
        .join(expected.relative_path.replace('/', "\\"));
    let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    if file.metadata().map_err(|e| e.to_string())?.len() > 128 * 1024 * 1024 {
        return Err("系统模块超过检查上限".into());
    }
    file.take(128 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 128 * 1024 * 1024 || format!("{:x}", Sha256::digest(&bytes)) != expected.sha256
    {
        return Err(format!(
            "{} 不是已核对版本，拒绝私有接口调用",
            expected.name
        ));
    }
    Ok(path)
}
pub(crate) unsafe fn loaded(expected: &Module) -> Result<usize, String> {
    unsafe {
        let expected_path = verified_file(expected)?;
        let module = GetModuleHandleW(crate::wide(&expected.name).as_ptr());
        if module.is_null() {
            return Err(format!("{} 尚未加载", expected.name));
        }
        let mut path = vec![0u16; 32768];
        let length = GetModuleFileNameW(module, path.as_mut_ptr(), path.len() as u32) as usize;
        if length == 0
            || length >= path.len()
            || !String::from_utf16_lossy(&path[..length])
                .eq_ignore_ascii_case(&expected_path.to_string_lossy())
        {
            return Err(format!("{} 的已加载路径不匹配", expected.name));
        }
        let identity = crate::loaded_identity(module as usize).ok_or("无法识别已加载系统模块")?;
        if !expected.matches_loaded(&identity) {
            return Err(format!("{} 的内存模块身份不匹配", expected.name));
        }
        Ok(module as usize)
    }
}
