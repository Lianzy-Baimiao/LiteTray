//! Passive feedback anchored to the clock which produced the audio action.
//! Independent of App borrows, so modal settings cannot lose this notification.
use super::*;
use litetray::platform::volume::{self, Feedback, Level};

const CLASS: &str = "LiteTray.Volume.1";
const HIDE: usize = 1;
const LIFETIME: Duration = Duration::from_millis(1400);
const WIDTH: i32 = 208;
const HEIGHT: i32 = 72;
thread_local! { static WINDOW: Cell<HWND> = const { Cell::new(null_mut()) }; }
struct Card {
    level: Option<Level>,
    dark: bool,
    scale: f64,
    fonts: [HFONT; 2],
    until: Instant,
}
impl Drop for Card {
    fn drop(&mut self) {
        unsafe {
            for font in self.fonts {
                DeleteObject(font);
            }
        }
    }
}
impl Card {
    unsafe fn new(level: Option<Level>, dark: bool, scale: f64) -> Self {
        unsafe {
            let face = design::font_face();
            Self {
                level,
                dark,
                scale,
                fonts: [
                    make_font((24. * scale).round() as i32, 600, &face),
                    make_font((12. * scale).round() as i32, 400, &face),
                ],
                until: Instant::now() + LIFETIME,
            }
        }
    }
    fn label(&self) -> String {
        match self.level {
            Some(level) => format!(
                "系统音量 {}%{}",
                level.percent,
                if level.muted { " · 已静音" } else { "" }
            ),
            None => "音频设备不可用".into(),
        }
    }
}
pub(super) unsafe fn message(owner: HWND, msg: u32) -> Option<LRESULT> {
    unsafe {
        if msg == volume::WM_FEEDBACK {
            if let Some(feedback) = volume::take_feedback(owner as usize) {
                show(
                    owner,
                    feedback,
                    PALETTE.with(Cell::get).map_or_else(system_dark, |p| p.dark),
                );
            }
            return Some(0);
        }
        if msg == WM_DESTROY || msg == WM_DISPLAYCHANGE {
            let hwnd = WINDOW.with(|w| w.replace(null_mut()));
            if IsWindow(hwnd) != 0 {
                DestroyWindow(hwnd);
            }
        }
        None
    }
}
unsafe fn show(owner: HWND, feedback: Feedback, dark: bool) -> HWND {
    unsafe {
        if feedback.created.elapsed() >= Duration::from_secs(2) || IsWindow(owner) == 0 {
            return null_mut();
        }
        let a = feedback.anchor;
        let anchor = RECT {
            left: a.left,
            top: a.top,
            right: a.right,
            bottom: a.bottom,
        };
        let monitor = MonitorFromRect(&anchor, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return null_mut();
        }
        let (mut dpi, mut dpi_y) = (96, 96);
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut dpi_y) < 0 {
            dpi = GetDpiForWindow(owner).max(96);
        }
        let w = info.rcWork;
        let Ok(placement) = popup::place_sized(
            a,
            popup::Rect {
                left: w.left,
                top: w.top,
                right: w.right,
                bottom: w.bottom,
            },
            dpi,
            WIDTH,
            HEIGHT,
        ) else {
            return null_mut();
        };
        let scale = dpi as f64 / 96. * placement.scale_percent as f64 / 100.;
        let bounds = placement.rect;
        let mut hwnd = WINDOW.with(Cell::get);
        if IsWindow(hwnd) == 0 {
            let class = wide(CLASS);
            let wc = WNDCLASSW {
                lpfnWndProc: Some(proc),
                hInstance: GetModuleHandleW(null()),
                lpszClassName: class.as_ptr(),
                ..std::mem::zeroed()
            };
            RegisterClassW(&wc);
            hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                class.as_ptr(),
                wide("").as_ptr(),
                WS_POPUP,
                bounds.left,
                bounds.top,
                bounds.right - bounds.left,
                bounds.bottom - bounds.top,
                owner,
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            if hwnd.is_null() {
                return hwnd;
            }
            WINDOW.with(|w| w.set(hwnd));
            let rounded = 2u32;
            DwmSetWindowAttribute(hwnd, 33, &rounded as *const _ as _, size_of::<u32>() as u32);
        }
        let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Card;
        if let Some(card) = raw.as_mut() {
            if card.scale != scale {
                *card = Card::new(feedback.level, dark, scale);
            }
            card.level = feedback.level;
            card.dark = dark;
            card.until = Instant::now() + LIFETIME;
        } else {
            SetWindowLongPtrW(
                hwnd,
                GWLP_USERDATA,
                Box::into_raw(Box::new(Card::new(feedback.level, dark, scale))) as isize,
            );
        }
        let card = &*(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Card);
        SetWindowTextW(hwnd, wide(&card.label()).as_ptr());
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            bounds.left,
            bounds.top,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        InvalidateRect(hwnd, null(), 0);
        // Replacing this timer extends the same surface during fast scrolling.
        if SetTimer(hwnd, HIDE, LIFETIME.as_millis() as u32, None) == 0 {
            ShowWindow(hwnd, SW_HIDE);
        }
        hwnd
    }
}
unsafe fn paint(hwnd: HWND, dc: HDC, card: &Card) {
    unsafe {
        let mut client: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut client);
        let buffer = design::PaintBuffer::new(dc, client);
        let dc = buffer.as_ref().map_or(dc, |b| b.dc);
        let t = design::Theme::new(card.dark);
        let s = |n: i32| (n as f64 * card.scale).round() as i32;
        let r = |x, y, w, h| design::rect(s(x), s(y), s(w), s(h));
        design::fill(dc, client, t.surface);
        let border = CreateSolidBrush(t.border);
        FrameRect(dc, &client, border);
        DeleteObject(border);
        let muted = card.level.is_none_or(|l| l.muted);
        icons::draw(
            dc,
            if muted {
                icons::Icon::Muted
            } else {
                icons::Icon::Volume
            },
            r(16, 16, 24, 24),
            t.text,
            t.surface,
        );
        if let Some(level) = card.level {
            design::text(
                dc,
                &format!("{}%", level.percent),
                r(52, 10, 82, 34),
                card.fonts[0],
                t.text,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
            if level.muted {
                design::text(
                    dc,
                    "已静音",
                    r(140, 16, 52, 24),
                    card.fonts[1],
                    t.muted,
                    DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
                );
            }
            let track = r(16, 54, 176, 4);
            design::rounded(dc, track, t.border, t.border, s(4));
            if level.percent > 0 {
                let fill = RECT {
                    right: track.left
                        + (track.right - track.left) * level.percent.min(100) as i32 / 100,
                    ..track
                };
                design::rounded(
                    dc,
                    fill,
                    if muted { t.muted } else { t.accent },
                    if muted { t.muted } else { t.accent },
                    s(4),
                );
            }
        } else {
            design::text(
                dc,
                "音频设备不可用",
                r(52, 10, 140, 38),
                card.fonts[1],
                t.text,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
        }
    }
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Card;
        if msg == WM_NCDESTROY {
            KillTimer(hwnd, HIDE);
            if WINDOW.with(Cell::get) == hwnd {
                WINDOW.with(|w| w.set(null_mut()));
            }
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if !raw.is_null() {
                drop(Box::from_raw(raw));
            }
            return DefWindowProcW(hwnd, msg, w, l);
        }
        match msg {
            WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
            WM_NCHITTEST => HTTRANSPARENT as isize,
            WM_ERASEBKGND => 1,
            WM_TIMER if w == HIDE => {
                if raw.as_ref().is_none_or(|card| Instant::now() >= card.until) {
                    KillTimer(hwnd, HIDE);
                    ShowWindow(hwnd, SW_HIDE);
                }
                0
            }
            WM_PAINT | WM_PRINTCLIENT if !raw.is_null() => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let dc = if msg == WM_PAINT {
                    BeginPaint(hwnd, &mut ps)
                } else {
                    w as HDC
                };
                paint(hwnd, dc, &*raw);
                if msg == WM_PAINT {
                    EndPaint(hwnd, &ps);
                }
                0
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn volume_feedback_reuses_passive_window_tracks_monitor_and_expires() {
        super::super::visual_checks::on_isolated_desktop(|| unsafe {
            let _ = RoInitialize(RO_INIT_SINGLETHREADED);
            let owner = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("Volume feedback fixture").as_ptr(),
                WS_POPUP,
                0,
                0,
                100,
                100,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            let foreground = GetForegroundWindow();
            let displays = displays::collect();
            assert!(!displays.displays.is_empty());
            let mut reused: HWND = null_mut();
            for display in &displays.displays {
                let work = display.work;
                let anchor = popup::Rect {
                    left: work[2] - 160,
                    top: work[3],
                    right: work[2],
                    bottom: work[3] + 40,
                };
                for dark in [false, true] {
                    for level in [
                        Some(Level {
                            percent: 0,
                            muted: false,
                        }),
                        Some(Level {
                            percent: 24,
                            muted: false,
                        }),
                        Some(Level {
                            percent: 100,
                            muted: true,
                        }),
                        None,
                    ] {
                        let hwnd = show(
                            owner,
                            Feedback {
                                anchor,
                                level,
                                created: Instant::now(),
                            },
                            dark,
                        );
                        assert!(!hwnd.is_null());
                        if !reused.is_null() {
                            assert_eq!(hwnd, reused, "scrolling must update the same window");
                        }
                        reused = hwnd;
                        assert_ne!(IsWindowVisible(hwnd), 0);
                        assert_eq!(GetForegroundWindow(), foreground);
                        assert_eq!(IsWindowVisible(owner), 0, "feedback must not open settings");
                        assert_eq!(
                            SendMessageW(hwnd, WM_MOUSEACTIVATE, 0, 0),
                            MA_NOACTIVATE as isize
                        );
                        assert_eq!(
                            SendMessageW(hwnd, WM_NCHITTEST, 0, 0),
                            HTTRANSPARENT as isize
                        );
                        assert_ne!(GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST, 0);
                        let mut bounds: RECT = std::mem::zeroed();
                        GetWindowRect(hwnd, &mut bounds);
                        assert!(
                            bounds.left >= work[0]
                                && bounds.right <= work[2]
                                && bounds.top >= work[1]
                                && bounds.bottom <= work[3]
                        );
                        let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Card;
                        assert_eq!((*raw).level, level);
                        assert!(level.is_none_or(|value| {
                            (*raw).label().contains(&format!("{}%", value.percent))
                        }));
                        if let Some(dir) = std::env::var_os("LITETRAY_VOLUME_TIP_RENDER_DIR") {
                            let dir = std::path::PathBuf::from(dir);
                            std::fs::create_dir_all(&dir).unwrap();
                            super::super::visual_checks::write_window(
                                hwnd,
                                &dir.join(format!(
                                    "volume-{}-{}-{}.bmp",
                                    if dark { "dark" } else { "light" },
                                    level.map_or(999, |l| l.percent),
                                    display.primary
                                )),
                            );
                        }
                        // A stale timer posted before the latest scroll must not
                        // close the refreshed value. The deadline owns expiry.
                        SendMessageW(hwnd, WM_TIMER, HIDE, 0);
                        assert_ne!(IsWindowVisible(hwnd), 0);
                        (*raw).until = Instant::now() - Duration::from_millis(1);
                        SendMessageW(hwnd, WM_TIMER, HIDE, 0);
                        assert_eq!(IsWindowVisible(hwnd), 0);
                    }
                }
            }
            DestroyWindow(owner);
            assert_eq!(IsWindow(reused), 0);
            assert!(WINDOW.with(Cell::get).is_null());
            RoUninitialize();
        });
    }
}
