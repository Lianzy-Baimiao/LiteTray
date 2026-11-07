//! Agenda editor and timer controls, backed by the same durable store as the
//! calendar and background notification pump.
use super::*;
use chrono::{NaiveDate, NaiveTime};
use litetray::organizer::{Entry, FocusSettings, Kind, Phase, Repeat, Store};
pub(super) const ID: u16 = 960;
pub(super) const CHANGED: u32 = WM_APP + 84;
pub(super) const OPEN_DATE: u32 = WM_APP + 85;
pub(super) const OPEN_ENTRY: u32 = WM_APP + 86;
pub(super) const OPEN_ALARMS: u32 = WM_APP + 87;
const CLASS: &str = "LiteTray.Organizer.1";
const SS_CENTER: u32 = 1;
const LIST: u16 = 4000;
const NEW: u16 = 4001;
const ALL_DATES: u16 = 4003;
const SCOPE: u16 = 4004;
const EMPTY: u16 = 4005;
const TITLE: u16 = 4010;
const DATE: u16 = 4012;
const TIME: u16 = 4013;
const REPEAT: u16 = 4014;
const REMIND: u16 = 4015;
const NOTES: u16 = 4016;
const DONE: u16 = 4017;
const ENABLED: u16 = 4018;
const COMMIT: u16 = 4020;
const DELETE: u16 = 4021;
const MESSAGE: u16 = 4022;
const FOCUS_TIME: u16 = 4100;
const FOCUS_PHASE: u16 = 4101;
const FOCUS_TOGGLE: u16 = 4102;
const FOCUS_RESET: u16 = 4103;
const FOCUS_SAVE: u16 = 4104;
const FOCUS_WORK: u16 = 4110;
const FOCUS_SHORT: u16 = 4111;
const FOCUS_LONG: u16 = 4112;
const FOCUS_ROUNDS: u16 = 4113;
const TIMER_TITLE: u16 = 4120;
const TIMER_MINUTES: u16 = 4121;
const TIMER_START: u16 = 4122;
const TIMER_TOGGLE: u16 = 4123;
const TIMER_DELETE: u16 = 4124;
const HISTORY: u16 = 4130;
const EMPTY_TIMERS: u16 = 4131;
const EMPTY_HISTORY: u16 = 4132;
const TYPE_TAB: u16 = 4200;
const TIMER_TAB: u16 = 4300;
const SOUND_CHOICE: u16 = 4400;
const SOUND_PATH: u16 = 4401;
const SOUND_SECONDS: u16 = 4402;
const SOUND_BROWSE: u16 = 4403;
const SOUND_PREVIEW: u16 = 4404;
const SOUND_SAVE: u16 = 4405;
const SOUND_STOP: u16 = 4406;
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

