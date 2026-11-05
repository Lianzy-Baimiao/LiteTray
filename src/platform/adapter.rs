//! Synchronous inspection session. The owner holds every OS resource until
//! after the callback roundtrip; the returned snapshot contains no COM pointer.
use litetray_adapter_protocol::{
    HostLayout, InspectRequest, InspectResponse, MAX_REQUEST, MAX_RESPONSE, RECEIVER_CLASS,
    REQUEST_TAG, RESPONSE_TAG, VERSION,
};
use std::{
    cell::RefCell,
    path::Path,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{DataExchange::COPYDATASTRUCT, LibraryLoader::*, Threading::GetCurrentProcessId},
    UI::WindowsAndMessaging::*,
};
thread_local! {
    static REPLY:RefCell<Option<Vec<u8>>>=const{RefCell::new(None)};
    static EXPECTED:std::cell::Cell<Option<(u64,u32,u32)>>=const{std::cell::Cell::new(None)};
}
struct Session {
    window: HWND,
    library: HMODULE,
    hook: HHOOK,
}
/// Exercise actual XAML controls created in this process. No taskbar discovery,
/// cross-process messaging or hook installation is involved.
pub fn self_test_clock() -> Result<serde_json::Value, String> {
    unsafe {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let dll = executable
            .parent()
            .ok_or("程序目录无效")?
            .join("litetray_taskbar_adapter.dll");
        let session = Session {
            window: null_mut(),
            hook: null_mut(),
            library: LoadLibraryExW(
                super::wide(&dll.to_string_lossy()).as_ptr(),
                null_mut(),
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            ),
        };
        if session.library.is_null() {
            return Err(format!(
                "加载适配 DLL 失败：{}",
                std::io::Error::last_os_error()
            ));
        }
        let function = GetProcAddress(session.library, c"LiteTraySelfTestClock".as_ptr().cast())
            .ok_or("适配 DLL 缺少隔离检查接口")?;
        let check: unsafe extern "system" fn(*mut u8, usize) -> i32 = std::mem::transmute(function);
        let mut output = vec![0u8; 16384];
        let status = check(output.as_mut_ptr(), output.len());
        let length = output.iter().position(|b| *b == 0).ok_or("检查响应过长")?;
        let value = serde_json::from_slice(&output[..length]).map_err(|_| "检查响应无效")?;
        if status != 0 {
            return Err(format!("隔离 XAML 检查失败：{value}"));
        }
        Ok(value)
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        EXPECTED.with(|e| e.set(None));
        unsafe {
            if !self.hook.is_null() {
                UnhookWindowsHookEx(self.hook);
            }
            if !self.library.is_null() {
                FreeLibrary(self.library);
            }
            if !self.window.is_null() {
                DestroyWindow(self.window);
            }
        }
    }
}

