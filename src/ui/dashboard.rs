//! Calendar/weather view shared by the home page and taskbar flyout. Date
//! cells are real named buttons; arrow keys retain a single calendar tab stop.
use super::*;
use chrono::{Datelike, NaiveDate};
use design::{Theme, fill, rect, rounded, text};
const CLASS: &str = "LiteTray.Dashboard.2";
pub(super) const ID: u16 = 950;
const FIRST: u16 = 1000;
const PREV: u16 = 1100;
const NEXT: u16 = 1101;
const TODAY: u16 = 1102;
const SELECT_DAY: u32 = WM_APP + 80;
struct View {
    calendar: calendar::Calendar,
    days: Vec<NaiveDate>,
    buttons: Vec<HWND>,
    weather: Option<weather::WeatherResult>,
    city: String,
    enabled: bool,
    busy: bool,
    lunar: bool,
    seconds: bool,
    flyout: bool,
    dark: bool,
    factor: f64,
    fonts: [HFONT; 5],
}
impl Drop for View {
    fn drop(&mut self) {
        unsafe {
            for font in self.fonts {
                if !font.is_null() {
                    DeleteObject(font);
                }
            }
        }
    }
}
pub(super) unsafe fn create(parent: HWND, bounds: RECT, dark: bool, flyout: bool) -> HWND {
    unsafe {
        let class = wide(CLASS);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: GetModuleHandleW(null()),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            class.as_ptr(),
            wide("日历与天气").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_TABSTOP,
            bounds.left,
            bounds.top,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            parent,
            ID as usize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        if hwnd.is_null() {
            return hwnd;
        }
        let today = Local::now().date_naive();
        let view = Box::new(View {
            calendar: calendar::Calendar::new(today),
            days: vec![],
            buttons: vec![],
            weather: None,
            city: String::new(),
            enabled: true,
            busy: false,
            lunar: true,
            seconds: true,
            flyout,
            dark,
            factor: 1.,
            fonts: [null_mut(); 5],
        });
        SetWindowLongPtrW(
            hwnd,
            GWLP_USERDATA,
            Box::into_raw(Box::new(RefCell::new(*view))) as isize,
        );
        layout(hwnd);
        hwnd
    }
}
pub(super) unsafe fn selection(hwnd: HWND) -> Option<calendar::Calendar> {
    unsafe {
        (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>)
            .as_ref()?
            .try_borrow()
            .ok()
            .map(|v| v.calendar)
    }
}
pub(super) unsafe fn restore_selection(hwnd: HWND, calendar: calendar::Calendar) {
    unsafe {
        let Some(cell) = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>).as_ref()
        else {
            return;
        };
        if let Ok(mut v) = cell.try_borrow_mut() {
            v.calendar = calendar;
        }
        layout(hwnd);
    }
}
pub(super) unsafe fn update(
    hwnd: HWND,
    config: &Config,
    weather: &Option<weather::WeatherResult>,
    busy: bool,
) {
    unsafe {
        let Some(cell) = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>).as_ref()
        else {
            return;
        };
        let Ok(mut view) = cell.try_borrow_mut() else {
            return;
        };
        view.city = config.weather.city.clone();
        view.enabled = config.weather.enabled;
        view.busy = busy;
        view.weather = weather.clone();
        view.lunar = config.clock.show_lunar;
        view.seconds = config.clock.show_seconds;
        let accessible = format!(
            "日历与天气。{}。{}",
            view.city,
            view.weather
                .as_ref()
                .map(|r| format!(
                    "{}。{}",
                    r.weather.as_ref().map(|w| w.summary()).unwrap_or_default(),
                    r.status()
                ))
                .unwrap_or_else(|| if view.enabled {
                    "天气暂无数据"
                } else {
                    "天气已关闭"
                }
                .into())
        );
        set_text(hwnd, &accessible);
        InvalidateRect(hwnd, null(), 0);
    }
}
unsafe fn layout(hwnd: HWND) {
    unsafe {
        let Some(cell) = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>).as_ref()
        else {
            return;
        };
        let Ok(mut guard) = cell.try_borrow_mut() else {
            return;
        };
        let v = &mut *guard;
        for b in v.buttons.drain(..) {
            DestroyWindow(b);
        }
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut r);
        v.factor = (r.right as f64 / 760.).min(r.bottom as f64 / 520.);
        let s = |n: i32| (n as f64 * v.factor).round() as i32;
        for font in v.fonts {
            if !font.is_null() {
                DeleteObject(font);
            }
        }
        v.fonts = [
            make_font(s(14), 400, "Microsoft YaHei UI"),
            make_font(s(12), 400, "Microsoft YaHei UI"),
            make_font(s(19), 600, "Microsoft YaHei UI"),
            make_font(s(52), 400, "Segoe UI"),
            make_font(s(20), 400, "Segoe MDL2 Assets"),
        ];
        v.days = v.calendar.grid();
        let mut button = |name: &str, id: u16, x, y, w, h, tab| {
            let b = CreateWindowExW(
                0,
                wide("BUTTON").as_ptr(),
                wide(name).as_ptr(),
                WS_CHILD | WS_VISIBLE | BS_OWNERDRAW as u32 | if tab { WS_TABSTOP } else { 0 },
                s(x),
                s(y),
                s(w),
                s(h),
                hwnd,
                id as usize as HMENU,
                GetModuleHandleW(null()),
                null(),
            );
            SendMessageW(b, WM_SETFONT, v.fonts[0] as usize, 0);
            SetWindowSubclass(b, Some(button_proc), 1, 0);
            SetWindowSubclass(b, Some(design::hover_proc), 2, 0);
            v.buttons.push(b);
        };
        button("上个月", PREV, 252, 139, 32, 32, true);
        button("今天", TODAY, 288, 139, 48, 32, true);
        button("下个月", NEXT, 340, 139, 32, 32, true);
        for (i, date) in v.days.iter().enumerate() {
            let x = 26 + (i % 7) as i32 * 50;
            let y = 210 + (i / 7) as i32 * 36;
            button(
                &format!(
                    "{}{}",
                    date.format("%Y年%m月%d日"),
                    if *date == Local::now().date_naive() {
                        "，今天"
                    } else {
                        ""
                    }
                ),
                FIRST + i as u16,
                x,
                y,
                44,
                32,
                *date == v.calendar.selected,
            );
        }
        button("刷新天气", REFRESH, 424, 438, 132, 36, true);
        button(
            if v.flyout { "设置" } else { "展开面板" },
            if v.flyout { 40 } else { CALENDAR },
            642,
            30,
            102,
            36,
            true,
        );
        InvalidateRect(hwnd, null(), 0);
    }
}
unsafe extern "system" fn button_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    unsafe {
        let id = GetDlgCtrlID(hwnd) as u16;
        if (FIRST..FIRST + 42).contains(&id) && msg == WM_GETDLGCODE {
            return DefSubclassProc(hwnd, msg, w, l) | DLGC_WANTARROWS as isize;
        }
        if (FIRST..FIRST + 42).contains(&id) && msg == WM_KEYDOWN {
            let delta = match w as u16 {
                VK_LEFT => -1,
                VK_RIGHT => 1,
                VK_UP => -7,
                VK_DOWN => 7,
                _ => 0,
            };
            if delta != 0 {
                PostMessageW(GetParent(hwnd), SELECT_DAY, delta as usize, 0);
                return 0;
            }
        }
        if msg == WM_KEYDOWN && w == VK_ESCAPE as usize {
            PostMessageW(GetAncestor(hwnd, GA_ROOT), WM_CLOSE, 0, 0);
            return 0;
        }
        if msg == WM_NCDESTROY {
            RemoveWindowSubclass(hwnd, Some(button_proc), 1);
        }
        DefSubclassProc(hwnd, msg, w, l)
    }
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCDESTROY {
            let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RefCell<View>;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if !raw.is_null() {
                drop(Box::from_raw(raw));
            }
            return DefWindowProcW(hwnd, msg, w, l);
        }
        let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RefCell<View>;
        if raw.is_null() {
            return DefWindowProcW(hwnd, msg, w, l);
        }
        match msg {
            WM_SETFOCUS => {
                let Ok(v) = (*raw).try_borrow() else {
                    return 0;
                };
                if let Some(i) = v.days.iter().position(|d| *d == v.calendar.selected) {
                    SetFocus(GetDlgItem(hwnd, (FIRST + i as u16) as i32));
                }
                0
            }
            WM_SIZE => {
                layout(hwnd);
                0
            }
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                if let Ok(v) = (*raw).try_borrow() {
                    paint(hwnd, &v);
                } else {
                    ValidateRect(hwnd, null());
                }
                0
            }
            WM_DRAWITEM if l != 0 => {
                if let Ok(v) = (*raw).try_borrow() {
                    draw_button(&v, &*(l as *const DRAWITEMSTRUCT));
                }
                1
            }
            WM_COMMAND | SELECT_DAY => {
                let Ok(mut guard) = (*raw).try_borrow_mut() else {
                    return 0;
                };
                let v = &mut *guard;
                let id = (w & 0xffff) as u16;
                let mut focus = None;
                if msg == SELECT_DAY {
                    let delta = w as isize as i64;
                    if !matches!(delta, -7 | -1 | 1 | 7) {
                        return 0;
                    }
                    v.calendar.move_days(delta);
                    focus = Some(v.calendar.selected);
                } else if (FIRST..FIRST + 42).contains(&id) {
                    v.calendar.select(v.days[(id - FIRST) as usize]);
                    focus = Some(v.calendar.selected);
                } else if id == PREV || id == NEXT {
                    v.calendar.move_month(id == NEXT);
                } else if id == TODAY {
                    v.calendar.select(Local::now().date_naive());
                    focus = Some(v.calendar.selected);
                } else {
                    PostMessageW(GetParent(hwnd), WM_COMMAND, w, l);
                    return 0;
                }
                drop(guard);
                layout(hwnd);
                if let Some(date) = focus {
                    let Ok(v) = (*raw).try_borrow() else {
                        return 0;
                    };
                    if let Some(i) = v.days.iter().position(|d| *d == date) {
                        SetFocus(GetDlgItem(hwnd, (FIRST + i as u16) as i32));
                    }
                } else {
                    SetFocus(GetDlgItem(hwnd, id as i32));
                }
                0
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }
}
unsafe fn draw_button(v: &View, item: &DRAWITEMSTRUCT) {
    unsafe {
        let t = Theme::new(v.dark);
        let s = |n: i32| (n as f64 * v.factor).round() as i32;
        let id = item.CtlID as u16;
        let focused = item.itemState & ODS_FOCUS != 0;
        let pressed = item.itemState & ODS_SELECTED != 0;
        let hover = !GetPropW(item.hwndItem, wide("LiteTray.Hover").as_ptr()).is_null();
        let day = (FIRST..FIRST + 42).contains(&id);
        let date = day.then(|| v.days[(id - FIRST) as usize]);
        let selected = date == Some(v.calendar.selected);
        fill(
            item.hDC,
            item.rcItem,
            if day || matches!(id, PREV | NEXT | TODAY | REFRESH) {
                t.surface
            } else {
                t.bg
            },
        );
        let color = if selected {
            t.accent
        } else if pressed || hover {
            t.tint
        } else if id == 40 || id == CALENDAR {
            t.field
        } else {
            t.surface
        };
        let border = if focused || date == Some(Local::now().date_naive()) {
            t.accent
        } else {
            color
        };
        rounded(
            item.hDC,
            item.rcItem,
            color,
            border,
            s(if day { 32 } else { 10 }),
        );
        let label = if let Some(date) = date {
            date.day().to_string()
        } else {
            match id {
                PREV => "‹".into(),
                NEXT => "›".into(),
                _ => window_text(item.hwndItem),
            }
        };
        let color = if selected {
            t.on_accent
        } else if date.is_some_and(|d| d.month() != v.calendar.month.month()) {
            t.muted
        } else if !day {
            t.accent
        } else {
            t.text
        };
        text(
            item.hDC,
            &label,
            item.rcItem,
            v.fonts[0],
            color,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    }
}
unsafe fn paint(hwnd: HWND, v: &View) {
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let dc = BeginPaint(hwnd, &mut ps);
        let mut client: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut client);
        let mem = CreateCompatibleDC(dc);
        let bitmap = CreateCompatibleBitmap(dc, client.right.max(1), client.bottom.max(1));
        let old = SelectObject(mem, bitmap);
        let t = Theme::new(v.dark);
        let s = |n: i32| (n as f64 * v.factor).round() as i32;
        let r = |x, y, w, h| rect(s(x), s(y), s(w), s(h));
        fill(mem, client, t.bg);
        let tx = |value: &str, x, y, w, h, font, color| {
            text(
                mem,
                value,
                r(x, y, w, h),
                v.fonts[font],
                color,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            )
        };
        let now = Local::now();
        tx(
            &now.format(if v.seconds { "%H:%M:%S" } else { "%H:%M" })
                .to_string(),
            4,
            6,
            340,
            66,
            3,
            t.text,
        );
        tx(
            &format!(
                "{}  {}",
                now.format("%Y年%m月%d日"),
                [
                    "星期一",
                    "星期二",
                    "星期三",
                    "星期四",
                    "星期五",
                    "星期六",
                    "星期日"
                ][now.weekday().num_days_from_monday() as usize]
            ),
            8,
            78,
            560,
            26,
            0,
            t.muted,
        );
        rounded(mem, r(0, 122, 394, 364), t.surface, t.border, s(18));
        rounded(mem, r(410, 122, 350, 364), t.surface, t.border, s(18));
        tx(
            &v.calendar.month.format("%Y年 %-m月").to_string(),
            26,
            140,
            210,
            30,
            2,
            t.text,
        );
        for (i, name) in ["一", "二", "三", "四", "五", "六", "日"]
            .iter()
            .enumerate()
        {
            text(
                mem,
                name,
                r(26 + i as i32 * 50, 179, 44, 24),
                v.fonts[1],
                t.muted,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        }
        let lunar = if v.lunar {
            Local
                .from_local_datetime(&v.calendar.selected.and_hms_opt(12, 0, 0).unwrap())
                .single()
                .and_then(lunar::describe)
                .unwrap_or_default()
        } else {
            String::new()
        };
        tx(
            &format!("{}  {}", v.calendar.selected.format("%-m月%-d日"), lunar),
            26,
            442,
            345,
            26,
            1,
            t.muted,
        );
        tx(&v.city, 434, 144, 232, 28, 2, t.text);
        if v.enabled
            && let Some(w) = v.weather.as_ref().and_then(|r| r.weather.as_ref())
        {
            tx(
                &format!("{:.0}°", w.temperature_c),
                432,
                181,
                180,
                72,
                3,
                t.text,
            );
            tx("°C", 681, 199, 48, 44, 2, t.accent);
            tx(&w.description, 436, 253, 280, 26, 0, t.text);
            tx(
                &format!(
                    "体感 {}  ·  湿度 {}",
                    w.feels_like_c
                        .map(|n| format!("{n:.0}°"))
                        .unwrap_or_else(|| "—".into()),
                    w.humidity_percent
                        .map(|n| format!("{n:.0}%"))
                        .unwrap_or_else(|| "—".into())
                ),
                436,
                282,
                300,
                24,
                1,
                t.muted,
            );
            tx(
                &format!(
                    "风速 {}",
                    w.wind_kmh
                        .map(|n| format!("{n:.0} km/h"))
                        .unwrap_or_else(|| "—".into())
                ),
                436,
                307,
                290,
                20,
                1,
                t.muted,
            );
            fill(mem, r(434, 335, 302, 1), t.border);
            if w.days.is_empty() {
                tx("此来源暂无每日预报", 436, 340, 290, 28, 1, t.muted);
            }
            for (i, day) in w.days.iter().take(3).enumerate() {
                let y = 343 + i as i32 * 28;
                tx(
                    day.date.get(5..).unwrap_or(&day.date),
                    436,
                    y,
                    64,
                    24,
                    1,
                    t.muted,
                );
                tx(&day.description, 508, y, 120, 24, 1, t.text);
                tx(
                    &format!("{:.0}° / {:.0}°", day.low_c, day.high_c),
                    642,
                    y,
                    96,
                    24,
                    1,
                    t.text,
                );
            }
            let status = if v.weather.as_ref().is_some_and(|r| r.cached) {
                "离线缓存 · 非实时".into()
            } else if v.weather.as_ref().is_some_and(|r| r.fallback) {
                format!("备用 · {}", w.provider.label())
            } else {
                format!(
                    "{} · {}",
                    w.provider.label(),
                    w.data_at.with_timezone(&Local).format("%H:%M")
                )
            };
            tx(&status, 568, 441, 169, 30, 1, t.muted);
        } else {
            tx("—°", 432, 187, 240, 76, 3, t.muted);
            tx(
                if !v.enabled {
                    "天气已关闭"
                } else if v.busy {
                    "正在获取天气"
                } else {
                    "暂时无法获取天气"
                },
                436,
                278,
                290,
                30,
                2,
                t.text,
            );
            let note = if !v.enabled {
                "在天气来源中开启，温度与预报会显示在这里。"
            } else {
                "检查网络或切换天气来源，日历仍可正常使用。"
            };
            text(
                mem,
                note,
                r(436, 323, 286, 72),
                v.fonts[0],
                t.muted,
                DT_LEFT | DT_WORDBREAK,
            );
        }
        tx("LITETRAY", 8, 494, 160, 24, 1, t.muted);
        tx("日历与天气，一眼即见", 500, 494, 250, 24, 1, t.muted);
        BitBlt(dc, 0, 0, client.right, client.bottom, mem, 0, 0, SRCCOPY);
        SelectObject(mem, old);
        DeleteObject(bitmap);
        DeleteDC(mem);
        EndPaint(hwnd, &ps);
    }
}
