use chrono::{Local, TimeZone, Utc};
use litetray::{
    clock,
    config::{self, Appearance, ClickAction, Config, MonitorScope, Provider},
    http, migration,
    platform::{clock_preview, credentials, diagnostics, lunar, startup, wide},
    popup,
    update::{self, UpdateStatus},
    weather,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    mem::size_of,
    ptr::{null, null_mut},
    rc::Rc,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use windows::Win32::System::WinRT::{RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::*, Gdi::*},
    System::{LibraryLoader::*, Threading::*},
    UI::{Controls::*, HiDpi::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
};

const CLASS: &str = "LiteTray.Settings.1";
const SS_LEFT: i32 = 0;
const PANEL_CLASS: &str = "LiteTray.Calendar.1";
const WM_TRAY: u32 = WM_APP + 1;
const WM_RESULT: u32 = WM_APP + 2;
const WM_SHOW_SETTINGS: u32 = WM_APP + 3;
const WM_SHOW_PANEL: u32 = WM_APP + 4;
const WM_CLOCK_ACTIVATED: u32 = WM_APP + 5;
const WM_PREVIEW_CLOCK: u32 = WM_APP + 6;
const NAV: u16 = 100;
const SAVE: u16 = 20;
const STATUS: u16 = 900;
const CLOCK_PREVIEW: u16 = 901;
const WEATHER_VIEW: u16 = 902;
const LUNAR_VIEW: u16 = 903;
const REFRESH: u16 = 30;
const UPDATE: u16 = 31;
const OPEN_RELEASE: u16 = 32;
const EXIT: u16 = 33;
const CALENDAR: u16 = 34;
const MONTHCAL: u16 = 35;
const PAGES: [&str; 7] = [
    "日历与天气",
    "时钟显示",
    "多屏托盘",
    "天气来源",
    "常规设置",
    "关于与更新",
    "配置迁移",
];
thread_local! {static STATE:RefCell<Option<App>>=const{RefCell::new(None)};}
// Independent of App's modal-dialog borrow. Each native call retains its own
// Rc, so reentrant window teardown cannot unload a module still on the stack.
thread_local! {static PREVIEW:RefCell<Option<Rc<clock_preview::Preview>>>=const{RefCell::new(None)};}
#[derive(Clone, Copy)]
struct Palette {
    background: HBRUSH,
    edit: HBRUSH,
    bg: u32,
    edit_bg: u32,
    text: u32,
    dark: bool,
    active: u16,
}
thread_local! {static PALETTE:Cell<Option<Palette>>=const{Cell::new(None)};}

enum Event {
    Weather(Box<(config::WeatherConfig, weather::WeatherResult)>),
    Update(Result<UpdateStatus, String>),
}
struct App {
    hwnd: HWND,
    panel: HWND,
    panel_font: HFONT,
    preview: Option<Rc<clock_preview::Preview>>,
    pending_activation: Option<litetray_adapter_protocol::activation::Activation>,
    children: Vec<HWND>,
    fields: HashMap<u16, HWND>,
    font: HFONT,
    title_font: HFONT,
    clock_font: HFONT,
    background: HBRUSH,
    edit_background: HBRUSH,
    dpi: u32,
    page: usize,
    draft: Config,
    saved: Config,
    status: String,
    weather: Option<weather::WeatherResult>,
    update: Option<UpdateStatus>,
    migration_source: Option<Vec<u8>>,
    migration: Option<migration::Report>,
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    weather_busy: bool,
    update_busy: bool,
    next_weather: Instant,
    engine: std::sync::Arc<std::sync::Mutex<weather::Engine>>,
    taskbar_created: u32,
    build: u32,
}

pub fn run(config: Config, mode: &str) -> Result<(), String> {
    unsafe {
        let profile_suffix = instance_suffix()?;
        let settings_class = format!("{CLASS}{profile_suffix}");
        let mutex_name = wide(&format!(
            "Local\\LiteTray.{}{profile_suffix}",
            std::env::var("USERNAME").unwrap_or_default(),
        ));
        let mutex = CreateMutexW(null(), 0, mutex_name.as_ptr());
        if mutex.is_null() {
            return Err("无法建立单实例锁".into());
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let other = FindWindowW(wide(&settings_class).as_ptr(), null());
            if !other.is_null() {
                PostMessageW(
                    other,
                    if mode == "--clock-preview" {
                        WM_PREVIEW_CLOCK
                    } else if mode == "--calendar" {
                        WM_SHOW_PANEL
                    } else {
                        WM_SHOW_SETTINGS
                    },
                    0,
                    0,
                );
            }
            CloseHandle(mutex);
            return Ok(());
        }
        let _ = RoInitialize(RO_INIT_SINGLETHREADED);
        let controls = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES | ICC_DATE_CLASSES,
        };
        InitCommonControlsEx(&controls);
        let instance = GetModuleHandleW(null());
        let class_name = wide(&settings_class);
        let panel_class = wide(PANEL_CLASS);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hIcon: LoadIconW(null_mut(), IDI_APPLICATION),
            lpszClassName: class_name.as_ptr(),
            ..std::mem::zeroed()
        };
        if RegisterClassW(&wc) == 0 {
            CloseHandle(mutex);
            return Err("注册窗口失败".into());
        }
        let pwc = WNDCLASSW {
            lpfnWndProc: Some(panel_proc),
            lpszClassName: panel_class.as_ptr(),
            ..wc
        };
        RegisterClassW(&pwc);
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            class_name.as_ptr(),
            wide("LiteTray · 轻托盘").as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            920,
            720,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            CloseHandle(mutex);
            return Err("创建设置窗口失败".into());
        }
        let dpi = GetDpiForWindow(hwnd).max(96);
        let (tx, rx) = mpsc::channel();
        let mut app = App {
            hwnd,
            panel: null_mut(),
            panel_font: null_mut(),
            preview: None,
            pending_activation: None,
            children: vec![],
            fields: HashMap::new(),
            font: null_mut(),
            title_font: null_mut(),
            clock_font: null_mut(),
            background: null_mut(),
            edit_background: null_mut(),
            dpi,
            page: 0,
            saved: config.clone(),
            draft: config,
            status: "欢迎使用 LiteTray。任务栏适配正在开发，现有系统托盘保持可用。".into(),
            weather: None,
            update: None,
            migration_source: None,
            migration: None,
            tx,
            rx,
            weather_busy: false,
            update_busy: false,
            next_weather: Instant::now(),
            engine: Default::default(),
            taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
            build: diagnostics::windows_build(),
        };
        app.recreate_resources();
        SetWindowPos(
            hwnd,
            null_mut(),
            0,
            0,
            app.scale(920),
            app.scale(720),
            SWP_NOMOVE | SWP_NOZORDER,
        );
        app.weather = weather::load_cache(
            &config::data_dir().join("weather-cache.json"),
            &app.draft.weather,
            Utc::now(),
        )
        .map(|w| weather::WeatherResult {
            weather: Some(w),
            cached: true,
            fallback: false,
            attempts: vec![],
            cache_error: None,
        });
        app.render();
        app.add_tray();
        app.start_weather(false);
        if app.draft.updates.check_on_startup {
            app.start_update();
        }
        STATE.with(|state| *state.borrow_mut() = Some(app));
        if mode != "--background" {
            if mode == "--clock-preview" {
                PostMessageW(hwnd, WM_PREVIEW_CLOCK, 0, 0);
            } else if mode == "--calendar" {
                PostMessageW(hwnd, WM_SHOW_PANEL, 0, 0);
            } else {
                ShowWindow(hwnd, SW_SHOW);
                UpdateWindow(hwnd);
            }
        }
        SetTimer(hwnd, 1, 1000, None);
        let mut message: MSG = std::mem::zeroed();
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            let preview = PREVIEW.with(|p| p.borrow().clone());
            if preview.as_ref().is_some_and(|p| p.pretranslate(&message)) {
                continue;
            }
            let panel = STATE.with(|s| s.borrow().as_ref().map(|a| a.panel).unwrap_or(null_mut()));
            if (!panel.is_null()
                && (message.hwnd == panel || IsChild(panel, message.hwnd) != 0)
                && IsDialogMessageW(panel, &message) != 0)
                || ((message.hwnd == hwnd || IsChild(hwnd, message.hwnd) != 0)
                    && IsDialogMessageW(hwnd, &message) != 0)
            {
                continue;
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        PREVIEW.with(|p| p.borrow_mut().take());
        STATE.with(|s| {
            if let Some(mut app) = s.borrow_mut().take() {
                app.preview.take();
                app.remove_tray();
                app.release_resources();
            }
        });
        CloseHandle(mutex);
        RoUninitialize();
        Ok(())
    }
}

