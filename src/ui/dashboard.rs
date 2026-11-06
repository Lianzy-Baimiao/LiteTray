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
const WIDTH: i32 = popup::WIDTH;
const HEIGHT: i32 = popup::HEIGHT;
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
struct ButtonSpec {
    name: String,
    id: u16,
    rect: RECT,
    tab: bool,
}
fn button_specs(v: &View) -> Vec<ButtonSpec> {
    let s = |n: i32| (n as f64 * v.factor).round() as i32;
    let spec = |name: String, id, x, y, w, h, tab| ButtonSpec {
        name,
        id,
        rect: rect(s(x), s(y), s(w), s(h)),
        tab,
    };
    let mut buttons = vec![
        spec("上个月".into(), PREV, 224, 84, 30, 32, true),
        spec("今天".into(), TODAY, 254, 84, 48, 32, true),
        spec("下个月".into(), NEXT, 304, 84, 30, 32, true),
        spec("刷新天气".into(), REFRESH, 500, 18, 86, 34, true),
        spec(
            if v.flyout { "设置" } else { "展开" }.into(),
            if v.flyout { 40 } else { CALENDAR },
            590,
            18,
            54,
            34,
            true,
        ),
    ];
    for (i, date) in v.days.iter().enumerate() {
        buttons.push(spec(
            format!(
                "{}{}",
                date.format("%Y年%m月%d日"),
                if *date == Local::now().date_naive() {
                    "，今天"
                } else {
                    ""
                }
            ),
            FIRST + i as u16,
            20 + (i % 7) as i32 * 46,
            157 + (i / 7) as i32 * 38,
            36,
            36,
            *date == v.calendar.selected,
        ));
    }
    buttons
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
        v.factor = (r.right as f64 / WIDTH as f64).min(r.bottom as f64 / HEIGHT as f64);
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
            make_font(s(44), 400, "Segoe UI"),
            make_font(s(20), 400, "Segoe MDL2 Assets"),
        ];
        v.days = v.calendar.grid();
        for b in button_specs(v) {
            let hwnd_button = CreateWindowExW(
                0,
                wide("BUTTON").as_ptr(),
                wide(&b.name).as_ptr(),
                WS_CHILD | WS_VISIBLE | BS_OWNERDRAW as u32 | if b.tab { WS_TABSTOP } else { 0 },
                b.rect.left,
                b.rect.top,
                b.rect.right - b.rect.left,
                b.rect.bottom - b.rect.top,
                hwnd,
                b.id as usize as HMENU,
                GetModuleHandleW(null()),
                null(),
            );
            SendMessageW(hwnd_button, WM_SETFONT, v.fonts[0] as usize, 0);
            SetWindowSubclass(hwnd_button, Some(button_proc), 1, 0);
            SetWindowSubclass(hwnd_button, Some(design::hover_proc), 2, 0);
            v.buttons.push(hwnd_button);
        }
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
        let hover = !item.hwndItem.is_null()
            && !GetPropW(item.hwndItem, wide("LiteTray.Hover").as_ptr()).is_null();
        let day = (FIRST..FIRST + 42).contains(&id);
        let date = day.then(|| v.days[(id - FIRST) as usize]);
        let selected = date == Some(v.calendar.selected);
        fill(item.hDC, item.rcItem, t.surface);
        let color = if selected {
            t.accent
        } else if pressed || hover {
            t.tint
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
                TODAY => "今天".into(),
                REFRESH => "刷新天气".into(),
                40 => "设置".into(),
                CALENDAR => "展开".into(),
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
        render(mem, client, v);
        BitBlt(dc, 0, 0, client.right, client.bottom, mem, 0, 0, SRCCOPY);
        SelectObject(mem, old);
        DeleteObject(bitmap);
        DeleteDC(mem);
        EndPaint(hwnd, &ps);
    }
}
// Shared by the live panel and the offscreen visual check. No HWND, process
// discovery, taskbar connection, settings, or network is needed to draw it.
unsafe fn render(dc: HDC, client: RECT, v: &View) {
    unsafe {
        let t = Theme::new(v.dark);
        let s = |n: i32| (n as f64 * v.factor).round() as i32;
        let r = |x, y, w, h| rect(s(x), s(y), s(w), s(h));
        fill(dc, client, t.surface);
        let tx = |value: &str, x, y, w, h, font, color| {
            text(
                dc,
                value,
                r(x, y, w, h),
                v.fonts[font],
                color,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            )
        };
        let now = Local::now();
        let weekday = [
            "星期一",
            "星期二",
            "星期三",
            "星期四",
            "星期五",
            "星期六",
            "星期日",
        ];
        tx(
            &format!(
                "{}  {}",
                now.format("%-m月%-d日"),
                weekday[now.weekday().num_days_from_monday() as usize]
            ),
            20,
            13,
            360,
            29,
            2,
            t.text,
        );
        tx(
            &format!(
                "{} · {}",
                now.format("%Y年"),
                now.format(if v.seconds { "%H:%M:%S" } else { "%H:%M" })
            ),
            20,
            44,
            380,
            20,
            1,
            t.muted,
        );
        fill(dc, r(20, 72, 620, 1), t.border);
        tx(
            &v.calendar.month.format("%Y年 %-m月").to_string(),
            20,
            84,
            200,
            32,
            0,
            t.text,
        );
        for (i, name) in ["一", "二", "三", "四", "五", "六", "日"]
            .iter()
            .enumerate()
        {
            text(
                dc,
                name,
                r(20 + i as i32 * 46, 124, 36, 25),
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
            20,
            394,
            318,
            22,
            1,
            t.muted,
        );
        rounded(dc, r(358, 88, 282, 320), t.field, t.field, s(16));
        tx(&v.city, 376, 102, 242, 28, 0, t.text);
        let status;
        if v.enabled
            && let Some(w) = v.weather.as_ref().and_then(|r| r.weather.as_ref())
        {
            tx(
                &format!("{:.0}°", w.temperature_c),
                374,
                136,
                182,
                66,
                3,
                t.text,
            );
            weather_icon(dc, r(570, 148, 48, 44), t.accent, &w.description);
            tx(&w.description, 378, 210, 230, 23, 0, t.text);
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
                378,
                237,
                240,
                22,
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
                378,
                261,
                240,
                22,
                1,
                t.muted,
            );
            fill(dc, r(376, 294, 246, 1), t.border);
            if w.days.is_empty() {
                tx("此来源暂无每日预报", 378, 312, 240, 24, 1, t.muted);
            }
            for (i, day) in w.days.iter().take(3).enumerate() {
                let y = 306 + i as i32 * 29;
                let name = NaiveDate::parse_from_str(&day.date, "%Y-%m-%d")
                    .ok()
                    .map(|date| {
                        if date == now.date_naive() {
                            "今天".into()
                        } else if date == now.date_naive() + chrono::Duration::days(1) {
                            "明天".into()
                        } else {
                            date.format("%-m/%-d").to_string()
                        }
                    })
                    .unwrap_or_else(|| day.date.clone());
                tx(&name, 378, y, 46, 24, 1, t.muted);
                tx(&day.description, 429, y, 102, 24, 1, t.text);
                text(
                    dc,
                    &format!("{:.0}° / {:.0}°", day.low_c, day.high_c),
                    r(536, y, 85, 24),
                    v.fonts[1],
                    t.text,
                    DT_RIGHT | DT_SINGLELINE | DT_VCENTER,
                );
            }
            status = if v.weather.as_ref().is_some_and(|r| r.cached) {
                format!(
                    "离线缓存 · 非实时 · {}",
                    w.data_at.with_timezone(&Local).format("%m-%d %H:%M")
                )
            } else {
                format!(
                    "{}{} · 数据时间 {}",
                    if v.weather.as_ref().is_some_and(|r| r.fallback) {
                        "备用来源 · "
                    } else {
                        ""
                    },
                    w.provider.label(),
                    w.data_at.with_timezone(&Local).format("%H:%M")
                )
            };
        } else {
            tx("—°", 374, 140, 230, 68, 3, t.muted);
            tx(
                if !v.enabled {
                    "天气已关闭"
                } else if v.busy {
                    "正在获取天气"
                } else {
                    "暂无天气数据"
                },
                378,
                220,
                240,
                28,
                0,
                t.text,
            );
            text(
                dc,
                if !v.enabled {
                    "在设置中启用天气，即可查看温度和每日预报。"
                } else {
                    "可刷新重试，或在设置中切换天气来源。"
                },
                r(378, 260, 232, 70),
                v.fonts[1],
                t.muted,
                DT_LEFT | DT_WORDBREAK,
            );
            status = if v.busy {
                "正在更新天气…"
            } else {
                "日历可离线使用"
            }
            .into();
        }
        fill(dc, r(0, 424, WIDTH, 1), t.border);
        tx(&status, 20, 427, 550, 23, 1, t.muted);
        text(
            dc,
            "轻托盘",
            r(566, 427, 74, 23),
            v.fonts[1],
            t.muted,
            DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
        );
    }
}
unsafe fn weather_icon(dc: HDC, r: RECT, color: u32, description: &str) {
    unsafe {
        let size = r.right - r.left;
        let sx = |n| r.left + size * n / 48;
        let sy = |n| r.top + size * n / 48;
        let pen = CreatePen(PS_SOLID, (size / 24).max(1), color);
        let old_pen = SelectObject(dc, pen);
        let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
        if description.contains('晴') {
            Ellipse(dc, sx(14), sy(8), sx(34), sy(28));
            for (a, b, c, d) in [
                (24, 1, 24, 5),
                (24, 31, 24, 35),
                (7, 18, 11, 18),
                (37, 18, 41, 18),
                (12, 6, 15, 9),
                (33, 27, 36, 30),
                (12, 30, 15, 27),
                (33, 9, 36, 6),
            ] {
                MoveToEx(dc, sx(a), sy(b), null_mut());
                LineTo(dc, sx(c), sy(d));
            }
        } else {
            let points = [
                (10, 30),
                (1, 30),
                (1, 18),
                (12, 18),
                (13, 3),
                (34, 3),
                (35, 18),
                (48, 15),
                (49, 30),
                (38, 30),
                (28, 30),
                (19, 30),
                (10, 30),
            ]
            .map(|(x, y)| POINT { x: sx(x), y: sy(y) });
            PolyBezier(dc, points.as_ptr(), points.len() as u32);
            if description.contains('雨') || description.contains('雪') {
                for x in [14, 25, 36] {
                    MoveToEx(dc, sx(x), sy(34), null_mut());
                    LineTo(dc, sx(x - 3), sy(40));
                }
            }
        }
        SelectObject(dc, old_brush);
        SelectObject(dc, old_pen);
        DeleteObject(pen);
    }
}

