//! QQNT's frameless, layered hover card clamps its X position to the primary
//! work area even after Shell_NotifyIconGetRect returns the secondary icon.
//! Observe its window events out of process; never inject into QQ or relocate
//! its regular windows. A fresh card is eligible only while the physical
//! pointer is inside that QQ process's icon on a managed secondary taskbar.
use crate::config::TrayConfig;
use std::{
    cell::RefCell,
    collections::VecDeque,
    mem::size_of,
    ptr::null_mut,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::Threading::*,
    UI::{Accessibility::*, Shell::*, WindowsAndMessaging::*},
};

thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }
#[derive(Default)]
struct State {
    processes: Vec<Process>,
    taskbars: Vec<usize>,
    armed: Vec<Armed>,
    recent: VecDeque<serde_json::Value>,
}
struct Process {
    pid: u32,
    receiver: usize,
    hooks: Vec<HWINEVENTHOOK>,
    icon_ids: Vec<u32>,
}
impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            for hook in &self.hooks {
                UnhookWinEvent(*hook);
            }
        }
    }
}
#[derive(Clone, Copy)]
struct Armed {
    hwnd: usize,
    pid: u32,
    taskbar: usize,
    anchor: RECT,
    work: RECT,
    expires: Instant,
    moves: u8,
}
pub struct Router {
    next_refresh: Instant,
}
impl Default for Router {
    fn default() -> Self {
        Self {
            next_refresh: Instant::now(),
        }
    }
}
impl Drop for Router {
    fn drop(&mut self) {
        STATE.with(|state| *state.borrow_mut() = State::default());
    }
}
impl Router {
    pub fn configure(&mut self, config: &TrayConfig) {
        self.next_refresh = Instant::now() + Duration::from_secs(10);
        if !config.enabled || !config.show_icons {
            STATE.with(|state| *state.borrow_mut() = State::default());
            return;
        }
        let diagnostics = super::diagnostics::collect_for(config);
        let targets = diagnostics
            .target_plan
            .map(|plan| {
                plan.targets
                    .into_iter()
                    .filter(|t| t.route.secondary)
                    .map(|t| t.route.window as usize)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut receivers: Vec<(u32, usize)> = vec![];
        unsafe {
            EnumWindows(Some(find_receivers), &mut receivers as *mut _ as isize);
        }
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.taskbars = targets;
            state.armed.clear();
            state
                .processes
                .retain(|p| receivers.contains(&(p.pid, p.receiver)));
            for (pid, receiver) in receivers {
                if state.processes.iter().any(|p| p.pid == pid) {
                    continue;
                }
                let mut hooks = vec![];
                unsafe {
                    for (first, last) in [
                        (EVENT_OBJECT_SHOW, EVENT_OBJECT_HIDE),
                        (EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE),
                    ] {
                        let hook = SetWinEventHook(
                            first,
                            last,
                            null_mut(),
                            Some(event),
                            pid,
                            0,
                            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                        );
                        if !hook.is_null() {
                            hooks.push(hook);
                        }
                    }
                }
                state.processes.push(Process {
                    pid,
                    receiver,
                    hooks,
                    icon_ids: vec![],
                });
            }
        });
    }
    pub fn tick(&mut self, config: &TrayConfig) {
        if Instant::now() >= self.next_refresh {
            self.configure(config);
        }
    }
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
fn candidate(name: &str, style: u32, exstyle: u32, width: i32, height: i32) -> bool {
    name == "Chrome_WidgetWin_1"
        && style & (WS_CHILD | WS_THICKFRAME | WS_MAXIMIZEBOX) == 0
        && exstyle & (WS_EX_LAYERED | WS_EX_TOPMOST) == (WS_EX_LAYERED | WS_EX_TOPMOST)
        && (24..=1200).contains(&width)
        && (24..=1800).contains(&height)
}
fn contains(rect: RECT, point: POINT) -> bool {
    point.x >= rect.left && point.x < rect.right && point.y >= rect.top && point.y < rect.bottom
}
fn placement(current: RECT, anchor: RECT, work: RECT) -> Option<(i32, i32)> {
    let width = current.right.checked_sub(current.left)?;
    let height = current.bottom.checked_sub(current.top)?;
    if width <= 0 || height <= 0 || work.right <= work.left || work.bottom <= work.top {
        return None;
    }
    let x = (anchor.left + (anchor.right - anchor.left - width) / 2)
        .clamp(work.left, (work.right - width).max(work.left));
    let y = if anchor.bottom <= work.top {
        anchor.bottom + 8
    } else {
        anchor.top - height - 8
    }
    .clamp(work.top, (work.bottom - height).max(work.top));
    Some((x, y))
}
unsafe fn icon_rect(receiver: usize, id: u32) -> Option<RECT> {
    unsafe {
        let identifier = NOTIFYICONIDENTIFIER {
            cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
            hWnd: receiver as HWND,
            uID: id,
            guidItem: windows_sys::core::GUID::from_u128(0),
        };
        let mut rect: RECT = std::mem::zeroed();
        (Shell_NotifyIconGetRect(&identifier, &mut rect) >= 0).then_some(rect)
    }
}
unsafe fn resolve(state: &mut State, pid: u32, hwnd: HWND) -> Option<Armed> {
    unsafe {
        let mut point = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut point) == 0 {
            return None;
        }
        let taskbar = state.taskbars.iter().copied().find(|host| {
            let mut r: RECT = std::mem::zeroed();
            IsWindow(*host as HWND) != 0
                && class(*host as HWND) == "Shell_SecondaryTrayWnd"
                && GetWindowRect(*host as HWND, &mut r) != 0
                && contains(r, point)
        })?;
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(
            MonitorFromWindow(taskbar as HWND, MONITOR_DEFAULTTONEAREST),
            &mut info,
        ) == 0
        {
            return None;
        }
        let process = state.processes.iter_mut().find(|p| p.pid == pid)?;
        if class(process.receiver as HWND) != "Electron_NotifyIconHostWindow" {
            return None;
        }
        // Cache successful IDs only. This bounded search occurs on a fresh
        // hover card, never on pointer movement or a periodic timer.
        let mut ids = process.icon_ids.clone();
        ids.extend((0..32).filter(|id| !process.icon_ids.contains(id)));
        for id in ids {
            if let Some(rect) = icon_rect(process.receiver, id) {
                if !process.icon_ids.contains(&id) {
                    process.icon_ids.push(id);
                }
                if contains(rect, point) {
                    return Some(Armed {
                        hwnd: hwnd as usize,
                        pid,
                        taskbar,
                        anchor: rect,
                        work: info.rcWork,
                        expires: Instant::now() + Duration::from_millis(1800),
                        moves: 0,
                    });
                }
            }
        }
        None
    }
}
unsafe extern "system" fn event(
    _hook: HWINEVENTHOOK,
    kind: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _thread: u32,
    _time: u32,
) {
    if object != OBJID_WINDOW || child != 0 || hwnd.is_null() {
        return;
    }
    STATE.with(|state| {
        let Ok(mut state) = state.try_borrow_mut() else { return; };
        unsafe {
            if kind == EVENT_OBJECT_HIDE { state.armed.retain(|a| a.hwnd != hwnd as usize); return; }
            if IsWindowVisible(hwnd) == 0 { return; }
            let mut pid = 0; GetWindowThreadProcessId(hwnd, &mut pid);
            if !state.processes.iter().any(|p| p.pid == pid) { return; }
            let mut current: RECT = std::mem::zeroed();
            if GetWindowRect(hwnd, &mut current) == 0 || !candidate(&class(hwnd), GetWindowLongW(hwnd, GWL_STYLE) as u32, GetWindowLongW(hwnd, GWL_EXSTYLE) as u32, current.right - current.left, current.bottom - current.top) { return; }
            state.armed.retain(|a| a.expires > Instant::now() && IsWindow(a.taskbar as HWND) != 0);
            let active = state.armed.iter().find(|a| a.hwnd == hwnd as usize && a.pid == pid).copied();
            let anchor = active.or_else(|| if kind == EVENT_OBJECT_SHOW { resolve(&mut state, pid, hwnd) } else { None });
            let Some(mut anchor) = anchor else { return; };
            if MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) == MonitorFromWindow(anchor.taskbar as HWND, MONITOR_DEFAULTTONEAREST) { return; }
            if anchor.moves >= 4 { return; }
            let Some((x, y)) = placement(current, anchor.anchor, anchor.work) else { return; };
            if SetWindowPos(hwnd, null_mut(), x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER | SWP_ASYNCWINDOWPOS) == 0 { return; }
            anchor.moves += 1;
            state.armed.retain(|a| a.hwnd != anchor.hwnd); state.armed.push(anchor);
            if state.recent.len() == 16 { state.recent.pop_front(); }
            state.recent.push_back(serde_json::json!({"process":pid,"window":hwnd as usize,"from":[current.left,current.top,current.right,current.bottom],"to":[x,y],"taskbar":anchor.taskbar}));
            if let Ok(bytes) = serde_json::to_vec_pretty(&state.recent) { let _ = crate::config::atomic_write(&crate::config::data_dir().join("popup-placement.json"), &bytes); }
        }
    });
}
pub fn diagnostics() -> serde_json::Value {
    STATE.with(|state| { let state = state.borrow(); serde_json::json!({"processes":state.processes.len(), "secondary_taskbars":state.taskbars.len(), "relocations":state.recent}) })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captured_qq_hover_card_is_relocated_but_main_chat_and_primary_are_not_candidates() {
        assert!(candidate(
            "Chrome_WidgetWin_1",
            0x14c20000,
            0x00280108,
            350,
            643
        ));
        assert!(!candidate(
            "Chrome_WidgetWin_1",
            0x14c70000,
            0x00200100,
            1230,
            1422
        ));
        assert!(!candidate(
            "Chrome_WidgetWin_1",
            0x14c20000,
            0x00200108,
            350,
            643
        ));
        assert!(!candidate("OtherApp", 0x14c20000, 0x00280108, 350, 643));
        let current = RECT {
            left: 3490,
            top: 898,
            right: 3840,
            bottom: 1541,
        };
        let anchor = RECT {
            left: 5975,
            top: 1540,
            right: 6015,
            bottom: 1600,
        };
        let work = RECT {
            left: 3840,
            top: 0,
            right: 6400,
            bottom: 1540,
        };
        assert_eq!(placement(current, anchor, work), Some((5820, 889)));
        assert!(contains(anchor, POINT { x: 5990, y: 1570 }));
        assert!(!contains(anchor, POINT { x: 3430, y: 2130 }));
    }
    #[test]
    fn actual_hidden_window_placement_preserves_size_and_foreground() {
        unsafe {
            let previous = windows_sys::Win32::UI::HiDpi::SetThreadDpiAwarenessContext(
                windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            );
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                super::super::wide("STATIC").as_ptr(),
                std::ptr::null(),
                WS_POPUP,
                3490,
                898,
                350,
                643,
                null_mut(),
                null_mut(),
                null_mut(),
                std::ptr::null(),
            );
            assert!(!hwnd.is_null());
            let foreground = GetForegroundWindow();
            let mut cursor: POINT = std::mem::zeroed();
            GetCursorPos(&mut cursor);
            let mut current: RECT = std::mem::zeroed();
            GetWindowRect(hwnd, &mut current);
            let (x, y) = placement(
                current,
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
            )
            .unwrap();
            assert_ne!(
                SetWindowPos(
                    hwnd,
                    null_mut(),
                    x,
                    y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER
                ),
                0
            );
            GetWindowRect(hwnd, &mut current);
            assert_eq!(
                [current.left, current.top, current.right, current.bottom],
                [5820, 889, 6170, 1532]
            );
            assert_eq!(GetForegroundWindow(), foreground);
            let mut after: POINT = std::mem::zeroed();
            GetCursorPos(&mut after);
            assert_eq!((cursor.x, cursor.y), (after.x, after.y));
            DestroyWindow(hwnd);
            windows_sys::Win32::UI::HiDpi::SetThreadDpiAwarenessContext(previous);
        }
    }
}
