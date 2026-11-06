//! Exact-build SystemTray entry points, resolved from the matching Microsoft
//! PDB offline. Reject changed images and pre-existing function patches.
use sha2::{Digest, Sha256};
use windows_sys::Win32::System::LibraryLoader::*;
const HASH: &str = "c5f5260a2a56c4d7b0ee25978aa06c387b5ddffc57c5fe412be429d1289913ce";
const ENTRIES: [(usize, [u8; 16]); 3] = [
    (
        772912,
        [
            0x40, 0x53, 0x56, 0x57, 0x48, 0x83, 0xec, 0x50, 0x0f, 0x29, 0x74, 0x24, 0x40, 0x48,
            0x8b, 0x05,
        ],
    ),
    (
        772612,
        [
            0x48, 0x89, 0x5c, 0x24, 0x20, 0x55, 0x56, 0x57, 0x48, 0x8b, 0xec, 0x48, 0x83, 0xec,
            0x50, 0x0f,
        ],
    ),
    (
        773136,
        [
            0x48, 0x89, 0x5c, 0x24, 0x08, 0x48, 0x89, 0x74, 0x24, 0x20, 0x55, 0x57, 0x41, 0x54,
            0x41, 0x56,
        ],
    ),
];
pub(crate) fn resolve() -> Result<[usize; 3], String> {
    unsafe {
        let module = GetModuleHandleW(crate::wide("SystemTray.dll").as_ptr());
        if module.is_null() {
            return Err("SystemTray.dll 尚未加载".into());
        }
        let mut path = vec![0u16; 32768];
        let length = GetModuleFileNameW(module, path.as_mut_ptr(), path.len() as u32) as usize;
        if length == 0 || length >= path.len() {
            return Err("无法核对托盘模块路径".into());
        }
        let bytes =
            std::fs::read(String::from_utf16_lossy(&path[..length])).map_err(|e| e.to_string())?;
        if format!("{:x}", Sha256::digest(bytes)) != HASH {
            return Err("此 SystemTray.dll 版本尚未验证".into());
        }
        let id = crate::loaded_identity(module as usize).ok_or("无法读取托盘模块身份")?;
        if id["machine"] != 0x8664
            || id["timestamp"] != 1789519242u32
            || id["image_size"] != 2138112
            || id["pdb_age"] != 1
            || id["pdb_guid"]
                != serde_json::json!([
                    137, 243, 7, 32, 157, 11, 22, 77, 175, 120, 186, 254, 119, 18, 39, 40
                ])
        {
            return Err("内存中的托盘模块身份不匹配".into());
        }
        let mut addresses = [0; 3];
        for (index, (rva, prologue)) in ENTRIES.iter().enumerate() {
            let address = module as usize + rva;
            crate::inspect::check_region(address, 16, module as usize, true)?;
            if crate::read_memory(address, 16).as_deref() != Some(prologue.as_slice()) {
                return Err("托盘坐标入口已变化或已被其他组件改写".into());
            }
            addresses[index] = address;
        }
        Ok(addresses)
    }
}