#[cfg(test)]
mod visual_checks {
    use super::*;
    #[test]
    #[ignore = "Generates offscreen panel artifacts; set LITETRAY_RENDER_DIR"]
    fn render_panel_artifacts() {
        let output = std::env::var_os("LITETRAY_RENDER_DIR").expect("explicit output directory");
        let output = std::path::PathBuf::from(output);
        std::fs::create_dir_all(&output).unwrap();
        for (dark, factor, empty) in [
            (false, 1., false),
            (true, 1., false),
            (false, 1.5, false),
            (true, 2., false),
            (false, 1., true),
            (true, 1., true),
        ] {
            unsafe {
                let s = |n: i32| (n as f64 * factor).round() as i32;
                let today = Local::now().date_naive();
                let calendar = calendar::Calendar::new(today);
                let now = Utc::now();
                let v = View {
                    calendar,
                    days: calendar.grid(),
                    buttons: vec![],
                    city: "示例 · 深圳".into(),
                    enabled: true,
                    busy: false,
                    lunar: true,
                    seconds: true,
                    flyout: true,
                    dark,
                    factor,
                    fonts: [
                        make_font(s(14), 400, "Microsoft YaHei UI"),
                        make_font(s(12), 400, "Microsoft YaHei UI"),
                        make_font(s(19), 600, "Microsoft YaHei UI"),
                        make_font(s(44), 400, "Segoe UI"),
                        make_font(s(20), 400, "Segoe MDL2 Assets"),
                    ],
                    weather: (!empty).then(|| weather::WeatherResult {
                        weather: Some(weather::Weather {
                            provider: Provider::OpenMeteo,
                            city: "示例 · 深圳".into(),
                            latitude: 22.6,
                            longitude: 114.,
                            data_at: now,
                            fetched_at: now,
                            temperature_c: 25.,
                            feels_like_c: Some(26.),
                            humidity_percent: Some(68.),
                            wind_kmh: Some(12.),
                            description: "多云".into(),
                            kind: "界面验证示例".into(),
                            days: (0..3)
                                .map(|i| weather::DailyForecast {
                                    date: (today + chrono::Duration::days(i)).to_string(),
                                    low_c: 22. - i as f64,
                                    high_c: 28. - i as f64,
                                    description: ["多云", "晴间多云", "小雨"][i as usize].into(),
                                })
                                .collect(),
                        }),
                        cached: false,
                        fallback: false,
                        attempts: vec![],
                        cache_error: None,
                    }),
                };
                let width = s(WIDTH);
                let height = s(HEIGHT);
                let dc = CreateCompatibleDC(null_mut());
                assert!(!dc.is_null());
                let mut info: BITMAPINFO = std::mem::zeroed();
                info.bmiHeader = BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..std::mem::zeroed()
                };
                let mut bits = null_mut();
                let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
                assert!(!bitmap.is_null() && !bits.is_null());
                let old = SelectObject(dc, bitmap);
                render(dc, rect(0, 0, width, height), &v);
                for button in button_specs(&v) {
                    assert!(
                        button.rect.left >= 0
                            && button.rect.top >= 0
                            && button.rect.right <= width
                            && button.rect.bottom <= height,
                        "button outside panel: {}",
                        button.name
                    );
                    draw_button(
                        &v,
                        &DRAWITEMSTRUCT {
                            CtlType: ODT_BUTTON,
                            CtlID: button.id as u32,
                            hDC: dc,
                            rcItem: button.rect,
                            ..std::mem::zeroed()
                        },
                    );
                }
                GdiFlush();
                let length = (width * height * 4) as usize;
                let mut bmp = Vec::with_capacity(54 + length);
                bmp.extend_from_slice(b"BM");
                bmp.extend_from_slice(&((54 + length) as u32).to_le_bytes());
                bmp.extend_from_slice(&[0; 4]);
                bmp.extend_from_slice(&54u32.to_le_bytes());
                bmp.extend_from_slice(&40u32.to_le_bytes());
                bmp.extend_from_slice(&width.to_le_bytes());
                bmp.extend_from_slice(&(-height).to_le_bytes());
                bmp.extend_from_slice(&1u16.to_le_bytes());
                bmp.extend_from_slice(&32u16.to_le_bytes());
                bmp.extend_from_slice(&[0; 24]);
                bmp.extend_from_slice(std::slice::from_raw_parts(bits.cast::<u8>(), length));
                let path = output.join(format!(
                    "panel-{}-{}-{}.bmp",
                    if dark { "dark" } else { "light" },
                    (factor * 100.) as u32,
                    if empty { "empty" } else { "sample" }
                ));
                std::fs::write(&path, bmp).unwrap();
                SelectObject(dc, old);
                DeleteObject(bitmap);
                DeleteDC(dc);
                println!("{}", path.display());
            }
        }
    }
}