pub fn inspect() -> Result<InspectResponse, String> {
    let module = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .ok_or("SystemRoot 不可用")?
        .join("System32/taskbar.dll");
    let bytes = std::fs::read(&module).map_err(|e| e.to_string())?;
    let identity = crate::pe::identity(&bytes)?;
    let path = crate::config::data_dir()
        .join("adapters")
        .join(format!("{}.json", identity.sha256));
    let map_bytes = std::fs::read(&path).map_err(|_| "没有匹配的任务栏符号，请先准备兼容性资料")?;
    if map_bytes.len() > 4 * 1024 * 1024 {
        return Err("符号映射尺寸无效".into());
    }
    let map: super::symbols::SymbolMap =
        serde_json::from_slice(&map_bytes).map_err(|_| "符号映射损坏")?;
    let layout = super::symbols::host_layout(&map, &bytes)?;
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    inspect_with_layout(
        &executable
            .parent()
            .ok_or("程序目录无效")?
            .join("litetray_taskbar_adapter.dll"),
        layout,
    )
}
fn inspect_with_layout(dll: &Path, layout: HostLayout) -> Result<InspectResponse, String> {
    unsafe {
        layout.validate()?;
        if !dll.is_file() {
            return Err("缺少任务栏适配 DLL".into());
        }
        let diagnostics = super::diagnostics::collect();
        let taskbar = diagnostics
            .taskbars
            .iter()
            .find(|t| t.class == "Shell_TrayWnd")
            .ok_or("没有找到任务栏")?;
        let class = super::wide(RECEIVER_CLASS);
        let instance = GetModuleHandleW(null());
        let wc = WNDCLASSW {
            lpfnWndProc: Some(receive),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            return Err("无法注册适配接收窗口".into());
        }
        let mut session = Session {
            window: CreateWindowExW(
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
            ),
            library: null_mut(),
            hook: null_mut(),
        };
        if session.window.is_null() {
            return Err("无法创建适配接收窗口".into());
        }
        session.library = LoadLibraryExW(
            super::wide(&dll.to_string_lossy()).as_ptr(),
            null_mut(),
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
        );
        if session.library.is_null() {
            return Err(format!(
                "加载适配 DLL 失败：{}",
                std::io::Error::last_os_error()
            ));
        }
        let function = GetProcAddress(session.library, c"LiteTrayCallWndProc".as_ptr().cast())
            .ok_or("适配 DLL 版本不匹配")?;
        let hook: unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT =
            std::mem::transmute(function);
        session.hook = SetWindowsHookExW(
            WH_CALLWNDPROC,
            Some(hook),
            session.library,
            taskbar.thread_id,
        );
        if session.hook.is_null() {
            return Err(format!(
                "任务栏线程接入失败：{}",
                std::io::Error::last_os_error()
            ));
        }
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos() as u64;
        let request = InspectRequest {
            version: VERSION,
            nonce,
            receiver: session.window as u64,
            host_pid: GetCurrentProcessId(),
            layout,
        };
        let bytes = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_REQUEST {
            return Err("任务栏请求过大".into());
        }
        REPLY.with(|r| *r.borrow_mut() = None);
        EXPECTED.with(|e| e.set(Some((nonce, taskbar.process_id, taskbar.thread_id))));
        let message = COPYDATASTRUCT {
            dwData: REQUEST_TAG,
            cbData: bytes.len() as u32,
            lpData: bytes.as_ptr() as *mut _,
        };
        let mut result = 0;
        let sent = SendMessageTimeoutW(
            taskbar.hwnd as HWND,
            WM_COPYDATA,
            session.window as usize,
            &message as *const _ as isize,
            SMTO_ABORTIFHUNG,
            5000,
            &mut result,
        );
        if sent == 0 {
            return Err("任务栏检查超时或访问受限，未启用接管".into());
        }
        let bytes = REPLY
            .with(|r| r.borrow_mut().take())
            .ok_or("任务栏未返回兼容响应，未启用接管")?;
        let response =
            InspectResponse::decode(&bytes, nonce, taskbar.process_id, taskbar.thread_id)?;
        drop(session);
        Ok(response)
    }
}
unsafe extern "system" fn receive(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_COPYDATA && l != 0 {
            let Some((nonce, expected_pid, expected_tid)) = EXPECTED.with(|e| e.get()) else {
                return 0;
            };
            let mut pid = 0;
            let tid = GetWindowThreadProcessId(w as HWND, &mut pid);
            if pid != expected_pid || tid != expected_tid {
                return 0;
            }
            let message = &*(l as *const COPYDATASTRUCT);
            if message.dwData != RESPONSE_TAG
                || message.cbData == 0
                || message.cbData as usize > MAX_RESPONSE
                || message.lpData.is_null()
            {
                return 0;
            }
            let bytes =
                std::slice::from_raw_parts(message.lpData.cast::<u8>(), message.cbData as usize)
                    .to_vec();
            if InspectResponse::decode(&bytes, nonce, pid, tid).is_err() {
                return 0;
            }
            REPLY.with(|r| *r.borrow_mut() = Some(bytes));
            return 1;
        }
        DefWindowProcW(hwnd, msg, w, l)
    }
}
