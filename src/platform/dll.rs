//! Hook DLL paths identify their bytes, not the mutable build output name.
//! Windows may keep a renamed DLL mapped and reuse it when the original path
//! is loaded again. A hook's RVA must never be resolved in that older image.
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::{Path, PathBuf},
    ptr::null_mut,
    sync::atomic::{AtomicU64, Ordering},
};
use windows_sys::Win32::{
    Foundation::{FreeLibrary, HMODULE},
    Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, MoveFileExW},
    System::LibraryLoader::{
        LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
    },
};

pub(crate) enum Kind {
    Taskbar,
    Popup,
}
impl Kind {
    fn stem(&self) -> &'static str {
        match self {
            Self::Taskbar => "litetray_taskbar_adapter",
            Self::Popup => "litetray_popup_hook",
        }
    }
}

pub(crate) struct Library {
    module: HMODULE,
    // Deny writes and renames between verification and loading, including the
    // later SetWindowsHookEx roundtrip into another process.
    _file: File,
}
impl Library {
    pub(crate) fn handle(&self) -> HMODULE {
        self.module
    }
}
impl Drop for Library {
    fn drop(&mut self) {
        unsafe { FreeLibrary(self.module) };
    }
}

pub(crate) fn load(kind: Kind) -> Result<Library, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    load_from(exe.parent().ok_or("程序目录无效")?, kind)
}

fn load_from(directory: &Path, kind: Kind) -> Result<Library, String> {
    let (path, file) = prepare(directory, kind)?;
    let module = unsafe {
        LoadLibraryExW(
            wide_path(&path).as_ptr(),
            null_mut(),
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    };
    if module.is_null() {
        return Err(format!(
            "加载内容校验后的 DLL 失败（{}）：{}",
            path.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(Library {
        module,
        _file: file,
    })
}

fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

fn open_regular(path: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|e| format!("无法读取 DLL（{}）：{e}", path.display()))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err(format!("DLL 不是普通文件：{}", path.display()));
    }
    Ok(file)
}

fn read_image(file: &mut File) -> Result<Vec<u8>, String> {
    const MAX_IMAGE: u64 = 128 * 1024 * 1024;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if size == 0 || size > MAX_IMAGE {
        return Err("DLL 文件大小无效".into());
    }
    let mut bytes = Vec::with_capacity(size as usize);
    file.take(MAX_IMAGE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 != size {
        return Err("读取期间 DLL 文件大小变化".into());
    }
    Ok(bytes)
}

fn validate_image(bytes: &[u8]) -> Result<(), String> {
    let pe = bytes
        .get(0x3c..0x40)
        .map(|n| u32::from_le_bytes(n.try_into().unwrap()) as usize)
        .ok_or("DLL PE 头被截断")?;
    if bytes.get(..2) != Some(b"MZ")
        || bytes.get(pe..pe + 4) != Some(b"PE\0\0")
        || bytes.get(pe + 4..pe + 6) != Some(&[0x64, 0x86])
        || bytes.get(pe + 24..pe + 26) != Some(&[0x0b, 0x02])
        || !bytes.get(pe + 23).is_some_and(|flags| flags & 0x20 != 0)
    {
        return Err("配套 DLL 不是有效的 Windows x64 DLL".into());
    }
    let sections = crate::pe::sections(bytes)?;
    if sections.is_empty()
        || sections.iter().any(|s| {
            (s.raw_offset as usize)
                .checked_add(s.raw_size as usize)
                .is_none_or(|end| end > bytes.len())
        })
    {
        return Err("配套 DLL 的节区数据被截断".into());
    }
    Ok(())
}

fn prepare(directory: &Path, kind: Kind) -> Result<(PathBuf, File), String> {
    let source = directory.join(format!("{}.dll", kind.stem()));
    // One handle and one byte buffer supply both the hash and the copy.
    let mut original = open_regular(&source)?;
    let bytes = read_image(&mut original)?;
    validate_image(&bytes)?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let path = directory.join(format!("{}.{digest}.dll", kind.stem()));
    if !path.try_exists().map_err(|e| e.to_string())? {
        publish(&path, &bytes).map_err(|e| {
            format!(
                "无法准备内容命名 DLL：{e}。请使用包含配套内容命名 DLL 的完整安装包，或将程序复制到可写目录"
            )
        })?;
    }
    let mut file = open_regular(&path)?;
    if format!("{:x}", Sha256::digest(read_image(&mut file)?)) != digest {
        return Err(format!(
            "内容命名 DLL 校验失败，不会覆盖或加载该文件：{}",
            path.display()
        ));
    }
    Ok((path, file))
}

struct Pending(PathBuf);
impl Drop for Pending {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn publish(destination: &Path, bytes: &[u8]) -> Result<(), String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let (pending, mut file) = loop {
        let path = destination.with_extension(format!(
            "{}.{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(0)
            .open(&path)
        {
            Ok(file) => break (Pending(path), file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
    };
    let written = file.write_all(bytes).and_then(|_| file.sync_all());
    drop(file);
    written.map_err(|e| e.to_string())?;
    // Unlike std::fs::rename on Windows, flags=0 never replaces an existing
    // destination. A concurrent publisher must still pass prepare's hash check.
    if unsafe {
        MoveFileExW(
            wide_path(&pending.0).as_ptr(),
            wide_path(destination).as_ptr(),
            0,
        )
    } == 0
    {
        let error = std::io::Error::last_os_error();
        if !destination.try_exists().map_err(|e| e.to_string())? {
            return Err(error.to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "dll_checks.rs"]
mod tests;
