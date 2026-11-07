//! Flyout lifetime is owned by each window, independently of settings/modal
//! borrows. A detached calendar is a persistent, draggable topmost window.
use super::*;
use windows_sys::Win32::System::SystemInformation::GetTickCount64;
mod input;
const OBSERVE: usize = 71;
pub(super) const DETACH: u16 = 45;
pub(super) const CLOSE: u16 = 46;
const OPEN_GRACE: u64 = 250;
struct State {
    detached: bool,
    opened: u64,
    settled_foreground: Option<HWND>,
    anchor: popup::Rect,
    buttons: bool,
}
unsafe fn state(hwnd: HWND) -> Option<&'static RefCell<State>> {
    unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<State>).as_ref() }
}
pub(super) unsafe fn init(hwnd: HWND, anchor: popup::Rect) {
    unsafe {
        SetWindowLongPtrW(
            hwnd,
            GWLP_USERDATA,
            Box::into_raw(Box::new(RefCell::new(State {
                detached: false,
                opened: GetTickCount64(),
                settled_foreground: None,
                anchor,
                buttons: buttons_down(),
            }))) as isize,
        );
    }
}
pub(super) unsafe fn detached(hwnd: HWND) -> bool {
    unsafe {
        state(hwnd)
            .and_then(|s| s.try_borrow().ok())
            .is_some_and(|s| s.detached)
    }
}
pub(super) unsafe fn set_detached(hwnd: HWND, value: bool) {
    unsafe {
        if let Some(cell) = state(hwnd)
            && let Ok(mut s) = cell.try_borrow_mut()
        {
            s.detached = value;
            s.opened = GetTickCount64();
        }
        dashboard::set_detached(GetDlgItem(hwnd, dashboard::ID as i32), value);
        if value {
            KillTimer(hwnd, OBSERVE);
            input::stop(hwnd);
        } else {
            SetTimer(hwnd, OBSERVE, 50, None);
            if IsWindowVisible(hwnd) != 0 {
                input::start(hwnd);
            }
        }
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}
pub(super) unsafe fn open(hwnd: HWND, anchor: popup::Rect, activate: bool) {
    unsafe {
        if let Some(cell) = state(hwnd)
            && let Ok(mut s) = cell.try_borrow_mut()
        {
            s.opened = GetTickCount64();
            s.settled_foreground = None;
            s.anchor = anchor;
            s.buttons = buttons_down();
        }
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        if !detached(hwnd) {
            SetTimer(hwnd, OBSERVE, 50, None);
            input::start(hwnd);
        }
        if activate && SetForegroundWindow(hwnd) != 0 {
            dashboard::focus_content(GetDlgItem(hwnd, dashboard::ID as i32));
        }
    }
}
pub(super) unsafe fn hide(hwnd: HWND) {
    unsafe {
        KillTimer(hwnd, OBSERVE);
        input::stop(hwnd);
        ShowWindow(hwnd, SW_HIDE);
    }
}
fn contains(rect: popup::Rect, point: POINT) -> bool {
    point.x >= rect.left && point.x < rect.right && point.y >= rect.top && point.y < rect.bottom
}
unsafe fn buttons_down() -> bool {
    unsafe {
        [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON]
            .iter()
            .any(|&key| GetAsyncKeyState(key as i32) < 0)
    }
}
unsafe fn shell_window(hwnd: HWND) -> bool {
    unsafe {
        let root = GetAncestor(hwnd, GA_ROOT);
        let mut text = [0u16; 128];
        let n = GetClassNameW(root, text.as_mut_ptr(), 128).max(0) as usize;
        matches!(
            String::from_utf16_lossy(&text[..n]).as_str(),
            "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
        )
    }
}
unsafe fn belongs_to_panel(hwnd: HWND, target: HWND) -> bool {
    unsafe {
        if target.is_null() {
            return false;
        }
        let mut current = target;
        for _ in 0..16 {
            if current == hwnd || IsChild(hwnd, current) != 0 {
                return true;
            }
            let root = GetAncestor(current, GA_ROOT);
            if root == hwnd {
                return true;
            }
            current = GetWindow(root, GW_OWNER);
            if current.is_null() {
                break;
            }
        }
        false
    }
}
pub(super) unsafe fn pointer_down(hwnd: HWND, point: POINT, buttons: u16, time: u32) {
    unsafe {
        if buttons == 0 || IsWindowVisible(hwnd) == 0 {
            return;
        }
        let Some(cell) = state(hwnd) else {
            return;
        };
        let Ok(s) = cell.try_borrow() else {
            return;
        };
        if s.detached || (time.wrapping_sub(s.opened as u32) as i32) < 0 {
            return;
        }
        let mut bounds: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut bounds) == 0 {
            return;
        }
        // The clock's left click is handled by the existing toggle on release.
        // Hiding on down there would make that same release reopen the panel.
        let clock_toggle = buttons == input::LEFT_DOWN && contains(s.anchor, point);
        let inside =
            PtInRect(&bounds, point) != 0 || belongs_to_panel(hwnd, WindowFromPoint(point));
        drop(s);
        if !inside && !clock_toggle {
            hide(hwnd);
        }
    }
}
unsafe fn observe(hwnd: HWND) {
    unsafe {
        if IsWindowVisible(hwnd) == 0 {
            KillTimer(hwnd, OBSERVE);
            return;
        }
        let Some(cell) = state(hwnd) else {
            return;
        };
        let Ok(mut s) = cell.try_borrow_mut() else {
            return;
        };
        if s.detached {
            return;
        }
        let pressed = !input::active(hwnd) && buttons_down();
        let click = pressed && !s.buttons;
        s.buttons = pressed;
        if GetTickCount64().saturating_sub(s.opened) < OPEN_GRACE {
            return;
        }
        let foreground = GetForegroundWindow();
        let mut point: POINT = std::mem::zeroed();
        GetCursorPos(&mut point);
        let mut bounds: RECT = std::mem::zeroed();
        GetWindowRect(hwnd, &mut bounds);
        let inside = PtInRect(&bounds, point) != 0;
        // Explorer can reclaim foreground while finishing the very click
        // which opened us. That is not an outside click or a user app switch.
        let outside_click = click && !inside && !contains(s.anchor, point);
        let changed_app = s
            .settled_foreground
            .is_some_and(|previous| previous != foreground)
            && !foreground.is_null()
            && !belongs_to_panel(hwnd, foreground)
            && !shell_window(foreground);
        if !foreground.is_null() && !shell_window(foreground) {
            s.settled_foreground = Some(foreground);
        }
        drop(s);
        if outside_click || changed_app {
            hide(hwnd);
        }
    }
}
pub(super) unsafe fn message(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> Option<LRESULT> {
    unsafe {
        match msg {
            WM_NCDESTROY => {
                KillTimer(hwnd, OBSERVE);
                input::stop(hwnd);
                let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RefCell<State>;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                if !raw.is_null() {
                    drop(Box::from_raw(raw));
                }
                None
            }
            WM_ACTIVATE if w & 0xffff == WA_INACTIVE as usize => {
                if !detached(hwnd) && IsWindowVisible(hwnd) != 0 {
                    let target = l as HWND;
                    let opening = state(hwnd)
                        .and_then(|s| s.try_borrow().ok())
                        .is_none_or(|s| GetTickCount64().saturating_sub(s.opened) < OPEN_GRACE);
                    if !opening
                        && !target.is_null()
                        && !shell_window(target)
                        && !belongs_to_panel(hwnd, target)
                    {
                        hide(hwnd);
                    } else {
                        SetTimer(hwnd, OBSERVE, 50, None);
                    }
                }
                Some(0)
            }
            WM_INPUT => {
                input::receive(hwnd, l as _);
                // Required cleanup for foreground raw-input packets. This
                // does not consume the legacy click destined for another app.
                Some(DefWindowProcW(hwnd, msg, w, l))
            }
            WM_SHOWWINDOW if w == 0 => {
                input::stop(hwnd);
                KillTimer(hwnd, OBSERVE);
                None
            }
            WM_TIMER if w == OBSERVE => {
                observe(hwnd);
                Some(0)
            }
            WM_CLOSE => {
                hide(hwnd);
                Some(0)
            }
            WM_KEYDOWN if w == VK_ESCAPE as usize => {
                if dashboard::dismiss_editor(GetDlgItem(hwnd, dashboard::ID as i32)) {
                    return Some(0);
                }
                if !detached(hwnd) {
                    hide(hwnd);
                }
                Some(0)
            }
            WM_COMMAND if w & 0xffff == DETACH as usize => {
                set_detached(hwnd, !detached(hwnd));
                Some(0)
            }
            WM_COMMAND if w & 0xffff == CLOSE as usize => {
                hide(hwnd);
                set_detached(hwnd, false);
                KillTimer(hwnd, OBSERVE);
                Some(0)
            }
            WM_COMMAND if w & 0xffff == IDCANCEL as usize || w & 0xffff == 41 => {
                if dashboard::dismiss_editor(GetDlgItem(hwnd, dashboard::ID as i32)) {
                    return Some(0);
                }
                if !detached(hwnd) {
                    hide(hwnd);
                }
                Some(0)
            }
            WM_NCHITTEST if detached(hwnd) => {
                let mut point = POINT {
                    x: l as u16 as i16 as i32,
                    y: (l >> 16) as u16 as i16 as i32,
                };
                ScreenToClient(hwnd, &mut point);
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut r);
                let factor = (r.right as f64 / popup::WIDTH as f64)
                    .min(r.bottom as f64 / popup::HEIGHT as f64);
                if point.y >= 0
                    && point.y < (72. * factor) as i32
                    && point.x < (425. * factor) as i32
                {
                    Some(HTCAPTION as isize)
                } else {
                    None
                }
            }
            WM_SIZE => {
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut r);
                SetWindowPos(
                    GetDlgItem(hwnd, dashboard::ID as i32),
                    null_mut(),
                    0,
                    0,
                    r.right,
                    r.bottom,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                Some(0)
            }
            WM_DPICHANGED if detached(hwnd) => {
                let r = &*(l as *const RECT);
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOACTIVATE,
                );
                Some(0)
            }
            WM_DISPLAYCHANGE if detached(hwnd) => {
                clamp_to_workarea(hwnd);
                Some(0)
            }
            _ => None,
        }
    }
}
pub(super) unsafe fn clamp_to_workarea(hwnd: HWND) {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(hwnd, &mut r);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut info) != 0 {
            let width = (r.right - r.left).min(info.rcWork.right - info.rcWork.left);
            let height = (r.bottom - r.top).min(info.rcWork.bottom - info.rcWork.top);
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                r.left.clamp(info.rcWork.left, info.rcWork.right - width),
                r.top.clamp(info.rcWork.top, info.rcWork.bottom - height),
                width,
                height,
                SWP_NOACTIVATE,
            );
        }
    }
}
