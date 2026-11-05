use crate::{
    config,
    pe::{self, ImageIdentity},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::Path,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{Diagnostics::Debug::*, Threading::*},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub decorated_name: String,
    pub rva: u64,
    pub size: u32,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct SymbolMap {
    pub schema: u32,
    pub image: ImageIdentity,
    pub symbols: BTreeMap<String, Vec<Symbol>>,
}
/// Resolve offline PDBs in a separate CLI process. DbgHelp is process-global;
/// this function is deliberately not called on Explorer's UI thread.
pub fn prepare(module: &Path, import_cache: Option<&Path>) -> Result<SymbolMap, String> {
    let bytes = std::fs::read(module).map_err(|e| e.to_string())?;
    let identity = pe::identity(&bytes)?;
    if identity.machine != 0x8664 {
        return Err("当前任务栏适配只支持 x64".into());
    }
    let directory = config::data_dir()
        .join("symbols")
        .join(&identity.pdb_name)
        .join(&identity.pdb_key);
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let pdb = directory.join(&identity.pdb_name);
    if !pdb.is_file()
        && let Some(root) = import_cache
    {
        let source = root
            .join(&identity.pdb_name)
            .join(&identity.pdb_key)
            .join(&identity.pdb_name);
        if source.is_file() {
            std::fs::copy(source, &pdb).map_err(|e| e.to_string())?;
        }
    }
    if !pdb.is_file() {
        let url = format!(
            "https://msdl.microsoft.com/download/symbols/{}/{}/{}",
            identity.pdb_name, identity.pdb_key, identity.pdb_name
        );
        let bytes = crate::http::Client::new(30).download_symbols(&url)?;
        config::atomic_write(&pdb, &bytes)?;
    }
    unsafe {
        let process = GetCurrentProcess();
        SymSetOptions(
            SYMOPT_EXACT_SYMBOLS
                | SYMOPT_FAIL_CRITICAL_ERRORS
                | SYMOPT_NO_PROMPTS
                | SYMOPT_IGNORE_NT_SYMPATH,
        );
        if SymInitializeW(
            process,
            super::wide(&directory.to_string_lossy()).as_ptr(),
            0,
        ) == 0
        {
            return Err("初始化系统符号解析器失败".into());
        }
        let result = resolve(process, module, &identity);
        SymCleanup(process);
        result
    }
}
unsafe fn resolve(
    process: HANDLE,
    module: &Path,
    identity: &ImageIdentity,
) -> Result<SymbolMap, String> {
    unsafe {
        let base = SymLoadModuleExW(
            process,
            null_mut(),
            super::wide(&module.to_string_lossy()).as_ptr(),
            null(),
            0x180000000,
            identity.image_size,
            null(),
            0,
        );
        if base == 0 {
            return Err(format!(
                "载入模块符号失败：{}",
                std::io::Error::last_os_error()
            ));
        }
        let mut groups = BTreeMap::new();
        for mask in [
            "*GetTaskbarHost*",
            "*TaskbarHost*FrameHeight*",
            "*Ref_count_base*Decref*",
            "*TaskBand*",
            "*TrayUI*StartTaskbar*",
            "*SecondaryTray*InitModelAndHost*",
        ] {
            let mut symbols = Vec::<Symbol>::new();
            if SymEnumSymbolsW(
                process,
                base,
                super::wide(mask).as_ptr(),
                Some(enumerate),
                &mut symbols as *mut _ as _,
            ) == 0
            {
                return Err(format!("枚举符号失败：{mask}"));
            }
            groups.insert(mask.into(), symbols);
        }
        let mut info: IMAGEHLP_MODULEW64 = std::mem::zeroed();
        info.SizeOfStruct = std::mem::size_of::<IMAGEHLP_MODULEW64>() as u32;
        if SymGetModuleInfoW64(process, base, &mut info) == 0
            || info.PdbUnmatched != 0
            || info.DbgUnmatched != 0
            || info.PdbAge != identity.pdb_age
        {
            return Err("PDB 与模块版本不匹配".into());
        }
        let guid_bytes = std::slice::from_raw_parts(&info.PdbSig70 as *const _ as *const u8, 16);
        if guid_bytes != identity.pdb_guid {
            return Err("PDB GUID 不匹配".into());
        }
        let map = SymbolMap {
            schema: 1,
            image: identity.clone(),
            symbols: groups,
        };
        let path = config::data_dir()
            .join("adapters")
            .join(format!("{}.json", identity.sha256));
        config::atomic_write(
            &path,
            &serde_json::to_vec_pretty(&map).map_err(|e| e.to_string())?,
        )?;
        Ok(map)
    }
}
unsafe extern "system" fn enumerate(
    info: *const SYMBOL_INFOW,
    _size: u32,
    data: *const core::ffi::c_void,
) -> i32 {
    unsafe {
        if info.is_null() || data.is_null() {
            return 0;
        }
        let info = &*info;
        if info.NameLen > 4096 || info.Address < info.ModBase {
            return 0;
        }
        let decorated_name = String::from_utf16_lossy(std::slice::from_raw_parts(
            info.Name.as_ptr(),
            info.NameLen as usize,
        ))
        .trim_end_matches('\0')
        .to_owned();
        let mut buffer = vec![0u16; 8192];
        let length = UnDecorateSymbolNameW(
            super::wide(&decorated_name).as_ptr(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            0,
        );
        let name = if length > 0 {
            String::from_utf16_lossy(&buffer[..length as usize])
        } else {
            decorated_name.clone()
        };
        let symbols = &mut *(data as *mut Vec<Symbol>);
        if symbols.len() >= 4096 {
            return 0;
        }
        symbols.push(Symbol {
            name,
            decorated_name,
            rva: info.Address - info.ModBase,
            size: info.Size,
        });
        1
    }
}
