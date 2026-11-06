//! Settings activation survives nested native message loops. Keep the request
//! outside App's RefCell so a modal/reentrant borrow cannot consume the click.
use super::*;
const RETRY: usize = 74;
#[derive(Clone, Copy)]
struct Request {
    window: HWND,
    source: HWND,
    started: Instant,
}
thread_local! {static PENDING:Cell<Option<Request>>=const {Cell::new(None)};}
pub(super) unsafe fn message(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> Option<LRESULT> {
    unsafe {
        let request = msg == WM_SHOW_SETTINGS
            || msg == WM_COMMAND && w & 0xffff == 40 && w >> 16 == BN_CLICKED as usize;
        if msg == WM_NCDESTROY || msg == WM_CLOSE {
            if PENDING.with(|p| p.get().is_some_and(|r| r.window == hwnd)) {
                PENDING.with(|p| p.set(None));
            }
            KillTimer(hwnd, RETRY);
            return None;
        }
        if !request && !(msg == WM_TIMER && w == RETRY) {
            return None;
        }
        if request {
            // WM_COMMAND carries the clicked native button in lParam. The
            // single-instance WM_SHOW_SETTINGS has no source popup.
            let source = owned_source(hwnd, l as HWND);
            PENDING.with(|p| {
                p.set(Some(Request {
                    window: hwnd,
                    source,
                    started: Instant::now(),
                }))
            });
            increment(hwnd, "LiteTray.SettingsOpen.Requested");
        }
        let Some(pending) = PENDING.with(Cell::get).filter(|r| r.window == hwnd) else {
            KillTimer(hwnd, RETRY);
            return Some(0);
        };
        let ready = STATE.with(|state| {
            let Ok(state) = state.try_borrow() else {
                return None;
            };
            let app = state.as_ref()?;
            if app.hwnd != hwnd || IsWindowEnabled(hwnd) == 0 {
                return None;
            }
            app.schedule_tick();
            Some(if pending.source.is_null() {
                owned_source(hwnd, app.panel)
            } else {
                pending.source
            })
        });
        let Some(source) = ready else {
            SetTimer(hwnd, RETRY, 16, None);
            return Some(0);
        };
        PENDING.with(|p| p.set(None));
        KillTimer(hwnd, RETRY);
        // ShowWindow/restore can synchronously send WM_SIZE and WM_ACTIVATE.
        // No App borrow is held here, so native callbacks can finish layout.
        let foreground = GetForegroundWindow();
        let minimized = IsIconic(hwnd) != 0;
        ShowWindow(hwnd, if minimized { SW_RESTORE } else { SW_SHOW });
        // On a process's first ShowWindow, Windows can replace our command
        // with STARTUPINFO.wShowWindow (e.g. Start-Process -WindowStyle Hidden).
        // An explicit user open must still show a usable window on that click.
        let show_retried = IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0;
        if show_retried && IsWindow(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        let activated =
            IsWindowVisible(hwnd) != 0 && IsIconic(hwnd) == 0 && SetForegroundWindow(hwnd) != 0;
        if IsWindowVisible(hwnd) != 0 && IsIconic(hwnd) == 0 {
            increment(hwnd, "LiteTray.SettingsOpen.Completed");
            // Retain the source's foreground entitlement through activation.
            // A detached popup has an independent lifetime and stays open.
            if !source.is_null() && IsWindow(source) != 0 && !panel::detached(source) {
                panel::hide(source);
            }
        }
        // Small inspectable diagnostics, without disk I/O on the click path.
        property(hwnd, "LiteTray.SettingsOpen.Source", source as usize);
        property(
            hwnd,
            "LiteTray.SettingsOpen.ForegroundBefore",
            foreground as usize,
        );
        property(
            hwnd,
            "LiteTray.SettingsOpen.ForegroundAfter",
            GetForegroundWindow() as usize,
        );
        property(hwnd, "LiteTray.SettingsOpen.Activated", activated as usize);
        property(hwnd, "LiteTray.SettingsOpen.Restored", minimized as usize);
        property(
            hwnd,
            "LiteTray.SettingsOpen.ShowRetried",
            show_retried as usize,
        );
        property(
            hwnd,
            "LiteTray.SettingsOpen.Micros",
            pending.started.elapsed().as_micros().max(1) as usize,
        );
        Some(0)
    }
}
unsafe fn owned_source(main: HWND, child: HWND) -> HWND {
    unsafe {
        if child.is_null() || IsWindow(child) == 0 {
            return null_mut();
        }
        let root = GetAncestor(child, GA_ROOT);
        let mut process = 0;
        GetWindowThreadProcessId(root, &mut process);
        if root != main && process == GetCurrentProcessId() && GetWindow(root, GW_OWNER) == main {
            root
        } else {
            null_mut()
        }
    }
}
unsafe fn property(hwnd: HWND, key: &str, value: usize) {
    unsafe {
        SetPropW(hwnd, wide(key).as_ptr(), value as HANDLE);
    }
}
unsafe fn increment(hwnd: HWND, key: &str) {
    unsafe {
        property(
            hwnd,
            key,
            (GetPropW(hwnd, wide(key).as_ptr()) as usize).wrapping_add(1),
        );
    }
}
