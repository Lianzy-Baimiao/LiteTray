//! A passive, date-anchored card. Hover owns its delay and lifetime; the card
//! never activates, changes the selected date, or starts network requests.
use super::*;
use chrono::NaiveDate;
use design::{Theme, rect, text};
pub(super) const TIMER: usize = 93;
const DELAY: Duration = Duration::from_millis(400);
const CLASS: &str = "LiteTray.Almanac.1";

#[derive(Default)]
pub(super) struct Hover {
    owner: HWND,
    button: HWND,
    pending: Option<(NaiveDate, Instant)>,
    card: HWND,
}
impl Hover {
    #[cfg(test)]
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub unsafe fn begin(&mut self, owner: HWND, button: HWND, date: NaiveDate) {
        unsafe {
            if self.button == button && self.pending.is_some_and(|(d, _)| d == date) {
                return;
            }
            self.hide();
            self.owner = owner;
            self.button = button;
            self.pending = Some((date, Instant::now()));
            SetTimer(owner, TIMER, 50, None);
        }
    }
    pub unsafe fn leave(&mut self, button: HWND) {
        if self.button == button {
            unsafe {
                self.hide();
            }
        }
    }
    pub unsafe fn hide(&mut self) {
        unsafe {
            if !self.owner.is_null() {
                KillTimer(self.owner, TIMER);
            }
            if !self.card.is_null() {
                DestroyWindow(self.card);
            }
            self.card = null_mut();
            self.owner = null_mut();
            self.button = null_mut();
            self.pending = None;
        }
    }
    pub unsafe fn tick(&mut self, dark: bool) {
        unsafe {
            let Some((date, started)) = self.pending else {
                return;
            };
            let mut point: POINT = std::mem::zeroed();
            let cursor_available = GetCursorPos(&mut point) != 0;
            // Also handles hiding a settings page or ancestor window, moving
            // a detached flyout, and a missed leave while a menu is opening.
            if !cursor_available
                || IsWindowVisible(self.button) == 0
                || WindowFromPoint(point) != self.button
            {
                self.hide();
                return;
            }
            if self.card.is_null() && started.elapsed() >= DELAY {
                self.card = show(self.owner, self.button, date, dark);
            }
        }
    }
}
impl Drop for Hover {
    fn drop(&mut self) {
        unsafe {
            self.hide();
        }
    }
}

struct Row {
    value: String,
    bounds: RECT,
    font: usize,
    color: u32,
}
struct Card {
    rows: Vec<Row>,
    fonts: [HFONT; 3],
    width: i32,
    height: i32,
    dark: bool,
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
    unsafe fn new(date: NaiveDate, dark: bool, scale: f64) -> Self {
        unsafe {
            let s = |n: i32| (n as f64 * scale).round() as i32;
            let face = design::font_face();
            let fonts = [
                make_font(s(19), 600, &face),
                make_font(s(14), 400, &face),
                make_font(s(12), 400, &face),
            ];
            let t = Theme::new(dark);
            let mut card = Self {
                rows: vec![],
                fonts,
                width: s(360),
                height: 0,
                dark,
            };
            let dc = GetDC(null_mut());
            let mut y = s(16);
            let mut add = |label: Option<(&str, u32)>, value: String, font: usize, color: u32| {
                let left = s(if label.is_some() { 54 } else { 18 });
                let mut bounds = rect(left, y, s(342) - left, 0);
                if let Some(height) = typography::measure(&value, fonts[font], bounds.right - left) {
                    bounds.bottom = y + height;
                } else {
                    let old = SelectObject(dc, fonts[font]);
                    DrawTextW(
                        dc,
                        wide(&value).as_ptr(),
                        -1,
                        &mut bounds,
                        DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX,
                    );
                    SelectObject(dc, old);
                }
                bounds.bottom = bounds.bottom.max(y + s(20));
                card.rows.push(Row {
                    value,
                    bounds,
                    font,
                    color,
                });
                if let Some((label, color)) = label {
                    card.rows.push(Row {
                        value: label.into(),
                        bounds: rect(s(18), y, s(28), s(24)),
                        font: 1,
                        color,
                    });
                }
                y = bounds.bottom + s(10);
            };
            add(
                None,
                format!("{} · 黄历", date.format("%Y年%-m月%-d日")),
                0,
                t.text,
            );
            if let Some(day) = litetray::almanac::for_date(date) {
                add(None, day.lunar, 1, t.text);
                add(None, day.cycles, 1, t.muted);
                let good = if t.high_contrast {
                    t.text
                } else if dark {
                    rgb(115, 209, 163)
                } else {
                    rgb(20, 117, 73)
                };
                let bad = if t.high_contrast {
                    t.text
                } else if dark {
                    rgb(255, 154, 157)
                } else {
                    rgb(184, 46, 56)
                };
                add(Some(("宜", good)), day.suitable, 1, t.text);
                add(Some(("忌", bad)), day.avoid, 1, t.text);
                add(None, day.detail, 1, t.muted);
            } else {
                add(
                    None,
                    "黄历支持 1901–2100 年，此日期暂无数据。".into(),
                    1,
                    t.muted,
                );
            }
            add(None, "传统民俗参考 · Tyme".into(), 2, t.muted);
            card.height = y + s(6);
            ReleaseDC(null_mut(), dc);
            card
        }
    }
    fn accessible(&self) -> String {
        // Labels precede their content in the accessible window name.
        let mut result = Vec::new();
        let mut index = 0;
        while index < self.rows.len() {
            if self
                .rows
                .get(index + 1)
                .is_some_and(|r| r.value == "宜" || r.value == "忌")
            {
                result.push(format!(
                    "{}：{}",
                    self.rows[index + 1].value,
                    self.rows[index].value
                ));
                index += 2;
            } else {
                result.push(self.rows[index].value.clone());
                index += 1;
            }
        }
        result.join("\r\n")
    }
}