impl App {
    fn scale(&self, value: i32) -> i32 {
        value * self.dpi as i32 / 96
    }
    fn dark(&self) -> bool {
        match self.draft.appearance {
            Appearance::Dark => true,
            Appearance::Light => false,
            Appearance::System => system_dark(),
        }
    }
    fn bg_color(&self) -> u32 {
        if self.dark() {
            rgb(32, 32, 32)
        } else {
            rgb(249, 249, 249)
        }
    }
    fn edit_color(&self) -> u32 {
        if self.dark() {
            rgb(43, 43, 43)
        } else {
            rgb(255, 255, 255)
        }
    }
    fn text_color(&self) -> u32 {
        if self.dark() {
            rgb(242, 242, 242)
        } else {
            rgb(28, 28, 28)
        }
    }
    unsafe fn recreate_resources(&mut self) {
        unsafe {
            self.release_resources();
            self.font = make_font(self.scale(15), 400, "Segoe UI");
            self.title_font = make_font(self.scale(25), 600, "Segoe UI");
            self.clock_font = make_font(self.scale(29), 400, "Segoe UI");
            self.background = CreateSolidBrush(self.bg_color());
            self.edit_background = CreateSolidBrush(self.edit_color());
            self.publish_palette();
            let dark = self.dark() as i32;
            if self.build >= 17763 {
                DwmSetWindowAttribute(
                    self.hwnd,
                    20,
                    &dark as *const _ as _,
                    size_of::<i32>() as u32,
                );
            }
        }
    }
    fn publish_palette(&self) {
        PALETTE.with(|p| {
            p.set(Some(Palette {
                background: self.background,
                edit: self.edit_background,
                bg: self.bg_color(),
                edit_bg: self.edit_color(),
                text: self.text_color(),
                dark: self.dark(),
                active: NAV + self.page as u16,
            }))
        });
    }
    unsafe fn release_resources(&mut self) {
        unsafe {
            self.close_panel();
            PALETTE.with(|p| p.set(None));
            for object in [
                self.font,
                self.title_font,
                self.clock_font,
                self.background,
                self.edit_background,
            ] {
                if !object.is_null() {
                    DeleteObject(object);
                }
            }
            self.font = null_mut();
            self.title_font = null_mut();
            self.clock_font = null_mut();
            self.background = null_mut();
            self.edit_background = null_mut();
        }
    }
    unsafe fn close_panel(&mut self) {
        unsafe {
            if !self.panel.is_null() {
                DestroyWindow(self.panel);
                self.panel = null_mut();
            }
            if !self.panel_font.is_null() {
                DeleteObject(self.panel_font);
                self.panel_font = null_mut();
            }
        }
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Thin Win32 control factory with position and size"
    )]
    unsafe fn control(
        &mut self,
        class: &str,
        text: &str,
        id: u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    ) -> HWND {
        unsafe {
            let hwnd = CreateWindowExW(
                if class == "EDIT" { WS_EX_CLIENTEDGE } else { 0 },
                wide(class).as_ptr(),
                wide(text).as_ptr(),
                WS_CHILD | WS_VISIBLE | style,
                self.scale(x),
                self.scale(y),
                self.scale(w),
                self.scale(h),
                self.hwnd,
                id as usize as HMENU,
                GetModuleHandleW(null()),
                null(),
            );
            SendMessageW(hwnd, WM_SETFONT, self.font as usize, 1);
            if class == "BUTTON" && style & 0xf == BS_AUTOCHECKBOX as u32 {
                SetWindowTheme(hwnd, wide("").as_ptr(), wide("").as_ptr());
            }
            if class == "SysMonthCal32" {
                theme_calendar(hwnd, self.font);
            }
            if class == "EDIT" {
                SendMessageW(hwnd, EM_SETLIMITTEXT, 2048, 0);
            }
            self.children.push(hwnd);
            if id != 0 {
                self.fields.insert(id, hwnd);
            }
            hwnd
        }
    }
    unsafe fn label(&mut self, text: &str, x: i32, y: i32, w: i32, h: i32) -> HWND {
        unsafe { self.control("STATIC", text, 0, SS_LEFT as u32, x, y, w, h) }
    }
    unsafe fn button(&mut self, text: &str, id: u16, x: i32, y: i32, w: i32) -> HWND {
        unsafe {
            self.control(
                "BUTTON",
                text,
                id,
                WS_TABSTOP | BS_OWNERDRAW as u32,
                x,
                y,
                w,
                34,
            )
        }
    }
    unsafe fn edit(&mut self, label: &str, value: &str, id: u16, x: i32, y: i32, w: i32) {
        unsafe {
            self.label(label, x, y, w, 22);
            self.control(
                "EDIT",
                value,
                id,
                WS_TABSTOP | ES_AUTOHSCROLL as u32,
                x,
                y + 25,
                w,
                30,
            );
        }
    }
    unsafe fn check(&mut self, label: &str, checked: bool, id: u16, x: i32, y: i32, w: i32) {
        unsafe {
            let hwnd = self.control(
                "BUTTON",
                label,
                id,
                WS_TABSTOP | BS_AUTOCHECKBOX as u32,
                x,
                y,
                w,
                28,
            );
            SendMessageW(
                hwnd,
                BM_SETCHECK,
                if checked { BST_CHECKED as usize } else { 0 },
                0,
            );
        }
    }
    unsafe fn combo(&mut self, items: &[&str], index: usize, id: u16, x: i32, y: i32, w: i32) {
        unsafe {
            let hwnd = self.control(
                "COMBOBOX",
                "",
                id,
                WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32,
                x,
                y,
                w,
                220,
            );
            for item in items {
                SendMessageW(hwnd, CB_ADDSTRING, 0, wide(item).as_ptr() as isize);
            }
            SendMessageW(hwnd, CB_SETCURSEL, index, 0);
        }
    }
    fn field(&self, id: u16) -> HWND {
        self.fields.get(&id).copied().unwrap_or(null_mut())
    }
    fn text(&self, id: u16) -> String {
        unsafe { window_text(self.field(id)) }
    }
    fn checked(&self, id: u16) -> bool {
        unsafe { SendMessageW(self.field(id), BM_GETCHECK, 0, 0) == BST_CHECKED as isize }
    }
    fn selected(&self, id: u16) -> usize {
        unsafe { SendMessageW(self.field(id), CB_GETCURSEL, 0, 0).max(0) as usize }
    }
    unsafe fn set_status(&mut self, text: impl Into<String>) {
        unsafe {
            self.status = text.into();
            SetWindowTextW(self.field(STATUS), wide(&self.status).as_ptr());
        }
    }

    unsafe fn render(&mut self) {
        unsafe {
            self.publish_palette();
            for child in self.children.drain(..) {
                DestroyWindow(child);
            }
            self.fields.clear();
            let mut rect = std::mem::zeroed();
            GetClientRect(self.hwnd, &mut rect);
            let width = rect.right * 96 / self.dpi as i32;
            let height = rect.bottom * 96 / self.dpi as i32;
            let brand = self.label("LiteTray", 22, 24, 170, 38);
            SendMessageW(brand, WM_SETFONT, self.title_font as usize, 1);
            self.label("轻托盘 · Rust 原生", 24, 66, 170, 25);
            for (i, title) in PAGES.iter().enumerate() {
                self.button(title, NAV + i as u16, 20, 112 + i as i32 * 46, 168);
            }
            self.label(
                &format!(
                    "v{}\r\nWindows build {}",
                    env!("CARGO_PKG_VERSION"),
                    self.build
                ),
                24,
                height - 90,
                165,
                50,
            );
            let heading = self.label(PAGES[self.page], 224, 26, width - 250, 38);
            SendMessageW(heading, WM_SETFONT, self.title_font as usize, 1);
            let c = self.draft.clone();
            match self.page {
                0 => {
                    self.control(
                        "STATIC",
                        "",
                        CLOCK_PREVIEW,
                        SS_LEFT as u32,
                        224,
                        82,
                        width - 250,
                        42,
                    );
                    SendMessageW(
                        self.field(CLOCK_PREVIEW),
                        WM_SETFONT,
                        self.clock_font as usize,
                        1,
                    );
                    self.label(
                        "本地日历与天气 · 点击托盘图标也可展开",
                        224,
                        128,
                        width - 250,
                        26,
                    );
                    self.control(
                        "SysMonthCal32",
                        "",
                        MONTHCAL,
                        WS_TABSTOP | MCS_NOTODAYCIRCLE,
                        224,
                        174,
                        300,
                        245,
                    );
                    self.control("STATIC", "", LUNAR_VIEW, SS_LEFT as u32, 224, 432, 310, 48);
                    self.control(
                        "STATIC",
                        "",
                        WEATHER_VIEW,
                        SS_LEFT as u32,
                        548,
                        174,
                        (width - 575).max(230),
                        300,
                    );
                    self.button("刷新天气", REFRESH, 548, 483, 112);
                    self.button("展开面板", CALENDAR, 674, 483, 112);
                }
                1 => {
                    self.edit("第一行模板", &c.clock.top_line, 201, 224, 84, width - 260);
                    self.edit(
                        "第二行模板",
                        &c.clock.bottom_line,
                        202,
                        224,
                        154,
                        width - 260,
                    );
                    self.label(
                        "常用：%date% %time% %weekday% %weather%\r\n格式：%year% %month2% %day2% %hour% %minute% %second%",
                        224,
                        218,
                        width - 260,
                        42,
                    );
                    self.check("显示秒", c.clock.show_seconds, 203, 224, 268, 140);
                    self.check("日历显示农历", c.clock.show_lunar, 204, 404, 268, 180);
                    self.edit("字体", &c.clock.font_family, 205, 224, 312, 260);
                    self.edit(
                        "字号（8–32）",
                        &c.clock.font_size.to_string(),
                        206,
                        506,
                        312,
                        130,
                    );
                    self.edit(
                        "时钟宽度（80–600）",
                        &c.clock.width.to_string(),
                        207,
                        656,
                        312,
                        160,
                    );
                    self.label("点击时钟", 224, 388, 180, 25);
                    self.combo(
                        &["日历与天气面板", "系统日历"],
                        if c.clock.click_action == ClickAction::CalendarWeather {
                            0
                        } else {
                            1
                        },
                        208,
                        224,
                        418,
                        330,
                    );
                    self.button("打开原生时钟预览", 209, 574, 418, 250);
                    self.label(
                        "原生预览使用 LiteTray 日历，可检查两行显示、鼠标与键盘操作；拖动窗口可检查其他屏幕。任务栏接管和系统日历行为仍待验证。",
                        224,
                        480,
                        width - 260,
                        55,
                    );
                }
                2 => {
                    self.label("显示范围", 224, 85, 300, 25);
                    self.combo(
                        &["所有显示器", "仅主显示器", "指定显示器"],
                        match c.tray.scope {
                            MonitorScope::All => 0,
                            MonitorScope::Primary => 1,
                            MonitorScope::Selected => 2,
                        },
                        301,
                        224,
                        114,
                        340,
                    );
                    self.check(
                        "显示托盘图标和隐藏图标入口",
                        c.tray.show_icons,
                        302,
                        224,
                        162,
                        400,
                    );
                    self.check(
                        "显示网络 / 音量 / 电池控制中心",
                        c.tray.show_control_center,
                        303,
                        224,
                        200,
                        400,
                    );
                    self.check(
                        "组件规则仅影响复制的副屏任务栏",
                        c.tray.copied_taskbars_only,
                        304,
                        224,
                        238,
                        460,
                    );
                    self.label(
                        "显示器设备名（指定显示器模式下用逗号分隔）",
                        224,
                        282,
                        width - 260,
                        24,
                    );
                    self.control(
                        "EDIT",
                        &c.tray.selected_devices.join(","),
                        305,
                        WS_TABSTOP | ES_AUTOHSCROLL as u32,
                        224,
                        310,
                        width - 260,
                        30,
                    );
                    let d = diagnostics::collect();
                    let text = d
                        .monitors
                        .iter()
                        .map(|m| {
                            format!(
                                "{}{}  {} × {}",
                                m.device,
                                if m.primary { " · 主屏" } else { "" },
                                m.rect[2] - m.rect[0],
                                m.rect[3] - m.rect[1]
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\r\n");
                    self.label(&text, 224, 360, width - 260, 100);
                    self.label(
                        "任务栏适配：尚未接管。配置不会禁用现有 Windhawk 模块。",
                        224,
                        482,
                        width - 260,
                        52,
                    );
                }
                3 => {
                    self.check("启用天气", c.weather.enabled, 400, 224, 82, 220);
                    self.edit("城市名称", &c.weather.city, 401, 224, 119, 240);
                    self.edit("纬度", &c.weather.latitude.to_string(), 402, 485, 119, 145);
                    self.edit("经度", &c.weather.longitude.to_string(), 403, 650, 119, 145);
                    self.label(
                        "来源优先级 · 上方优先，失败或过期时自动回退",
                        224,
                        189,
                        570,
                        24,
                    );
                    let list = self.control(
                        "LISTBOX",
                        "",
                        404,
                        WS_TABSTOP | WS_BORDER | WS_VSCROLL | LBS_NOTIFY as u32,
                        224,
                        221,
                        300,
                        123,
                    );
                    for provider in &c.weather.providers {
                        SendMessageW(
                            list,
                            LB_ADDSTRING,
                            0,
                            wide(provider.label()).as_ptr() as isize,
                        );
                    }
                    SendMessageW(list, LB_SETCURSEL, 0, 0);
                    self.combo(&Provider::ALL.map(|p| p.label()), 0, 405, 543, 221, 250);
                    self.button("添加", 406, 543, 260, 75);
                    self.button("移除", 407, 630, 260, 75);
                    self.button("上移", 408, 543, 305, 75);
                    self.button("下移", 409, 630, 305, 75);
                    self.edit(
                        "刷新间隔（分钟）",
                        &c.weather.refresh_minutes.to_string(),
                        414,
                        224,
                        359,
                        160,
                    );
                    self.edit(
                        "请求超时（秒）",
                        &c.weather.timeout_seconds.to_string(),
                        415,
                        404,
                        359,
                        160,
                    );
                    self.edit(
                        "和风 API Host",
                        &c.weather.qweather_host,
                        416,
                        584,
                        359,
                        210,
                    );
                    self.label("API Key（保存在 Windows 凭据管理器）", 224, 428, 560, 23);
                    self.combo(&["和风天气", "OpenWeather"], 0, 410, 224, 457, 160);
                    self.control(
                        "EDIT",
                        "",
                        411,
                        WS_TABSTOP | ES_AUTOHSCROLL as u32 | ES_PASSWORD as u32,
                        400,
                        457,
                        220,
                        30,
                    );
                    self.button("保存 Key", 412, 634, 455, 85);
                    self.button("删除 Key", 413, 729, 455, 85);
                    self.label("无需密钥：Open-Meteo / MET Norway / wttr.in。各来源更新频率与服务条款不同。",224,507,width-260,40);
                }
                4 => {
                    self.label("外观", 224, 90, 200, 24);
                    self.combo(
                        &["跟随 Windows", "浅色", "深色"],
                        match c.appearance {
                            Appearance::System => 0,
                            Appearance::Light => 1,
                            Appearance::Dark => 2,
                        },
                        501,
                        224,
                        120,
                        300,
                    );
                    self.check(
                        "登录 Windows 时启动",
                        c.start_with_windows,
                        502,
                        224,
                        176,
                        350,
                    );
                    self.label("关闭设置窗口会收至托盘；右键托盘图标可退出程序。\r\n配置和天气缓存保存在当前用户目录，API Key 不写入配置文件。",224,231,width-260,65);
                    self.button("打开配置目录", 503, 224, 320, 170);
                    self.button("导出诊断信息", 504, 412, 320, 170);
                    self.button("退出程序", EXIT, 600, 320, 170);
                    self.label("兼容策略：先检测系统能力，再启用对应任务栏适配。\r\nWindows 10 经典任务栏和 Windows 11 XAML 任务栏分别实现。\r\n未知系统版本不使用未验证的内存偏移。",224,389,width-260,100);
                }
                5 => {
                    self.label(&format!("LiteTray 轻托盘  {}\r\nRust 原生 Windows 应用 · x64\r\n多屏托盘、自定义时钟、日历和可回退天气",env!("CARGO_PKG_VERSION")),224,86,width-260,80);
                    self.edit(
                        "GitHub 更新仓库（owner/repository）",
                        &c.updates.repository,
                        601,
                        224,
                        187,
                        width - 260,
                    );
                    self.check(
                        "允许预览版本",
                        c.updates.include_prerelease,
                        602,
                        224,
                        253,
                        220,
                    );
                    self.check(
                        "启动时检查更新",
                        c.updates.check_on_startup,
                        603,
                        480,
                        253,
                        240,
                    );
                    self.label(
                        "私有仓库 Token（仅需本仓库 Contents 读取权限）",
                        224,
                        302,
                        width - 260,
                        24,
                    );
                    self.control(
                        "EDIT",
                        "",
                        604,
                        WS_TABSTOP | ES_AUTOHSCROLL as u32 | ES_PASSWORD as u32,
                        224,
                        334,
                        360,
                        30,
                    );
                    self.button("保存 Token", 605, 602, 332, 105);
                    self.button("删除 Token", 606, 720, 332, 105);
                    self.button("检查更新", UPDATE, 224, 397, 130);
                    self.button("打开发布页面", OPEN_RELEASE, 372, 397, 145);
                    self.button("打开项目", 607, 535, 397, 120);
                    self.label("检查通过 GitHub Releases 获取版本信息。私有仓库需要单独保存访问凭据。\r\n检测到新版后打开 GitHub 下载页，不会静默替换正在运行的程序。",224,458,width-260,65);
                    self.button("退出 LiteTray", EXIT, 224, 535, 145);
                }
                6 => {
                    self.label("选择 tools/Export-WindhawkProfile.ps1 导出的 JSON 文件，查看逐项转换结果。",224,84,width-260,45);
                    self.button("选择导出文件", 701, 224, 137, 170);
                    self.button("采用到设置草稿", 702, 410, 137, 170);
                    self.button("导出迁移报告", 703, 596, 137, 170);
                    EnableWindow(self.field(702), self.migration.is_some() as i32);
                    EnableWindow(self.field(703), self.migration.is_some() as i32);
                    let text=self.migration.as_ref().map(migration::Report::text).unwrap_or_else(||"尚未选择文件。\r\n\r\n迁移只更新 LiteTray 的设置草稿；检查后点击“保存设置”生效。\r\n原 Windhawk 设置保持不变。".into());
                    let report_control = self.control(
                        "EDIT",
                        "",
                        704,
                        WS_TABSTOP
                            | WS_VSCROLL
                            | ES_MULTILINE as u32
                            | ES_AUTOVSCROLL as u32
                            | ES_READONLY as u32,
                        224,
                        186,
                        width - 260,
                        (height - 290).max(220),
                    );
                    SendMessageW(report_control, EM_SETLIMITTEXT, 2 * 1024 * 1024, 0);
                    SetWindowTextW(report_control, wide(&text).as_ptr());
                }
                _ => {}
            }
            self.control(
                "STATIC",
                &self.status.clone(),
                STATUS,
                SS_LEFT as u32,
                224,
                height - 79,
                width - 415,
                64,
            );
            self.button("保存设置", SAVE, width - 155, height - 68, 125);
            self.update_views();
            if self.page == 0 {
                update_selected_date(self.hwnd, self.draft.clock.show_lunar);
            }
            InvalidateRect(self.hwnd, null(), 1);
        }
    }

    fn collect_page(&mut self) -> Result<(), String> {
        let mut c = self.draft.clone();
        match self.page {
            1 => {
                c.clock.top_line = self.text(201);
                c.clock.bottom_line = self.text(202);
                c.clock.show_seconds = self.checked(203);
                c.clock.show_lunar = self.checked(204);
                c.clock.font_family = self.text(205);
                c.clock.font_size = self.text(206).parse().map_err(|_| "字号必须是整数")?;
                c.clock.width = self.text(207).parse().map_err(|_| "宽度必须是整数")?;
                c.clock.click_action = if self.selected(208) == 0 {
                    ClickAction::CalendarWeather
                } else {
                    ClickAction::SystemCalendar
                };
            }
            2 => {
                c.tray.scope = match self.selected(301) {
                    0 => MonitorScope::All,
                    1 => MonitorScope::Primary,
                    _ => MonitorScope::Selected,
                };
                c.tray.show_icons = self.checked(302);
                c.tray.show_control_center = self.checked(303);
                c.tray.copied_taskbars_only = self.checked(304);
                c.tray.selected_devices = self
                    .text(305)
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect();
            }
            3 => {
                c.weather.enabled = self.checked(400);
                c.weather.city = self.text(401);
                c.weather.latitude = self.text(402).parse().map_err(|_| "纬度必须是数字")?;
                c.weather.longitude = self.text(403).parse().map_err(|_| "经度必须是数字")?;
                c.weather.refresh_minutes =
                    self.text(414).parse().map_err(|_| "刷新间隔必须是整数")?;
                c.weather.timeout_seconds = self.text(415).parse().map_err(|_| "超时必须是整数")?;
                c.weather.qweather_host = self.text(416).trim().into();
            }
            4 => {
                c.appearance = match self.selected(501) {
                    0 => Appearance::System,
                    1 => Appearance::Light,
                    _ => Appearance::Dark,
                };
                c.start_with_windows = self.checked(502);
            }
            5 => {
                c.updates.repository = self.text(601).trim().into();
                c.updates.include_prerelease = self.checked(602);
                c.updates.check_on_startup = self.checked(603);
            }
            _ => {}
        }
        // Full validation belongs to Save; adding a keyed provider is possible before entering its Host.
        self.draft = c;
        Ok(())
    }
    unsafe fn save(&mut self) -> Result<(), String> {
        unsafe {
            self.collect_page()?;
            self.draft.validate()?;
            let old = self.saved.clone();
            if old.start_with_windows != self.draft.start_with_windows {
                startup::set_enabled(self.draft.start_with_windows)?;
            }
            if let Err(error) = config::save(&config::data_dir().join("config.json"), &self.draft) {
                if old.start_with_windows != self.draft.start_with_windows {
                    let _ = startup::set_enabled(old.start_with_windows);
                }
                return Err(error);
            }
            self.saved = self.draft.clone();
            self.status = "设置已保存".into();
            self.engine = Default::default();
            self.recreate_resources();
            self.render();
            self.next_weather = Instant::now();
            self.start_weather(true);
            Ok(())
        }
    }
    unsafe fn update_views(&mut self) {
        unsafe {
            let now = Local::now();
            let summary = self
                .weather
                .as_ref()
                .and_then(|r| r.weather.as_ref())
                .map(|w| w.summary())
                .unwrap_or_else(|| "天气暂无数据".into());
            if self.page == 0 {
                set_text(
                    self.field(CLOCK_PREVIEW),
                    &clock::format(&self.draft.clock.top_line, &self.draft.clock, now, &summary),
                );
                set_text(self.field(WEATHER_VIEW), &self.weather_text());
            }
            if let Some(preview) = &self.preview {
                let _ = preview.update(&clock_preview::frame(&self.draft.clock, &summary));
            }
            if !self.panel.is_null() {
                let weather = GetDlgItem(self.panel, WEATHER_VIEW as i32);
                set_text(weather, &self.weather_text());
                let title = GetDlgItem(self.panel, CLOCK_PREVIEW as i32);
                set_text(
                    title,
                    &format!(
                        "{}    {}",
                        now.format("%Y年%m月%d日"),
                        now.format(if self.saved.clock.show_seconds {
                            "%H:%M:%S"
                        } else {
                            "%H:%M"
                        })
                    ),
                );
            }
        }
    }
    fn weather_text(&self) -> String {
        if !self.saved.weather.enabled {
            return "天气已关闭".into();
        }
        let Some(result) = &self.weather else {
            return "正在获取天气…\r\n可在天气来源中配置城市和备用来源".into();
        };
        let Some(w) = &result.weather else {
            return format!(
                "天气暂不可用\r\n\r\n{}",
                result
                    .attempts
                    .iter()
                    .map(|a| format!("{}：{}", a.provider.label(), a.error))
                    .collect::<Vec<_>>()
                    .join("\r\n")
            );
        };
        let mut lines = vec![
            w.city.clone(),
            format!("{:.1}°C  {}", w.temperature_c, w.description),
        ];
        if let Some(t) = w.feels_like_c {
            lines.push(format!("体感 {t:.0}°C"));
        }
        if let Some(h) = w.humidity_percent {
            lines.push(format!("湿度 {h:.0}%"));
        }
        if let Some(wind) = w.wind_kmh {
            lines.push(format!("风速 {wind:.0} km/h"));
        }
        lines.push(String::new());
        for day in &w.days {
            lines.push(format!(
                "{}  {}  {:.0}° / {:.0}°",
                day.date.get(5..).unwrap_or(&day.date),
                day.description,
                day.low_c,
                day.high_c
            ));
        }
        lines.push(String::new());
        lines.push(result.status());
        if let Some(e) = &result.cache_error {
            lines.push(format!("缓存保存失败：{e}"));
        }
        lines.join("\r\n")
    }
    unsafe fn start_weather(&mut self, force: bool) {
        unsafe {
            if self.weather_busy || !self.saved.weather.enabled {
                return;
            }
            self.weather_busy = true;
            self.next_weather = Instant::now()
                + Duration::from_secs(self.saved.weather.refresh_minutes as u64 * 60);
            let config = self.saved.weather.clone();
            let sender = self.tx.clone();
            let window = self.hwnd as usize;
            let engine = self.engine.clone();
            thread::spawn(move || {
                let result = engine.lock().unwrap_or_else(|p| p.into_inner()).refresh(
                    &http::Client::new(config.timeout_seconds),
                    &credentials::WindowsSecrets,
                    &config,
                    &config::data_dir().join("weather-cache.json"),
                    Utc::now(),
                    force,
                );
                let _ = sender.send(Event::Weather(Box::new((config, result))));
                PostMessageW(window as HWND, WM_RESULT, 0, 0);
            });
        }
    }
    unsafe fn start_update(&mut self) {
        unsafe {
            if self.update_busy {
                return;
            }
            if let Err(e) = self.collect_page() {
                self.set_status(e);
                return;
            }
            if let Err(e) = config::validate_repository(&self.draft.updates.repository) {
                self.set_status(e);
                return;
            }
            self.update_busy = true;
            self.set_status("正在连接 GitHub 检查更新…");
            let config = self.draft.updates.clone();
            let sender = self.tx.clone();
            let window = self.hwnd as usize;
            thread::spawn(move || {
                let result = credentials::read(update::CREDENTIAL_TARGET).and_then(|token| {
                    update::check(
                        &http::Client::new(15),
                        &config,
                        token.as_deref(),
                        env!("CARGO_PKG_VERSION"),
                    )
                });
                let _ = sender.send(Event::Update(result));
                PostMessageW(window as HWND, WM_RESULT, 0, 0);
            });
        }
    }
    unsafe fn receive(&mut self) {
        unsafe {
            while let Ok(event) = self.rx.try_recv() {
                match event {
                    Event::Weather(response) => {
                        let (request_config, result) = *response;
                        self.weather_busy = false;
                        if serde_json::to_value(&request_config).ok()
                            != serde_json::to_value(&self.saved.weather).ok()
                        {
                            self.start_weather(true);
                            continue;
                        }
                        self.set_status(result.status());
                        self.weather = Some(result);
                        self.update_views();
                    }
                    Event::Update(result) => {
                        self.update_busy = false;
                        match result {
                            Ok(status) => {
                                self.set_status(status.text());
                                self.update = Some(status);
                            }
                            Err(error) => self.set_status(error),
                        }
                    }
                }
            }
        }
    }
    unsafe fn schedule_tick(&self) {
        unsafe {
            let visible = IsWindowVisible(self.hwnd) != 0
                || (!self.panel.is_null() && IsWindowVisible(self.panel) != 0)
                || self.preview.as_ref().is_some_and(|p| p.visible());
            let interval = if visible
                && (clock::needs_second_ticks(&self.draft.clock)
                    || clock::needs_second_ticks(&self.saved.clock))
            {
                1000
            } else {
                60000
            };
            // An existing native preview owns a leased clock session even
            // while hidden; keep updates within its heartbeat budget.
            let interval = if self.preview.is_some() {
                interval.min(5000)
            } else {
                interval
            };
            SetTimer(self.hwnd, 1, interval, None);
        }
    }
    unsafe fn add_tray(&self) {
        unsafe {
            let mut data: NOTIFYICONDATAW = std::mem::zeroed();
            data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
            data.hWnd = self.hwnd;
            data.uID = 1;
            data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
            data.uCallbackMessage = WM_TRAY;
            data.hIcon = LoadIconW(null_mut(), IDI_APPLICATION);
            let tip = wide("LiteTray · 日历与天气\n左键展开面板，右键打开菜单");
            for (dest, src) in data.szTip.iter_mut().zip(tip) {
                *dest = src;
            }
            Shell_NotifyIconW(NIM_ADD, &data);
        }
    }
    unsafe fn remove_tray(&self) {
        unsafe {
            let mut data: NOTIFYICONDATAW = std::mem::zeroed();
            data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
            data.hWnd = self.hwnd;
            data.uID = 1;
            Shell_NotifyIconW(NIM_DELETE, &data);
        }
    }
    unsafe fn tray_menu(&mut self) {
        unsafe {
            let menu = CreatePopupMenu();
            AppendMenuW(
                menu,
                MF_STRING,
                CALENDAR as usize,
                wide("日历与天气").as_ptr(),
            );
            AppendMenuW(menu, MF_STRING, 40, wide("设置").as_ptr());
            AppendMenuW(menu, MF_STRING, UPDATE as usize, wide("检查更新").as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
            AppendMenuW(menu, MF_STRING, EXIT as usize, wide("退出").as_ptr());
            let mut point = std::mem::zeroed();
            GetCursorPos(&mut point);
            SetForegroundWindow(self.hwnd);
            let command = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                0,
                self.hwnd,
                null(),
            );
            DestroyMenu(menu);
            if command != 0 {
                PostMessageW(self.hwnd, WM_COMMAND, command as usize, 0);
            }
            PostMessageW(self.hwnd, WM_NULL, 0, 0);
        }
    }
    unsafe fn show_panel(&mut self) {
        unsafe {
            // Prefer the actual tray icon bounds for keyboard activation too.
            let id = NOTIFYICONIDENTIFIER {
                cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
                hWnd: self.hwnd,
                uID: 1,
                guidItem: std::mem::zeroed(),
            };
            let mut rect: RECT = std::mem::zeroed();
            if Shell_NotifyIconGetRect(&id, &mut rect) < 0 {
                let mut point = POINT { x: 0, y: 0 };
                GetCursorPos(&mut point);
                rect = RECT {
                    left: point.x,
                    top: point.y,
                    right: point.x.saturating_add(1),
                    bottom: point.y.saturating_add(1),
                };
            }
            self.show_panel_at(popup::Rect {
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
            });
        }
    }
    unsafe fn show_panel_at(&mut self, anchor: popup::Rect) {
        unsafe {
            if anchor.validate().is_err() {
                return;
            }
            let rect = RECT {
                left: anchor.left,
                top: anchor.top,
                right: anchor.right,
                bottom: anchor.bottom,
            };
            let monitor = MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST);
            if !self.panel.is_null()
                && IsWindowVisible(self.panel) != 0
                && MonitorFromWindow(self.panel, MONITOR_DEFAULTTONEAREST) == monitor
            {
                ShowWindow(self.panel, SW_HIDE);
                return;
            }
            self.close_panel();
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info) == 0 {
                return;
            }
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_CONTROLPARENT,
                wide(PANEL_CLASS).as_ptr(),
                wide("LiteTray 日历与天气").as_ptr(),
                WS_POPUP | WS_BORDER | WS_CLIPCHILDREN,
                anchor.left,
                anchor.top,
                1,
                1,
                self.hwnd,
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            if hwnd.is_null() {
                self.set_status("无法创建日历天气面板");
                return;
            }
            self.panel = hwnd;
            // Derive scale from the popup's actual monitor. Settings can be on
            // a completely different display with a different effective DPI.
            let dpi = GetDpiForWindow(hwnd).max(96) as i32;
            let work = popup::Rect {
                left: info.rcWork.left,
                top: info.rcWork.top,
                right: info.rcWork.right,
                bottom: info.rcWork.bottom,
            };
            let placement = match popup::place(anchor, work, dpi as u32) {
                Ok(p) => p,
                Err(e) => {
                    self.close_panel();
                    self.set_status(e);
                    return;
                }
            };
            let scale = |value: i32| {
                (i64::from(value) * i64::from(dpi) * i64::from(placement.scale_percent) / 9600)
                    .max(1) as i32
            };
            self.panel_font = make_font(scale(15), 400, "Segoe UI");
            let width = placement.rect.right - placement.rect.left;
            let height = placement.rect.bottom - placement.rect.top;
            let x = placement.rect.left;
            let y = placement.rect.top;
            SetWindowPos(
                hwnd,
                null_mut(),
                x,
                y,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            let control =
                |class: &str, text: &str, id: u16, style: u32, x: i32, y: i32, w: i32, h: i32| {
                    let style = if class == "BUTTON" {
                        (style & !0xf) | BS_OWNERDRAW as u32
                    } else {
                        style
                    };
                    let c = CreateWindowExW(
                        0,
                        wide(class).as_ptr(),
                        wide(text).as_ptr(),
                        WS_CHILD | WS_VISIBLE | style,
                        scale(x),
                        scale(y),
                        scale(w),
                        scale(h),
                        hwnd,
                        id as usize as HMENU,
                        GetModuleHandleW(null()),
                        null(),
                    );
                    SendMessageW(c, WM_SETFONT, self.panel_font as usize, 1);
                    if class == "SysMonthCal32" {
                        theme_calendar(c, self.panel_font);
                    }
                    c
                };
            control("STATIC", "", CLOCK_PREVIEW, 0, 20, 18, 540, 34);
            control(
                "BUTTON",
                "设置",
                40,
                WS_TABSTOP | BS_PUSHBUTTON as u32,
                574,
                15,
                78,
                32,
            );
            control(
                "SysMonthCal32",
                "",
                MONTHCAL,
                WS_TABSTOP | MCS_NOTODAYCIRCLE,
                20,
                72,
                305,
                260,
            );
            let lunar = if self.saved.clock.show_lunar {
                lunar::describe(Local::now()).unwrap_or_default()
            } else {
                String::new()
            };
            control("STATIC", &lunar, LUNAR_VIEW, 0, 22, 354, 300, 30);
            control(
                "STATIC",
                &self.weather_text(),
                WEATHER_VIEW,
                0,
                352,
                75,
                300,
                300,
            );
            control(
                "BUTTON",
                "刷新天气",
                REFRESH,
                WS_TABSTOP | BS_PUSHBUTTON as u32,
                352,
                390,
                115,
                32,
            );
            control(
                "BUTTON",
                "关闭",
                41,
                WS_TABSTOP | BS_PUSHBUTTON as u32,
                545,
                390,
                105,
                32,
            );
            let rounded = 2_i32;
            if self.build >= 22000 {
                DwmSetWindowAttribute(hwnd, 33, &rounded as *const _ as _, 4);
            }
            self.update_views();
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
            SetFocus(GetDlgItem(hwnd, MONTHCAL as i32));
            self.schedule_tick();
        }
    }
    unsafe fn command(&mut self, id: u16) -> bool {
        unsafe {
            if (NAV..NAV + PAGES.len() as u16).contains(&id) {
                if let Err(e) = self.collect_page() {
                    self.set_status(e);
                    return false;
                }
                self.page = (id - NAV) as usize;
                self.render();
                return false;
            }
            match id {
                SAVE => {
                    if let Err(e) = self.save() {
                        self.set_status(format!("保存失败：{e}"));
                    }
                }
                REFRESH => {
                    self.set_status("正在刷新天气…");
                    self.start_weather(true);
                }
                UPDATE => self.start_update(),
                CALENDAR => self.show_panel(),
                209 => {
                    if let Err(e) = self.collect_page().and_then(|_| self.draft.validate()) {
                        self.set_status(e);
                        return false;
                    }
                    let summary = self
                        .weather
                        .as_ref()
                        .and_then(|r| r.weather.as_ref())
                        .map(|w| w.summary())
                        .unwrap_or_else(|| "天气暂无数据".into());
                    if let Some(preview) = &self.preview {
                        preview.show();
                    } else {
                        match clock_preview::Preview::open(
                            self.hwnd,
                            clock_preview::frame(&self.draft.clock, &summary),
                        ) {
                            Ok(preview) => {
                                let preview = Rc::new(preview);
                                PREVIEW.with(|p| *p.borrow_mut() = Some(preview.clone()));
                                self.preview = Some(preview);
                            }
                            Err(e) => self.set_status(e),
                        }
                    }
                    self.schedule_tick();
                }
                40 => {
                    ShowWindow(self.panel, SW_HIDE);
                    ShowWindow(self.hwnd, SW_SHOW);
                    SetForegroundWindow(self.hwnd);
                }
                EXIT => {
                    if let Err(e) = self.collect_page() {
                        self.set_status(e);
                        return false;
                    }
                    if serde_json::to_value(&self.draft).ok()
                        != serde_json::to_value(&self.saved).ok()
                    {
                        let choice = MessageBoxW(
                            self.hwnd,
                            wide("设置尚未保存，是否保存后退出？").as_ptr(),
                            wide("LiteTray").as_ptr(),
                            MB_YESNOCANCEL | MB_ICONQUESTION,
                        );
                        if choice == IDCANCEL {
                            return false;
                        }
                        if choice == IDYES
                            && let Err(e) = self.save()
                        {
                            self.set_status(e);
                            return false;
                        }
                    }
                    return true;
                }
                406..=409 => {
                    if let Err(e) = self.collect_page() {
                        self.set_status(e);
                        return false;
                    }
                    let selected = SendMessageW(self.field(404), LB_GETCURSEL, 0, 0);
                    let len = self.draft.weather.providers.len();
                    match id {
                        406 => {
                            let provider = Provider::ALL[self.selected(405).min(4)];
                            if !self.draft.weather.providers.contains(&provider) {
                                self.draft.weather.providers.push(provider);
                            }
                        }
                        407 if selected >= 0 && (selected as usize) < len => {
                            self.draft.weather.providers.remove(selected as usize);
                        }
                        408 if selected > 0 && (selected as usize) < len => {
                            self.draft
                                .weather
                                .providers
                                .swap(selected as usize, selected as usize - 1);
                        }
                        409 if selected >= 0 && (selected as usize) + 1 < len => {
                            self.draft
                                .weather
                                .providers
                                .swap(selected as usize, selected as usize + 1);
                        }
                        _ => {}
                    }
                    self.render();
                }
                412 | 413 | 605 | 606 => {
                    let (target, field) = if id == 605 || id == 606 {
                        (update::CREDENTIAL_TARGET, 604)
                    } else {
                        (
                            if self.selected(410) == 0 {
                                "LiteTray/weather/qweather"
                            } else {
                                "LiteTray/weather/openweather"
                            },
                            411,
                        )
                    };
                    let result = if id == 413 || id == 606 {
                        credentials::delete(target)
                    } else {
                        credentials::save(target, &self.text(field))
                    };
                    match result {
                        Ok(()) => {
                            SetWindowTextW(self.field(field), wide("").as_ptr());
                            self.set_status(if id == 413 || id == 606 {
                                "凭据已删除"
                            } else {
                                "凭据已保存到 Windows 凭据管理器"
                            });
                        }
                        Err(e) => self.set_status(e),
                    }
                }
                503 => {
                    let _ = std::fs::create_dir_all(config::data_dir());
                    open_target(&config::data_dir().to_string_lossy());
                }
                504 => {
                    let path = config::data_dir().join("diagnostics.json");
                    let result = serde_json::to_vec_pretty(&diagnostics::collect())
                        .map_err(|e| e.to_string())
                        .and_then(|data| config::atomic_write(&path, &data));
                    match result {
                        Ok(()) => self.set_status(format!("诊断已保存：{}", path.display())),
                        Err(e) => self.set_status(e),
                    }
                }
                OPEN_RELEASE => {
                    let link = if let Some(UpdateStatus::Available(r)) = &self.update {
                        r.url.clone()
                    } else {
                        format!(
                            "https://github.com/{}/releases",
                            self.saved.updates.repository
                        )
                    };
                    open_target(&link);
                }
                607 => open_target("https://github.com/Lianzy-Baimiao/LiteTray"),
                701 => {
                    let result = choose_migration_file(self.hwnd).and_then(|path| {
                        let Some(path) = path else { return Ok(None) };
                        let bytes = migration::read_bytes(&path)?;
                        let report = migration::preview(&bytes, &self.draft)?;
                        Ok(Some((bytes, report)))
                    });
                    // Common dialogs pump messages; drain network completions
                    // that arrived while App was borrowed by this command.
                    self.receive();
                    match result {
                        Ok(Some((bytes, report))) => {
                            self.migration_source = Some(bytes);
                            self.migration = Some(report);
                            self.status = "迁移预览已生成，请核对近似转换和需确认项目".into();
                            self.render();
                        }
                        Ok(None) => {}
                        Err(error) => self.set_status(error),
                    }
                }
                702 => {
                    if let Some(bytes) = &self.migration_source {
                        // Rebase against the latest draft so inspecting a report,
                        // then editing other pages, does not revert those edits.
                        match migration::preview(bytes, &self.draft) {
                            Ok(report) => {
                                self.draft = report.candidate.clone();
                                self.migration = Some(report);
                                self.status = "转换结果已采用到草稿；核对后点击“保存设置”".into();
                                self.render();
                            }
                            Err(error) => self.set_status(error),
                        }
                    }
                }
                703 => {
                    if let Some(bytes) = &self.migration_source {
                        let path = config::data_dir().join("migration-report.json");
                        let result = migration::preview(bytes, &self.draft)
                            .and_then(|report| {
                                serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())
                            })
                            .and_then(|bytes| config::atomic_write(&path, &bytes));
                        match result {
                            Ok(()) => self.set_status(format!("报告已保存：{}", path.display())),
                            Err(error) => self.set_status(error),
                        }
                    }
                }
                _ => {}
            }
            false
        }
    }
}

