use serde_json::Value;
use std::{
    cell::RefCell,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{DataExchange::COPYDATASTRUCT, LibraryLoader::*},
    UI::WindowsAndMessaging::*,
};
const CLASS: &str = "LiteTray.ProbeReceiver.v1";
const PROTOCOL: usize = 0x4c545001;
thread_local! {static RESPONSE:RefCell<Option<Value>>=const{RefCell::new(None)};static NONCE:std::cell::Cell<u64>=const{std::cell::Cell::new(0)};}
/// A scoped, read-only callback on Explorer's taskbar thread. This is an
/// integration check, not the long-lived adapter lifecycle.
pub fn run() -> Result<Value, String> {
    unsafe {
        let taskbars = super::diagnostics::collect().taskbars;
        let taskbar = taskbars
            .iter()
            .find(|t| t.class == "Shell_TrayWnd")
            .ok_or("未找到系统任务栏")?;
        let library = super::dll::load(super::dll::Kind::Taskbar)?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos() as u64
            & i64::MAX as u64;
        let class = super::wide(CLASS);
        let instance = GetModuleHandleW(null());
        let wc = WNDCLASSW {
            lpfnWndProc: Some(receiver),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            return Err("无法注册探测接收窗口".into());
        }
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            return Err("创建探测窗口失败".into());
        }
        let Some(address) =
            GetProcAddress(library.handle(), c"LiteTrayCallWndProc".as_ptr().cast())
        else {
            DestroyWindow(hwnd);
            return Err("任务栏 DLL 缺少兼容入口".into());
        };
        let callback: unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT =
            std::mem::transmute(address);
        let hook = SetWindowsHookExW(
            WH_CALLWNDPROC,
            Some(callback),
            library.handle(),
            taskbar.thread_id,
        );
        if hook.is_null() {
            let error = std::io::Error::last_os_error();
            DestroyWindow(hwnd);
            return Err(format!("无法接入任务栏线程：{error}"));
        }
        NONCE.with(|n| n.set(nonce));
        RESPONSE.with(|r| *r.borrow_mut() = None);
        let message = RegisterWindowMessageW(super::wide("LiteTray.TaskbarProbe.v1").as_ptr());
        let mut result = 0;
        let sent = SendMessageTimeoutW(
            taskbar.hwnd as HWND,
            message,
            hwnd as usize,
            nonce as isize,
            SMTO_ABORTIFHUNG,
            3000,
            &mut result,
        );
        UnhookWindowsHookEx(hook);
        DestroyWindow(hwnd);
        if sent == 0 {
            return Err("任务栏线程探测超时或消息被拒绝".into());
        }
        let response = RESPONSE
            .with(|r| r.borrow_mut().take())
            .ok_or("任务栏没有返回兼容的探测响应")?;
        if response["process_id"].as_u64() != Some(taskbar.process_id as u64)
            || response["thread_id"].as_u64() != Some(taskbar.thread_id as u64)
        {
            return Err("任务栏响应进程/线程不匹配".into());
        }
        Ok(response)
    }
}
unsafe extern "system" fn receiver(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if message == WM_COPYDATA && l != 0 {
            let copy = &*(l as *const COPYDATASTRUCT);
            if copy.dwData != PROTOCOL
                || copy.cbData == 0
                || copy.cbData > 64 * 1024
                || copy.lpData.is_null()
            {
                return 0;
            }
            if let Ok(value) = serde_json::from_slice::<Value>(std::slice::from_raw_parts(
                copy.lpData.cast::<u8>(),
                copy.cbData as usize,
            )) && value["protocol"].as_u64() == Some(1)
                && value["nonce"].as_u64() == Some(NONCE.with(|n| n.get()))
            {
                RESPONSE.with(|r| *r.borrow_mut() = Some(value));
                return 1;
            }
            return 0;
        }
        DefWindowProcW(hwnd, message, w, l)
    }
}