fn placement(anchor: RECT, work: RECT, width: i32, height: i32, gap: i32) -> RECT {
    let x = if anchor.right + gap + width <= work.right {
        anchor.right + gap
    } else {
        anchor.left - gap - width
    };
    let x = x.clamp(work.left, (work.right - width).max(work.left));
    let y = anchor
        .top
        .clamp(work.top, (work.bottom - height).max(work.top));
    rect(x, y, width, height)
}
unsafe fn show(owner: HWND, button: HWND, date: NaiveDate, dark: bool) -> HWND {
    unsafe {
        let mut anchor: RECT = std::mem::zeroed();
        GetWindowRect(button, &mut anchor);
        let mut monitor: MONITORINFO = std::mem::zeroed();
        monitor.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(
            MonitorFromRect(&anchor, MONITOR_DEFAULTTONEAREST),
            &mut monitor,
        ) == 0
        {
            return null_mut();
        }
        let mut scale = GetDpiForWindow(button).max(96) as f64 / 96.;
        scale = scale.min((monitor.rcWork.right - monitor.rcWork.left) as f64 / 380.);
        let mut card = Card::new(date, dark, scale);
        let available = (monitor.rcWork.bottom - monitor.rcWork.top - 16).max(1);
        if card.height > available {
            scale *= available as f64 / card.height as f64;
            card = Card::new(date, dark, scale);
        }
        let bounds = placement(
            anchor,
            monitor.rcWork,
            card.width,
            card.height,
            (10. * scale) as i32,
        );
        let class = wide(CLASS);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: GetModuleHandleW(null()),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TRANSPARENT,
            wide(CLASS).as_ptr(),
            wide(&card.accessible()).as_ptr(),
            WS_POPUP,
            bounds.left,
            bounds.top,
            card.width,
            card.height,
            GetAncestor(owner, GA_ROOT),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if hwnd.is_null() {
            return hwnd;
        }
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(Box::new(card)) as isize);
        let rounded = 2u32;
        DwmSetWindowAttribute(hwnd, 33, &rounded as *const _ as _, size_of::<u32>() as u32);
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            bounds.left,
            bounds.top,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        hwnd
    }
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Card;
        if msg == WM_NCDESTROY {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if !raw.is_null() {
                drop(Box::from_raw(raw));
            }
        }
        match msg {
            WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
            WM_NCHITTEST => HTTRANSPARENT as isize,
            WM_ERASEBKGND => 1,
            WM_PAINT | WM_PRINTCLIENT if !raw.is_null() => {
                let card = &*raw;
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let dc = if msg == WM_PAINT {
                    BeginPaint(hwnd, &mut ps)
                } else {
                    w as HDC
                };
                let t = Theme::new(card.dark);
                let bounds = rect(0, 0, card.width, card.height);
                let buffer = design::PaintBuffer::new(dc, bounds);
                let dc = buffer.as_ref().map_or(dc, |b| b.dc);
                design::fill(dc, bounds, t.surface);
                let border = CreateSolidBrush(t.border);
                FrameRect(dc, &bounds, border);
                DeleteObject(border);
                for row in &card.rows {
                    text(
                        dc,
                        &row.value,
                        row.bounds,
                        card.fonts[row.font],
                        row.color,
                        DT_LEFT | DT_WORDBREAK | DT_NOPREFIX,
                    );
                }
                drop(buffer);
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
    fn native_almanac_card_is_passive_readable_and_cleans_up() {
        super::super::visual_checks::on_isolated_desktop(|| unsafe {
            let _ = RoInitialize(RO_INIT_SINGLETHREADED);
            let parent = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("Almanac fixture").as_ptr(),
                WS_POPUP | WS_VISIBLE,
                100,
                100,
                700,
                660,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            let button = CreateWindowExW(
                0,
                wide("BUTTON").as_ptr(),
                wide("日期").as_ptr(),
                WS_CHILD | WS_VISIBLE,
                20,
                156,
                44,
                44,
                parent,
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            let before = GetForegroundWindow();
            for dark in [false, true] {
                let date = NaiveDate::from_ymd_opt(2026, 11, 7).unwrap();
                for scale in [1., 1.5, 2.] {
                    let card = Card::new(date, dark, scale);
                    assert!(card.rows.iter().all(|r| r.bounds.left >= 0
                        && r.bounds.right <= card.width
                        && r.bounds.bottom < card.height));
                }
                let mut hover = Hover::default();
                hover.begin(parent, button, date);
                assert!(
                    hover.card.is_null(),
                    "a passing pointer must not display the card immediately"
                );
                let started = hover.pending.unwrap().1;
                hover.begin(parent, button, date);
                assert_eq!(
                    hover.pending.unwrap().1,
                    started,
                    "movement within a date reset the delay"
                );
                let start = Instant::now();
                let tip = show(parent, button, date, dark);
                eprintln!(
                    "almanac cold card (dark={dark}): {}us",
                    start.elapsed().as_micros()
                );
                assert!(!tip.is_null());
                hover.card = tip;
                assert_ne!(IsWindowVisible(tip), 0);
                assert_eq!(GetForegroundWindow(), before, "hover card stole foreground");
                assert_eq!(
                    SendMessageW(tip, WM_MOUSEACTIVATE, 0, 0),
                    MA_NOACTIVATE as isize
                );
                assert_eq!(
                    SendMessageW(tip, WM_NCHITTEST, 0, 0),
                    HTTRANSPARENT as isize
                );
                let name = window_text(tip);
                assert!(
                    name.contains("2026年11月7日")
                        && name.contains("宜：")
                        && name.contains("忌：")
                );
                assert_eq!(
                    MonitorFromWindow(tip, MONITOR_DEFAULTTONEAREST),
                    MonitorFromWindow(button, MONITOR_DEFAULTTONEAREST)
                );
                if let Some(output) = std::env::var_os("LITETRAY_ALMANAC_RENDER_DIR") {
                    let output = std::path::PathBuf::from(output);
                    std::fs::create_dir_all(&output).unwrap();
                    super::super::visual_checks::write_window(
                        tip,
                        &output.join(format!(
                            "almanac-{}.bmp",
                            if dark { "dark" } else { "light" }
                        )),
                    );
                }
                hover.leave(null_mut());
                assert_ne!(
                    IsWindow(tip),
                    0,
                    "leave from an old date closed the current one"
                );
                hover.leave(button);
                assert_eq!(IsWindow(tip), 0);
                assert!(!hover.is_pending());
                hover.begin(parent, button, date);
                ShowWindow(parent, SW_HIDE);
                hover.tick(dark);
                assert!(!hover.is_pending(), "hidden ancestor left a pending hover");
                ShowWindow(parent, SW_SHOWNA);
            }
            DestroyWindow(parent);
        });
    }
    #[test]
    fn card_placement_stays_on_its_monitor_and_clear_of_date() {
        for work in [rect(-1920, -1080, 1920, 1040), rect(2560, 0, 1920, 1040)] {
            for x in [work.left + 8, work.right - 56] {
                for y in [work.top + 8, work.bottom - 56] {
                    let anchor = rect(x, y, 44, 44);
                    let tip = placement(anchor, work, 360, 310, 10);
                    assert!(tip.left >= work.left && tip.right <= work.right);
                    assert!(tip.top >= work.top && tip.bottom <= work.bottom);
                    assert!(tip.right <= anchor.left || tip.left >= anchor.right);
                }
            }
        }
    }
}
