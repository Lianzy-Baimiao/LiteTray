//! All main-window entry points share one activation path. Keep the request
//! outside App's RefCell so a modal/reentrant borrow cannot consume the click.
use super::*;
const RETRY: usize = 74;
pub(super) const OPEN_FOCUS: u32 = WM_APP + 88;
#[derive(Clone, Copy)]
enum Target {
    Current,
    Date(i32),
    Entry(u64),
    Alarms,
    Focus,
}
#[derive(Clone, Copy)]
struct Request {
    window: HWND,
    source: HWND,
    started: Instant,
    target: Target,
}
thread_local! {static PENDING:Cell<Option<Request>>=const {Cell::new(None)};}
pub(super) unsafe fn message(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> Option<LRESULT> {
    unsafe {
        let target = match msg {
            WM_SHOW_SETTINGS => Some(Target::Current),
            WM_COMMAND if w & 0xffff == 40 && w >> 16 == BN_CLICKED as usize => {
                Some(Target::Current)
            }
            agenda::OPEN_DATE => Some(Target::Date(w as i32)),
            agenda::OPEN_ENTRY => Some(Target::Entry(w as u64)),
            agenda::OPEN_ALARMS => Some(Target::Alarms),
            OPEN_FOCUS => Some(Target::Focus),
            WM_TRAY if l as u32 == NIN_BALLOONUSERCLICK => Some(Target::Focus),
            _ => None,
        };
        if msg == WM_NCDESTROY || msg == WM_CLOSE {
            if PENDING.with(|p| p.get().is_some_and(|r| r.window == hwnd)) {
                PENDING.with(|p| p.set(None));
            }
            KillTimer(hwnd, RETRY);
            return None;
        }
        if target.is_none() && !(msg == WM_TIMER && w == RETRY) {
            return None;
        }
        if let Some(target) = target {
            // WM_COMMAND carries the clicked native button in lParam. The
            // single-instance WM_SHOW_SETTINGS has no source popup.
            let source = owned_source(hwnd, l as HWND);
            PENDING.with(|p| {
                p.set(Some(Request {
                    window: hwnd,
                    source,
                    started: Instant::now(),
                    target,
                }))
            });
            increment(hwnd, "LiteTray.SettingsOpen.Requested");
        }
        let Some(pending) = PENDING.with(Cell::get).filter(|r| r.window == hwnd) else {
            KillTimer(hwnd, RETRY);
            return Some(0);
        };
        let ready = STATE.with(|state| {
            let Ok(mut state) = state.try_borrow_mut() else {
                return None;
            };
            let app = state.as_mut()?;
            if app.hwnd != hwnd || IsWindowEnabled(hwnd) == 0 {
                return None;
            }
            // Preserve settings drafts before navigating; invalid text stays
            // in its existing controls and the visible window shows the error.
            if !matches!(pending.target, Target::Current) {
                if let Err(error) = app.collect_page() {
                    app.set_status(error);
                } else {
                    app.page = if matches!(pending.target, Target::Focus) {
                        9
                    } else {
                        8
                    };
                    app.render();
                    match pending.target {
                        Target::Date(days) => {
                            if let Some(date) = chrono::NaiveDate::from_num_days_from_ce_opt(days) {
                                app.agenda_scope = Some(date);
                                agenda::open_date(app.field(agenda::ID), date);
                            }
                        }
                        Target::Entry(id) => agenda::open_entry(app.field(agenda::ID), id),
                        Target::Alarms => agenda::open_alarms(app.field(agenda::ID)),
                        _ => {}
                    }
                }
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
