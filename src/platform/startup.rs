use windows_sys::Win32::{Foundation::*, System::Registry::*};
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    unsafe {
        let key_name = super::wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let name = super::wide("LiteTray");
        let mut key = std::ptr::null_mut();
        let status = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            key_name.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut key,
            std::ptr::null_mut(),
        );
        if status != ERROR_SUCCESS {
            return Err("无法修改当前用户的开机启动项".into());
        }
        let result = if enabled {
            let exe = std::env::current_exe().map_err(|e| e.to_string());
            match exe {
                Ok(exe) => {
                    let value = super::wide(&format!("\"{}\" --background", exe.display()));
                    RegSetValueExW(
                        key,
                        name.as_ptr(),
                        0,
                        REG_SZ,
                        value.as_ptr().cast(),
                        (value.len() * 2) as u32,
                    )
                }
                Err(_) => ERROR_FILE_NOT_FOUND,
            }
        } else {
            RegDeleteValueW(key, name.as_ptr())
        };
        RegCloseKey(key);
        if result == ERROR_SUCCESS || (!enabled && result == ERROR_FILE_NOT_FOUND) {
            Ok(())
        } else {
            Err("更新开机启动项失败".into())
        }
    }
}
