//! Explorer-thread capability probe. It does not install persistent subclasses,
//! modify XAML, or patch functions. The host removes the scoped message hook
//! after the synchronous roundtrip completes.
#![cfg(windows)]
use std::ptr::null_mut;
use windows_sys::Win32::{
    Foundation::*,
    System::{
        DataExchange::COPYDATASTRUCT, Diagnostics::Debug::ReadProcessMemory, LibraryLoader::*,
        Threading::*,
    },
    UI::WindowsAndMessaging::*,
};
const MESSAGE: &str = "LiteTray.TaskbarProbe.v1";
const PROTOCOL: usize = 0x4c545001;
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// # Safety
/// Called exclusively by Windows as a WH_CALLWNDPROC hook. The LPARAM must be
/// the CWPSTRUCT supplied by Windows for HC_ACTION.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn LiteTrayCallWndProc(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if code == HC_ACTION as i32 && l != 0 {
            let call = &*(l as *const CWPSTRUCT);
            let message = RegisterWindowMessageW(wide(MESSAGE).as_ptr());
            if message != 0 && call.message == message && call.wParam != 0 && is_taskbar(call.hwnd)
            {
                let reply = call.wParam as HWND;
                if class_name(reply) == "LiteTray.ProbeReceiver.v1" {
                    report(reply, call.lParam as u64);
                }
            }
        }
        CallNextHookEx(null_mut(), code, w, l)
    }
}
unsafe fn is_taskbar(hwnd: HWND) -> bool {
    unsafe {
        matches!(
            class_name(hwnd).as_str(),
            "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        )
    }
}
unsafe fn class_name(hwnd: HWND) -> String {
    unsafe {
        let mut b = [0u16; 256];
        let n = GetClassNameW(hwnd, b.as_mut_ptr(), b.len() as i32);
        String::from_utf16_lossy(&b[..n.max(0) as usize])
    }
}
unsafe fn report(reply: HWND, nonce: u64) {
    unsafe {
        let mut windows = Vec::<serde_json::Value>::new();
        EnumThreadWindows(
            GetCurrentThreadId(),
            Some(enum_window),
            &mut windows as *mut _ as isize,
        );
        let mut modules = Vec::new();
        for name in [
            "explorer.exe",
            "taskbar.dll",
            "Taskbar.View.dll",
            "SystemTray.dll",
            "Windows.UI.Xaml.dll",
            "twinui.pcshell.dll",
        ] {
            let module = GetModuleHandleW(wide(name).as_ptr());
            if module.is_null() {
                continue;
            }
            let mut path = vec![0u16; 32768];
            let n = GetModuleFileNameW(module, path.as_mut_ptr(), path.len() as u32) as usize;
            modules.push(serde_json::json!({"name":name,"base":module as usize,"path":String::from_utf16_lossy(&path[..n.min(path.len())]),"loaded_identity":loaded_identity(module as usize)}));
        }
        let payload = serde_json::json!({"protocol":1,"nonce":nonce,"process_id":GetCurrentProcessId(),"thread_id":GetCurrentThreadId(),"taskbars":windows,"modules":modules,"changes_applied":false});
        let Ok(bytes) = serde_json::to_vec(&payload) else {
            return;
        };
        if bytes.len() > 64 * 1024 {
            return;
        }
        let copy = COPYDATASTRUCT {
            dwData: PROTOCOL,
            cbData: bytes.len() as u32,
            lpData: bytes.as_ptr() as *mut _,
        };
        let mut result = 0;
        SendMessageTimeoutW(
            reply,
            WM_COPYDATA,
            0,
            &copy as *const _ as isize,
            SMTO_ABORTIFHUNG,
            500,
            &mut result,
        );
    }
}
// Read through the kernel API so a trimmed/discarded debug section is a
// recoverable missing capability, not an access violation inside Explorer.
unsafe fn read_memory(address: usize, length: usize) -> Option<Vec<u8>> {
    unsafe {
        if length > 65536 {
            return None;
        }
        let mut bytes = vec![0; length];
        let mut copied = 0;
        if ReadProcessMemory(
            GetCurrentProcess(),
            address as *const _,
            bytes.as_mut_ptr().cast(),
            length,
            &mut copied,
        ) == 0
            || copied != length
        {
            return None;
        }
        Some(bytes)
    }
}
fn get32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}
unsafe fn loaded_identity(base: usize) -> Option<serde_json::Value> {
    unsafe {
        let header = read_memory(base, 4096)?;
        if header.get(..2) != Some(b"MZ") {
            return None;
        }
        let pe = get32(&header, 0x3c)? as usize;
        if header.get(pe..pe + 4) != Some(b"PE\0\0") {
            return None;
        }
        let optional = pe + 24;
        let timestamp = get32(&header, pe + 8)?;
        let image_size = get32(&header, optional + 56)?;
        let debug_rva = get32(&header, optional + 112 + 6 * 8)? as usize;
        let debug_size = get32(&header, optional + 112 + 6 * 8 + 4)? as usize;
        if debug_size > 4096 || debug_rva.checked_add(debug_size)? > image_size as usize {
            return None;
        }
        let debug = read_memory(base.checked_add(debug_rva)?, debug_size)?;
        for offset in (0..debug.len()).step_by(28) {
            if get32(&debug, offset + 12)? != 2 {
                continue;
            }
            let size = get32(&debug, offset + 16)? as usize;
            let rva = get32(&debug, offset + 20)? as usize;
            if !(24..=4096).contains(&size) || rva.checked_add(size)? > image_size as usize {
                continue;
            }
            let cv = read_memory(base.checked_add(rva)?, size)?;
            if cv.get(..4) != Some(b"RSDS") {
                continue;
            }
            return Some(
                serde_json::json!({"timestamp":timestamp,"image_size":image_size,"pdb_guid":&cv[4..20],"pdb_age":get32(&cv,20)?}),
            );
        }
        None
    }
}
unsafe extern "system" fn enum_window(hwnd: HWND, data: LPARAM) -> i32 {
    unsafe {
        if !is_taskbar(hwnd) {
            return 1;
        }
        let mut rect = std::mem::zeroed();
        GetWindowRect(hwnd, &mut rect);
        let mut children = Vec::<serde_json::Value>::new();
        EnumChildWindows(hwnd, Some(enum_child), &mut children as *mut _ as isize);
        (&mut *(data as *mut Vec<serde_json::Value>)).push(serde_json::json!({"hwnd":hwnd as usize,"class":class_name(hwnd),"rect":[rect.left,rect.top,rect.right,rect.bottom],"children":children}));
        1
    }
}
unsafe extern "system" fn enum_child(hwnd: HWND, data: LPARAM) -> i32 {
    unsafe {
        let children = &mut *(data as *mut Vec<serde_json::Value>);
        if children.len() >= 128 {
            return 0;
        }
        children.push(serde_json::json!({"hwnd":hwnd as usize,"class":class_name(hwnd)}));
        1
    }
}
