//! A thread-scoped Windows hook for QQNT's notification receiver. No API
//! detours. Only the frameless layered hover card receives a window subclass.
//! Its final WINDOWPOS is corrected after QQ's handler, before Windows applies
//! or paints it. Inert without a live, same-installation host and fresh lease.
#![cfg(windows)]
use litetray_adapter_protocol::popup::{HOST_PROPERTY, MAX_BYTES, Request, TAG};
use std::{cell::RefCell, mem::size_of, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{
        DataExchange::COPYDATASTRUCT, LibraryLoader::*, SystemInformation::GetTickCount64,
        Threading::*,
    },
    UI::{Shell::*, WindowsAndMessaging::*},
};
const SUBCLASS: usize = 0x4c545051;
struct Lease {
    host: HWND,
    process: HANDLE,
    receiver: HWND,
    taskbars: Vec<HWND>,
    until: u64,
    icon: Option<u32>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.process);
        }
    }
}
thread_local! { static LEASE: RefCell<Option<Lease>> = const { RefCell::new(None) }; }
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn class(hwnd: HWND) -> String {
    unsafe {
        let mut text = [0; 128];
        let n = GetClassNameW(hwnd, text.as_mut_ptr(), 128).max(0) as usize;
        String::from_utf16_lossy(&text[..n])
    }
}
fn contains(r: RECT, p: POINT) -> bool {
    p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom
}
fn candidate(hwnd: HWND, width: i32, height: i32) -> bool {
    unsafe {
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        class(hwnd) == "Chrome_WidgetWin_1"
            && style & (WS_CHILD | WS_THICKFRAME | WS_MAXIMIZEBOX) == 0
            && ex & (WS_EX_LAYERED | WS_EX_TOPMOST) == (WS_EX_LAYERED | WS_EX_TOPMOST)
            && (24..=1200).contains(&width)
            && (24..=1800).contains(&height)
    }
}
unsafe fn same_installation(process: HANDLE) -> bool {
    unsafe {
        let mut path = [0u16; 32768];
        let mut len = path.len() as u32;
        if QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut len) == 0 {
            return false;
        }
        let host = String::from_utf16_lossy(&path[..len as usize]);
        let mut module = null_mut();
        if GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            LiteTrayPopupHook as *const u16,
            &mut module,
        ) == 0
        {
            return false;
        }
        let n = GetModuleFileNameW(module, path.as_mut_ptr(), path.len() as u32) as usize;
        if n == 0 || n >= path.len() {
            return false;
        }
        let dll = String::from_utf16_lossy(&path[..n]);
        std::path::Path::new(&dll).parent().is_some_and(|p| {
            p.join("LiteTray.exe")
                .to_string_lossy()
                .eq_ignore_ascii_case(&host)
        })
    }
}
unsafe fn configure(call: &CWPSTRUCT) {
    unsafe {
        if class(call.hwnd) != "Electron_NotifyIconHostWindow" || call.lParam == 0 {
            return;
        }
        let data = &*(call.lParam as *const COPYDATASTRUCT);
        if data.dwData != TAG
            || data.cbData as usize > MAX_BYTES
            || data.cbData == 0
            || data.lpData.is_null()
        {
            return;
        }
        let Some(request) = Request::decode(std::slice::from_raw_parts(
            data.lpData.cast(),
            data.cbData as usize,
        )) else {
            return;
        };
        let host = request.host as HWND;
        let mut pid = 0;
        GetWindowThreadProcessId(host, &mut pid);
        if request.host as usize != call.wParam
            || pid != request.host_pid
            || !class(host).starts_with("LiteTray.Settings.1")
        {
            return;
        }
        let process = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        );
        if process.is_null() {
            return;
        }
        if !same_installation(process) {
            CloseHandle(process);
            return;
        }
        let taskbars: Vec<HWND> = request.taskbars.into_iter().map(|h| h as HWND).collect();
        if taskbars
            .iter()
            .any(|&h| class(h) != "Shell_SecondaryTrayWnd")
        {
            CloseHandle(process);
            return;
        }
        LEASE.with(|cell| {
            if let Ok(mut lease) = cell.try_borrow_mut() {
                if taskbars.is_empty() {
                    *lease = None;
                    CloseHandle(process);
                    RemovePropW(call.hwnd, wide(HOST_PROPERTY).as_ptr());
                } else {
                    let icon = lease.as_ref().and_then(|l| l.icon);
                    *lease = Some(Lease {
                        host,
                        process,
                        receiver: call.hwnd,
                        taskbars,
                        until: GetTickCount64() + 30000,
                        icon,
                    });
                    SetPropW(call.hwnd, wide(HOST_PROPERTY).as_ptr(), host);
                }
            } else {
                CloseHandle(process);
            }
        });
    }
}
unsafe fn anchor() -> Option<(RECT, RECT)> {
    unsafe {
        #[cfg(test)]
        if let Some(value) = TEST_ANCHOR.with(|a| *a.borrow()) {
            return Some(value);
        }
        LEASE.with(|cell| {
            let mut borrow = cell.try_borrow_mut().ok()?;
            let lease = borrow.as_mut()?;
            if GetTickCount64() > lease.until
                || WaitForSingleObject(lease.process, 0) != WAIT_TIMEOUT
                || IsWindow(lease.host) == 0
            {
                return None;
            }
            let mut point = POINT { x: 0, y: 0 };
            if GetCursorPos(&mut point) == 0 {
                return None;
            }
            let taskbar = *lease.taskbars.iter().find(|&&hwnd| {
                let mut r: RECT = std::mem::zeroed();
                class(hwnd) == "Shell_SecondaryTrayWnd"
                    && GetWindowRect(hwnd, &mut r) != 0
                    && contains(r, point)
            })?;
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(
                MonitorFromWindow(taskbar, MONITOR_DEFAULTTONEAREST),
                &mut info,
            ) == 0
            {
                return None;
            }
            for id in lease
                .icon
                .into_iter()
                .chain((0..32).filter(|id| Some(*id) != lease.icon))
            {
                let identifier = NOTIFYICONIDENTIFIER {
                    cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
                    hWnd: lease.receiver,
                    uID: id,
                    guidItem: windows_sys::core::GUID::from_u128(0),
                };
                let mut rect: RECT = std::mem::zeroed();
                if Shell_NotifyIconGetRect(&identifier, &mut rect) >= 0 && contains(rect, point) {
                    lease.icon = Some(id);
                    return Some((rect, info.rcWork));
                }
            }
            None
        })
    }
}
unsafe fn repair(hwnd: HWND, position: &mut WINDOWPOS) {
    unsafe {
        if position.flags & SWP_HIDEWINDOW != 0 {
            return;
        }
        let mut current: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut current) == 0 {
            return;
        }
        let (width, height) = if position.flags & SWP_NOSIZE == 0 {
            (position.cx, position.cy)
        } else {
            (current.right - current.left, current.bottom - current.top)
        };
        if !candidate(hwnd, width, height) {
            return;
        }
        let Some((icon, work)) = anchor() else {
            return;
        };
        let x = (icon.left + (icon.right - icon.left - width) / 2)
            .clamp(work.left, (work.right - width).max(work.left));
        let y = if icon.bottom <= work.top {
            icon.bottom + 8
        } else {
            icon.top - height - 8
        }
        .clamp(work.top, (work.bottom - height).max(work.top));
        position.x = x;
        position.y = y;
        position.flags &= !SWP_NOMOVE;
        let key = wide("LiteTray.PopupHook.Corrections.v1");
        let count = GetPropW(hwnd, key.as_ptr()) as usize;
        SetPropW(hwnd, key.as_ptr(), count.saturating_add(1) as HANDLE);
    }
}
unsafe extern "system" fn card_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        let result = DefSubclassProc(hwnd, msg, w, l);
        if msg == WM_WINDOWPOSCHANGING && l != 0 {
            repair(hwnd, &mut *(l as *mut WINDOWPOS));
        }
        if msg == WM_NCDESTROY {
            RemoveWindowSubclass(hwnd, Some(card_proc), SUBCLASS);
        }
        result
    }
}
/// # Safety
/// Called only by Windows with a valid CWPSTRUCT for this QQ receiver thread.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn LiteTrayPopupHook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if code == HC_ACTION as i32 && l != 0 {
            let call = &*(l as *const CWPSTRUCT);
            if call.message == WM_COPYDATA {
                configure(call);
            }
            if call.message == WM_WINDOWPOSCHANGING
                && call.lParam != 0
                && LEASE.with(|s| s.try_borrow().is_ok_and(|v| v.is_some()))
            {
                let pos = &*(call.lParam as *const WINDOWPOS);
                let mut r: RECT = std::mem::zeroed();
                GetWindowRect(call.hwnd, &mut r);
                let (width, height) = if pos.flags & SWP_NOSIZE == 0 {
                    (pos.cx, pos.cy)
                } else {
                    (r.right - r.left, r.bottom - r.top)
                };
                if candidate(call.hwnd, width, height) {
                    // A subclass must never outlive its code, even if QQ retains a
                    // hidden card after the host exits. The lease then makes it inert.
                    let mut module = null_mut();
                    if GetModuleHandleExW(
                        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN,
                        LiteTrayPopupHook as *const u16,
                        &mut module,
                    ) != 0
                    {
                        SetWindowSubclass(call.hwnd, Some(card_proc), SUBCLASS, 0);
                    }
                }
            }
        }
        CallNextHookEx(null_mut(), code, w, l)
    }
}
#[cfg(test)]
thread_local! { static TEST_ANCHOR: RefCell<Option<(RECT,RECT)>> = const { RefCell::new(None) }; }
#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::null;
    unsafe extern "system" fn clamps_to_primary(
        hwnd: HWND,
        msg: u32,
        w: WPARAM,
        l: LPARAM,
    ) -> LRESULT {
        unsafe {
            if msg == WM_WINDOWPOSCHANGING && l != 0 {
                let p = &mut *(l as *mut WINDOWPOS);
                p.x = 3490;
                p.y = 898;
                p.flags &= !SWP_NOMOVE;
            }
            DefWindowProcW(hwnd, msg, w, l)
        }
    }
    #[test]
    fn corrects_qq_clamping_before_windows_applies_the_first_and_repeated_positions() {
        unsafe {
            let class_name = wide("Chrome_WidgetWin_1");
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(clamps_to_primary),
                hInstance: GetModuleHandleW(null()),
                lpszClassName: class_name.as_ptr(),
                ..std::mem::zeroed()
            });
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                class_name.as_ptr(),
                null(),
                WS_POPUP,
                3490,
                898,
                350,
                643,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!hwnd.is_null());
            TEST_ANCHOR.with(|a| {
                *a.borrow_mut() = Some((
                    RECT {
                        left: 5975,
                        top: 1540,
                        right: 6015,
                        bottom: 1600,
                    },
                    RECT {
                        left: 3840,
                        top: 0,
                        right: 6400,
                        bottom: 1540,
                    },
                ))
            });
            SetWindowSubclass(hwnd, Some(card_proc), SUBCLASS, 0);
            let foreground = GetForegroundWindow();
            let mut point: POINT = std::mem::zeroed();
            GetCursorPos(&mut point);
            let mut positions = vec![];
            for _ in 0..100 {
                SetWindowPos(
                    hwnd,
                    null_mut(),
                    3490,
                    898,
                    350,
                    643,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
                let mut r: RECT = std::mem::zeroed();
                GetWindowRect(hwnd, &mut r);
                positions.push((r.left, r.top));
            }
            let mut after: POINT = std::mem::zeroed();
            GetCursorPos(&mut after);
            assert_eq!(foreground, GetForegroundWindow());
            assert_eq!((point.x, point.y), (after.x, after.y));
            assert_eq!(IsWindowVisible(hwnd), 0);
            // Revocation/host absence must restore QQ's unmodified positioning.
            TEST_ANCHOR.with(|a| *a.borrow_mut() = None);
            SetWindowPos(
                hwnd,
                null_mut(),
                3490,
                898,
                350,
                643,
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
            let mut unmodified: RECT = std::mem::zeroed();
            GetWindowRect(hwnd, &mut unmodified);
            assert_eq!((unmodified.left, unmodified.top), (3490, 898));
            // A normal QQ window must not match, even at the same dimensions.
            SetWindowLongW(hwnd, GWL_STYLE, (WS_POPUP | WS_THICKFRAME) as i32);
            assert!(!candidate(hwnd, 350, 643));
            DestroyWindow(hwnd);
            TEST_ANCHOR.with(|a| *a.borrow_mut() = None);
            assert!(
                positions.iter().all(|p| *p == (5820, 889)),
                "QQ's primary-screen position reached Windows: {:?}",
                positions.first()
            );
        }
    }
}