fn instance_suffix() -> Result<String, String> {
    use sha2::{Digest, Sha256};
    // Keep the default profile compatible with earlier versions. Explicit
    // developer/portable data directories each get their own instance lock.
    if std::env::var_os("LITETRAY_DATA_DIR").is_none() {
        return Ok(String::new());
    }
    let directory = config::data_dir();
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let resolved = std::fs::canonicalize(directory).map_err(|e| e.to_string())?;
    let normal = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map(|p| p.join("LiteTray"));
    if normal
        .and_then(|p| std::fs::canonicalize(p).ok())
        .is_some_and(|p| p == resolved)
    {
        return Ok(String::new());
    }
    let key = resolved.to_string_lossy().to_lowercase();
    Ok(format!(".{:x}", Sha256::digest(key.as_bytes())))
}
unsafe fn choose_migration_file(owner: HWND) -> Result<Option<std::path::PathBuf>, String> {
    unsafe {
        use std::os::windows::ffi::OsStringExt;
        use windows_sys::Win32::UI::Controls::Dialogs::*;
        let mut path = vec![0u16; 32768];
        let filter = wide("Windhawk 导出文件 (*.json)\0*.json\0所有文件\0*.*\0");
        let title = wide("选择 Windhawk 配置导出文件");
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

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_TIMER && w == 1 {
            let preview = PREVIEW.with(|p| p.borrow().clone());
            if let Some(preview) = preview {
                let _ = preview.heartbeat();
            }
        }
        if let Some(result) = paint_message(hwnd, msg, w, l) {
            return result;
        }
        if msg == WM_DESTROY {
            PostQuitMessage(0);
            return 0;
        }
        if msg == WM_GETMINMAXINFO {
            let info = &mut *(l as *mut MINMAXINFO);
            let dpi = GetDpiForWindow(hwnd).max(96) as i32;
            info.ptMinTrackSize = POINT {
                x: 880 * dpi / 96,
                y: 700 * dpi / 96,
            };
            return 0;
        }
        let mut quit = false;
        let handled = STATE.with(|state| {
            let Ok(mut state) = state.try_borrow_mut() else {
                return None;
            };
            let app = state.as_mut()?;
            if msg == app.taskbar_created {
                app.add_tray();
                return Some(0);
            }
            match msg {
                WM_PREVIEW_CLOCK => {
                    app.command(209);
                    Some(0)
                }
                WM_COPYDATA => {
                    use litetray_adapter_protocol::activation::{EVENT_TAG, MAX_EVENT};
                    if l == 0 || app.pending_activation.is_some() {
                        return Some(0);
                    }
                    let copy =
                        &*(l as *const windows_sys::Win32::System::DataExchange::COPYDATASTRUCT);
                    if copy.dwData != EVENT_TAG
                        || copy.cbData == 0
                        || copy.cbData as usize > MAX_EVENT
                        || copy.lpData.is_null()
                    {
                        return Some(0);
                    }
                    let Some(preview) = &mut app.preview else {
                        return Some(0);
                    };
                    let bytes =
                        std::slice::from_raw_parts(copy.lpData.cast::<u8>(), copy.cbData as usize);
                    let Ok(event) = preview.accept(w as HWND, bytes) else {
                        return Some(0);
                    };
                    if PostMessageW(hwnd, WM_CLOCK_ACTIVATED, 0, 0) == 0 {
                        return Some(0);
                    }
                    app.pending_activation = Some(event);
                    Some(1)
                }
                WM_CLOCK_ACTIVATED => {
                    if let Some(event) = app.pending_activation.take() {
                        app.show_panel_at(event.anchor);
                    }
                    Some(0)
                }
                WM_COMMAND => {
                    let id = (w & 0xffff) as u16;
                    if (w >> 16) == BN_CLICKED as usize {
                        quit = app.command(id);
                    }
                    Some(0)
                }
                WM_CLOSE => {
                    ShowWindow(hwnd, SW_HIDE);
                    app.schedule_tick();
                    Some(0)
                }
                WM_TIMER => {
                    if IsWindowVisible(hwnd) != 0
                        || (!app.panel.is_null() && IsWindowVisible(app.panel) != 0)
                        || app.preview.is_some()
                    {
                        app.update_views();
                    }
                    if Instant::now() >= app.next_weather {
                        app.start_weather(false);
                    }
                    app.schedule_tick();
                    Some(0)
                }
                WM_RESULT => {
                    app.receive();
                    Some(0)
                }
                WM_SHOW_SETTINGS => {
                    ShowWindow(hwnd, SW_SHOW);
                    SetForegroundWindow(hwnd);
                    app.schedule_tick();
                    Some(0)
                }
                WM_SHOW_PANEL => {
                    app.show_panel();
                    Some(0)
                }
                WM_TRAY => {
                    match l as u32 {
                        WM_LBUTTONUP => app.show_panel(),
                        WM_RBUTTONUP | WM_CONTEXTMENU => app.tray_menu(),
                        _ => {}
                    }
                    Some(0)
                }
                WM_SIZE => {
                    if w != SIZE_MINIMIZED as usize {
                        let _ = app.collect_page();
                        app.render();
                    }
                    Some(0)
                }
                WM_DPICHANGED => {
                    let _ = app.collect_page();
                    app.dpi = (w & 0xffff) as u32;
                    app.recreate_resources();
                    let rect = &*(l as *const RECT);
                    SetWindowPos(
                        hwnd,
                        null_mut(),
                        rect.left,
                        rect.top,
                        rect.right - rect.left,
                        rect.bottom - rect.top,
                        SWP_NOZORDER,
                    );
                    app.render();
                    Some(0)
                }
                WM_SETTINGCHANGE => {
                    if app.draft.appearance == Appearance::System {
                        let _ = app.collect_page();
                        app.recreate_resources();
                        app.render();
                    }
                    Some(0)
                }
                WM_ERASEBKGND => {
                    let mut rect = std::mem::zeroed();
                    GetClientRect(hwnd, &mut rect);
                    FillRect(w as HDC, &rect, app.background);
                    Some(1)
                }
                WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                    SetTextColor(w as HDC, app.text_color());
                    SetBkColor(w as HDC, app.bg_color());
                    Some(app.background as isize)
                }
                WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
                    SetTextColor(w as HDC, app.text_color());
                    SetBkColor(w as HDC, app.edit_color());
                    Some(app.edit_background as isize)
                }
                WM_NOTIFY => {
                    calendar_selection(hwnd, l, app.draft.clock.show_lunar);
                    Some(0)
                }
                _ => None,
            }
        });
        if quit {
            PREVIEW.with(|p| p.borrow_mut().take());
            STATE.with(|s| {
                if let Some(app) = s.borrow_mut().as_mut() {
                    app.preview.take();
                }
            });
            DestroyWindow(hwnd);
            return 0;
        }
        handled.unwrap_or_else(|| DefWindowProcW(hwnd, msg, w, l))
    }
}
unsafe extern "system" fn panel_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        // A display scale change invalidates the popup's control layout. The
        // next invocation recreates it on the current monitor with fresh fonts.
        if msg == WM_DPICHANGED || msg == WM_DISPLAYCHANGE {
            ShowWindow(hwnd, SW_HIDE);
            return 0;
        }
        if let Some(result) = paint_message(hwnd, msg, w, l) {
            return result;
        }
        if msg == WM_ACTIVATE && (w & 0xffff) == WA_INACTIVE as usize {
            ShowWindow(hwnd, SW_HIDE);
            return 0;
        }
        if msg == WM_CLOSE || msg == WM_KEYDOWN && w == VK_ESCAPE as usize {
            ShowWindow(hwnd, SW_HIDE);
            return 0;
        }
        if msg == WM_COMMAND {
            let id = (w & 0xffff) as u16;
            if id == 41 || id == IDCANCEL as u16 {
                ShowWindow(hwnd, SW_HIDE);
                return 0;
            }
            if id == 40 {
                ShowWindow(hwnd, SW_HIDE);
            }
            let main = GetWindow(hwnd, GW_OWNER);
            PostMessageW(main, WM_COMMAND, w, l);
            return 0;
        }
        let handled = STATE.with(|state| {
            let Ok(state) = state.try_borrow() else {
                return None;
            };
            let app = state.as_ref()?;
            match msg {
                WM_ERASEBKGND => {
                    let mut rect = std::mem::zeroed();
                    GetClientRect(hwnd, &mut rect);
                    FillRect(w as HDC, &rect, app.background);
                    Some(1)
                }
                WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                    SetTextColor(w as HDC, app.text_color());
                    SetBkColor(w as HDC, app.bg_color());
                    Some(app.background as isize)
                }
                WM_NOTIFY => {
                    calendar_selection(hwnd, l, app.saved.clock.show_lunar);
                    Some(0)
                }
                _ => None,
            }
        });
        handled.unwrap_or_else(|| DefWindowProcW(hwnd, msg, w, l))
    }
}
unsafe fn calendar_selection(hwnd: HWND, l: LPARAM, show_lunar: bool) {
    unsafe {
        if l == 0 {
            return;
        }
        let hdr = &*(l as *const NMHDR);
        if hdr.idFrom != MONTHCAL as usize || hdr.code != MCN_SELCHANGE {
            return;
        }
        update_selected_date(hwnd, show_lunar);
    }
}
unsafe fn update_selected_date(hwnd: HWND, show_lunar: bool) {
    unsafe {
        let calendar = GetDlgItem(hwnd, MONTHCAL as i32);
        if calendar.is_null() {
            return;
        }
        let mut date: SYSTEMTIME = std::mem::zeroed();
        if SendMessageW(calendar, MCM_GETCURSEL, 0, &mut date as *mut _ as isize) != 0 {
            let text = Local
                .with_ymd_and_hms(
                    date.wYear as i32,
                    date.wMonth as u32,
                    date.wDay as u32,
                    12,
                    0,
                    0,
                )
                .single()
                .map(|d| {
                    format!(
                        "{} {}",
                        d.format("%Y年%m月%d日"),
                        if show_lunar {
                            lunar::describe(d).unwrap_or_default()
                        } else {
                            String::new()
                        }
                    )
                })
                .unwrap_or_default();
            set_text(GetDlgItem(hwnd, LUNAR_VIEW as i32), &text);
        }
    }
}
unsafe fn set_text(hwnd: HWND, text: &str) {
    unsafe {
        if !hwnd.is_null() && window_text(hwnd) != text {
            SetWindowTextW(hwnd, wide(text).as_ptr());
        }
    }
}
unsafe fn theme_calendar(hwnd: HWND, font: HFONT) {
    unsafe {
        SetWindowTheme(hwnd, wide("").as_ptr(), wide("").as_ptr());
        SendMessageW(hwnd, WM_SETFONT, font as usize, 1);
        if let Some(p) = PALETTE.with(Cell::get) {
            // The classic month calendar reuses TITLEBK for weekday text.
            // A near-background title color therefore makes weekdays vanish.
            for (slot, color) in [
                (MCSC_BACKGROUND, p.bg),
                (MCSC_MONTHBK, p.bg),
                (MCSC_TEXT, p.text),
                (
                    MCSC_TITLEBK,
                    if p.dark {
                        rgb(92, 155, 225)
                    } else {
                        rgb(0, 95, 168)
                    },
                ),
                (MCSC_TITLETEXT, rgb(255, 255, 255)),
                (
                    MCSC_TRAILINGTEXT,
                    if p.dark {
                        rgb(155, 155, 155)
                    } else {
                        rgb(105, 105, 105)
                    },
                ),
            ] {
                SendMessageW(hwnd, MCM_SETCOLOR, slot as usize, color as isize);
            }
        }
    }
}
// Painting can re-enter synchronously while a page is being built. Keep its
// immutable data outside the mutably borrowed application state.
unsafe fn paint_message(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> Option<LRESULT> {
    unsafe {
        let p = PALETTE.with(Cell::get)?;
        match msg {
            WM_ERASEBKGND => {
                let mut rect = std::mem::zeroed();
                GetClientRect(hwnd, &mut rect);
                FillRect(w as HDC, &rect, p.background);
                Some(1)
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                SetTextColor(w as HDC, p.text);
                SetBkColor(w as HDC, p.bg);
                Some(p.background as isize)
            }
            WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
                SetTextColor(w as HDC, p.text);
                SetBkColor(w as HDC, p.edit_bg);
                Some(p.edit as isize)
            }
            WM_DRAWITEM if l != 0 => {
                let item = &*(l as *const DRAWITEMSTRUCT);
                if item.CtlType != ODT_BUTTON {
                    return None;
                }
                let selected = item.CtlID == p.active as u32;
                let pressed = item.itemState & ODS_SELECTED != 0;
                let fill = if selected {
                    if p.dark {
                        rgb(47, 65, 86)
                    } else {
                        rgb(226, 237, 250)
                    }
                } else if pressed {
                    if p.dark {
                        rgb(65, 65, 65)
                    } else {
                        rgb(222, 222, 222)
                    }
                } else {
                    p.edit_bg
                };
                FillRect(item.hDC, &item.rcItem, p.background);
                let brush = CreateSolidBrush(fill);
                let border = CreatePen(
                    PS_SOLID,
                    1,
                    if p.dark {
                        rgb(75, 75, 75)
                    } else {
                        rgb(205, 205, 205)
                    },
                );
                let old_brush = SelectObject(item.hDC, brush);
                let old_pen = SelectObject(item.hDC, border);
                RoundRect(
                    item.hDC,
                    item.rcItem.left,
                    item.rcItem.top,
                    item.rcItem.right,
                    item.rcItem.bottom,
                    8,
                    8,
                );
                SelectObject(item.hDC, old_brush);
                SelectObject(item.hDC, old_pen);
                DeleteObject(brush);
                DeleteObject(border);
                let font = SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as HFONT;
                let old_font = SelectObject(item.hDC, font);
                SetBkMode(item.hDC, TRANSPARENT as i32);
                SetTextColor(item.hDC, p.text);
                let mut rect = item.rcItem;
                let mut text = wide(&window_text(item.hwndItem));
                DrawTextW(
                    item.hDC,
                    text.as_mut_ptr(),
                    -1,
                    &mut rect,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
                if item.itemState & ODS_FOCUS != 0 {
                    let mut focus = item.rcItem;
                    InflateRect(&mut focus, -4, -4);
                    DrawFocusRect(item.hDC, &focus);
                }
                SelectObject(item.hDC, old_font);
                Some(1)
            }
            _ => None,
        }
    }
}
fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}
unsafe fn make_font(height: i32, weight: i32, family: &str) -> HFONT {
    unsafe {
        CreateFontW(
            -height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32,
            wide(family).as_ptr(),
        )
    }
}
unsafe fn window_text(hwnd: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(hwnd).max(0) as usize;
        let mut buffer = vec![0; n + 1];
        let actual = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32).max(0) as usize;
        String::from_utf16_lossy(&buffer[..actual])
    }
}
unsafe fn open_target(target: &str) {
    unsafe {
        ShellExecuteW(
            null_mut(),
            wide("open").as_ptr(),
            wide(target).as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        );
    }
}
fn system_dark() -> bool {
    unsafe {
        use windows_sys::Win32::System::Registry::*;
        let mut value = 1u32;
        let mut size = 4;
        let result = RegGetValueW(
            HKEY_CURRENT_USER,
            wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize").as_ptr(),
            wide("AppsUseLightTheme").as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            &mut value as *mut _ as _,
            &mut size,
        );
        result == ERROR_SUCCESS && value == 0
    }
}