#[derive(Clone)]
struct Draft {
    selected: u64,
    text: HashMap<u16, String>,
    choices: HashMap<u16, usize>,
    checks: HashMap<u16, bool>,
}
#[derive(Clone)]
pub(super) struct Snapshot {
    pub(super) timer_mode: bool,
    selected: u64,
    filter: usize,
    scope: Option<NaiveDate>,
    text: HashMap<u16, String>,
    choices: HashMap<u16, usize>,
    checks: HashMap<u16, bool>,
    drafts: HashMap<usize, Draft>,
    timer_tab: usize,
}
struct View {
    hwnd: HWND,
    root: HWND,
    dark: bool,
    store: Rc<RefCell<Store>>,
    controls: HashMap<u16, HWND>,
    children: Vec<HWND>,
    fonts: [HFONT; 3],
    dpi: u32,
    timer_mode: bool,
    selected: u64,
    scope: Option<NaiveDate>,
    filter: usize,
    ids: Vec<u64>,
    message: String,
    brush: HBRUSH,
    field_brush: HBRUSH,
    drafts: HashMap<usize, Draft>,
    timer_tab: usize,
    timer_groups: Vec<Vec<HWND>>,
}
impl Drop for View {
    fn drop(&mut self) {
        unsafe {
            for font in self.fonts {
                if !font.is_null() {
                    DeleteObject(font);
                }
            }
            DeleteObject(self.brush);
            DeleteObject(self.field_brush);
        }
    }
}
pub(super) unsafe fn create(
    parent: HWND,
    bounds: RECT,
    dark: bool,
    store: Rc<RefCell<Store>>,
    timers: bool,
    scope: Option<NaiveDate>,
    saved: Option<&Snapshot>,
) -> HWND {
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
            wide(if timers {
                "闹钟与专注"
            } else {
                "日程与待办"
            })
            .as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN,
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
        let dpi = GetDpiForWindow(hwnd).max(96);
        let scale = |n| n * dpi as i32 / 96;
        let face = design::font_face();
        let t = design::Theme::new(dark);
        let error = store.borrow().error.clone().unwrap_or_default();
        let mut view = View {
            hwnd,
            root: GetAncestor(parent, GA_ROOT),
            dark,
            store,
            controls: HashMap::new(),
            children: vec![],
            fonts: [
                make_font(scale(14), 400, &face),
                make_font(scale(12), 400, &face),
                make_font(scale(48), 500, "Segoe UI"),
            ],
            dpi,
            timer_mode: timers,
            selected: 0,
            scope,
            filter: 2,
            ids: vec![],
            message: error,
            brush: CreateSolidBrush(t.surface),
            field_brush: CreateSolidBrush(t.field),
            drafts: HashMap::new(),
            timer_tab: 0,
            timer_groups: vec![],
        };
        if let Some(saved) = saved.filter(|s| s.timer_mode == timers) {
            view.selected = saved.selected;
            view.scope = saved.scope;
            view.filter = saved.filter;
            view.drafts = saved.drafts.clone();
            view.timer_tab = saved.timer_tab;
        }
        // Build before installing state: synchronous native control messages
        // during construction cannot reenter a partially built editor.
        view.layout();
        view.populate();
        if let Some(saved) = saved.filter(|s| s.timer_mode == timers) {
            view.restore(saved);
        }
        SetWindowLongPtrW(
            hwnd,
            GWLP_USERDATA,
            Box::into_raw(Box::new(RefCell::new(view))) as isize,
        );
        hwnd
    }
}
pub(super) unsafe fn snapshot(hwnd: HWND) -> Option<Snapshot> {
    unsafe {
        (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>)
            .as_ref()?
            .try_borrow()
            .ok()
            .map(|v| v.snapshot())
    }
}
pub(super) unsafe fn tick(hwnd: HWND) {
    unsafe {
        if let Some(cell) =
            (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>).as_ref()
            && let Ok(view) = cell.try_borrow()
        {
            view.update_timer_labels();
            InvalidateRect(view.field(LIST), null(), 0);
        }
    }
}
pub(super) unsafe fn refresh(hwnd: HWND) {
    unsafe {
        if let Some(cell) =
            (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>).as_ref()
            && let Ok(mut view) = cell.try_borrow_mut()
        {
            view.populate();
        }
    }
}
pub(super) unsafe fn open_date(hwnd: HWND, date: NaiveDate) {
    unsafe {
        if let Some(cell) =
            (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>).as_ref()
            && let Ok(mut view) = cell.try_borrow_mut()
        {
            view.scope = Some(date);
            view.selected = 0;
            view.filter = 2;
            view.load_entry(&Entry::new(Kind::Event, date));
            set_text(
                view.field(SCOPE),
                &date.format(tr("%Y年%m月%d日")).to_string(),
            );
            view.populate();
        }
    }
}
pub(super) unsafe fn open_entry(hwnd: HWND, id: u64) {
    unsafe {
        if let Some(cell) =
            (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>).as_ref()
            && let Ok(mut view) = cell.try_borrow_mut()
        {
            let entry = view
                .store
                .borrow()
                .data
                .entries
                .iter()
                .find(|e| e.id == id)
                .cloned();
            if let Some(entry) = entry {
                view.selected = id;
                view.scope = None;
                view.filter = Kind::ALL.iter().position(|k| *k == entry.kind).unwrap() + 1;
                set_text(view.field(SCOPE), "全部日期 · 即将到来");
                view.load_entry(&entry);
                view.populate();
            }
        }
    }
}
pub(super) unsafe fn open_alarms(hwnd: HWND) {
    unsafe {
        if let Some(cell) =
            (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<View>).as_ref()
            && let Ok(mut view) = cell.try_borrow_mut()
        {
            view.filter = 6;
            view.scope = None;
            view.selected = 0;
            let mut entry = Entry::new(Kind::Alarm, Local::now().date_naive());
            entry.time = NaiveTime::from_hms_opt(8, 0, 0);
            entry.repeat = Repeat::Daily;
            view.load_entry(&entry);
            view.populate();
        }
    }
}
impl View {
    fn kind(&self) -> Kind {
        Kind::ALL[self.filter.saturating_sub(1).min(5)]
    }
    unsafe fn switch_kind(&mut self, index: usize) {
        unsafe {
            if index == self.filter {
                return;
            }
            let old = self.snapshot();
            self.drafts.insert(
                self.filter,
                Draft {
                    selected: self.selected,
                    text: old.text,
                    choices: old.choices,
                    checks: old.checks,
                },
            );
            self.filter = index;
            if let Some(draft) = self.drafts.get(&index).cloned() {
                self.selected = draft.selected;
                for (id, text) in draft.text {
                    set_text(self.field(id), &text);
                }
                for (id, choice) in draft.choices {
                    SendMessageW(self.field(id), CB_SETCURSEL, choice, 0);
                }
                for (id, check) in draft.checks {
                    SendMessageW(self.field(id), BM_SETCHECK, usize::from(check), 0);
                }
                EnableWindow(self.field(DELETE), i32::from(self.selected != 0));
            } else {
                self.new_entry();
            }
            self.populate();
        }
    }
    unsafe fn new_entry(&mut self) {
        unsafe {
            self.selected = 0;
            let mut entry = Entry::new(
                self.kind(),
                self.scope.unwrap_or_else(|| Local::now().date_naive()),
            );
            if entry.kind == Kind::Alarm {
                entry.time = NaiveTime::from_hms_opt(8, 0, 0);
                entry.repeat = Repeat::Daily;
            }
            self.load_entry(&entry);
        }
    }
    fn read_sound(&self) -> Result<organizer::SoundSettings, String> {
        let settings = organizer::SoundSettings {
            tone: self.choice(SOUND_CHOICE) as u8,
            custom_file: self.value(SOUND_PATH).trim().into(),
            seconds: self
                .value(SOUND_SECONDS)
                .trim()
                .parse()
                .map_err(|_| "响铃时长请填写整数")?,
        };
        settings.validate()?;
        if settings.tone != 0 && settings.path().is_none_or(|p| !p.is_file()) {
            return Err("铃声文件不存在，请重新选择".into());
        }
        Ok(settings)
    }
    fn s(&self, n: i32) -> i32 {
        n * self.dpi as i32 / 96
    }
    fn field(&self, id: u16) -> HWND {
        self.controls.get(&id).copied().unwrap_or(null_mut())
    }
    fn value(&self, id: u16) -> String {
        unsafe { window_text(self.field(id)) }
    }
    fn choice(&self, id: u16) -> usize {
        unsafe { SendMessageW(self.field(id), CB_GETCURSEL, 0, 0).max(0) as usize }
    }
    fn checked(&self, id: u16) -> bool {
        unsafe { SendMessageW(self.field(id), BM_GETCHECK, 0, 0) == BST_CHECKED as isize }
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Native control rectangle and identity"
    )]
    unsafe fn control(
        &mut self,
        class: &str,
        label: &str,
        id: u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    ) -> HWND {
        unsafe {
            let style = if class == "STATIC" {
                style | 0x80 // SS_NOPREFIX
            } else if class == "COMBOBOX" {
                style | CBS_OWNERDRAWFIXED as u32 | CBS_HASSTRINGS as u32
            } else {
                style
            };
            let hwnd = CreateWindowExW(
                0,
                wide(class).as_ptr(),
                wide(if class == "EDIT" { label } else { tr(label) }).as_ptr(),
                WS_CHILD | WS_VISIBLE | style,
                self.s(x),
                self.s(y),
                self.s(w.max(30)),
                self.s(h),
                self.hwnd,
                id as usize as HMENU,
                GetModuleHandleW(null()),
                null(),
            );
            SendMessageW(hwnd, WM_SETFONT, self.fonts[0] as usize, 0);
            if class == "EDIT" {
                SetPropW(hwnd, wide("LiteTray.Field").as_ptr(), 1usize as HANDLE);
                SendMessageW(
                    hwnd,
                    EM_SETMARGINS,
                    (EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize,
                    (8 << 16) | 8,
                );
                SendMessageW(
                    hwnd,
                    EM_SETLIMITTEXT,
                    if id == NOTES { 2000 } else { 120 },
                    0,
                );
            }
            if class == "COMBOBOX" || class == "LISTBOX" || class == "BUTTON" {
                SetWindowTheme(
                    hwnd,
                    wide(if self.dark {
                        "DarkMode_Explorer"
                    } else {
                        "Explorer"
                    })
                    .as_ptr(),
                    null(),
                );
            }
            if class == "BUTTON" && style & 0xf == BS_OWNERDRAW as u32 {
                SetWindowSubclass(hwnd, Some(design::hover_proc), 2, 0);
            }
            if class == "COMBOBOX" {
                design::style_combo(hwnd, self.dark, self.dpi);
            }
            if class == "EDIT" {
                widgets::style_edit(hwnd, self.dark, self.dpi);
            }
            self.children.push(hwnd);
            if id != 0 {
                self.controls.insert(id, hwnd);
            }
            hwnd
        }
    }
    unsafe fn label(&mut self, label: &str, x: i32, y: i32, w: i32) {
        unsafe {
            self.control("STATIC", label, 0, 0, x, y, w, 24);
        }
    }
    unsafe fn button(&mut self, label: &str, id: u16, x: i32, y: i32, w: i32) {
        unsafe {
            self.control(
                "BUTTON",
                label,
                id,
                WS_TABSTOP | BS_OWNERDRAW as u32,
                x,
                y,
                w,
                34,
            );
        }
    }
    unsafe fn edit(&mut self, label: &str, value: &str, id: u16, x: i32, y: i32, w: i32) {
        unsafe {
            self.label(label, x, y, w);
            self.control(
                "EDIT",
                value,
                id,
                WS_TABSTOP | ES_AUTOHSCROLL as u32,
                x,
                y + 28,
                w,
                32,
            );
        }
    }
    unsafe fn combo(&mut self, items: &[&str], index: usize, id: u16, x: i32, y: i32, w: i32) {
        unsafe {
            let hwnd = self.control(
                "COMBOBOX",
                "",
                id,
                WS_TABSTOP | CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
                x,
                y,
                w,
                240,
            );
            for item in items {
                SendMessageW(hwnd, CB_ADDSTRING, 0, wide(tr(item)).as_ptr() as isize);
            }
            SendMessageW(hwnd, CB_SETCURSEL, index, 0);
        }
    }
    unsafe fn layout(&mut self) {
        unsafe {
            let mut rect: RECT = std::mem::zeroed();
            GetClientRect(self.hwnd, &mut rect);
            let width = rect.right * 96 / self.dpi as i32;
            let height = rect.bottom * 96 / self.dpi as i32;
            if self.timer_mode {
                self.timer_layout(width, height);
                return;
            }
            let tab_width = (width - 40) / 6;
            for (index, kind) in Kind::ALL.into_iter().enumerate() {
                self.button(
                    kind.label(),
                    TYPE_TAB + index as u16,
                    20 + index as i32 * tab_width,
                    16,
                    tab_width - 6,
                );
            }
            self.rule(20, 68, width - 40);
            let left = (width * 36 / 100).clamp(250, 320);
            self.control(
                "STATIC",
                &self
                    .scope
                    .map(|d| d.format(tr("%Y年%m月%d日")).to_string())
                    .unwrap_or_else(|| "全部日期 · 即将到来".into()),
                SCOPE,
                0,
                24,
                98,
                left - 86,
                24,
            );
            self.button("全部", ALL_DATES, left - 58, 90, 58);
            self.button("新建事项", NEW, 24, 132, left - 24);
            let list = self.control(
                "LISTBOX",
                "",
                LIST,
                WS_TABSTOP
                    | WS_VSCROLL
                    | LBS_NOTIFY as u32
                    | LBS_OWNERDRAWFIXED as u32
                    | LBS_HASSTRINGS as u32
                    | LBS_NOINTEGRALHEIGHT as u32,
                24,
                191,
                left - 24,
                height - 293,
            );
            SendMessageW(list, LB_SETITEMHEIGHT, 0, self.s(65) as isize);
            self.control(
                "STATIC",
                "暂无事项\r\n点击“新建事项”开始记录",
                EMPTY,
                SS_CENTER,
                34,
                239,
                left - 44,
                70,
            );
            let x = left + 32;
            let w = width - x - 24;
            let half = (w - 16) / 2;
            let heading = self.control("STATIC", "新建日程", 4023, 0, x, 94, w, 26);
            widgets::heading(heading, self.dpi);
            let title = self.control(
                "EDIT",
                "",
                TITLE,
                WS_TABSTOP | ES_AUTOHSCROLL as u32,
                x,
                132,
                w,
                32,
            );
            SendMessageW(
                title,
                EM_SETCUEBANNER,
                0,
                wide(tr("输入事项标题")).as_ptr() as isize,
            );
            self.edit(
                "日期 · YYYY-MM-DD",
                &self
                    .scope
                    .unwrap_or_else(|| Local::now().date_naive())
                    .to_string(),
                DATE,
                x,
                175,
                half,
            );
            self.edit("时间 · 留空为全天", "", TIME, x + half + 16, 175, half);
            self.label("重复", x, 252, half);
            self.combo(&Repeat::ALL.map(Repeat::label), 0, REPEAT, x, 280, half);
            self.label("提醒", x + half + 16, 252, half);
            self.combo(
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
                1,
                REMIND,
                x + half + 16,
                280,
                half,
            );
            self.label("备注", x, 331, w);
            let bottom = (height - 152).min(546);
            self.control(
                "EDIT",
                "",
                NOTES,
                WS_TABSTOP | ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_WANTRETURN as u32,
                x,
                359,
                w,
                (bottom - 371).max(40),
            );
            let enabled = self.control(
                "BUTTON",
                "启用提醒",
                ENABLED,
                WS_TABSTOP | BS_AUTOCHECKBOX as u32,
                x,
                bottom,
                half,
                28,
            );
            SendMessageW(enabled, BM_SETCHECK, BST_CHECKED as usize, 0);
            self.control(
                "BUTTON",
                "已完成",
                DONE,
                WS_TABSTOP | BS_AUTOCHECKBOX as u32,
                x + half + 16,
                bottom,
                half,
                28,
            );
            self.button("保存", COMMIT, x, bottom + 42, half);
            self.button("删除", DELETE, x + half + 16, bottom + 42, half);
            self.rule(20, height - 65, width - 40);
            self.control(
                "STATIC",
                &self.message.clone(),
                MESSAGE,
                0,
                24,
                height - 52,
                width - 48,
                24,
            );
            self.note(
                "全天事项在 09:00 提醒 · 日历同时显示农历",
                24,
                height - 28,
                width - 48,
                24,
            );
            self.update_tabs();
        }
    }
    unsafe fn timer_layout(&mut self, width: i32, height: i32) {
        unsafe {
            let tab_width = (width - 184) / 4;
            for (index, label) in ["番茄钟", "定时提醒", "提醒记录", "铃声"]
                .into_iter()
                .enumerate()
            {
                self.button(
                    label,
                    TIMER_TAB + index as u16,
                    20 + index as i32 * tab_width,
                    16,
                    tab_width - 6,
                );
            }
            self.button("管理闹钟", 4190, width - 144, 16, 124);
            self.rule(20, 68, width - 40);
            let half = (width - 88) / 2;
            let right = 64 + half;
            let start = self.children.len();
            self.heading("当前专注", 24, 94, half);
            self.control(
                "STATIC",
                "准备专注",
                FOCUS_PHASE,
                SS_CENTER,
                24,
                153,
                half,
                26,
            );
            let clock = self.control(
                "STATIC", "00:25:00", FOCUS_TIME, SS_CENTER, 24, 194, half, 76,
            );
            SendMessageW(clock, WM_SETFONT, self.fonts[2] as usize, 0);
            let actions = (half - 16) / 2;
            self.button("开始专注", FOCUS_TOGGLE, 24, 315, actions);
            self.button("重置", FOCUS_RESET, 40 + actions, 315, actions);
            self.note(
                "阶段结束后响铃。\r\n准备好后，再开始下一段专注或休息。",
                24,
                373,
                half,
                48,
            );
            self.heading("专注节奏", right, 94, half);
            let cell = (half - 20) / 2;
            let settings = self.store.borrow().data.focus.clone();
            for (i, (name, id, value)) in [
                ("专注 / 分钟", FOCUS_WORK, settings.work),
                ("短休息 / 分钟", FOCUS_SHORT, settings.short_break),
                ("长休息 / 分钟", FOCUS_LONG, settings.long_break),
                ("长休息间隔 / 轮", FOCUS_ROUNDS, settings.rounds),
            ]
            .into_iter()
            .enumerate()
            {
                self.edit(
                    name,
                    &value.to_string(),
                    id,
                    right + i as i32 % 2 * (cell + 20),
                    145 + i as i32 / 2 * 83,
                    cell,
                );
            }
            self.button("保存节奏", FOCUS_SAVE, right, 315, half);
            self.note(
                "保存后从下一阶段开始使用。\r\n铃声和响铃时长可在“铃声”中调整。",
                right,
                373,
                half,
                48,
            );
            self.timer_groups.push(self.children[start..].to_vec());
            let start = self.children.len();
            self.heading("进行中的计时", 24, 94, half);
            let list_height = (height - 316).max(164);
            let list = self.control(
                "LISTBOX",
                "",
                LIST,
                WS_TABSTOP
                    | WS_VSCROLL
                    | LBS_NOTIFY as u32
                    | LBS_OWNERDRAWFIXED as u32
                    | LBS_HASSTRINGS as u32
                    | LBS_NOINTEGRALHEIGHT as u32,
                24,
                142,
                half,
                list_height,
            );
            SendMessageW(list, LB_SETITEMHEIGHT, 0, self.s(65) as isize);
            self.control(
                "STATIC",
                "还没有计时器\r\n在右侧创建第一条提醒",
                EMPTY_TIMERS,
                SS_CENTER,
                36,
                226,
                half - 24,
                60,
            );
            self.button("暂停 / 继续", TIMER_TOGGLE, 24, 158 + list_height, actions);
            self.button(
                "删除",
                TIMER_DELETE,
                40 + actions,
                158 + list_height,
                actions,
            );
            self.heading("新建提醒", right, 94, half);
            self.edit("提醒内容", "喝水 / 休息一下", TIMER_TITLE, right, 145, half);
            self.edit("多久后提醒 / 分钟", "10", TIMER_MINUTES, right, 228, half);
            self.button("开始计时", TIMER_START, right, 315, half);
            self.note(
                "可同时创建多个计时器。\r\n选中左侧计时器后可暂停、继续或删除。",
                right,
                373,
                half,
                48,
            );
            self.timer_groups.push(self.children[start..].to_vec());
            let start = self.children.len();
            self.heading("最近提醒", 24, 94, width - 48);
            self.note(
                "提醒记录保存在本机，可在这里回看。",
                24,
                132,
                width - 48,
                24,
            );
            self.control(
                "LISTBOX",
                "",
                HISTORY,
                WS_TABSTOP | WS_VSCROLL | LBS_HASSTRINGS as u32 | LBS_NOINTEGRALHEIGHT as u32,
                24,
                177,
                width - 48,
                height - 285,
            );
            self.control(
                "STATIC",
                "还没有提醒记录\r\n闹钟、日程或计时到期后，会出现在这里",
                EMPTY_HISTORY,
                SS_CENTER,
                24,
                250,
                width - 48,
                64,
            );
            self.timer_groups.push(self.children[start..].to_vec());
            let start = self.children.len();
            let sound = self.store.borrow().data.sound.clone();
            self.heading("声音与时长", 24, 94, width - 48);
            self.label("提醒铃声", 24, 145, half);
            self.combo(
                &[
                    "静音",
                    "Windows 闹钟 1",
                    "Windows 闹钟 2",
                    "Windows 闹钟 3",
                    "Windows 闹钟 4",
                    "Windows 闹钟 5",
                    "Windows 闹钟 6",
                    "Windows 闹钟 7",
                    "Windows 闹钟 8",
                    "Windows 闹钟 9",
                    "Windows 闹钟 10",
                    "自定义 WAV",
                ],
                sound.tone as usize,
                SOUND_CHOICE,
                24,
                173,
                half,
            );
            self.edit(
                "响铃时长 / 秒（5–120）",
                &sound.seconds.to_string(),
                SOUND_SECONDS,
                right,
                145,
                half,
            );
            self.edit(
                "自定义音乐 · WAV 文件",
                &sound.custom_file,
                SOUND_PATH,
                24,
                234,
                width - 210,
            );
            SendMessageW(self.field(SOUND_PATH), EM_SETLIMITTEXT, 32767, 0);
            SendMessageW(
                self.field(SOUND_PATH),
                EM_SETCUEBANNER,
                0,
                wide(tr("选择自定义 WAV 时填写")).as_ptr() as isize,
            );
            self.button("选择文件…", SOUND_BROWSE, width - 166, 262, 142);
            self.button("保存铃声", SOUND_SAVE, 24, 330, 160);
            self.button("试听 5 秒", SOUND_PREVIEW, 200, 330, 144);
            self.note(
                "适用于闹钟、日程、定时提醒与专注。\r\n使用当前系统音量，选择“静音”后仅显示通知。",
                24,
                392,
                width - 48,
                48,
            );
            self.timer_groups.push(self.children[start..].to_vec());
            self.rule(20, height - 100, width - 40);
            self.button("停止响铃", SOUND_STOP, width - 150, height - 82, 130);
            self.control(
                "STATIC",
                &self.message.clone(),
                MESSAGE,
                0,
                24,
                height - 78,
                width - 194,
                28,
            );
            self.note("关闭窗口后仍会计时与提醒", 24, height - 36, width - 48, 24);
            self.update_tabs();
        }
    }
    unsafe fn heading(&mut self, text: &str, x: i32, y: i32, width: i32) {
        unsafe {
            let hwnd = self.control("STATIC", text, 0, 0, x, y, width, 26);
            widgets::heading(hwnd, self.dpi);
        }
    }
    unsafe fn note(&mut self, text: &str, x: i32, y: i32, width: i32, height: i32) {
        unsafe {
            let hwnd = self.control("STATIC", text, 0, 0, x, y, width, height);
            SetPropW(hwnd, wide("LiteTray.Muted").as_ptr(), 1usize as HANDLE);
        }
    }
    unsafe fn rule(&mut self, x: i32, y: i32, width: i32) {
        unsafe {
            let hwnd = self.control("STATIC", "", 0, 0, x, y, width, 1);
            widgets::divider(hwnd);
        }
    }
    unsafe fn update_tabs(&self) {
        unsafe {
            for index in 0..if self.timer_mode { 4 } else { 6 } {
                InvalidateRect(
                    self.field(if self.timer_mode { TIMER_TAB } else { TYPE_TAB } + index),
                    null(),
                    0,
                );
            }
            if self.timer_mode {
                for id in [TIMER_TOGGLE, TIMER_DELETE] {
                    EnableWindow(self.field(id), i32::from(self.ids.contains(&self.selected)));
                }
                for (index, group) in self.timer_groups.iter().enumerate() {
                    for &hwnd in group {
                        ShowWindow(
                            hwnd,
                            if index == self.timer_tab {
                                SW_SHOWNA
                            } else {
                                SW_HIDE
                            },
                        );
                    }
                }
                if self.timer_tab == 2 {
                    let empty = SendMessageW(self.field(HISTORY), LB_GETCOUNT, 0, 0) == 0;
                    ShowWindow(self.field(HISTORY), if empty { SW_HIDE } else { SW_SHOWNA });
                    ShowWindow(
                        self.field(EMPTY_HISTORY),
                        if empty { SW_SHOWNA } else { SW_HIDE },
                    );
                }
                if self.timer_tab == 1 {
                    ShowWindow(
                        self.field(LIST),
                        if self.ids.is_empty() {
                            SW_HIDE
                        } else {
                            SW_SHOWNA
                        },
                    );
                    ShowWindow(
                        self.field(EMPTY_TIMERS),
                        if self.ids.is_empty() {
                            SW_SHOWNA
                        } else {
                            SW_HIDE
                        },
                    );
                }
            } else {
                ShowWindow(
                    self.field(DONE),
                    if self.filter == 1 { SW_SHOWNA } else { SW_HIDE },
                );
                set_text(
                    self.field(COMMIT),
                    &format!(
                        "{}{}{}",
                        tr("保存"),
                        if i18n::english() { " " } else { "" },
                        tr(self.kind().label())
                    ),
                );
                set_text(
                    self.field(4023),
                    &format!(
                        "{}{}{}",
                        tr(if self.selected == 0 {
                            "新建"
                        } else {
                            "编辑"
                        }),
                        if i18n::english() { " " } else { "" },
                        tr(self.kind().label())
                    ),
                );
                set_text(self.field(NEW), &format!("新建{}", self.kind().label()));
            }
        }
    }
    unsafe fn populate(&mut self) {
        unsafe {
            let store = self.store.borrow();
            self.ids = if self.timer_mode {
                store
                    .data
                    .timers
                    .iter()
                    .filter(|t| t.phase == Phase::Countdown)
                    .map(|t| t.id)
                    .collect()
            } else {
                let mut entries = store
                    .data
                    .entries
                    .iter()
                    .filter(|e| {
                        (self.filter == 0 || Kind::ALL.get(self.filter - 1) == Some(&e.kind))
                            && self.scope.is_none_or(|d| {
                                e.occurs_on(d) || (e.date == d && (e.done || !e.enabled))
                            })
                    })
                    .collect::<Vec<_>>();
                entries.sort_by_key(|e| {
                    (
                        e.done,
                        !e.enabled,
                        e.next_date(Local::now().date_naive()).unwrap_or(e.date),
                        e.time,
                        e.id,
                    )
                });
                entries.into_iter().map(|e| e.id).collect()
            };
            let titles = self
                .ids
                .iter()
                .map(|id| {
                    if self.timer_mode {
                        store
                            .data
                            .timers
                            .iter()
                            .find(|t| t.id == *id)
                            .map(|t| t.title.clone())
                            .unwrap_or_default()
                    } else {
                        store
                            .data
                            .entries
                            .iter()
                            .find(|e| e.id == *id)
                            .map(|e| e.title.clone())
                            .unwrap_or_default()
                    }
                })
                .collect::<Vec<_>>();
            let history = store
                .data
                .notices
                .iter()
                .rev()
                .take(20)
                .map(|n| {
                    format!(
                        "{}  {} · {}",
                        n.at.with_timezone(&Local).format("%m-%d %H:%M"),
                        n.title,
                        n.body.lines().next().unwrap_or("")
                    )
                })
                .collect::<Vec<_>>();
            drop(store);
            SendMessageW(self.field(LIST), LB_RESETCONTENT, 0, 0);
            for title in titles {
                SendMessageW(
                    self.field(LIST),
                    LB_ADDSTRING,
                    0,
                    wide(&title).as_ptr() as isize,
                );
            }
            if let Some(index) = self.ids.iter().position(|id| *id == self.selected) {
                SendMessageW(self.field(LIST), LB_SETCURSEL, index, 0);
            }
            if self.timer_mode {
                SendMessageW(self.field(HISTORY), LB_RESETCONTENT, 0, 0);
                for item in history {
                    SendMessageW(
                        self.field(HISTORY),
                        LB_ADDSTRING,
                        0,
                        wide(&item).as_ptr() as isize,
                    );
                }
            }
            self.update_timer_labels();
            let empty = self.field(if self.timer_mode { EMPTY_TIMERS } else { EMPTY });
            // Native listboxes repaint their whole client area. Hide the empty
            // list so it cannot overpaint the sibling hint during later ticks.
            ShowWindow(
                self.field(LIST),
                if self.ids.is_empty() {
                    SW_HIDE
                } else {
                    SW_SHOWNA
                },
            );
            if self.ids.is_empty() {
                // A visible sibling can still sit behind the native listbox.
                // Raise the empty-state label without activating the window.
                SetWindowPos(
                    empty,
                    HWND_TOP,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            } else {
                ShowWindow(empty, SW_HIDE);
            }
            InvalidateRect(self.hwnd, null(), 0);
            self.update_tabs();
        }
    }
    unsafe fn update_timer_labels(&self) {
        unsafe {
            if !self.timer_mode {
                return;
            }
            let store = self.store.borrow();
            if let Some(timer) = store
                .data
                .timers
                .iter()
                .find(|t| t.phase != Phase::Countdown)
            {
                let time = timer.display(Utc::now());
                set_text(
                    self.field(FOCUS_TIME),
                    time.strip_prefix("00:").unwrap_or(&time),
                );
                set_text(
                    self.field(FOCUS_PHASE),
                    &format!("{} · 已完成 {} 轮", timer.phase.label(), timer.rounds),
                );
                set_text(
                    self.field(FOCUS_TOGGLE),
                    if timer.due.is_some() {
                        "暂停"
                    } else if timer.phase == Phase::Focus {
                        "开始专注"
                    } else {
                        "开始休息"
                    },
                );
            } else {
                set_text(
                    self.field(FOCUS_TIME),
                    format!(
                        "{:02}:{:02}:00",
                        store.data.focus.work / 60,
                        store.data.focus.work % 60
                    )
                    .trim_start_matches("00:"),
                );
                set_text(self.field(FOCUS_PHASE), "准备专注");
                set_text(self.field(FOCUS_TOGGLE), "开始专注");
            }
        }
    }
    unsafe fn load_entry(&self, entry: &Entry) {
        unsafe {
            for (id, value) in [
                (TITLE, entry.title.clone()),
                (DATE, entry.date.to_string()),
                (
                    TIME,
                    entry
                        .time
                        .map(|t| t.format("%H:%M").to_string())
                        .unwrap_or_default(),
                ),
                (NOTES, entry.notes.clone()),
            ] {
                set_text(self.field(id), &value);
            }
            for (id, index) in [
                (
                    REPEAT,
                    Repeat::ALL.iter().position(|r| *r == entry.repeat).unwrap(),
                ),
                (
                    REMIND,
                    LEADS
                        .iter()
                        .position(|m| *m == entry.remind_minutes)
                        .unwrap_or(1),
                ),
            ] {
                SendMessageW(self.field(id), CB_SETCURSEL, index, 0);
            }
            SendMessageW(
                self.field(DONE),
                BM_SETCHECK,
                if entry.done { BST_CHECKED as usize } else { 0 },
                0,
            );
            SendMessageW(
                self.field(ENABLED),
                BM_SETCHECK,
                if entry.enabled {
                    BST_CHECKED as usize
                } else {
                    0
                },
                0,
            );
            EnableWindow(self.field(DELETE), i32::from(entry.id != 0));
        }
    }
    fn read_entry(&self) -> Result<Entry, String> {
        let date = NaiveDate::parse_from_str(self.value(DATE).trim(), "%Y-%m-%d")
            .map_err(|_| "日期格式为 YYYY-MM-DD，例如 2026-11-06")?;
        let time = if self.value(TIME).trim().is_empty() {
            None
        } else {
            Some(
                NaiveTime::parse_from_str(self.value(TIME).trim(), "%H:%M")
                    .map_err(|_| "时间格式为 HH:MM，例如 08:30")?,
            )
        };
        let kind = self.kind();
        let entry = Entry {
            id: self.selected,
            kind,
            title: self.value(TITLE),
            date,
            time,
            repeat: Repeat::ALL[self.choice(REPEAT).min(4)],
            remind_minutes: LEADS[self.choice(REMIND).min(7)],
            notes: self.value(NOTES),
            done: kind == Kind::Todo && self.checked(DONE),
            enabled: self.checked(ENABLED),
        };
        entry.validate()?;
        Ok(entry)
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            timer_mode: self.timer_mode,
            selected: self.selected,
            filter: self.filter,
            scope: self.scope,
            drafts: self.drafts.clone(),
            timer_tab: self.timer_tab,
            text: [
                TITLE,
                DATE,
                TIME,
                NOTES,
                TIMER_TITLE,
                TIMER_MINUTES,
                FOCUS_WORK,
                FOCUS_SHORT,
                FOCUS_LONG,
                FOCUS_ROUNDS,
                SOUND_PATH,
                SOUND_SECONDS,
            ]
            .into_iter()
            .filter(|id| !self.field(*id).is_null())
            .map(|id| (id, self.value(id)))
            .collect(),
            choices: [REPEAT, REMIND, SOUND_CHOICE]
                .into_iter()
                .filter(|id| !self.field(*id).is_null())
                .map(|id| (id, self.choice(id)))
                .collect(),
            checks: [DONE, ENABLED]
                .into_iter()
                .filter(|id| !self.field(*id).is_null())
                .map(|id| (id, self.checked(id)))
                .collect(),
        }
    }
    unsafe fn restore(&self, snapshot: &Snapshot) {
        unsafe {
            for (&id, value) in &snapshot.text {
                set_text(self.field(id), value);
            }
            for (&id, &index) in &snapshot.choices {
                SendMessageW(self.field(id), CB_SETCURSEL, index, 0);
            }
            for (&id, &checked) in &snapshot.checks {
                SendMessageW(
                    self.field(id),
                    BM_SETCHECK,
                    if checked { BST_CHECKED as usize } else { 0 },
                    0,
                );
            }
        }
    }
    unsafe fn command(&mut self, id: u16, notification: u16) -> Result<(), String> {
        unsafe {
            if id == LIST && notification == LBN_SELCHANGE as u16 {
                let index = SendMessageW(self.field(LIST), LB_GETCURSEL, 0, 0);
                self.selected = self.ids.get(index as usize).copied().unwrap_or(0);
                if !self.timer_mode
                    && let Some(entry) = self
                        .store
                        .borrow()
                        .data
                        .entries
                        .iter()
                        .find(|e| e.id == self.selected)
                        .cloned()
                {
                    self.load_entry(&entry);
                }
                self.update_tabs();
                return Ok(());
            }
            if notification != BN_CLICKED as u16 {
                return Ok(());
            }
            if (TYPE_TAB..TYPE_TAB + 6).contains(&id) {
                self.switch_kind((id - TYPE_TAB + 1) as usize);
                return Ok(());
            }
            if (TIMER_TAB..TIMER_TAB + 4).contains(&id) {
                self.timer_tab = (id - TIMER_TAB) as usize;
                self.update_tabs();
                InvalidateRect(self.hwnd, null(), 0);
                return Ok(());
            }
            match id {
                SOUND_BROWSE => {
                    if let Some(path) = choose_sound_file(self.root)? {
                        set_text(self.field(SOUND_PATH), &path.to_string_lossy());
                        SendMessageW(self.field(SOUND_CHOICE), CB_SETCURSEL, 11, 0);
                    }
                }
                SOUND_PREVIEW => {
                    litetray::platform::sound::play(&self.read_sound()?, true)?;
                    self.message = "试听中 · 5 秒后自动停止".into();
                }
                SOUND_STOP => {
                    litetray::platform::sound::stop();
                    self.message = "已停止响铃".into();
                }
                SOUND_SAVE => {
                    let settings = self.read_sound()?;
                    self.store.borrow_mut().change(|data| {
                        data.sound = settings;
                        Ok(())
                    })?;
                    self.message = "铃声已保存".into();
                }
                NEW => {
                    self.new_entry();
                    self.message = "新建日程 · 填写后保存".into();
                }
                ALL_DATES => {
                    self.scope = None;
                    set_text(self.field(SCOPE), "全部日期 · 即将到来");
                    self.populate();
                }
                COMMIT => {
                    let entry = self.read_entry()?;
                    self.selected = self.store.borrow_mut().change(|data| data.put(entry))?;
                    self.message = "日程已保存".into();
                    self.populate();
                }
                DELETE => {
                    if self.selected == 0 {
                        return Err("请先选择一条日程".into());
                    }
                    let selected = self.selected;
                    self.store.borrow_mut().change(|data| {
                        data.entries.retain(|e| e.id != selected);
                        Ok(())
                    })?;
                    self.new_entry();
                    self.message = "日程已删除".into();
                    self.populate();
                }
                FOCUS_TOGGLE => {
                    self.store.borrow_mut().change(|data| {
                        if let Some(id) = data
                            .timers
                            .iter()
                            .find(|t| t.phase != Phase::Countdown)
                            .map(|t| t.id)
                        {
                            data.toggle_timer(id, Utc::now())
                        } else {
                            data.start_timer("番茄钟", data.focus.work, true, Utc::now())
                                .map(|_| ())
                        }
                    })?;
                    self.update_timer_labels();
                }
                FOCUS_RESET => {
                    self.store.borrow_mut().change(|data| {
                        data.timers.retain(|t| t.phase == Phase::Countdown);
                        Ok(())
                    })?;
                    self.update_timer_labels();
                }
                FOCUS_SAVE => {
                    let number = |id| {
                        self.value(id)
                            .trim()
                            .parse::<u32>()
                            .map_err(|_| "节奏设置请填写整数".to_string())
                    };
                    let settings = FocusSettings {
                        work: number(FOCUS_WORK)?,
                        short_break: number(FOCUS_SHORT)?,
                        long_break: number(FOCUS_LONG)?,
                        rounds: number(FOCUS_ROUNDS)?,
                    };
                    settings.validate()?;
                    self.store.borrow_mut().change(|data| {
                        data.focus = settings;
                        Ok(())
                    })?;
                    self.message = "节奏已保存，从下一阶段开始使用".into();
                    self.update_timer_labels();
                }
                TIMER_START => {
                    let title = self.value(TIMER_TITLE);
                    let minutes = self
                        .value(TIMER_MINUTES)
                        .trim()
                        .parse()
                        .map_err(|_| "分钟数请填写整数")?;
                    self.selected = self
                        .store
                        .borrow_mut()
                        .change(|data| data.start_timer(&title, minutes, false, Utc::now()))?;
                    self.message = "计时已开始，关闭窗口仍会继续".into();
                    self.populate();
                }
                TIMER_TOGGLE => {
                    let id = self.selected;
                    self.store
                        .borrow_mut()
                        .change(|data| data.toggle_timer(id, Utc::now()))?;
                    self.populate();
                }
                TIMER_DELETE => {
                    if self.selected == 0 {
                        return Err("请先选择计时器".into());
                    }
                    let id = self.selected;
                    self.store.borrow_mut().change(|data| {
                        data.timers
                            .retain(|t| t.id != id || t.phase != Phase::Countdown);
                        Ok(())
                    })?;
                    self.selected = 0;
                    self.populate();
                }
                4190 => {
                    PostMessageW(self.root, OPEN_ALARMS, 0, 0);
                }
                _ => return Ok(()),
            }
            PostMessageW(self.root, CHANGED, 0, 0);
            set_text(self.field(MESSAGE), &self.message);
            Ok(())
        }
    }
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RefCell<View>;
        if msg == WM_NCDESTROY {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if !raw.is_null() {
                drop(Box::from_raw(raw));
            }
            return DefWindowProcW(hwnd, msg, w, l);
        }
        if raw.is_null() {
            return DefWindowProcW(hwnd, msg, w, l);
        }
        match msg {
            WM_COMMAND => {
                if let Ok(mut view) = (*raw).try_borrow_mut()
                    && let Err(error) = view.command((w & 0xffff) as u16, (w >> 16) as u16)
                {
                    view.message = error;
                    set_text(view.field(MESSAGE), &view.message);
                }
                0
            }
            WM_ERASEBKGND => 1,
            WM_PAINT | WM_PRINTCLIENT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let dc = if msg == WM_PRINTCLIENT {
                    w as HDC
                } else {
                    BeginPaint(hwnd, &mut ps)
                };
                if let Ok(view) = (*raw).try_borrow() {
                    let t = design::Theme::new(view.dark);
                    let mut r: RECT = std::mem::zeroed();
                    GetClientRect(hwnd, &mut r);
                    design::fill(dc, r, t.surface);
                    page::paint_fields(hwnd, dc, t);
                }
                if msg != WM_PRINTCLIENT {
                    EndPaint(hwnd, &ps);
                }
                0
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
                if let Ok(view) = (*raw).try_borrow() {
                    let t = design::Theme::new(view.dark);
                    SetTextColor(
                        w as HDC,
                        if !GetPropW(l as HWND, wide("LiteTray.Muted").as_ptr()).is_null() {
                            t.muted
                        } else {
                            t.text
                        },
                    );
                    SetBkColor(
                        w as HDC,
                        if msg == WM_CTLCOLOREDIT {
                            t.field
                        } else {
                            t.surface
                        },
                    );
                    return if msg == WM_CTLCOLOREDIT {
                        view.field_brush as isize
                    } else {
                        view.brush as isize
                    };
                }
                0
            }
            WM_DRAWITEM if l != 0 => {
                if let Ok(view) = (*raw).try_borrow() {
                    draw_item(&view, &*(l as *const DRAWITEMSTRUCT));
                }
                1
            }
            WM_MOUSEWHEEL => SendMessageW(GetParent(hwnd), msg, w, l),
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }
}
unsafe fn draw_item(view: &View, item: &DRAWITEMSTRUCT) {
    unsafe {
        let (_buffer, buffered) = design::PaintBuffer::item(item);
        let item = &buffered;
        if item.CtlType == ODT_COMBOBOX {
            design::draw_combo(item, view.dark);
            return;
        }
        let t = design::Theme::new(view.dark);
        let selected = item.itemState & ODS_SELECTED != 0;
        let focused = item.itemState & ODS_FOCUS != 0;
        let inset = |r: RECT, x, y, h| {
            design::rect(
                r.left + view.s(x),
                r.top + view.s(y),
                r.right - r.left - view.s(2 * x),
                view.s(h),
            )
        };
        if item.CtlID == LIST as u32 {
            design::fill(
                item.hDC,
                item.rcItem,
                if selected { t.tint } else { t.surface },
            );
            if let Some(id) = view.ids.get(item.itemID as usize) {
                let data = &view.store.borrow().data;
                let row = if view.timer_mode {
                    data.timers
                        .iter()
                        .find(|timer| timer.id == *id)
                        .map(|timer| {
                            (
                                timer.title.clone(),
                                format!(
                                    "{} · {}",
                                    timer.display(Utc::now()),
                                    if timer.finished {
                                        "已结束"
                                    } else if timer.due.is_none() {
                                        "已暂停"
                                    } else {
                                        "计时中"
                                    }
                                ),
                            )
                        })
                } else {
                    data.entries
                        .iter()
                        .find(|entry| entry.id == *id)
                        .map(|entry| {
                            (
                                entry.title.clone(),
                                entry.describe(Local::now().date_naive()),
                            )
                        })
                };
                if let Some((title, detail)) = row {
                    design::text(
                        item.hDC,
                        &title,
                        inset(item.rcItem, 12, 8, 24),
                        view.fonts[0],
                        t.text,
                        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
                    );
                    design::text(
                        item.hDC,
                        &detail,
                        inset(item.rcItem, 12, 34, 23),
                        view.fonts[1],
                        t.muted,
                        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
                    );
                }
            }
            if focused {
                DrawFocusRect(item.hDC, &item.rcItem);
            }
        } else {
            let id = item.CtlID as u16;
            let active_tab = (TYPE_TAB..TYPE_TAB + 6).contains(&id)
                && id - TYPE_TAB + 1 == view.filter as u16
                || (TIMER_TAB..TIMER_TAB + 4).contains(&id)
                    && id - TIMER_TAB == view.timer_tab as u16;
            let primary = matches!(id, COMMIT | FOCUS_TOGGLE | TIMER_START | SOUND_SAVE);
            let hover = !GetPropW(item.hwndItem, wide("LiteTray.Hover").as_ptr()).is_null();
            let disabled = item.itemState & ODS_DISABLED != 0;
            let color = if disabled {
                t.field
            } else if primary {
                t.accent
            } else if active_tab || hover || selected {
                t.tint
            } else {
                t.field
            };
            design::fill(item.hDC, item.rcItem, t.surface);
            design::rounded(
                item.hDC,
                item.rcItem,
                color,
                if active_tab || focused {
                    t.accent
                } else {
                    t.border
                },
                view.s(8),
            );
            design::text(
                item.hDC,
                &window_text(item.hwndItem),
                item.rcItem,
                view.fonts[0],
                if disabled {
                    t.muted
                } else if primary {
                    t.on_accent
                } else if active_tab {
                    t.accent
                } else {
                    t.text
                },
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        }
    }
}

unsafe fn choose_sound_file(owner: HWND) -> Result<Option<std::path::PathBuf>, String> {
    unsafe {
        use std::os::windows::ffi::OsStringExt;
        use windows_sys::Win32::UI::Controls::Dialogs::*;
        let mut path = vec![0u16; 32768];
        let filter = wide(tr("WAV 音乐 (*.wav)\0*.wav\0"));
        let title = wide(tr("选择提醒音乐"));
        let mut dialog: OPENFILENAMEW = std::mem::zeroed();
        dialog.lStructSize = size_of::<OPENFILENAMEW>() as u32;
        dialog.hwndOwner = owner;
        dialog.lpstrFilter = filter.as_ptr();
        dialog.lpstrTitle = title.as_ptr();
        dialog.lpstrFile = path.as_mut_ptr();
        dialog.nMaxFile = path.len() as u32;
        dialog.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | OFN_EXPLORER;
        if GetOpenFileNameW(&mut dialog) == 0 {
            let error = CommDlgExtendedError();
            return if error == 0 {
                Ok(None)
            } else {
                Err(format!("文件选择失败：0x{error:08X}"))
            };
        }
        let length = path.iter().position(|c| *c == 0).ok_or("文件路径过长")?;
        Ok(Some(std::ffi::OsString::from_wide(&path[..length]).into()))
    }
}
