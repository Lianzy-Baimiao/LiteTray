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
const ADD_EVENT: u16 = 1103;
const FOCUS: u16 = 1104;
const MANAGE: u16 = 1106;
const EARLIER: u16 = 1107;
const LATER: u16 = 1108;
const FIRST_ENTRY: u16 = 1150;
const SELECT_DAY: u32 = WM_APP + 80;
const WIDTH: i32 = popup::WIDTH;
const HEIGHT: i32 = popup::HEIGHT;
struct View {
    calendar: calendar::Calendar,
    displayed: Option<calendar::Calendar>,
    today: NaiveDate,
    days: Vec<NaiveDate>,
    buttons: Vec<HWND>,
    weather: Option<weather::WeatherResult>,
    city: String,
    enabled: bool,
    busy: bool,
    lunar: bool,
    almanac: bool,
    almanac_hover: almanac_tip::Hover,
    seconds: bool,
    flyout: bool,
    detached: bool,
    dark: bool,
    factor: f64,
    fonts: [HFONT; 5],
    store: Option<Rc<RefCell<organizer::Store>>>,
    revision: u64,
    lunar_labels: Vec<String>,
    solar_terms: Vec<Option<String>>,
    markers: Vec<bool>,
    entries: Vec<(u64, String, String)>,
    entry_count: usize,
    entry_offset: usize,
    editor: HWND,
    editing: bool,
    tips: Option<icons::Tips>,
    holidays: Option<Rc<RefCell<holidays::Catalog>>>,
    holiday_revision: u64,
    holiday_days: Vec<Option<holidays::Day>>,
    holiday_status: String,
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
            displayed: None,
            today,
            days: vec![],
            buttons: vec![],
            weather: None,
            city: String::new(),
            enabled: true,
            busy: false,
            lunar: true,
            almanac: true,
            almanac_hover: Default::default(),
            seconds: true,
            flyout,
            detached: false,
            dark,
            factor: 1.,
            fonts: [null_mut(); 5],
            store: None,
            revision: u64::MAX,
            lunar_labels: vec![],
            solar_terms: vec![],
            markers: vec![],
            entries: vec![],
            entry_count: 0,
            entry_offset: 0,
            editor: null_mut(),
            editing: false,
            tips: None,
            holidays: None,
            holiday_revision: u64::MAX,
            holiday_days: vec![],
            holiday_status: String::new(),
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
pub(super) unsafe fn bind_store(hwnd: HWND, store: Rc<RefCell<organizer::Store>>) {
    unsafe {
        if let Some(cell) =
            (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>).as_ref()
            && let Ok(mut v) = cell.try_borrow_mut()
        {
            v.store = Some(store);
        }
        layout(hwnd);
    }
}
pub(super) unsafe fn bind_holidays(hwnd: HWND, catalog: Rc<RefCell<holidays::Catalog>>) {
    unsafe {
        if let Some(cell) =
            (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>).as_ref()
            && let Ok(mut view) = cell.try_borrow_mut()
        {
            view.holidays = Some(catalog);
            view.holiday_revision = u64::MAX;
        }
        layout(hwnd);
    }
}
pub(super) unsafe fn set_detached(hwnd: HWND, detached: bool) {
    unsafe {
        if let Some(cell) =
            (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>).as_ref()
            && let Ok(mut v) = cell.try_borrow_mut()
        {
            v.detached = detached;
        }
        layout(hwnd);
        InvalidateRect(hwnd, null(), 0);
    }
}
pub(super) unsafe fn refresh_theme(hwnd: HWND, dark: bool) {
    unsafe {
        if let Some(cell) =
            (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>).as_ref()
            && let Ok(mut v) = cell.try_borrow_mut()
        {
            v.dark = dark;
            v.factor = 0.;
        }
        layout(hwnd);
        RedrawWindow(hwnd, null(), null_mut(), RDW_INVALIDATE | RDW_ALLCHILDREN);
    }
}
fn refresh_agenda(v: &mut View) {
    let Some(store) = &v.store else {
        return;
    };
    let store = store.borrow();
    v.revision = store.revision;
    v.markers = v
        .days
        .iter()
        .map(|date| store.data.entries.iter().any(|e| e.occurs_on(*date)))
        .collect();
    let mut entries = store
        .data
        .entries
        .iter()
        .filter(|e| e.occurs_on(v.calendar.selected))
        .collect::<Vec<_>>();
    entries.sort_by_key(|e| (e.time, e.id));
    v.entry_count = entries.len();
    v.entry_offset = v.entry_offset.min(entries.len().saturating_sub(1) / 3 * 3);
    v.entries = entries
        .iter()
        .skip(v.entry_offset)
        .take(3)
        .map(|e| {
            (
                e.id,
                e.title.clone(),
                format!(
                    "{} · {}",
                    e.time
                        .map(|time| time.format("%H:%M").to_string())
                        .unwrap_or_else(|| "全天".into()),
                    e.kind.label()
                ),
            )
        })
        .collect();
}
pub(super) unsafe fn editing(hwnd: HWND) -> bool {
    unsafe {
        (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>)
            .as_ref()
            .and_then(|v| v.try_borrow().ok())
            .is_some_and(|v| v.editing)
    }
}
pub(super) unsafe fn focus_content(hwnd: HWND) {
    unsafe {
        let editor = GetDlgItem(hwnd, quick_agenda::ID as i32);
        SetFocus(if editing(hwnd) {
            GetDlgItem(editor, 4610)
        } else {
            GetDlgItem(hwnd, TODAY as i32)
        });
    }
}
pub(super) unsafe fn dismiss_editor(hwnd: HWND) -> bool {
    unsafe {
        let Some(cell) = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>).as_ref()
        else {
            return false;
        };
        let Ok(v) = cell.try_borrow() else {
            return false;
        };
        v.editing && quick_agenda::dismiss(v.editor)
    }
}
unsafe fn edit(hwnd: HWND, id: Option<u64>) {
    unsafe {
        let Some(cell) = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>).as_ref()
        else {
            return;
        };
        let Ok(mut v) = cell.try_borrow_mut() else {
            return;
        };
        let Some(store) = v.store.clone() else {
            return;
        };
        let entry = if let Some(id) = id {
            store
                .borrow()
                .data
                .entries
                .iter()
                .find(|e| e.id == id)
                .cloned()
        } else {
            Some(organizer::Entry::new(
                organizer::Kind::Event,
                v.calendar.selected,
            ))
        };
        let Some(entry) = entry else {
            return;
        };
        if v.editor.is_null() {
            v.editor = quick_agenda::create(hwnd, store, v.dark);
        }
        if !quick_agenda::open(v.editor, entry) {
            return;
        }
        v.editing = true;
        drop(v);
        layout(hwnd);
        InvalidateRect(hwnd, null(), 0);
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
        let weather_changed = view.city != config.weather.city
            || view.enabled != config.weather.enabled
            || view.busy != busy
            || &view.weather != weather;
        let lunar_changed = view.lunar != config.clock.show_lunar;
        let day_changed = view.today != Local::now().date_naive();
        view.city = config.weather.city.clone();
        view.enabled = config.weather.enabled;
        view.busy = busy;
        view.weather = weather.clone();
        view.lunar = config.clock.show_lunar;
        view.almanac = config.clock.show_almanac;
        if !view.almanac {
            view.almanac_hover.hide();
        }
        view.seconds = clock::needs_second_ticks(&config.clock);
        let changed = view
            .store
            .as_ref()
            .is_some_and(|s| s.borrow().revision != view.revision);
        let holiday_changed = view
            .holidays
            .as_ref()
            .is_some_and(|h| h.borrow().revision != view.holiday_revision);
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
        if window_text(hwnd) != accessible {
            set_text(hwnd, &accessible);
        }
        let factor = view.factor;
        drop(view);
        if changed || day_changed || holiday_changed {
            layout(hwnd);
        }
        invalidate_part(hwnd, factor, rect(20, 13, 380, 53));
        if weather_changed {
            invalidate_part(hwnd, factor, rect(380, 88, 320, 390));
            invalidate_part(hwnd, factor, rect(0, 628, WIDTH, 28));
        }
        if lunar_changed {
            for id in FIRST..FIRST + 42 {
                InvalidateRect(GetDlgItem(hwnd, id as i32), null(), 0);
            }
            invalidate_part(hwnd, factor, rect(20, 444, 340, 44));
        }
    }
}
struct ButtonSpec {
    name: String,
    id: u16,
    rect: RECT,
    tab: bool,
    visible: bool,
}
fn button_specs(v: &View) -> Vec<ButtonSpec> {
    let s = |n: i32| (n as f64 * v.factor).round() as i32;
    let spec = |name: String, id, x, y, w, h, tab| ButtonSpec {
        name,
        id,
        rect: rect(s(x), s(y), s(w), s(h)),
        tab,
        visible: true,
    };
    let mut buttons = vec![
        spec("上个月".into(), PREV, 244, 84, 30, 32, true),
        spec("今天".into(), TODAY, 275, 84, 48, 32, true),
        spec("下个月".into(), NEXT, 325, 84, 30, 32, true),
        spec("刷新天气".into(), REFRESH, 574, 18, 36, 34, true),
        spec(
            if v.flyout { "设置" } else { "展开" }.into(),
            if v.flyout { 40 } else { CALENDAR },
            if v.flyout { 618 } else { 662 },
            18,
            36,
            34,
            true,
        ),
        spec(
            "新建事项".into(),
            ADD_EVENT,
            if v.editing { 286 } else { 618 },
            496,
            32,
            30,
            true,
        ),
        spec(
            "完整日程管理".into(),
            MANAGE,
            if v.editing { 326 } else { 662 },
            496,
            32,
            30,
            true,
        ),
        spec("闹钟与专注".into(), FOCUS, 530, 18, 36, 34, true),
    ];
    for (id, x, label, visible) in [
        (
            EARLIER,
            if v.editing { 214 } else { 546 },
            "上一组日程",
            v.entry_offset > 0,
        ),
        (
            LATER,
            if v.editing { 246 } else { 578 },
            "下一组日程",
            v.entry_offset + 3 < v.entry_count,
        ),
    ] {
        let mut button = spec(label.into(), id, x, 496, 30, 30, true);
        button.visible = visible;
        buttons.push(button);
    }
    if v.flyout {
        let mut detach = spec(
            "分离面板，可拖动并保持展开".into(),
            panel::DETACH,
            662,
            18,
            36,
            34,
            true,
        );
        detach.visible = !v.detached;
        let mut close = spec("关闭分离面板".into(), panel::CLOSE, 662, 18, 36, 34, true);
        close.visible = v.detached;
        buttons.push(detach);
        buttons.push(close);
    }
    for (i, date) in v.days.iter().enumerate() {
        buttons.push(spec(
            format!(
                "{}{}{}{}",
                date.format("%Y年%m月%d日"),
                if *date == Local::now().date_naive() {
                    "，今天"
                } else {
                    ""
                },
                v.holiday_days
                    .get(i)
                    .and_then(|d| d.as_ref())
                    .map(|d| format!("，{}", d.description()))
                    .unwrap_or_default(),
                v.solar_terms
                    .get(i)
                    .and_then(|name| name.as_ref())
                    .map(|name| format!("，{name}"))
                    .unwrap_or_default()
            ),
            FIRST + i as u16,
            20 + (i % 7) as i32 * 48,
            156 + (i / 7) as i32 * 48,
            44,
            44,
            *date == v.calendar.selected,
        ));
    }
    for i in 0..3 {
        let entry = v.entries.get(i);
        let mut row = spec(
            entry
                .map(|(_, title, detail)| format!("{detail}，{title}"))
                .unwrap_or_default(),
            FIRST_ENTRY + i as u16,
            20,
            535 + i as i32 * 29,
            if v.editing { 340 } else { 680 },
            28,
            true,
        );
        row.visible = entry.is_some();
        buttons.push(row);
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
        let previous = v.displayed;
        let selection_changed = previous.is_none_or(|c| c.selected != v.calendar.selected);
        let month_changed = previous.is_none_or(|c| c.month != v.calendar.month);
        let today = Local::now().date_naive();
        let today_changed = v.today != today;
        v.today = today;
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut r);
        let factor = (r.right as f64 / WIDTH as f64)
            .min(r.bottom as f64 / HEIGHT as f64)
            .max(0.01);
        let resized = v.fonts[0].is_null() || (factor - v.factor).abs() > f64::EPSILON;
        if resized || month_changed || selection_changed {
            v.almanac_hover.hide();
        }
        v.factor = factor;
        let s = |n: i32| (n as f64 * factor).round() as i32;
        let old_fonts = v.fonts;
        if resized {
            let face = design::font_face();
            v.fonts = [
                make_font(s(14), 400, &face),
                make_font(s(11), 400, &face),
                make_font(s(19), 600, &face),
                make_font(s(44), 400, "Segoe UI"),
                make_font(s(20), 400, "Segoe MDL2 Assets"),
            ];
        }
        if month_changed {
            v.days = v.calendar.grid();
            v.solar_terms = v
                .days
                .iter()
                .map(|date| solar_terms::on_date(*date))
                .collect();
            v.lunar_labels = v
                .days
                .iter()
                .map(|date| {
                    lunar::for_date(*date)
                        .map(|day| day.short)
                        .unwrap_or_default()
                })
                .collect();
        }
        let old_holidays = v.holiday_days.clone();
        let old_holiday_status = v.holiday_status.clone();
        if let Some(catalog) = &v.holidays {
            let catalog = catalog.borrow();
            if month_changed || catalog.revision != v.holiday_revision {
                v.holiday_days = v.days.iter().map(|d| catalog.day(*d).cloned()).collect();
                v.holiday_revision = catalog.revision;
                v.holiday_status = catalog.status(v.calendar.month.year());
            }
        }
        let old_markers = v.markers.clone();
        let old_entries = v.entries.clone();
        if selection_changed {
            v.entry_offset = 0;
        }
        if selection_changed
            || month_changed
            || v.store
                .as_ref()
                .is_some_and(|store| store.borrow().revision != v.revision)
        {
            refresh_agenda(v);
        }
        let entries_changed = old_entries != v.entries;
        let mut dirty = vec![];
        for b in button_specs(v) {
            let mut child = GetDlgItem(hwnd, b.id as i32);
            let created = child.is_null();
            let mut changed = created || resized;
            if created {
                child = CreateWindowExW(
                    0,
                    wide("BUTTON").as_ptr(),
                    wide(&b.name).as_ptr(),
                    WS_CHILD
                        | BS_OWNERDRAW as u32
                        | if b.tab { WS_TABSTOP } else { 0 }
                        | if b.visible { WS_VISIBLE } else { 0 },
                    b.rect.left,
                    b.rect.top,
                    b.rect.right - b.rect.left,
                    b.rect.bottom - b.rect.top,
                    hwnd,
                    b.id as usize as HMENU,
                    GetModuleHandleW(null()),
                    null(),
                );
                SetWindowSubclass(child, Some(button_proc), 1, 0);
                SetWindowSubclass(child, Some(design::hover_proc), 2, 0);
                v.buttons.push(child);
            } else {
                let mut actual: RECT = std::mem::zeroed();
                GetWindowRect(child, &mut actual);
                MapWindowPoints(null_mut(), hwnd, &mut actual as *mut RECT as *mut POINT, 2);
                if resized
                    || actual.left != b.rect.left
                    || actual.top != b.rect.top
                    || actual.right != b.rect.right
                    || actual.bottom != b.rect.bottom
                {
                    changed = true;
                    SetWindowPos(
                        child,
                        null_mut(),
                        b.rect.left,
                        b.rect.top,
                        b.rect.right - b.rect.left,
                        b.rect.bottom - b.rect.top,
                        SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOREDRAW,
                    );
                }
                if window_text(child) != b.name {
                    set_text(child, &b.name);
                    changed = true;
                }
                let style = GetWindowLongW(child, GWL_STYLE) as u32;
                if (style & WS_TABSTOP != 0) != b.tab {
                    SetWindowLongW(
                        child,
                        GWL_STYLE,
                        (if b.tab {
                            style | WS_TABSTOP
                        } else {
                            style & !WS_TABSTOP
                        }) as i32,
                    );
                    changed = true;
                }
                if (style & WS_VISIBLE != 0) != b.visible {
                    ShowWindow(child, if b.visible { SW_SHOWNA } else { SW_HIDE });
                    changed = true;
                }
            }
            if created || resized {
                SendMessageW(child, WM_SETFONT, v.fonts[0] as usize, 0);
            }
            if (FIRST..FIRST + 42).contains(&b.id) {
                let i = (b.id - FIRST) as usize;
                changed |= month_changed || today_changed || old_markers.get(i) != v.markers.get(i);
                changed |= old_holidays.get(i) != v.holiday_days.get(i);
            }
            if changed && b.visible {
                dirty.push(child);
            }
        }
        v.displayed = Some(v.calendar);
        if v.tips.is_none() {
            let mut tips = icons::Tips::new(hwnd);
            for b in button_specs(v) {
                if button_icon(b.id).is_some() {
                    tips.add(hwnd, GetDlgItem(hwnd, b.id as i32), &b.name);
                }
            }
            v.tips = Some(tips);
        }
        let editor = v.editor;
        let dark = v.dark;
        let editor_bounds = rect(s(376), s(84), s(324), s(540));
        let holiday_status_changed =
            old_holiday_status != v.holiday_status || old_holidays != v.holiday_days;
        let month = v.calendar.month;
        if resized {
            for font in old_fonts {
                if !font.is_null() {
                    DeleteObject(font);
                }
            }
        }
        drop(guard);
        quick_agenda::resize(editor, editor_bounds, dark);
        // Keep every native button alive. Only dates whose state changed and
        // the selected day's summary are invalidated; weather stays intact.
        for child in dirty {
            InvalidateRect(child, null(), 0);
        }
        if resized {
            InvalidateRect(hwnd, null(), 0);
        } else {
            if month_changed {
                invalidate_part(hwnd, factor, rect(20, 84, 210, 32));
            }
            if selection_changed || holiday_status_changed {
                invalidate_part(hwnd, factor, rect(20, 444, 340, 44));
            }
            if entries_changed {
                invalidate_part(hwnd, factor, rect(20, 535, 680, 90));
            }
        }
        if month_changed {
            let root = GetAncestor(hwnd, GA_ROOT);
            let owner = GetWindow(root, GW_OWNER);
            PostMessageW(
                if owner.is_null() { root } else { owner },
                holiday_data::REQUEST,
                month.year() as usize,
                month.month() as isize,
            );
        }
    }
}

