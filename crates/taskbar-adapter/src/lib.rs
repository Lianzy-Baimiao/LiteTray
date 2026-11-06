//! Explorer-thread probe and reversible clock/tray sessions. Inspection is
//! read-only. Native tray sessions additionally install scoped input/geometry
//! hooks and process-lifetime cached-menu guards; see ABI.md.
#![cfg(windows)]
mod clock;
mod clock_style;
mod clock_style_checks;
mod clock_text;
mod clock_text_checks;
mod discovery;
mod discovery_checks;
mod hooks;
mod input;
mod input_box;
mod input_checks;
mod inspect;
mod island;
mod lifecycle_checks;
mod preview;
mod property;
mod routing;
mod routing_checks;
mod session;
mod shell_source_checks;
mod taskbar;
mod taskbar_source;
mod tray;
mod tray_checks;
mod tray_interaction;
mod tray_menu;
mod tray_symbols;
mod visibility;
mod watchdog;
mod window_guard;
mod xaml;
mod xaml_abi;
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
/// Runs the isolated clock check in the calling application process.
/// # Safety
/// `output` must reference `capacity` writable bytes. Call on an STA thread
/// before creating any other XAML runtime on that thread.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn LiteTraySelfTestClock(output: *mut u8, capacity: usize) -> i32 {
    if output.is_null() || capacity < 2 {
        return -1;
    }
    let (status, result) = match clock::self_test() {
        Ok(result) => (0, result),
        Err(error) => (
            1,
            serde_json::json!({"passed":false,"environment":"isolated_xaml_in_host_process","explorer_accessed":false,"error":error}),
        ),
    };
    let bytes = result.to_string();
    if bytes.len() >= capacity {
        return -1;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), output, bytes.len());
        *output.add(bytes.len()) = 0;
    }
    status
}
/// Exercises only freshly created private hosts in the calling application.
/// Refuses every system image except the one inspected for this experiment.
/// # Safety
/// `output` must reference `capacity` writable bytes. Call on a fresh STA thread
/// with no previously initialized XAML runtime. Never call inside Explorer.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn LiteTraySelfTestShellSource(output: *mut u8, capacity: usize) -> i32 {
    if output.is_null() || capacity < 2 {
        return -1;
    }
    let (status, result) = match shell_source_checks::run() {
        Ok(result) => (0, result),
        Err(error) => (
            1,
            serde_json::json!({"passed":false,"environment":"isolated_private_shell_host_in_app_process","explorer_accessed":false,"error":error}),
        ),
    };
    let bytes = result.to_string();
    if bytes.len() >= capacity {
        return -1;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), output, bytes.len());
        *output.add(bytes.len()) = 0;
    }
    status
}
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
                let mut pid = 0;
                GetWindowThreadProcessId(reply, &mut pid);
                if class_name(reply) == "LiteTray.ProbeReceiver.v1" && same_installation(pid) {
                    report(reply, call.lParam as u64);
                }
            }
            if call.message == WM_COPYDATA && call.lParam != 0 && is_taskbar(call.hwnd) {
                taskbar::request(call.hwnd, call.wParam as HWND, call.lParam);
                inspect_request(call.hwnd, call.wParam as HWND, call.lParam);
            }
        }
        CallNextHookEx(null_mut(), code, w, l)
    }
}
unsafe fn inspect_request(taskbar: HWND, sender: HWND, data: isize) {
    unsafe {
        use litetray_adapter_protocol::*;
        if class_name(sender) != RECEIVER_CLASS {
            return;
        }
        let copy = &*(data as *const COPYDATASTRUCT);
        if copy.dwData != REQUEST_TAG
            || copy.cbData == 0
            || copy.cbData as usize > MAX_REQUEST
            || copy.lpData.is_null()
        {
            return;
        }
        let Ok(request) = InspectRequest::decode(std::slice::from_raw_parts(
            copy.lpData.cast(),
            copy.cbData as usize,
        )) else {
            return;
        };
        if request.receiver != sender as u64 {
            return;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(sender, &mut pid);
        if pid == 0 || pid != request.host_pid || !same_installation(pid) {
            return;
        }
        let response = inspect::run(&request.layout, request.nonce);
        let Ok(bytes) = serde_json::to_vec(&response) else {
            return;
        };
        if bytes.len() > MAX_RESPONSE {
            return;
        }
        let reply = COPYDATASTRUCT {
            dwData: RESPONSE_TAG,
            cbData: bytes.len() as u32,
            lpData: bytes.as_ptr() as *mut _,
        };
        let mut result = 0;
        SendMessageTimeoutW(
            sender,
            WM_COPYDATA,
            taskbar as usize,
            &reply as *const _ as isize,
            SMTO_ABORTIFHUNG,
            1000,
            &mut result,
        );
    }
}
unsafe fn same_installation(pid: u32) -> bool {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return false;
        }
        let mut path = vec![0u16; 32768];
        let mut length = path.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length) != 0;
        CloseHandle(process);
        if !ok {
            return false;
        }
        let host = String::from_utf16_lossy(&path[..length as usize]);
        let mut own = std::ptr::null_mut();
        if GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            LiteTrayCallWndProc as *const u16,
            &mut own,
        ) == 0
        {
            return false;
        }
        let length = GetModuleFileNameW(own, path.as_mut_ptr(), path.len() as u32) as usize;
        if length == 0 || length >= path.len() {
            return false;
        }
        let dll = String::from_utf16_lossy(&path[..length]);
        let parent = std::path::Path::new(&dll).parent();
        parent.is_some_and(|parent| {
            parent
                .join("LiteTray.exe")
                .to_string_lossy()
                .eq_ignore_ascii_case(&host)
        })
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
        let machine = u16::from_le_bytes(header.get(pe + 4..pe + 6)?.try_into().ok()?);
        if header.get(optional..optional + 2) != Some(&[0x0b, 0x02]) {
            return None;
        }
        let timestamp = get32(&header, pe + 8)?;
        let image_size = get32(&header, optional + 56)?;
        let debug_rva = get32(&header, optional + 112 + 6 * 8)? as usize;
        let debug_size = get32(&header, optional + 112 + 6 * 8 + 4)? as usize;
        if debug_size == 0
            || !debug_size.is_multiple_of(28)
            || debug_size > 4096
            || debug_rva.checked_add(debug_size)? > image_size as usize
        {
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
                serde_json::json!({"machine":machine,"timestamp":timestamp,"image_size":image_size,"pdb_guid":&cv[4..20],"pdb_age":get32(&cv,20)?}),
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
