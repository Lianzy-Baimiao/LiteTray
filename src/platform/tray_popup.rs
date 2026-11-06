//! QQNT hover routing is installed on its notification receiver thread. The
//! companion DLL corrects WINDOWPOS before display; this worker only manages
//! the scoped hook and renews its lease. No synchronous discovery runs on the
//! calendar/UI thread, and no input is generated.
use super::wide;
use crate::config::TrayConfig;
use litetray_adapter_protocol::popup::{HOST_PROPERTY, Request, TAG};
use std::{
    ptr::null_mut,
    sync::{Mutex, mpsc},
    thread,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::*,
    System::{DataExchange::COPYDATASTRUCT, LibraryLoader::*, Threading::*},
    UI::WindowsAndMessaging::*,
};
static REPORT: Mutex<Option<serde_json::Value>> = Mutex::new(None);
#[derive(Default)]
pub struct Router {
    worker: Option<Worker>,
}
struct Worker {
    tx: mpsc::Sender<Option<(TrayConfig, usize)>>,
    join: Option<thread::JoinHandle<()>>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.tx.send(None);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
impl Router {
    pub fn configure(&mut self, config: &TrayConfig, owner: HWND) {
        if self.worker.is_none() {
            let (tx, rx) = mpsc::channel();
            self.worker = Some(Worker {
                tx,
                join: Some(thread::spawn(move || run(rx))),
            });
        }
        if let Some(worker) = &self.worker {
            let _ = worker.tx.send(Some((config.clone(), owner as usize)));
        }
    }
}
struct Hook {
    receiver: HWND,
    pid: u32,
    thread: u32,
    hook: HHOOK,
}
impl Drop for Hook {
    fn drop(&mut self) {
        unsafe {
            UnhookWindowsHookEx(self.hook);
        }
    }
}
unsafe fn send(receiver: HWND, host: HWND, targets: &[usize]) -> bool {
    unsafe {
        let request = Request {
            version: 1,
            host: host as u64,
            host_pid: GetCurrentProcessId(),
            taskbars: targets.iter().map(|&h| h as u64).collect(),
        };
        let Ok(bytes) = serde_json::to_vec(&request) else {
            return false;
        };
        let data = COPYDATASTRUCT {
            dwData: TAG,
            cbData: bytes.len() as u32,
            lpData: bytes.as_ptr() as *mut _,
        };
        let mut result = 0;
        SendMessageTimeoutW(
            receiver,
            WM_COPYDATA,
            host as usize,
            &data as *const _ as isize,
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            100,
            &mut result,
        ) != 0
            && (targets.is_empty() || GetPropW(receiver, wide(HOST_PROPERTY).as_ptr()) == host)
    }
}
fn run(rx: mpsc::Receiver<Option<(TrayConfig, usize)>>) {
    let mut current = None;
    let mut hooks: Vec<Hook> = vec![];
    let mut module: HMODULE = null_mut();
    loop {
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Some(configuration)) => current = Some(configuration),
            Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        let Some((config, host)) = &current else {
            continue;
        };
        let targets = if config.enabled && config.show_icons {
            super::diagnostics::collect_for(config)
                .target_plan
                .map(|plan| {
                    plan.targets
                        .into_iter()
                        .filter(|t| t.route.secondary)
                        .map(|t| t.route.window as usize)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            vec![]
        };
        unsafe {
            if targets.is_empty() {
                for hook in &hooks {
                    send(hook.receiver, *host as HWND, &[]);
                }
                hooks.clear();
                if let Ok(mut report) = REPORT.lock() {
                    *report = Some(
                        serde_json::json!({"mode":"before_window_position","hooks":0,"secondary_taskbars":0}),
                    );
                }
                continue;
            }
            if module.is_null() {
                let path = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|p| p.join("litetray_popup_hook.dll")));
                if let Some(path) = path {
                    module = LoadLibraryExW(
                        wide(&path.to_string_lossy()).as_ptr(),
                        null_mut(),
                        LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
                    );
                }
            }
            let callback = if module.is_null() {
                None
            } else {
                GetProcAddress(module, c"LiteTrayPopupHook".as_ptr().cast())
            };
            let mut receivers: Vec<(u32, usize)> = vec![];
            EnumWindows(Some(find_receivers), &mut receivers as *mut _ as isize);
            hooks.retain(|h| {
                receivers.contains(&(h.pid, h.receiver as usize))
                    && GetWindowThreadProcessId(h.receiver, null_mut()) == h.thread
            });
            for (pid, receiver) in receivers {
                if hooks
                    .iter()
                    .any(|h| h.pid == pid && h.receiver as usize == receiver)
                {
                    continue;
                }
                let tid = GetWindowThreadProcessId(receiver as HWND, null_mut());
                if let Some(callback) = callback {
                    let callback = std::mem::transmute::<
                        unsafe extern "system" fn() -> isize,
                        unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT,
                    >(callback);
                    let hook = SetWindowsHookExW(WH_CALLWNDPROC, Some(callback), module, tid);
                    if !hook.is_null() {
                        hooks.push(Hook {
                            receiver: receiver as HWND,
                            pid,
                            thread: tid,
                            hook,
                        });
                    }
                }
            }
            let live = hooks.iter().map(|h| serde_json::json!({"process":h.pid,"thread":h.thread,"receiver":h.receiver as usize,"ready":send(h.receiver,*host as HWND,&targets)})).collect::<Vec<_>>();
            let report = serde_json::json!({"mode":"before_window_position","module_loaded":!module.is_null(),"hooks":live,"secondary_taskbars":targets.len()});
            if let Ok(bytes) = serde_json::to_vec_pretty(&report) {
                let _ = crate::config::atomic_write(
                    &crate::config::data_dir().join("popup-placement.json"),
                    &bytes,
                );
            }
            if let Ok(mut slot) = REPORT.lock() {
                *slot = Some(report);
            }
        }
    }
    unsafe {
        if let Some((_, host)) = current {
            for hook in &hooks {
                send(hook.receiver, host as HWND, &[]);
            }
        }
        hooks.clear();
        if !module.is_null() {
            FreeLibrary(module);
        }
    }
}
pub fn diagnostics() -> serde_json::Value {
    REPORT
        .lock()
        .ok()
        .and_then(|v| v.clone())
        .unwrap_or_else(|| serde_json::json!({"mode":"before_window_position","started":false}))
}
fn class(hwnd: HWND) -> String {
    unsafe {
        let mut text = [0; 128];
        let n = GetClassNameW(hwnd, text.as_mut_ptr(), text.len() as i32).max(0) as usize;
        String::from_utf16_lossy(&text[..n])
    }
}
unsafe extern "system" fn find_receivers(hwnd: HWND, data: LPARAM) -> i32 {
    unsafe {
        if class(hwnd) != "Electron_NotifyIconHostWindow" {
            return 1;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return 1;
        }
        let mut path = [0; 1024];
        let mut len = path.len() as u32;
        let read = QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut len) != 0;
        CloseHandle(process);
        if read
            && String::from_utf16_lossy(&path[..len as usize])
                .rsplit('\\')
                .next()
                .is_some_and(|name| name.eq_ignore_ascii_case("QQ.exe"))
        {
            (&mut *(data as *mut Vec<(u32, usize)>)).push((pid, hwnd as usize));
        }
        1
    }
}