unsafe fn invalidate_part(hwnd: HWND, factor: f64, r: RECT) {
    let s = |n: i32| (n as f64 * factor).round() as i32;
    let bounds = RECT {
        left: s(r.left),
        top: s(r.top),
        right: s(r.right),
        bottom: s(r.bottom),
    };
    unsafe {
        InvalidateRect(hwnd, &bounds, 0);
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
        if (FIRST..FIRST + 42).contains(&id)
            && matches!(
                msg,
                WM_MOUSEMOVE | WM_MOUSELEAVE | WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_KEYDOWN
            )
        {
            let owner = GetParent(hwnd);
            if let Some(cell) =
                (GetWindowLongPtrW(owner, GWLP_USERDATA) as *const RefCell<View>).as_ref()
                && let Ok(mut view) = cell.try_borrow_mut()
            {
                match msg {
                    WM_MOUSEMOVE if view.almanac => {
                        if let Some(date) = view.days.get((id - FIRST) as usize).copied() {
                            view.almanac_hover.begin(owner, hwnd, date);
                        }
                    }
                    WM_MOUSELEAVE => view.almanac_hover.leave(hwnd),
                    WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_KEYDOWN => view.almanac_hover.hide(),
                    _ => {}
                }
            }
        }
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
            if dismiss_editor(GetParent(hwnd)) {
                return 0;
            }
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
            WM_TIMER if w == almanac_tip::TIMER => {
                if let Ok(mut v) = (*raw).try_borrow_mut() {
                    let dark = v.dark;
                    v.almanac_hover.tick(dark);
                }
                0
            }
            WM_COMMAND if w & 0xffff == 40 && w >> 16 == BN_CLICKED as usize => {
                // Opening settings does not depend on the calendar's paint or
                // data state. Never discard this command during a view borrow.
                PostMessageW(GetParent(hwnd), WM_COMMAND, w, l);
                0
            }
            quick_agenda::CLOSED | quick_agenda::SAVED => {
                if let Ok(mut v) = (*raw).try_borrow_mut() {
                    v.editing = false;
                    refresh_agenda(&mut v);
                }
                layout(hwnd);
                InvalidateRect(hwnd, null(), 0);
                0
            }
            WM_NCHITTEST => {
                if let Ok(v) = (*raw).try_borrow()
                    && v.flyout
                    && v.detached
                {
                    let mut point = POINT {
                        x: l as u16 as i16 as i32,
                        y: (l >> 16) as u16 as i16 as i32,
                    };
                    ScreenToClient(hwnd, &mut point);
                    if point.y >= 0
                        && point.y < (72. * v.factor) as i32
                        && point.x < (425. * v.factor) as i32
                    {
                        return HTTRANSPARENT as isize;
                    }
                }
                DefWindowProcW(hwnd, msg, w, l)
            }
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
            WM_PRINTCLIENT => {
                if let Ok(v) = (*raw).try_borrow() {
                    let mut client: RECT = std::mem::zeroed();
                    GetClientRect(hwnd, &mut client);
                    render(w as HDC, client, &v);
                }
                0
            }
            WM_DRAWITEM if l != 0 => {
                if let Ok(v) = (*raw).try_borrow() {
                    draw_button_buffered(&v, &*(l as *const DRAWITEMSTRUCT));
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
                } else if id == MANAGE {
                    PostMessageW(
                        GetAncestor(hwnd, GA_ROOTOWNER),
                        agenda::OPEN_DATE,
                        v.calendar.selected.num_days_from_ce() as usize,
                        0,
                    );
                    return 0;
                } else if id == ADD_EVENT {
                    drop(guard);
                    edit(hwnd, None);
                    return 0;
                } else if id == EARLIER || id == LATER {
                    v.entry_offset = if id == EARLIER {
                        v.entry_offset.saturating_sub(3)
                    } else {
                        v.entry_offset + 3
                    };
                    refresh_agenda(v);
                    drop(guard);
                    layout(hwnd);
                    invalidate_part(hwnd, (*raw).borrow().factor, rect(20, 496, 680, 129));
                    return 0;
                } else if id == FOCUS {
                    PostMessageW(
                        GetAncestor(hwnd, GA_ROOTOWNER),
                        WM_COMMAND,
                        (NAV + 9) as usize,
                        0,
                    );
                    PostMessageW(GetAncestor(hwnd, GA_ROOTOWNER), WM_SHOW_SETTINGS, 0, 0);
                    return 0;
                } else if (FIRST_ENTRY..FIRST_ENTRY + 3).contains(&id) {
                    let entry = v.entries.get((id - FIRST_ENTRY) as usize).map(|e| e.0);
                    drop(guard);
                    if let Some(entry) = entry {
                        edit(hwnd, Some(entry));
                    }
                    return 0;
                } else {
                    PostMessageW(GetParent(hwnd), WM_COMMAND, w, l);
                    return 0;
                }
                if v.editing && focus.is_some() {
                    quick_agenda::set_date(v.editor, v.calendar.selected);
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
fn button_icon(id: u16) -> Option<icons::Icon> {
    use icons::Icon;
    Some(match id {
        PREV | EARLIER => Icon::Left,
        NEXT | LATER => Icon::Right,
        REFRESH => Icon::Refresh,
        40 => Icon::Settings,
        CALENDAR | panel::DETACH => Icon::Detach,
        panel::CLOSE => Icon::Close,
        ADD_EVENT => Icon::Plus,
        FOCUS => Icon::Timer,
        MANAGE => Icon::List,
        _ => return None,
    })
}
fn holiday_colors(t: Theme, off: bool) -> (u32, u32) {
    if t.high_contrast {
        (t.accent, t.on_accent)
    } else {
        (
            if off {
                rgb(196, 43, 60)
            } else {
                rgb(32, 93, 188)
            },
            rgb(255, 255, 255),
        )
    }
}
fn day_sublabel(v: &View, index: usize) -> Option<&str> {
    v.solar_terms
        .get(index)
        .and_then(|s| s.as_deref())
        .or_else(|| {
            v.holiday_days
                .get(index)
                .and_then(|d| d.as_ref())
                .map(|d| d.name.as_str())
        })
        .or_else(|| {
            v.lunar
                .then(|| v.lunar_labels.get(index))
                .flatten()
                .map(String::as_str)
        })
}
fn selected_date_summary(v: &View) -> String {
    let mut labels = vec![v.calendar.selected.format("%-m月%-d日").to_string()];
    if v.lunar
        && let Some(day) = lunar::for_date(v.calendar.selected)
    {
        labels.push(day.full);
    }
    if let Some(term) = v
        .days
        .iter()
        .position(|d| *d == v.calendar.selected)
        .and_then(|i| v.solar_terms.get(i))
        .and_then(|s| s.as_ref())
    {
        labels.push(term.clone());
    }
    labels.join(" · ")
}
unsafe fn draw_button_buffered(v: &View, item: &DRAWITEMSTRUCT) {
    unsafe {
        let width = item.rcItem.right - item.rcItem.left;
        let height = item.rcItem.bottom - item.rcItem.top;
        let dc = CreateCompatibleDC(item.hDC);
        let bitmap = CreateCompatibleBitmap(item.hDC, width.max(1), height.max(1));
        if dc.is_null() || bitmap.is_null() {
            if !dc.is_null() {
                DeleteDC(dc);
            }
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
            draw_button(v, item);
            return;
        }
        let old = SelectObject(dc, bitmap);
        let buffered = DRAWITEMSTRUCT {
            hDC: dc,
            rcItem: rect(0, 0, width, height),
            ..*item
        };
        draw_button(v, &buffered);
        BitBlt(
            item.hDC,
            item.rcItem.left,
            item.rcItem.top,
            width,
            height,
            dc,
            0,
            0,
            SRCCOPY,
        );
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
    }
}
unsafe fn draw_button(v: &View, item: &DRAWITEMSTRUCT) {
    unsafe {
        let t = Theme::new(v.dark);
        let s = |n: i32| (n as f64 * v.factor).round() as i32;
        let id = item.CtlID as u16;
        if (FIRST_ENTRY..FIRST_ENTRY + 3).contains(&id) {
            let hovered = !item.hwndItem.is_null()
                && !GetPropW(item.hwndItem, wide("LiteTray.Hover").as_ptr()).is_null();
            fill(
                item.hDC,
                item.rcItem,
                if hovered { t.tint } else { t.surface },
            );
            if let Some((_, title, detail)) = v.entries.get((id - FIRST_ENTRY) as usize) {
                text(
                    item.hDC,
                    detail,
                    rect(
                        item.rcItem.left + s(8),
                        item.rcItem.top,
                        s(150),
                        item.rcItem.bottom - item.rcItem.top,
                    ),
                    v.fonts[1],
                    t.muted,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                );
                text(
                    item.hDC,
                    title,
                    rect(
                        item.rcItem.left + s(166),
                        item.rcItem.top,
                        item.rcItem.right - item.rcItem.left - s(174),
                        item.rcItem.bottom - item.rcItem.top,
                    ),
                    v.fonts[0],
                    t.text,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
            }
            return;
        }
        let focused = item.itemState & ODS_FOCUS != 0;
        let pressed = item.itemState & ODS_SELECTED != 0;
        let hover = !item.hwndItem.is_null()
            && !GetPropW(item.hwndItem, wide("LiteTray.Hover").as_ptr()).is_null();
        let day = (FIRST..FIRST + 42).contains(&id);
        let date = day.then(|| v.days[(id - FIRST) as usize]);
        let holiday = day
            .then(|| v.holiday_days.get((id - FIRST) as usize))
            .flatten()
            .and_then(|d| d.as_ref());
        let sublabel = day
            .then(|| day_sublabel(v, (id - FIRST) as usize))
            .flatten();
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
        rounded(item.hDC, item.rcItem, color, border, s(10));
        if let Some(icon) = button_icon(id) {
            let side = s(19);
            icons::draw(
                item.hDC,
                icon,
                rect(
                    item.rcItem.left + (item.rcItem.right - item.rcItem.left - side) / 2,
                    item.rcItem.top + (item.rcItem.bottom - item.rcItem.top - side) / 2,
                    side,
                    side,
                ),
                t.text,
                color,
            );
            return;
        }
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
                ADD_EVENT => "添加日程".into(),
                FOCUS => "专注计时".into(),
                panel::DETACH => "分离".into(),
                panel::CLOSE => "关闭".into(),
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
            if day && sublabel.is_some() {
                rect(
                    item.rcItem.left,
                    item.rcItem.top + s(2),
                    item.rcItem.right
                        - item.rcItem.left
                        - if holiday.is_some() { s(14) } else { 0 },
                    s(22),
                )
            } else {
                item.rcItem
            },
            v.fonts[0],
            color,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        if day {
            let index = (id - FIRST) as usize;
            if let Some(label) = sublabel {
                text(
                    item.hDC,
                    label,
                    rect(
                        item.rcItem.left,
                        item.rcItem.top + s(24),
                        item.rcItem.right - item.rcItem.left,
                        s(18),
                    ),
                    v.fonts[1],
                    if selected {
                        t.on_accent
                    } else if v.solar_terms.get(index).is_some_and(Option::is_some)
                        && date.is_some_and(|d| d.month() == v.calendar.month.month())
                    {
                        t.accent
                    } else {
                        t.muted
                    },
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
            }
            if let Some(holiday) = holiday {
                let badge = rect(
                    item.rcItem.right - s(15),
                    item.rcItem.top + s(3),
                    s(14),
                    s(15),
                );
                let (background, foreground) = holiday_colors(t, holiday.off);
                rounded(item.hDC, badge, background, background, s(3));
                text(
                    item.hDC,
                    holiday.badge(),
                    badge,
                    v.fonts[1],
                    foreground,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
            }
            if v.markers.get(index) == Some(&true) {
                fill(
                    item.hDC,
                    rect(
                        item.rcItem.left + s(3),
                        item.rcItem.top + s(5),
                        s(3).max(1),
                        s(3).max(1),
                    ),
                    if selected { t.on_accent } else { t.accent },
                );
            }
        }
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
                "{}  {}{}",
                now.format("%-m月%-d日"),
                weekday[now.weekday().num_days_from_monday() as usize],
                solar_terms::on_date(now.date_naive())
                    .map(|term| format!(" · {term}"))
                    .unwrap_or_default()
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
                "{} · {}{}",
                now.format("%Y年"),
                now.format(if v.seconds { "%H:%M:%S" } else { "%H:%M" }),
                if v.detached { " · 已分离" } else { "" }
            ),
            20,
            44,
            380,
            20,
            1,
            t.muted,
        );
        fill(dc, r(20, 72, 680, 1), t.border);
        tx(
            &v.calendar.month.format("%Y年 %-m月").to_string(),
            20,
            84,
            156,
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
                r(20 + i as i32 * 48, 124, 44, 25),
                v.fonts[1],
                t.muted,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        }
        tx(&selected_date_summary(v), 20, 446, 340, 20, 1, t.muted);
        let selected_holiday = v
            .days
            .iter()
            .position(|d| *d == v.calendar.selected)
            .and_then(|i| v.holiday_days.get(i))
            .and_then(|d| d.as_ref());
        tx(
            &selected_holiday
                .map(|d| d.description())
                .unwrap_or_else(|| {
                    if v.holiday_days.iter().any(Option::is_some) {
                        "休 · 放假    班 · 调休补班".into()
                    } else if v.holiday_status.contains("暂无") || v.holiday_status.contains("失败")
                    {
                        v.holiday_status.clone()
                    } else {
                        String::new()
                    }
                }),
            20,
            468,
            340,
            18,
            1,
            if selected_holiday.is_some() {
                t.accent
            } else {
                t.muted
            },
        );
        rounded(dc, r(380, 88, 320, 390), t.field, t.border, s(12));
        tx(&v.city, 400, 106, 276, 28, 0, t.text);
        let status;
        if v.enabled
            && let Some(w) = v.weather.as_ref().and_then(|r| r.weather.as_ref())
        {
            tx(
                &format!("{:.0}°", w.temperature_c),
                398,
                144,
                204,
                66,
                3,
                t.text,
            );
            weather_icon(dc, r(638, 160, 44, 44), t.accent, &w.description);
            tx(&w.description, 402, 218, 276, 25, 0, t.text);
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
                402,
                250,
                276,
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
                402,
                280,
                276,
                22,
                1,
                t.muted,
            );
            fill(dc, r(400, 320, 280, 1), t.border);
            if w.days.is_empty() {
                tx("此来源暂无每日预报", 402, 340, 276, 24, 1, t.muted);
            }
            for (i, day) in w.days.iter().take(3).enumerate() {
                let y = 338 + i as i32 * 36;
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
                tx(&name, 402, y, 48, 24, 1, t.muted);
                tx(&day.description, 456, y, 126, 24, 1, t.text);
                text(
                    dc,
                    &format!("{:.0}° / {:.0}°", day.low_c, day.high_c),
                    r(592, y, 86, 24),
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
            tx("—°", 398, 144, 276, 68, 3, t.muted);
            tx(
                if !v.enabled {
                    "天气已关闭"
                } else if v.busy {
                    "正在获取天气"
                } else {
                    "暂无天气数据"
                },
                402,
                224,
                276,
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
                r(402, 266, 274, 70),
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
        fill(dc, r(20, 488, 680, 1), t.border);
        tx(
            &if v.entry_count > 0 {
                format!("当日安排 · {}", v.entry_count)
            } else {
                "当日安排".into()
            },
            20,
            496,
            if v.editing { 192 } else { 490 },
            30,
            0,
            t.text,
        );
        if v.entries.is_empty() {
            tx(
                "这一天还没有安排，留一点时间给自己。",
                28,
                550,
                if v.editing { 320 } else { 650 },
                28,
                1,
                t.muted,
            );
        }
        fill(dc, r(0, 628, WIDTH, 1), t.border);
        tx(&status, 20, 631, 590, 23, 1, t.muted);
        text(
            dc,
            "轻托盘",
            r(626, 631, 74, 23),
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
pub(super) unsafe fn with_borrowed_view(hwnd: HWND, action: impl FnOnce()) {
    unsafe {
        let cell = &*(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>);
        let _held = cell.borrow_mut();
        action();
    }
}
#[cfg(test)]
mod visual_checks {
    use super::*;
    #[test]
    fn solar_terms_are_visible_without_hover_and_survive_month_navigation() {
        super::super::visual_checks::on_isolated_desktop(|| unsafe {
            let _ = RoInitialize(RO_INIT_SINGLETHREADED);
            let parent = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("Solar term calendar").as_ptr(),
                WS_POPUP,
                0,
                0,
                WIDTH,
                HEIGHT,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            let hwnd = create(parent, rect(0, 0, WIDTH, HEIGHT), false, true);
            bind_holidays(hwnd, Rc::new(RefCell::new(holidays::Catalog::default())));
            let cell = &*(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>);
            let first_button = GetDlgItem(hwnd, FIRST as i32);
            let date = NaiveDate::from_ymd_opt(2026, 11, 7).unwrap();
            restore_selection(hwnd, calendar::Calendar::new(date));
            let index = cell.borrow().days.iter().position(|d| *d == date).unwrap();
            let button = GetDlgItem(hwnd, FIRST as i32 + index as i32);
            assert_eq!(day_sublabel(&cell.borrow(), index), Some("立冬"));
            assert!(window_text(button).contains("立冬"));
            assert!(selected_date_summary(&cell.borrow()).ends_with("立冬"));
            assert_ne!(day_sublabel(&cell.borrow(), index + 1), Some("立冬"));
            let mut config = Config::default();
            config.clock.show_lunar = false;
            config.clock.show_almanac = false;
            update(hwnd, &config, &None, false);
            assert_eq!(day_sublabel(&cell.borrow(), index), Some("立冬"));
            assert_eq!(day_sublabel(&cell.borrow(), index + 1), None);
            SendMessageW(hwnd, WM_COMMAND, NEXT as usize, 0);
            assert!(!window_text(button).contains("立冬"));
            assert_eq!(GetDlgItem(hwnd, FIRST as i32), first_button);
            let qingming = NaiveDate::from_ymd_opt(2026, 4, 5).unwrap();
            restore_selection(hwnd, calendar::Calendar::new(qingming));
            let i = cell
                .borrow()
                .days
                .iter()
                .position(|d| *d == qingming)
                .unwrap();
            assert_eq!(day_sublabel(&cell.borrow(), i), Some("清明"));
            assert_eq!(
                cell.borrow().holiday_days[i].as_ref().unwrap().badge(),
                "休"
            );
            DestroyWindow(parent);
        });
    }
    #[test]
    fn almanac_toggle_and_month_navigation_cancel_pending_hover() {
        super::super::visual_checks::on_isolated_desktop(|| unsafe {
            let _ = RoInitialize(RO_INIT_SINGLETHREADED);
            let parent = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("Almanac calendar").as_ptr(),
                WS_POPUP,
                0,
                0,
                WIDTH,
                HEIGHT,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            let hwnd = create(parent, rect(0, 0, WIDTH, HEIGHT), false, true);
            let cell = &*(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>);
            let button = GetDlgItem(hwnd, FIRST as i32);
            let selected = cell.borrow().calendar.selected;
            SendMessageW(button, WM_MOUSEMOVE, 0, 0);
            assert!(cell.borrow().almanac_hover.is_pending());
            assert_eq!(cell.borrow().calendar.selected, selected);
            let mut config = Config::default();
            config.clock.show_almanac = false;
            update(hwnd, &config, &None, false);
            assert!(!cell.borrow().almanac_hover.is_pending());
            SendMessageW(button, WM_MOUSEMOVE, 0, 0);
            assert!(!cell.borrow().almanac_hover.is_pending());
            config.clock.show_almanac = true;
            config.clock.show_lunar = false;
            update(hwnd, &config, &None, false);
            SendMessageW(button, WM_MOUSEMOVE, 0, 0);
            assert!(
                cell.borrow().almanac_hover.is_pending(),
                "almanac must be independent of date cell lunar labels"
            );
            SendMessageW(hwnd, WM_COMMAND, NEXT as usize, 0);
            assert!(!cell.borrow().almanac_hover.is_pending());
            assert_eq!(
                GetDlgItem(hwnd, FIRST as i32),
                button,
                "hover support recreated date cells"
            );
            SendMessageW(button, WM_MOUSEMOVE, 0, 0);
            SendMessageW(button, WM_LBUTTONDOWN, 0, 0);
            assert!(!cell.borrow().almanac_hover.is_pending());
            DestroyWindow(parent);
        });
    }
    unsafe fn flush_editor(hwnd: HWND) {
        unsafe {
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(
                &mut message,
                hwnd,
                quick_agenda::CLOSED,
                quick_agenda::SAVED,
                PM_REMOVE,
            ) != 0
            {
                DispatchMessageW(&message);
            }
        }
    }
    #[test]
    fn inline_editor_saves_six_kinds_without_opening_settings_and_preserves_drafts() {
        super::super::visual_checks::on_isolated_desktop(|| unsafe {
            let _ = RoInitialize(RO_INIT_SINGLETHREADED);
            let path = std::env::temp_dir().join(format!(
                "litetray-inline-{}-{}.json",
                std::process::id(),
                Utc::now().timestamp_nanos_opt().unwrap()
            ));
            let store = Rc::new(RefCell::new(organizer::Store::open(&path)));
            let parent = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("Inline fixture").as_ptr(),
                WS_POPUP,
                0,
                0,
                WIDTH,
                HEIGHT,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!parent.is_null());
            let hwnd = create(parent, rect(0, 0, WIDTH, HEIGHT), false, true);
            bind_store(hwnd, store.clone());
            bind_holidays(hwnd, Rc::new(RefCell::new(holidays::Catalog::default())));
            let date = NaiveDate::from_ymd_opt(2027, 2, 14).unwrap();
            restore_selection(hwnd, calendar::Calendar::new(date));
            for (index, kind) in organizer::Kind::ALL.iter().enumerate() {
                SendMessageW(hwnd, WM_COMMAND, ADD_EVENT as usize, 0);
                let editor = GetDlgItem(hwnd, quick_agenda::ID as i32);
                assert!(!editor.is_null());
                assert!(editing(hwnd));
                set_text(GetDlgItem(editor, 4610), &format!("{}回归", kind.label()));
                set_text(GetDlgItem(editor, 4615), "备注不得因类型或主题切换丢失");
                SendMessageW(editor, WM_COMMAND, 4600 + index, 0);
                let title = GetDlgItem(editor, 4610);
                refresh_theme(hwnd, true);
                refresh_theme(hwnd, false);
                assert_eq!(GetDlgItem(editor, 4610), title);
                assert_eq!(window_text(title), format!("{}回归", kind.label()));
                // These windows never leave the hidden test surface.
                assert_eq!(IsWindowVisible(parent), 0);
                SendMessageW(editor, WM_COMMAND, 4620, 0);
                flush_editor(hwnd);
                assert!(!editing(hwnd));
                assert_eq!(store.borrow().data.entries.len(), index + 1);
                let saved = organizer::Store::open(&path);
                let entry = &saved.data.entries[index];
                assert_eq!(entry.kind, *kind);
                assert_eq!(entry.date, date);
                assert!(entry.notes.contains("不得"));
                if matches!(
                    kind,
                    organizer::Kind::Birthday | organizer::Kind::Anniversary
                ) {
                    assert_eq!(entry.repeat, organizer::Repeat::Yearly);
                }
            }
            let cell = &*(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>);
            assert_eq!(cell.borrow().entry_count, 6);
            SendMessageW(hwnd, WM_COMMAND, LATER as usize, 0);
            assert_eq!(cell.borrow().entry_offset, 3);
            let edited_id = cell.borrow().entries[0].0;
            SendMessageW(hwnd, WM_COMMAND, FIRST_ENTRY as usize, 0);
            let editor = GetDlgItem(hwnd, quick_agenda::ID as i32);
            set_text(GetDlgItem(editor, 4610), "弹层内修改");
            let target = date.succ_opt().unwrap();
            let index = cell
                .borrow()
                .days
                .iter()
                .position(|d| *d == target)
                .unwrap();
            SendMessageW(hwnd, WM_COMMAND, (FIRST + index as u16) as usize, 0);
            assert_eq!(window_text(GetDlgItem(editor, 4611)), target.to_string());
            set_text(GetDlgItem(editor, 4612), "25:90");
            SendMessageW(editor, WM_COMMAND, 4620, 0);
            flush_editor(hwnd);
            assert!(
                editing(hwnd),
                "invalid input must keep the editor and draft"
            );
            assert!(
                !store
                    .borrow()
                    .data
                    .entries
                    .iter()
                    .any(|e| e.title == "弹层内修改")
            );
            set_text(GetDlgItem(editor, 4612), "10:30");
            SendMessageW(editor, WM_COMMAND, 4620, 0);
            flush_editor(hwnd);
            let changed = store
                .borrow()
                .data
                .entries
                .iter()
                .find(|e| e.id == edited_id)
                .unwrap()
                .clone();
            assert_eq!(changed.title, "弹层内修改");
            assert_eq!(changed.date, target);
            edit(hwnd, Some(edited_id));
            SendMessageW(editor, WM_COMMAND, 4621, 0);
            assert!(
                store
                    .borrow()
                    .data
                    .entries
                    .iter()
                    .any(|e| e.id == edited_id),
                "first delete click only arms confirmation"
            );
            SendMessageW(editor, WM_COMMAND, 4621, 0);
            flush_editor(hwnd);
            assert!(
                !store
                    .borrow()
                    .data
                    .entries
                    .iter()
                    .any(|e| e.id == edited_id)
            );
            let mut message: MSG = std::mem::zeroed();
            assert_eq!(
                PeekMessageW(
                    &mut message,
                    parent,
                    agenda::OPEN_DATE,
                    agenda::OPEN_ENTRY,
                    PM_REMOVE
                ),
                0,
                "quick editing must not launch the full organizer"
            );
            assert_eq!(IsWindowVisible(parent), 0);
            DestroyWindow(parent);
            super::super::visual_checks::assert_external_foreground();
            std::fs::remove_file(path).unwrap();
        });
    }

    #[test]
    #[ignore = "Renders inline editor with hidden native controls"]
    fn render_inline_editor_artifacts() {
        let output = std::path::PathBuf::from(std::env::var_os("LITETRAY_RENDER_DIR").unwrap());
        std::fs::create_dir_all(&output).unwrap();
        unsafe {
            let _ = RoInitialize(RO_INIT_SINGLETHREADED);
            for (dark, factor) in [(false, 1.), (true, 1.), (false, 1.5), (true, 2.)] {
                let s = |n: i32| (n as f64 * factor).round() as i32;
                let parent = CreateWindowExW(
                    0,
                    wide("STATIC").as_ptr(),
                    wide("Inline render").as_ptr(),
                    WS_POPUP,
                    0,
                    0,
                    s(WIDTH),
                    s(HEIGHT),
                    null_mut(),
                    null_mut(),
                    GetModuleHandleW(null()),
                    null(),
                );
                let hwnd = create(parent, rect(0, 0, s(WIDTH), s(HEIGHT)), dark, true);
                let date = NaiveDate::from_ymd_opt(2027, 2, 14).unwrap();
                restore_selection(hwnd, calendar::Calendar::new(date));
                let store = Rc::new(RefCell::new(organizer::Store::open(
                    &output.join("unused-inline.json"),
                )));
                for (kind, title) in [
                    (organizer::Kind::Todo, "整理本周计划"),
                    (organizer::Kind::Event, "和家人一起吃晚饭"),
                ] {
                    let mut entry = organizer::Entry::new(kind, date);
                    entry.title = title.into();
                    store.borrow_mut().data.put(entry).unwrap();
                }
                bind_store(hwnd, store);
                bind_holidays(hwnd, Rc::new(RefCell::new(holidays::Catalog::default())));
                edit(hwnd, None);
                let editor = GetDlgItem(hwnd, quick_agenda::ID as i32);
                set_text(GetDlgItem(editor, 4610), "整理下周的工作计划");
                set_text(GetDlgItem(editor, 4612), "09:30");
                set_text(
                    GetDlgItem(editor, 4615),
                    "列出三个重要目标，留出专注的时间。",
                );
                let mut child = GetWindow(editor, GW_CHILD);
                while !child.is_null() {
                    let mut b: RECT = std::mem::zeroed();
                    GetWindowRect(child, &mut b);
                    MapWindowPoints(null_mut(), editor, &mut b as *mut RECT as *mut POINT, 2);
                    let mut client: RECT = std::mem::zeroed();
                    GetClientRect(editor, &mut client);
                    assert!(
                        b.left >= 0
                            && b.top >= 0
                            && b.right <= client.right + 1
                            && b.bottom <= client.bottom + 1,
                        "editor control outside bounds: {}",
                        GetDlgCtrlID(child)
                    );
                    child = GetWindow(child, GW_HWNDNEXT);
                }
                super::super::visual_checks::write_window(
                    hwnd,
                    &output.join(format!(
                        "panel-{}-{}-editor.bmp",
                        if dark { "dark" } else { "light" },
                        (factor * 100.) as u32
                    )),
                );
                DestroyWindow(parent);
            }
            super::super::visual_checks::assert_external_foreground();
        }
    }

    #[test]
    fn holiday_updates_keep_native_dates_without_source_button() {
        super::super::visual_checks::on_isolated_desktop(|| unsafe {
            let parent = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("Holiday regression").as_ptr(),
                WS_POPUP,
                0,
                0,
                WIDTH,
                HEIGHT,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!parent.is_null());
            let hwnd = create(parent, rect(0, 0, WIDTH, HEIGHT), false, true);
            let date = NaiveDate::from_ymd_opt(2027, 2, 14).unwrap();
            restore_selection(hwnd, calendar::Calendar::new(date));
            let catalog = Rc::new(RefCell::new(holidays::Catalog::default()));
            bind_holidays(hwnd, catalog.clone());
            let cell = &*(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>);
            let before = cell.borrow().buttons.clone();
            let fonts = cell.borrow().fonts;
            let index = cell.borrow().days.iter().position(|d| *d == date).unwrap();
            let selected = GetDlgItem(hwnd, (FIRST + index as u16) as i32);
            assert!(window_text(selected).contains("春节 · 调休补班"));
            let mut config = Config::default();
            config.clock.show_lunar = false;
            update(hwnd, &config, &None, false);
            assert_eq!(
                cell.borrow().holiday_days[index].as_ref().unwrap().badge(),
                "班"
            );
            let mut data = catalog.borrow().seed(2027).unwrap();
            data.days.iter_mut().find(|d| d.date == date).unwrap().off = true;
            catalog.borrow_mut().apply(
                2027,
                holidays::Outcome {
                    data: Some(data),
                    next_check: Utc::now(),
                    note: "更新回归".into(),
                },
            );
            RedrawWindow(
                hwnd,
                null(),
                null_mut(),
                RDW_VALIDATE | RDW_NOERASE | RDW_ALLCHILDREN,
            );
            update(hwnd, &config, &None, false);
            assert!(window_text(selected).contains("春节 · 放假"));
            assert_eq!(cell.borrow().calendar.selected, date);
            assert_eq!(
                cell.borrow().buttons,
                before,
                "download must not recreate the calendar"
            );
            assert_eq!(cell.borrow().fonts, fonts);
            let mut region: RECT = std::mem::zeroed();
            let dirty = (FIRST..FIRST + 42)
                .filter(|id| GetUpdateRect(GetDlgItem(hwnd, *id as i32), &mut region, 0) != 0)
                .count();
            // Hidden controls may consume their update region synchronously
            // when SetWindowText changes the accessible date description.
            assert!(
                dirty <= 1,
                "unrelated holiday dates were invalidated: {dirty}"
            );
            assert!(
                GetDlgItem(hwnd, 1105).is_null(),
                "source links must not crowd the calendar toolbar"
            );
            assert_eq!(IsWindowVisible(parent), 0);
            DestroyWindow(parent);
            super::super::visual_checks::assert_external_foreground();
        });
    }

    unsafe extern "system" fn count_destroyed(
        hwnd: HWND,
        msg: u32,
        w: WPARAM,
        l: LPARAM,
        id: usize,
        data: usize,
    ) -> LRESULT {
        unsafe {
            if msg == WM_NCDESTROY {
                let count = &*(data as *const Cell<usize>);
                count.set(count.get() + 1);
                RemoveWindowSubclass(hwnd, Some(count_destroyed), id);
            }
            DefSubclassProc(hwnd, msg, w, l)
        }
    }

    #[test]
    fn changing_date_preserves_calendar_controls_and_weather() {
        // Each Rust test has its own STA thread. WinRT's process-wide factory
        // cache can outlive an exited apartment; use the same fresh-process,
        // non-input desktop harness as the other native window tests.
        super::super::visual_checks::on_isolated_desktop(|| {
            for dark in [false, true] {
                for flyout in [false, true] {
                    unsafe {
                        check_date_repaints(dark, flyout);
                    }
                }
            }
        });
    }

    unsafe fn check_date_repaints(dark: bool, flyout: bool) {
        unsafe {
            let _ = RoInitialize(RO_INIT_SINGLETHREADED);

            let parent = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("Calendar repaint regression").as_ptr(),
                WS_POPUP,
                0,
                0,
                WIDTH,
                HEIGHT,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!parent.is_null());
            let hwnd = create(parent, rect(0, 0, WIDTH, HEIGHT), dark, flyout);
            let date = NaiveDate::from_ymd_opt(2026, 11, 7).unwrap();
            restore_selection(hwnd, calendar::Calendar::new(date));
            let store = Rc::new(RefCell::new(organizer::Store::open(
                &std::env::temp_dir().join(format!(
                    "litetray-calendar-{}-{}.json",
                    std::process::id(),
                    Utc::now().timestamp_nanos_opt().unwrap()
                )),
            )));
            let mut entry = organizer::Entry::new(organizer::Kind::Event, date.succ_opt().unwrap());
            entry.title = "Date-switch regression fixture".into();
            store.borrow_mut().data.put(entry).unwrap();
            bind_store(hwnd, store);
            bind_holidays(hwnd, Rc::new(RefCell::new(holidays::Catalog::default())));
            update(hwnd, &Config::default(), &None, false);
            let cell = &*(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>);
            let before = cell.borrow().buttons.clone();
            let fonts = cell.borrow().fonts;
            let destroyed = Cell::new(0usize);
            for child in &before {
                SetWindowSubclass(
                    *child,
                    Some(count_destroyed),
                    91,
                    &destroyed as *const _ as usize,
                );
            }
            RedrawWindow(
                hwnd,
                null(),
                null_mut(),
                RDW_VALIDATE | RDW_NOERASE | RDW_ALLCHILDREN,
            );
            let index = cell
                .borrow()
                .days
                .iter()
                .position(|d| *d == date.succ_opt().unwrap())
                .unwrap();
            SendMessageW(
                GetDlgItem(hwnd, (FIRST + index as u16) as i32),
                BM_CLICK,
                0,
                0,
            );
            let selected = cell.borrow().calendar.selected;
            let destroyed_on_selection = destroyed.get();
            let mut region: RECT = std::mem::zeroed();
            let weather_invalidated =
                GetUpdateRect(GetDlgItem(hwnd, REFRESH as i32), &mut region, 0) != 0;
            let dirty_dates = (FIRST..FIRST + 42)
                .filter(|id| GetUpdateRect(GetDlgItem(hwnd, *id as i32), &mut region, 0) != 0)
                .count();
            let damaged = CreateRectRgn(0, 0, 0, 0);
            GetUpdateRgn(hwnd, damaged, 0);
            let weather_damaged = PtInRegion(damaged, 500, 300) != 0;
            DeleteObject(damaged);
            assert_eq!(
                cell.borrow().entries.len(),
                1,
                "selected-day agenda must still update"
            );
            // Repeated clicks, keyboard arrows across a year, month navigation,
            // returning to today, and clock ticks all keep native objects alive.
            for n in 0..60 {
                let target = if n % 2 == 0 {
                    date
                } else {
                    date.succ_opt().unwrap()
                };
                let index = cell
                    .borrow()
                    .days
                    .iter()
                    .position(|d| *d == target)
                    .unwrap();
                SendMessageW(
                    GetDlgItem(hwnd, (FIRST + index as u16) as i32),
                    BM_CLICK,
                    0,
                    0,
                );
            }
            restore_selection(
                hwnd,
                calendar::Calendar::new(NaiveDate::from_ymd_opt(2026, 12, 31).unwrap()),
            );
            let index = cell
                .borrow()
                .days
                .iter()
                .position(|d| *d == NaiveDate::from_ymd_opt(2026, 12, 31).unwrap())
                .unwrap();
            SendMessageW(
                GetDlgItem(hwnd, (FIRST + index as u16) as i32),
                WM_KEYDOWN,
                VK_RIGHT as usize,
                0,
            );
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(&mut message, hwnd, SELECT_DAY, SELECT_DAY, PM_REMOVE) != 0 {
                DispatchMessageW(&message);
            }
            assert_eq!(
                cell.borrow().calendar.selected,
                NaiveDate::from_ymd_opt(2027, 1, 1).unwrap()
            );
            SendMessageW(GetDlgItem(hwnd, PREV as i32), BM_CLICK, 0, 0);
            assert_eq!(
                cell.borrow().calendar.month,
                NaiveDate::from_ymd_opt(2026, 12, 1).unwrap()
            );
            SendMessageW(GetDlgItem(hwnd, NEXT as i32), BM_CLICK, 0, 0);
            SendMessageW(GetDlgItem(hwnd, TODAY as i32), BM_CLICK, 0, 0);
            assert_eq!(cell.borrow().calendar.selected, Local::now().date_naive());
            RedrawWindow(
                hwnd,
                null(),
                null_mut(),
                RDW_VALIDATE | RDW_NOERASE | RDW_ALLCHILDREN,
            );
            update(hwnd, &Config::default(), &None, false);
            let dirty_after_tick = (FIRST..FIRST + 42)
                .any(|id| GetUpdateRect(GetDlgItem(hwnd, id as i32), &mut region, 0) != 0);
            let destroyed_after_all = destroyed.get();
            assert_eq!(cell.borrow().fonts, fonts, "date switches must reuse fonts");
            assert_eq!(
                cell.borrow().buttons,
                before,
                "date switches must preserve all native button identities"
            );
            for child in &before {
                RemoveWindowSubclass(*child, Some(count_destroyed), 91);
            }
            DestroyWindow(parent);
            super::super::visual_checks::assert_external_foreground();
            assert_eq!(selected, date.succ_opt().unwrap());
            assert_eq!(
                destroyed_on_selection, 0,
                "a date click destroyed native calendar controls, exposing the whole background between frames"
            );
            assert_eq!(
                destroyed_after_all, 0,
                "calendar navigation rebuilt native controls"
            );
            assert!(
                dirty_dates <= 2,
                "same-month selection repainted {dirty_dates} date cells"
            );
            assert!(
                !weather_invalidated,
                "a date click invalidated the unrelated weather control"
            );
            assert!(
                !weather_damaged,
                "a date click invalidated the weather background"
            );
            assert!(!dirty_after_tick, "clock tick repainted calendar cells");
        }
    }

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
                let _ = RoInitialize(RO_INIT_SINGLETHREADED);
                let s = |n: i32| (n as f64 * factor).round() as i32;
                let today = Local::now().date_naive();
                let calendar = calendar::Calendar::new(if empty {
                    NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()
                } else {
                    NaiveDate::from_ymd_opt(2027, 2, 14).unwrap()
                });
                let holidays = holidays::Catalog::default();
                let now = Utc::now();
                let v = View {
                    calendar,
                    displayed: Some(calendar),
                    today,
                    days: calendar.grid(),
                    buttons: vec![],
                    city: "示例 · 深圳".into(),
                    enabled: true,
                    busy: false,
                    lunar: true,
                    almanac: true,
                    almanac_hover: Default::default(),
                    seconds: true,
                    flyout: true,
                    detached: false,
                    dark,
                    factor,
                    store: None,
                    revision: 0,
                    entry_count: if empty { 0 } else { 2 },
                    entry_offset: 0,
                    editor: null_mut(),
                    editing: false,
                    tips: None,
                    holiday_days: calendar
                        .grid()
                        .iter()
                        .map(|d| holidays.day(*d).cloned())
                        .collect(),
                    holiday_status: holidays.status(calendar.month.year()),
                    holiday_revision: holidays.revision,
                    holidays: Some(Rc::new(RefCell::new(holidays))),
                    lunar_labels: calendar
                        .grid()
                        .iter()
                        .map(|d| lunar::for_date(*d).map(|l| l.short).unwrap_or_default())
                        .collect(),
                    solar_terms: calendar
                        .grid()
                        .iter()
                        .map(|d| solar_terms::on_date(*d))
                        .collect(),
                    markers: calendar
                        .grid()
                        .iter()
                        .map(|d| !empty && (*d == calendar.selected || d.day() == 6))
                        .collect(),
                    entries: if empty {
                        vec![]
                    } else {
                        vec![
                            (1, "阅读半小时".into(), "19:30 · 待办".into()),
                            (2, "家人的生日".into(), "全天 · 生日".into()),
                        ]
                    },
                    fonts: [
                        make_font(s(14), 400, &design::font_face()),
                        make_font(s(11), 400, &design::font_face()),
                        make_font(s(19), 600, &design::font_face()),
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
                for button in button_specs(&v).into_iter().filter(|b| b.visible) {
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
