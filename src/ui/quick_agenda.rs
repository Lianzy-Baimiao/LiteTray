//! Compact durable editor inside the calendar. A hidden flyout keeps its
//! draft; saving notifies both calendars and the full organizer page.
use super::*;
use chrono::{NaiveDate, NaiveTime};
use organizer::{Entry, Kind, Repeat, Store};
pub(super) const ID: u16 = 970;
pub(super) const CLOSED: u32 = WM_APP + 93;
pub(super) const SAVED: u32 = WM_APP + 94;
const CLASS: &str = "LiteTray.QuickAgenda.1";
const KIND: u16 = 4600;
const TITLE: u16 = 4610;
const DATE: u16 = 4611;
const TIME: u16 = 4612;
const REPEAT: u16 = 4613;
const REMIND: u16 = 4614;
const NOTES: u16 = 4615;
const DONE: u16 = 4616;
const ENABLED: u16 = 4617;
const SAVE: u16 = 4620;
const REMOVE: u16 = 4621;
const CLOSE: u16 = 4622;
const LEADS: [Option<u32>; 8] = [
    None,
    Some(0),
    Some(5),
    Some(15),
    Some(30),
    Some(60),
    Some(1440),
    Some(10080),
];
const WIDTH: i32 = 324;
const HEIGHT: i32 = 540;
struct View {
    hwnd: HWND,
    store: Rc<RefCell<Store>>,
    entry: Entry,
    baseline: String,
    dark: bool,
    factor: f64,
    fonts: [HFONT; 3],
    brush: HBRUSH,
    field_brush: HBRUSH,
    controls: Vec<(HWND, RECT)>,
    message: String,
    deleting: bool,
    tips: Option<icons::Tips>,
}
impl Drop for View {
    fn drop(&mut self) {
        unsafe {
            for font in self.fonts {
                DeleteObject(font);
            }
            DeleteObject(self.brush);
            DeleteObject(self.field_brush);
        }
    }
}
unsafe fn state(hwnd: HWND) -> Option<&'static RefCell<View>> {
    unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>).as_ref() }
}
pub(super) unsafe fn create(parent: HWND, store: Rc<RefCell<Store>>, dark: bool) -> HWND {
    unsafe {
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: GetModuleHandleW(null()),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            lpszClassName: wide(CLASS).as_ptr(),
            ..std::mem::zeroed()
        });
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT | WS_EX_COMPOSITED,
            wide(CLASS).as_ptr(),
            wide("日程快速编辑").as_ptr(),
            WS_CHILD | WS_CLIPCHILDREN,
            0,
            0,
            WIDTH,
            HEIGHT,
            parent,
            ID as usize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        if hwnd.is_null() {
            return hwnd;
        }
        let mut view = View {
            hwnd,
            store,
            entry: Entry::new(Kind::Event, Local::now().date_naive()),
            baseline: String::new(),
            dark,
            factor: 0.,
            fonts: [null_mut(); 3],
            brush: null_mut(),
            field_brush: null_mut(),
            controls: vec![],
            message: String::new(),
            deleting: false,
            tips: None,
        };
        for (i, kind) in Kind::ALL.iter().enumerate() {
            view.control(
                "BUTTON",
                kind.label(),
                KIND + i as u16,
                BS_OWNERDRAW as u32,
                design::rect(16 + i as i32 * 49, 54, 48, 32),
            );
        }
        view.control(
            "BUTTON",
            "关闭编辑",
            CLOSE,
            BS_OWNERDRAW as u32,
            design::rect(276, 12, 32, 32),
        );
        view.control(
            "EDIT",
            "",
            TITLE,
            ES_AUTOHSCROLL as u32,
            design::rect(18, 126, 288, 32),
        );
        view.control(
            "EDIT",
            "",
            DATE,
            ES_AUTOHSCROLL as u32,
            design::rect(18, 196, 160, 32),
        );
        view.control(
            "EDIT",
            "",
            TIME,
            ES_AUTOHSCROLL as u32,
            design::rect(194, 196, 112, 32),
        );
        view.combo(
            REPEAT,
            &Repeat::ALL.map(Repeat::label),
            design::rect(18, 266, 136, 200),
        );
        view.combo(
            REMIND,
            &[
                "不提醒",
                "准时",
                "提前 5 分钟",
                "提前 15 分钟",
                "提前 30 分钟",
                "提前 1 小时",
                "提前 1 天",
                "提前 7 天",
            ],
            design::rect(170, 266, 136, 200),
        );
        view.control(
            "EDIT",
            "",
            NOTES,
            ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_WANTRETURN as u32,
            design::rect(18, 336, 288, 68),
        );
        view.control(
            "BUTTON",
            "已完成",
            DONE,
            BS_OWNERDRAW as u32,
            design::rect(16, 422, 140, 28),
        );
        view.control(
            "BUTTON",
            "启用",
            ENABLED,
            BS_OWNERDRAW as u32,
            design::rect(170, 422, 138, 28),
        );
        view.control(
            "BUTTON",
            "删除",
            REMOVE,
            BS_OWNERDRAW as u32,
            design::rect(16, 494, 104, 34),
        );
        view.control(
            "BUTTON",
            "保存",
            SAVE,
            BS_OWNERDRAW as u32,
            design::rect(164, 494, 144, 34),
        );
        SendMessageW(
            view.field(TITLE),
            EM_SETCUEBANNER,
            0,
            wide("记录一件重要的事").as_ptr() as isize,
        );
        SendMessageW(
            view.field(TIME),
            EM_SETCUEBANNER,
            0,
            wide("全天").as_ptr() as isize,
        );
        SendMessageW(
            view.field(NOTES),
            EM_SETCUEBANNER,
            0,
            wide("补充备注（可选）").as_ptr() as isize,
        );
        let mut tips = icons::Tips::new(hwnd);
        tips.add(hwnd, view.field(CLOSE), "关闭编辑（未保存的修改会取消）");
        view.tips = Some(tips);
        view.arrange(1., dark);
        SetWindowLongPtrW(
            hwnd,
            GWLP_USERDATA,
            Box::into_raw(Box::new(RefCell::new(view))) as isize,
        );
        hwnd
    }
}
pub(super) unsafe fn open(hwnd: HWND, entry: Entry) -> bool {
    unsafe {
        let Some(cell) = state(hwnd) else {
            return false;
        };
        let Ok(mut v) = cell.try_borrow_mut() else {
            return false;
        };
        if GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_VISIBLE != 0 {
            if v.entry.id == entry.id {
                return true;
            }
            if v.signature() != v.baseline {
                v.message = "请先保存或关闭当前编辑，再选择其他事项".into();
                InvalidateRect(hwnd, null(), 0);
                return false;
            }
        }
        v.entry = entry;
        v.load();
        v.baseline = v.signature();
        v.message = String::new();
        v.deleting = false;
        drop(v);
        ShowWindow(hwnd, SW_SHOWNA);
        InvalidateRect(hwnd, null(), 0);
        if IsWindowVisible(GetAncestor(hwnd, GA_ROOT)) != 0 {
            SetFocus(GetDlgItem(hwnd, TITLE as i32));
        }
        true
    }
}
pub(super) unsafe fn set_date(hwnd: HWND, date: NaiveDate) {
    unsafe {
        if let Some(cell) = state(hwnd)
            && let Ok(v) = cell.try_borrow()
        {
            set_text(v.field(DATE), &date.to_string());
        }
    }
}
pub(super) unsafe fn resize(hwnd: HWND, bounds: RECT, dark: bool) {
    unsafe {
        if hwnd.is_null() {
            return;
        }
        SetWindowPos(
            hwnd,
            HWND_TOP,
            bounds.left,
            bounds.top,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            SWP_NOACTIVATE,
        );
        if let Some(cell) = state(hwnd)
            && let Ok(mut v) = cell.try_borrow_mut()
        {
            let factor = (bounds.right - bounds.left) as f64 / WIDTH as f64;
            if (factor - v.factor).abs() > 0.001 || dark != v.dark {
                v.arrange(factor, dark);
            }
        }
    }
}
pub(super) unsafe fn dismiss(hwnd: HWND) -> bool {
    unsafe {
        if hwnd.is_null() || GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_VISIBLE == 0 {
            return false;
        }
        ShowWindow(hwnd, SW_HIDE);
        PostMessageW(GetParent(hwnd), CLOSED, 0, 0);
        true
    }
}
impl View {
    fn s(&self, n: i32) -> i32 {
        (n as f64 * self.factor).round() as i32
    }
    fn field(&self, id: u16) -> HWND {
        unsafe { GetDlgItem(self.hwnd, id as i32) }
    }
    fn value(&self, id: u16) -> String {
        unsafe { window_text(self.field(id)) }
    }
    fn choice(&self, id: u16) -> usize {
        unsafe { SendMessageW(self.field(id), CB_GETCURSEL, 0, 0).max(0) as usize }
    }
    fn signature(&self) -> String {
        serde_json::json!([
            self.entry.kind,
            self.value(TITLE),
            self.value(DATE),
            self.value(TIME),
            self.choice(REPEAT),
            self.choice(REMIND),
            self.value(NOTES),
            self.entry.done,
            self.entry.enabled
        ])
        .to_string()
    }
    unsafe fn control(
        &mut self,
        class: &str,
        label: &str,
        id: u16,
        style: u32,
        bounds: RECT,
    ) -> HWND {
        unsafe {
            let hwnd = CreateWindowExW(
                0,
                wide(class).as_ptr(),
                wide(label).as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | style,
                bounds.left,
                bounds.top,
                bounds.right - bounds.left,
                bounds.bottom - bounds.top,
                self.hwnd,
                id as usize as HMENU,
                GetModuleHandleW(null()),
                null(),
            );
            if class == "EDIT" {
                SetPropW(hwnd, wide("LiteTray.Field").as_ptr(), 1usize as HANDLE);
                SendMessageW(
                    hwnd,
                    EM_SETLIMITTEXT,
                    if id == NOTES { 2000 } else { 120 },
                    0,
                );
                SendMessageW(
                    hwnd,
                    EM_SETMARGINS,
                    (EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize,
                    (8 << 16) | 8,
                );
            }
            if class == "BUTTON" {
                SetWindowSubclass(hwnd, Some(design::hover_proc), 2, 0);
            }
            self.controls.push((hwnd, bounds));
            hwnd
        }
    }
    unsafe fn combo(&mut self, id: u16, items: &[&str], bounds: RECT) {
        unsafe {
            let hwnd = self.control(
                "COMBOBOX",
                "",
                id,
                CBS_DROPDOWNLIST as u32
                    | CBS_OWNERDRAWFIXED as u32
                    | CBS_HASSTRINGS as u32
                    | WS_VSCROLL,
                bounds,
            );
            for item in items {
                SendMessageW(hwnd, CB_ADDSTRING, 0, wide(item).as_ptr() as isize);
            }
        }
    }
    unsafe fn arrange(&mut self, factor: f64, dark: bool) {
        unsafe {
            self.factor = factor;
            self.dark = dark;
            let old = self.fonts;
            let face = design::font_face();
            self.fonts = [
                make_font(self.s(14), 400, &face),
                make_font(self.s(12), 400, &face),
                make_font(self.s(18), 600, &face),
            ];
            let t = design::Theme::new(dark);
            DeleteObject(self.brush);
            DeleteObject(self.field_brush);
            self.brush = CreateSolidBrush(t.surface);
            self.field_brush = CreateSolidBrush(t.field);
            for (hwnd, bounds) in &self.controls {
                let id = GetDlgCtrlID(*hwnd) as u16;
                // Keep the native one-line edit at its text height, centered
                // inside the larger field surface painted by the parent.
                let single_line = matches!(id, TITLE | DATE | TIME);
                SetWindowPos(
                    *hwnd,
                    null_mut(),
                    self.s(bounds.left),
                    self.s(bounds.top + if single_line { 6 } else { 0 }),
                    self.s(bounds.right - bounds.left),
                    self.s(if single_line {
                        20
                    } else {
                        bounds.bottom - bounds.top
                    }),
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                SendMessageW(
                    *hwnd,
                    WM_SETFONT,
                    self.fonts[usize::from((KIND..KIND + 6).contains(&id))] as usize,
                    0,
                );
                if matches!(id, REPEAT | REMIND) {
                    design::style_combo(*hwnd, dark, (96. * factor) as u32);
                }
                if matches!(id, TITLE | DATE | TIME | NOTES) {
                    widgets::style_edit(*hwnd, dark, (96. * factor) as u32);
                    let margin = self.s(8) as isize;
                    SendMessageW(
                        *hwnd,
                        EM_SETMARGINS,
                        (EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize,
                        (margin << 16) | margin,
                    );
                }
            }
            for font in old {
                if !font.is_null() {
                    DeleteObject(font);
                }
            }
            InvalidateRect(self.hwnd, null(), 0);
        }
    }
    unsafe fn load(&self) {
        unsafe {
            for (id, value) in [
                (TITLE, self.entry.title.clone()),
                (DATE, self.entry.date.to_string()),
                (
                    TIME,
                    self.entry
                        .time
                        .map(|t| t.format("%H:%M").to_string())
                        .unwrap_or_default(),
                ),
                (NOTES, self.entry.notes.clone()),
            ] {
                set_text(self.field(id), &value);
            }
            SendMessageW(
                self.field(REPEAT),
                CB_SETCURSEL,
                Repeat::ALL
                    .iter()
                    .position(|r| *r == self.entry.repeat)
                    .unwrap(),
                0,
            );
            SendMessageW(
                self.field(REMIND),
                CB_SETCURSEL,
                LEADS
                    .iter()
                    .position(|r| *r == self.entry.remind_minutes)
                    .unwrap_or(1),
                0,
            );
            self.update_buttons();
        }
    }
    unsafe fn update_buttons(&self) {
        unsafe {
            ShowWindow(
                self.field(DONE),
                if self.entry.kind == Kind::Todo {
                    SW_SHOWNA
                } else {
                    SW_HIDE
                },
            );
            set_text(
                self.field(REMOVE),
                if self.entry.id == 0 {
                    "取消"
                } else if self.deleting {
                    "确认删除"
                } else {
                    "删除"
                },
            );
            set_text(
                self.field(SAVE),
                &format!("保存{}", self.entry.kind.label()),
            );
            for (hwnd, _) in &self.controls {
                if !matches!(
                    GetDlgCtrlID(*hwnd) as u16,
                    TITLE | DATE | TIME | NOTES | REPEAT | REMIND
                ) {
                    InvalidateRect(*hwnd, null(), 0);
                }
            }
        }
    }
    fn read(&self) -> Result<Entry, String> {
        let mut entry = self.entry.clone();
        entry.title = self.value(TITLE);
        entry.notes = self.value(NOTES);
        entry.date = NaiveDate::parse_from_str(self.value(DATE).trim(), "%Y-%m-%d")
            .map_err(|_| "日期格式：2027-02-06，也可点选左侧日期")?;
        entry.time = if self.value(TIME).trim().is_empty() {
            None
        } else {
            Some(
                NaiveTime::parse_from_str(self.value(TIME).trim(), "%H:%M")
                    .map_err(|_| "时间格式：08:30；留空表示全天")?,
            )
        };
        entry.repeat = Repeat::ALL[self.choice(REPEAT).min(4)];
        entry.remind_minutes = LEADS[self.choice(REMIND).min(7)];
        entry.done = entry.kind == Kind::Todo && entry.done;
        entry.validate()?;
        Ok(entry)
    }
    unsafe fn command(&mut self, id: u16) -> Result<(), String> {
        unsafe {
            if (KIND..KIND + 6).contains(&id) {
                let kind = Kind::ALL[(id - KIND) as usize];
                if self.entry.id == 0 && kind != self.entry.kind {
                    let repeat = if matches!(kind, Kind::Birthday | Kind::Anniversary) {
                        4
                    } else if kind == Kind::Alarm {
                        1
                    } else {
                        0
                    };
                    SendMessageW(self.field(REPEAT), CB_SETCURSEL, repeat, 0);
                    if kind == Kind::Alarm && self.value(TIME).is_empty() {
                        set_text(self.field(TIME), "08:00");
                    }
                    if kind == Kind::Alarm && self.choice(REMIND) == 0 {
                        SendMessageW(self.field(REMIND), CB_SETCURSEL, 1, 0);
                    }
                }
                self.entry.kind = kind;
                self.deleting = false;
                self.message = String::new();
                self.update_buttons();
                InvalidateRect(self.hwnd, null(), 0);
                return Ok(());
            }
            match id {
                DONE => {
                    self.entry.done = !self.entry.done;
                    self.update_buttons();
                }
                ENABLED => {
                    self.entry.enabled = !self.entry.enabled;
                    self.update_buttons();
                }
                SAVE => {
                    let entry = self.read()?;
                    self.store.borrow_mut().change(|data| data.put(entry))?;
                    PostMessageW(GetParent(self.hwnd), SAVED, 0, 0);
                    PostMessageW(GetAncestor(self.hwnd, GA_ROOTOWNER), agenda::CHANGED, 0, 0);
                    dismiss(self.hwnd);
                }
                REMOVE if self.entry.id != 0 => {
                    if !self.deleting {
                        self.deleting = true;
                        self.message = "再次点击“确认删除”移除此事项".into();
                        self.update_buttons();
                        InvalidateRect(self.hwnd, null(), 0);
                        return Ok(());
                    }
                    let selected = self.entry.id;
                    self.store.borrow_mut().change(|data| {
                        data.entries.retain(|e| e.id != selected);
                        Ok(())
                    })?;
                    PostMessageW(GetParent(self.hwnd), SAVED, 0, 0);
                    PostMessageW(GetAncestor(self.hwnd, GA_ROOTOWNER), agenda::CHANGED, 0, 0);
                    dismiss(self.hwnd);
                }
                REMOVE | CLOSE => {
                    dismiss(self.hwnd);
                }
                _ => {}
            }
            Ok(())
        }
    }
    unsafe fn paint(&self, dc: HDC) {
        unsafe {
            let t = design::Theme::new(self.dark);
            let r = |x, y, w, h| design::rect(self.s(x), self.s(y), self.s(w), self.s(h));
            design::rounded(dc, r(0, 0, WIDTH, HEIGHT), t.surface, t.border, self.s(14));
            let tx = |value: &str, x, y, w, h, font, color| {
                design::text(
                    dc,
                    value,
                    r(x, y, w, h),
                    self.fonts[font],
                    color,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                )
            };
            tx(
                &format!(
                    "{}{}",
                    if self.entry.id == 0 {
                        "新建"
                    } else {
                        "编辑"
                    },
                    self.entry.kind.label()
                ),
                18,
                16,
                240,
                28,
                2,
                t.text,
            );
            for (label, x, y, w) in [
                ("标题", 18, 100, 288),
                ("日期 · 可点选左侧", 18, 170, 166),
                ("时间", 194, 170, 112),
                ("重复", 18, 240, 136),
                ("提醒", 170, 240, 136),
                ("备注", 18, 310, 288),
            ] {
                tx(label, x, y, w, 22, 1, t.muted);
            }
            for (hwnd, bounds) in &self.controls {
                if matches!(GetDlgCtrlID(*hwnd) as u16, TITLE | DATE | TIME | NOTES) {
                    let b = r(
                        bounds.left - 2,
                        bounds.top - 2,
                        bounds.right - bounds.left + 4,
                        bounds.bottom - bounds.top + 4,
                    );
                    design::rounded(dc, b, t.field, t.border, self.s(8));
                    if GetFocus() == *hwnd {
                        design::fill(
                            dc,
                            design::rect(
                                b.left + self.s(3),
                                b.bottom - self.s(2),
                                b.right - b.left - self.s(6),
                                self.s(2),
                            ),
                            t.accent,
                        );
                    }
                }
            }
            design::text(
                dc,
                &self.message,
                r(18, 458, 288, 30),
                self.fonts[1],
                t.text,
                DT_LEFT | DT_WORDBREAK,
            );
        }
    }
    unsafe fn draw(&self, item: &DRAWITEMSTRUCT) {
        unsafe {
            if item.CtlType == ODT_COMBOBOX {
                design::draw_combo(item, self.dark);
                return;
            }
            let t = design::Theme::new(self.dark);
            let id = item.CtlID as u16;
            let active = (KIND..KIND + 6).contains(&id)
                && Kind::ALL[(id - KIND) as usize] == self.entry.kind;
            let primary = id == SAVE;
            let hover = item.itemState & ODS_SELECTED != 0
                || !GetPropW(item.hwndItem, wide("LiteTray.Hover").as_ptr()).is_null();
            let background = if primary {
                t.accent
            } else if active || hover {
                t.tint
            } else {
                t.surface
            };
            design::fill(item.hDC, item.rcItem, t.surface);
            design::rounded(
                item.hDC,
                item.rcItem,
                background,
                if item.itemState & ODS_FOCUS != 0 {
                    t.accent
                } else {
                    background
                },
                self.s(8),
            );
            let color = if primary {
                t.on_accent
            } else if active {
                t.accent
            } else {
                t.text
            };
            if id == CLOSE {
                icons::draw(
                    item.hDC,
                    icons::Icon::Close,
                    design::rect(
                        item.rcItem.left + self.s(6),
                        item.rcItem.top + self.s(6),
                        self.s(20),
                        self.s(20),
                    ),
                    color,
                    background,
                );
            } else if matches!(id, DONE | ENABLED) {
                let checked = if id == DONE {
                    self.entry.done
                } else {
                    self.entry.enabled
                };
                let b = design::rect(
                    item.rcItem.left + self.s(4),
                    item.rcItem.top + self.s(6),
                    self.s(16),
                    self.s(16),
                );
                design::rounded(
                    item.hDC,
                    b,
                    if checked { t.accent } else { t.field },
                    t.border,
                    self.s(4),
                );
                if checked {
                    design::text(
                        item.hDC,
                        "✓",
                        b,
                        self.fonts[1],
                        t.on_accent,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    );
                }
                let mut label = item.rcItem;
                label.left += self.s(27);
                design::text(
                    item.hDC,
                    &window_text(item.hwndItem),
                    label,
                    self.fonts[1],
                    color,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                );
            } else {
                design::text(
                    item.hDC,
                    &window_text(item.hwndItem),
                    item.rcItem,
                    self.fonts[usize::from((KIND..KIND + 6).contains(&id))],
                    color,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
                if active {
                    design::fill(
                        item.hDC,
                        design::rect(
                            item.rcItem.left + self.s(14),
                            item.rcItem.bottom - self.s(3),
                            self.s(20),
                            self.s(2),
                        ),
                        t.accent,
                    );
                }
            }
        }
    }
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        let Some(cell) = state(hwnd) else {
            return DefWindowProcW(hwnd, msg, w, l);
        };
        match msg {
            WM_NCDESTROY => {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(cell as *const _ as *mut RefCell<View>));
                DefWindowProcW(hwnd, msg, w, l)
            }
            WM_COMMAND => {
                if let Ok(mut v) = cell.try_borrow_mut()
                    && let Err(error) = v.command((w & 0xffff) as u16)
                {
                    v.message = error;
                    InvalidateRect(hwnd, null(), 0);
                }
                0
            }
            WM_ERASEBKGND => 1,
            WM_PAINT | WM_PRINTCLIENT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let dc = if msg == WM_PAINT {
                    BeginPaint(hwnd, &mut ps)
                } else {
                    w as HDC
                };
                if let Ok(v) = cell.try_borrow() {
                    v.paint(dc);
                }
                if msg == WM_PAINT {
                    EndPaint(hwnd, &ps);
                }
                0
            }
            WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC | WM_CTLCOLORLISTBOX | WM_CTLCOLORBTN => {
                if let Ok(v) = cell.try_borrow() {
                    let t = design::Theme::new(v.dark);
                    SetTextColor(w as HDC, t.text);
                    SetBkColor(
                        w as HDC,
                        if msg == WM_CTLCOLOREDIT {
                            t.field
                        } else {
                            t.surface
                        },
                    );
                    if msg == WM_CTLCOLOREDIT {
                        v.field_brush as isize
                    } else {
                        v.brush as isize
                    }
                } else {
                    0
                }
            }
            WM_DRAWITEM if l != 0 => {
                if let Ok(v) = cell.try_borrow() {
                    v.draw(&*(l as *const DRAWITEMSTRUCT));
                }
                1
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }
}
