mod agenda;
mod calendar;
mod dashboard;
mod design;
mod page;
#[cfg(test)]
mod visual_checks;
use chrono::{Local, TimeZone, Utc};
use litetray::{
    clock,
    config::{self, Appearance, ClickAction, Config, MonitorScope, Provider},
    http, migration, monitors, organizer,
    platform::{
        app_icon, clock_preview, credentials, diagnostics, displays, lunar, startup, taskbar_clock,
        tray_popup, wide,
    },
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
const UI_REVISION: &str = "开发版 5";
const SS_LEFT: i32 = 0;
const PANEL_CLASS: &str = "LiteTray.Calendar.1";
const WM_TRAY: u32 = WM_APP + 1;
const WM_RESULT: u32 = WM_APP + 2;
const WM_SHOW_SETTINGS: u32 = WM_APP + 3;
const WM_SHOW_PANEL: u32 = WM_APP + 4;
const WM_CLOCK_ACTIVATED: u32 = WM_APP + 5;
const WM_PREVIEW_CLOCK: u32 = WM_APP + 6;
const WM_SHOW_SECONDARY: u32 = WM_APP + 7;
const WM_PREPARE_PANELS: u32 = WM_APP + 8;
const NAV: u16 = 100;
const SAVE: u16 = 20;
const STATUS: u16 = 900;
const REFRESH: u16 = 30;
const UPDATE: u16 = 31;
const OPEN_RELEASE: u16 = 32;
const EXIT: u16 = 33;
const CALENDAR: u16 = 34;
const PAGES: [&str; 10] = [
    "日历与天气",
    "时钟显示",
    "多屏托盘",
    "天气来源",
    "常规设置",
    "关于与更新",
    "配置迁移",
    "时钟样式",
    "日程与待办",
    "闹钟与专注",
];
thread_local! {static STATE:RefCell<Option<App>>=const{RefCell::new(None)};}
// Independent of App's modal-dialog borrow. Each native call retains its own
// Rc, so reentrant window teardown cannot unload a module still on the stack.
thread_local! {static PREVIEW:RefCell<Option<Rc<clock_preview::Preview>>>=const{RefCell::new(None)};}
thread_local! {static TASKBAR:RefCell<Option<Rc<taskbar_clock::Service>>>=const{RefCell::new(None)};}
thread_local! {static TASKBAR_CREATED:Cell<u32>=const{Cell::new(0)};}
thread_local! {static TRAY_READD:Cell<bool>=const{Cell::new(false)};}
thread_local! {static CLOCK_RETRY:Cell<Option<Instant>>=const{Cell::new(None)};}
#[derive(Clone, Copy)]
struct Palette {
    background: HBRUSH,
    edit: HBRUSH,
    surface: HBRUSH,
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
    page_host: HWND,
    organizer: Rc<RefCell<organizer::Store>>,
    agenda_state: [Option<agenda::Snapshot>; 2],
    agenda_scope: Option<chrono::NaiveDate>,
    notices: std::collections::VecDeque<organizer::Notice>,
    next_notice: Instant,
    popup_router: tray_popup::Router,
    panel_cache: Vec<HWND>,
    panel: HWND,
    preview: Option<Rc<clock_preview::Preview>>,
    taskbar: Option<Rc<taskbar_clock::Service>>,
    pending_activation: Option<litetray_adapter_protocol::activation::Activation>,
    children: Vec<HWND>,
    fields: HashMap<u16, HWND>,
    font: HFONT,
    title_font: HFONT,
    background: HBRUSH,
    edit_background: HBRUSH,
    surface_background: HBRUSH,
    dpi: u32,
    page: usize,
    calendar: Option<calendar::Calendar>,
    draft: Config,
    saved: Config,
    status: String,
    weather: Option<weather::WeatherResult>,
    update: Option<UpdateStatus>,
    migration_source: Option<Vec<u8>>,
    migration: Option<migration::Report>,
    monitor_snapshot: monitors::Snapshot,
    monitor_rows: Vec<monitors::Choice>,
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    weather_busy: bool,
    update_busy: bool,
    next_weather: Instant,
    engine: std::sync::Arc<std::sync::Mutex<weather::Engine>>,
    taskbar_created: u32,
    build: u32,
}

fn secondary_page() -> usize {
    match std::env::args().nth(2).as_deref() {
        Some("calendar") => 1,
        Some("agenda") => 9,
        Some("timers") => 10,
        Some("clock") => 2,
        Some("appearance") => 5,
        _ => 0,
    }
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
            let result = (|| {
                if other.is_null() {
                    return Err("另一个 LiteTray 实例正在启动或退出，请稍后再试。".into());
                }
                // Sharing a profile is a single-instance invariant. A copy in
                // another directory must not silently impersonate this build.
                let running = running_executable(other)?;
                let requested = std::env::current_exe().map_err(|e| e.to_string())?;
                if !running.eq_ignore_ascii_case(&requested.to_string_lossy()) {
                    MessageBoxW(
                        null_mut(),
                        wide(&format!(
                            "另一个目录的 LiteTray 正在运行，本次新版尚未启动。\r\n\r\n正在运行：\r\n{running}\r\n\r\n你刚打开：\r\n{}\r\n\r\n请从旧版的“常规设置 → 退出程序”或托盘菜单退出，再重新打开新版。只点窗口右上角关闭会收至托盘。",
                            requested.display()
                        )).as_ptr(),
                        wide("LiteTray · 需要先退出旧版").as_ptr(),
                        MB_OK | MB_ICONINFORMATION,
                    );
                    return Ok(());
                }
                PostMessageW(
                    other,
                    if mode == "--show-secondary" {
                        WM_SHOW_SECONDARY
                    } else if mode == "--clock-preview" {
                        WM_PREVIEW_CLOCK
                    } else if mode == "--calendar" {
                        WM_SHOW_PANEL
                    } else {
                        WM_SHOW_SETTINGS
                    },
                    if mode == "--show-secondary" {
                        secondary_page()
                    } else {
                        0
                    },
                    0,
                );
                Ok(())
            })();
            CloseHandle(mutex);
            return result;
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
            hIcon: app_icon::load(GetDpiForSystem().max(96), true),
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
            wide(&format!("LiteTray · 轻托盘 · {UI_REVISION}")).as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1080,
            840,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            CloseHandle(mutex);
            return Err("创建设置窗口失败".into());
        }
        SetPropW(
            hwnd,
            wide("LiteTray.SettingsSurface").as_ptr(),
            1usize as HANDLE,
        );
        let dpi = GetDpiForWindow(hwnd).max(96);
        app_icon::apply(hwnd, dpi);
        let (tx, rx) = mpsc::channel();
        let mut app = App {
            hwnd,
            page_host: null_mut(),
            organizer: Rc::new(RefCell::new(organizer::Store::open(
                &config::data_dir().join("agenda.json"),
            ))),
            agenda_state: [None, None],
            agenda_scope: None,
            notices: std::collections::VecDeque::new(),
            next_notice: Instant::now(),
            popup_router: tray_popup::Router::default(),
            panel_cache: vec![],
            panel: null_mut(),
            preview: None,
            taskbar: Some(Rc::new(taskbar_clock::service(
                hwnd,
                config.clone(),
                taskbar_now(),
            )?)),
            pending_activation: None,
            children: vec![],
            fields: HashMap::new(),
            font: null_mut(),
            title_font: null_mut(),
            background: null_mut(),
            edit_background: null_mut(),
            surface_background: null_mut(),
            dpi,
            page: 0,
            calendar: None,
            saved: config.clone(),
            draft: config,
            status: "开发版 · 在“时钟显示”中启用任务栏时钟".into(),
            weather: None,
            update: None,
            migration_source: None,
            migration: None,
            monitor_snapshot: displays::collect(),
            monitor_rows: Vec::new(),
            tx,
            rx,
            weather_busy: false,
            update_busy: false,
            next_weather: Instant::now(),
            engine: Default::default(),
            taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
            build: diagnostics::windows_build(),
        };
        TASKBAR.with(|v| *v.borrow_mut() = app.taskbar.clone());
        app.recreate_resources();
        SetWindowPos(
            hwnd,
            null_mut(),
            0,
            0,
            app.scale(1100),
            app.scale(840),
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
        app.popup_router.configure(&app.saved.tray, app.hwnd);
        app.add_tray();
        app.start_weather(false);
        if app.draft.updates.check_on_startup {
            app.start_update();
        }
        STATE.with(|state| *state.borrow_mut() = Some(app));
        PostMessageW(hwnd, WM_PREPARE_PANELS, 0, 0);
        if mode != "--background" {
            if mode == "--show-secondary" {
                PostMessageW(hwnd, WM_SHOW_SECONDARY, secondary_page(), 0);
            } else if mode == "--clock-preview" {
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
        TASKBAR.with(|p| p.borrow_mut().take());
        STATE.with(|s| {
            if let Some(mut app) = s.borrow_mut().take() {
                app.preview.take();
                app.taskbar.take();
                app.remove_tray();
                app.release_resources();
            }
        });
        CloseHandle(mutex);
        RoUninitialize();
        litetray::platform::sound::stop();
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
        design::Theme::new(self.dark()).bg
    }
    fn edit_color(&self) -> u32 {
        design::Theme::new(self.dark()).field
    }
    fn text_color(&self) -> u32 {
        design::Theme::new(self.dark()).text
    }
    unsafe fn recreate_resources(&mut self) {
        unsafe {
            self.release_resources();
            let face = design::font_face();
            self.font = make_font(self.scale(14), 400, &face);
            self.title_font = make_font(self.scale(28), 600, &face);
            self.background = CreateSolidBrush(self.bg_color());
            self.edit_background = CreateSolidBrush(self.edit_color());
            self.surface_background = CreateSolidBrush(design::Theme::new(self.dark()).surface);
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
                surface: self.surface_background,
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
                self.background,
                self.edit_background,
                self.surface_background,
            ] {
                if !object.is_null() {
                    DeleteObject(object);
                }
            }
            self.font = null_mut();
            self.title_font = null_mut();
            self.background = null_mut();
            self.edit_background = null_mut();
            self.surface_background = null_mut();
        }
    }
    unsafe fn close_panel(&mut self) {
        unsafe {
            for panel in self.panel_cache.drain(..) {
                DestroyWindow(panel);
            }
            self.panel = null_mut();
        }
    }
    unsafe fn hide_panel(&self) {
        unsafe {
            if !self.panel.is_null() {
                ShowWindow(self.panel, SW_HIDE);
            }
        }
    }
    unsafe fn prewarm_panels(&mut self) {
        unsafe {
            let mut areas: Vec<RECT> = vec![];
            unsafe extern "system" fn collect(
                monitor: HMONITOR,
                _: HDC,
                _: *mut RECT,
                data: LPARAM,
            ) -> i32 {
                unsafe {
                    let mut info: MONITORINFO = std::mem::zeroed();
                    info.cbSize = size_of::<MONITORINFO>() as u32;
                    if GetMonitorInfoW(monitor, &mut info) != 0 {
                        (&mut *(data as *mut Vec<RECT>)).push(info.rcWork);
                    }
                    1
                }
            }
            EnumDisplayMonitors(
                null_mut(),
                null(),
                Some(collect),
                &mut areas as *mut _ as isize,
            );
            for area in areas {
                self.prepare_panel_at(popup::Rect {
                    left: area.right - 80,
                    top: area.bottom,
                    right: area.right,
                    bottom: area.bottom + 1,
                });
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
            let content = x >= 224 && y >= 80 && id != STATUS && id != SAVE && id != 42;
            let style = if class == "COMBOBOX" {
                style | CBS_OWNERDRAWFIXED as u32 | CBS_HASSTRINGS as u32
            } else {
                style
            };
            let x = if content {
                x - 224 + 20
            } else if x >= 224 {
                x + 24
            } else {
                x
            };
            let y = if content { y - 80 + 20 } else { y };
            let hwnd = CreateWindowExW(
                0,
                wide(class).as_ptr(),
                wide(text).as_ptr(),
                WS_CHILD
                    | WS_VISIBLE
                    | (if class == "EDIT" {
                        style & !WS_BORDER
                    } else {
                        style
                    }),
                self.scale(x),
                self.scale(y),
                self.scale(w),
                self.scale(h),
                if content { self.page_host } else { self.hwnd },
                id as usize as HMENU,
                GetModuleHandleW(null()),
                null(),
            );
            SendMessageW(hwnd, WM_SETFONT, self.font as usize, 1);
            if content {
                SetPropW(hwnd, wide("LiteTray.Surface").as_ptr(), 1usize as HANDLE);
            }
            if class == "EDIT" {
                SetPropW(hwnd, wide("LiteTray.Field").as_ptr(), 1usize as HANDLE);
                SendMessageW(
                    hwnd,
                    EM_SETMARGINS,
                    (EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize,
                    (10 << 16) | 10,
                );
            }
            if class == "COMBOBOX" {
                design::style_combo(hwnd, self.dark(), self.dpi);
            }
            if class == "BUTTON" && style & 0xf == BS_AUTOCHECKBOX as u32 {
                SetWindowTheme(
                    hwnd,
                    wide(if self.dark() {
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
    fn input_snapshot(&self) -> Vec<(u16, String)> {
        self.fields
            .iter()
            .filter_map(|(&id, &hwnd)| unsafe {
                let mut name = [0u16; 32];
                let count = GetClassNameW(hwnd, name.as_mut_ptr(), 32).max(0) as usize;
                (String::from_utf16_lossy(&name[..count]) == "Edit")
                    .then(|| (id, window_text(hwnd)))
                    .or_else(|| {
                        (String::from_utf16_lossy(&name[..count]) == "EDIT")
                            .then(|| (id, window_text(hwnd)))
                    })
            })
            .collect()
    }
    unsafe fn restore_inputs(&self, fields: Vec<(u16, String)>) {
        unsafe {
            for (id, value) in fields {
                set_text(self.field(id), &value);
            }
        }
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
            if let Some(calendar) = dashboard::selection(self.field(dashboard::ID)) {
                self.calendar = Some(calendar);
            }
            if let Some(snapshot) = agenda::snapshot(self.field(agenda::ID)) {
                let index = snapshot.timer_mode as usize;
                self.agenda_state[index] = Some(snapshot);
            }
            for child in self.children.drain(..) {
                DestroyWindow(child);
            }
            self.fields.clear();
            let mut rect = std::mem::zeroed();
            GetClientRect(self.hwnd, &mut rect);
            let width = rect.right * 96 / self.dpi as i32 - 56;
            let height = rect.bottom * 96 / self.dpi as i32;
            self.page_host = page::create(
                self.hwnd,
                design::rect(
                    self.scale(248),
                    self.scale(124),
                    rect.right - self.scale(276),
                    rect.bottom - self.scale(220),
                ),
            );
            self.children.push(self.page_host);
            let brand = self.label("LiteTray", 22, 24, 170, 38);
            SendMessageW(brand, WM_SETFONT, self.title_font as usize, 1);
            self.label("轻托盘", 26, 72, 194, 25);
            for (row, i) in [0, 8, 9, 1, 7, 2, 3, 4, 5, 6].into_iter().enumerate() {
                self.button(PAGES[i], NAV + i as u16, 16, 134 + row as i32 * 44, 208);
            }
            self.label(
                &format!("{UI_REVISION}  v{}", env!("CARGO_PKG_VERSION")),
                24,
                height - 90,
                165,
                50,
            );
            let heading = self.label(PAGES[self.page], 224, 30, width - 250, 42);
            SendMessageW(heading, WM_SETFONT, self.title_font as usize, 1);
            let descriptions = [
                "月历、日程和天气。",
                "设置任务栏时钟的内容与点击行为。",
                "管理各屏幕上的时钟与托盘图标。",
                "设置城市、天气来源与更新频率。",
                "外观跟随 Windows，也可以选择固定明暗模式。",
                "关于 LiteTray，以及应用更新。",
                "带上熟悉的设置，开始使用 LiteTray。",
                "调整每一行的字体、颜色和对齐方式。",
                "记录待办、日程和重要日期。点击保存后保存在本机。",
                "专注计时、定时提醒与最近提醒记录。",
            ];
            self.label(descriptions[self.page], 224, 78, width - 260, 26);
            let c = self.draft.clone();
            match self.page {
                0 => {
                    let view = dashboard::create(
                        self.page_host,
                        design::rect(
                            self.scale(8),
                            self.scale(8),
                            self.scale(width - 236),
                            self.scale((height - 238).min(680)),
                        ),
                        self.dark(),
                        false,
                    );
                    self.children.push(view);
                    self.fields.insert(dashboard::ID, view);
                    dashboard::bind_store(view, self.organizer.clone());
                    if let Some(calendar) = self.calendar {
                        dashboard::restore_selection(view, calendar);
                    }
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
                        "宽度（留空自动）",
                        &if c.clock.width == 0 {
                            String::new()
                        } else {
                            c.clock.width.to_string()
                        },
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
                    self.button("预览时钟", 209, 574, 418, 250);
                    self.control(
                        "STATIC",
                        "预览：尚未打开",
                        210,
                        SS_LEFT as u32,
                        224,
                        466,
                        width - 260,
                        58,
                    );
                    self.button("启用任务栏时钟", 212, 224, 532, 250);
                    self.button("停用并恢复", 213, 492, 532, 170);
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
                        "复制主屏托盘图标和隐藏图标入口",
                        c.tray.enabled && c.tray.show_icons,
                        308,
                        224,
                        162,
                        400,
                    );
                    self.check(
                        "包含网络 / 音量 / 输入法等系统图标",
                        c.tray.enabled && c.tray.show_icons,
                        303,
                        224,
                        200,
                        400,
                    );
                    EnableWindow(self.field(303), 0);
                    self.check(
                        "图标复制到选中副屏；主屏图标保持原位",
                        c.tray.copied_taskbars_only,
                        304,
                        224,
                        238,
                        460,
                    );
                    EnableWindow(self.field(304), 0);
                    self.label(
                        "指定屏幕 · 单击或空格切换，可多选",
                        224,
                        278,
                        width - 410,
                        25,
                    );
                    self.button("刷新显示器", 307, width - 170, 270, 134);
                    self.monitor_rows =
                        monitors::choices(&self.monitor_snapshot, &c.tray.selected_devices);
                    let list = self.control(
                        "LISTBOX",
                        "",
                        305,
                        WS_TABSTOP
                            | WS_BORDER
                            | WS_VSCROLL
                            | LBS_NOTIFY as u32
                            | LBS_MULTIPLESEL as u32,
                        224,
                        310,
                        width - 260,
                        136,
                    );
                    for (index, row) in self.monitor_rows.iter().enumerate() {
                        SendMessageW(list, LB_ADDSTRING, 0, wide(&row.label).as_ptr() as isize);
                        SendMessageW(list, LB_SETSEL, usize::from(row.selected), index as isize);
                    }
                    self.control(
                        "STATIC",
                        &self.monitor_summary(),
                        306,
                        SS_LEFT as u32,
                        224,
                        455,
                        width - 260,
                        64,
                    );
                    self.label(
                        "保存后生效。图标顺序、显隐与宽度跟随主屏，按实际内容排列。",
                        224,
                        527,
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
                    self.label("强调色、高对比度与界面字体跟随 Windows，系统外观变化时自动更新。\r\n关闭窗口后继续后台运行；右键托盘图标可退出。",224,231,width-260,65);
                    self.button("打开配置目录", 503, 224, 320, 170);
                    self.button("导出诊断信息", 504, 412, 320, 170);
                    self.button("退出程序", EXIT, 600, 320, 170);
                    self.label("保存后的日程与计时进度保留在本机。需要登录后继续提醒时，请开启随 Windows 启动。\r\n通知遵循 Windows 的通知设置；最近提醒也会保留在“闹钟与专注”中。",224,389,width-260,100);
                }
                5 => {
                    self.label(&format!("LiteTray 轻托盘  {} · {UI_REVISION}\r\n自动宽度时钟、日历天气、主屏托盘图标复制",env!("CARGO_PKG_VERSION")),224,86,width-260,80);
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
                        (height - 408).max(220),
                    );
                    SendMessageW(report_control, EM_SETLIMITTEXT, 2 * 1024 * 1024, 0);
                    SetWindowTextW(report_control, wide(&text).as_ptr());
                }
                7 => {
                    use litetray_adapter_protocol::clock::Alignment;
                    let alignment_index = |alignment| match alignment {
                        Alignment::Native => 0,
                        Alignment::Left => 1,
                        Alignment::Center => 2,
                        Alignment::Right => 3,
                    };
                    let column = (width - 284) / 2;
                    for (title, line, color, color_id, font_id, x) in [
                        (
                            "第一行",
                            &c.clock.top_style,
                            &c.clock.top_color,
                            801,
                            806,
                            224,
                        ),
                        (
                            "第二行",
                            &c.clock.bottom_style,
                            &c.clock.bottom_color,
                            802,
                            809,
                            248 + column,
                        ),
                    ] {
                        self.label(title, x, 84, column, 24);
                        self.edit(
                            "字体",
                            line.font_family.as_deref().unwrap_or(""),
                            font_id,
                            x,
                            115,
                            column - 88,
                        );
                        self.edit(
                            "字号",
                            &line
                                .font_size
                                .map(|size| size.to_string())
                                .unwrap_or_default(),
                            font_id + 1,
                            x + column - 72,
                            115,
                            72,
                        );
                        self.edit("颜色", color, color_id, x, 180, column - 164);
                        self.label("对齐", x + column - 148, 180, 148, 22);
                        self.combo(
                            &["沿用公共对齐", "保留原样", "左对齐", "居中", "右对齐"],
                            line.alignment.map(|a| alignment_index(a) + 1).unwrap_or(0),
                            font_id + 2,
                            x + column - 148,
                            205,
                            148,
                        );
                    }
                    self.label("字体、字号留空时沿用“时钟显示”页；字号范围 8–32。\r\n颜色留空使用原生颜色；支持 #RRGGBB 或 #AARRGGBB。",224,248,width-260,50);
                    self.label("公共文字对齐", 224, 309, 190, 25);
                    self.combo(
                        &["保留原样", "左对齐", "居中", "右对齐"],
                        alignment_index(c.clock.alignment),
                        803,
                        420,
                        305,
                        235,
                    );
                    self.label("悬停提示模板 · 留空保留原生提示", 224, 354, width - 260, 25);
                    let tip = self.control(
                        "EDIT",
                        &c.clock.tooltip_line.replace('\n', "\r\n"),
                        804,
                        WS_TABSTOP
                            | WS_BORDER
                            | WS_VSCROLL
                            | ES_MULTILINE as u32
                            | ES_AUTOVSCROLL as u32
                            | ES_WANTRETURN as u32,
                        224,
                        388,
                        width - 260,
                        76,
                    );
                    SendMessageW(tip, EM_SETLIMITTEXT, 1024, 0);
                    self.label("支持 %date% %time% %weekday% %weather% 和换行。\r\n在原生预览时钟上停留鼠标可查看提示。",224,474,width-260,44);
                    self.button("应用草稿到原生预览", 209, 224, 530, 250);
                    self.button("重置本页", 805, 492, 530, 185);
                }
                8 | 9 => {
                    let timers = self.page == 9;
                    ShowScrollBar(self.page_host, SB_VERT, 0);
                    let mut bounds: RECT = std::mem::zeroed();
                    GetClientRect(self.page_host, &mut bounds);
                    let view = agenda::create(
                        self.page_host,
                        bounds,
                        self.dark(),
                        self.organizer.clone(),
                        timers,
                        self.agenda_scope,
                        self.agenda_state[timers as usize].as_ref(),
                    );
                    self.children.push(view);
                    self.fields.insert(agenda::ID, view);
                }
                _ => {}
            }
            self.control(
                "STATIC",
                &if self.page == 8 {
                    "选择日程后编辑 · 点击保存后生效".into()
                } else if self.page == 9 {
                    "使用 Windows 通知 · 最近提醒保存在本机".into()
                } else {
                    self.status.clone()
                },
                STATUS,
                SS_LEFT as u32,
                224,
                height - 79,
                width - 415,
                64,
            );
            if self.page < 8 {
                self.button(
                    if self.page == 0 {
                        "设置时钟"
                    } else {
                        "保存设置"
                    },
                    if self.page == 0 { 42 } else { SAVE },
                    width - 121,
                    height - 68,
                    125,
                );
            }
            if self.page < 8 {
                page::finish(self.page_host);
            }
            self.update_views();
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
                c.clock.width = if self.text(207).trim().is_empty() {
                    0
                } else {
                    self.text(207)
                        .parse()
                        .map_err(|_| "宽度留空自动，或填写 80–600")?
                };
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
                c.tray.enabled = self.checked(308);
                c.tray.show_icons = self.checked(308);
                let selected = (0..self.monitor_rows.len())
                    .map(|i| unsafe { SendMessageW(self.field(305), LB_GETSEL, i, 0) > 0 })
                    .collect::<Vec<_>>();
                c.tray.selected_devices = monitors::collect_choices(
                    &self.monitor_rows,
                    &selected,
                    &c.tray.selected_devices,
                )?;
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
            7 => {
                use litetray_adapter_protocol::clock::Alignment;
                c.clock.top_color = self.text(801).trim().into();
                c.clock.bottom_color = self.text(802).trim().into();
                c.clock.alignment = match self.selected(803) {
                    1 => Alignment::Left,
                    2 => Alignment::Center,
                    3 => Alignment::Right,
                    _ => Alignment::Native,
                };
                c.clock.tooltip_line = self.text(804).replace("\r\n", "\n");
                for (line, id, label) in [
                    (&mut c.clock.top_style, 806, "第一行"),
                    (&mut c.clock.bottom_style, 809, "第二行"),
                ] {
                    let family = self.text(id).trim().to_owned();
                    line.font_family = (!family.is_empty()).then_some(family);
                    let size = self.text(id + 1);
                    line.font_size = if size.trim().is_empty() {
                        None
                    } else {
                        Some(
                            size.trim()
                                .parse()
                                .map_err(|_| format!("{label}字号应为整数或留空"))?,
                        )
                    };
                    line.alignment = match self.selected(id + 2) {
                        0 => None,
                        2 => Some(Alignment::Left),
                        3 => Some(Alignment::Center),
                        4 => Some(Alignment::Right),
                        _ => Some(Alignment::Native),
                    };
                }
            }
            _ => {}
        }
        // Full validation belongs to Save; adding a keyed provider is possible before entering its Host.
        self.draft = c;
        Ok(())
    }
    fn monitor_summary(&self) -> String {
        let result = monitors::resolve(&self.draft.tray, &self.monitor_snapshot);
        let mut text = format!(
            "当前范围匹配 {} 个桌面区域。选择按设备保存，不受 DISPLAY 编号变化影响。",
            result.devices.len()
        );
        if !result.pending.is_empty() {
            text.push_str(&format!(
                "\r\n{} 项尚未匹配；未连接的选择会保留。",
                result.pending.len()
            ));
        }
        if let Some(warning) = result.warnings.first() {
            text.push_str(&format!("\r\n{warning}"));
        }
        text
    }
    unsafe fn refresh_displays(&mut self) {
        unsafe {
            if self.page == 2 {
                // An unidentified row cannot be persisted. Clear only those
                // temporary selections so Refresh can recover a missing ID.
                for (index, row) in self.monitor_rows.iter().enumerate() {
                    if row.id.is_none() {
                        SendMessageW(self.field(305), LB_SETSEL, 0, index as isize);
                    }
                }
            }
            if self.page == 2
                && let Err(error) = self.collect_page()
            {
                self.set_status(error);
                return;
            }
            self.monitor_snapshot = displays::collect();
            if self.page == 2 {
                self.render();
            }
        }
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
            self.popup_router.configure(&self.saved.tray, self.hwnd);
            self.status = match self
                .taskbar
                .as_ref()
                .map(|s| s.configure(self.saved.clone(), taskbar_now()))
                .transpose()
            {
                Ok(_) => "设置已保存".into(),
                Err(error) => format!("设置已保存；任务栏恢复尚未完成：{error}"),
            };
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
            agenda::tick(self.field(agenda::ID));
            let summary = self
                .weather
                .as_ref()
                .and_then(|r| r.weather.as_ref())
                .map(|w| w.summary())
                .unwrap_or_else(|| "天气暂无数据".into());
            if self.page == 0 {
                dashboard::update(
                    self.field(dashboard::ID),
                    &self.saved,
                    &self.weather,
                    self.weather_busy,
                );
            }
            if let Some(preview) = &self.preview {
                let _ = preview.update(&clock_preview::frame(&self.draft.clock, &summary));
            }
            if let Some(taskbar) = &self.taskbar {
                taskbar.set_weather(&summary);
                if self.page == 1 {
                    let state = taskbar.snapshot();
                    set_text(
                        self.field(210),
                        &format!(
                            "{}\r\n{}",
                            state.text(),
                            self.preview
                                .as_ref()
                                .map_or_else(|| "独立预览：尚未打开".into(), |p| p.status_text())
                        ),
                    );
                    set_text(
                        self.field(212),
                        if self.saved.tray.clock_enabled {
                            "重新应用"
                        } else {
                            "启用任务栏时钟"
                        },
                    );
                    EnableWindow(
                        self.field(212),
                        i32::from(!matches!(
                            state.status,
                            litetray::taskbar_service::Status::Preparing
                                | litetray::taskbar_service::Status::Busy
                        )),
                    );
                    EnableWindow(self.field(213), i32::from(self.saved.tray.clock_enabled));
                }
            }
            if !self.panel.is_null() {
                dashboard::update(
                    GetDlgItem(self.panel, dashboard::ID as i32),
                    &self.saved,
                    &self.weather,
                    self.weather_busy,
                );
            }
        }
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
                        if self.page == 0 {
                            self.set_status(result.status());
                        }
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
            let interval = if let Some(preview) = &self.preview {
                interval.min(preview.tick_millis())
            } else {
                interval
            };
            let interval = if let Some(service) = &self.taskbar {
                interval.min(service.tick_millis(Local::now()))
            } else {
                interval
            };
            let interval = if !self.notices.is_empty()
                || self
                    .organizer
                    .borrow()
                    .data
                    .timers
                    .iter()
                    .any(|t| t.due.is_some())
                || self
                    .organizer
                    .borrow()
                    .data
                    .entries
                    .iter()
                    .any(|e| e.enabled && !e.done && e.remind_minutes.is_some())
            {
                interval.min(1000)
            } else {
                interval
            };
            let interval = if self.saved.tray.enabled && self.saved.tray.show_icons {
                interval.min(10000)
            } else {
                interval
            };
            SetTimer(self.hwnd, 1, interval, None);
        }
    }
    unsafe fn organizer_tick(&mut self) {
        unsafe {
            let result = self.organizer.borrow_mut().tick(Utc::now());
            match result {
                Ok(notices) => {
                    if !notices.is_empty() {
                        self.notices.extend(notices);
                        agenda::refresh(self.field(agenda::ID));
                    }
                }
                Err(error) => self.set_status(format!("提醒暂未发送，保存失败：{error}")),
            }
            if Instant::now() < self.next_notice {
                return;
            }
            let Some(notice) = self.notices.pop_front() else {
                return;
            };
            let mut data: NOTIFYICONDATAW = std::mem::zeroed();
            data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
            data.hWnd = self.hwnd;
            data.uID = 1;
            data.uFlags = NIF_INFO;
            data.dwInfoFlags = NIIF_INFO | NIIF_RESPECT_QUIET_TIME | NIIF_NOSOUND;
            let sound = self.organizer.borrow().data.sound.clone();
            if let Err(error) = litetray::platform::sound::play(&sound, false) {
                self.set_status(error);
            }
            for (target, source) in data
                .szInfoTitle
                .iter_mut()
                .take(63)
                .zip(notice.title.encode_utf16())
            {
                *target = source;
            }
            for (target, source) in data
                .szInfo
                .iter_mut()
                .take(255)
                .zip(notice.body.encode_utf16())
            {
                *target = source;
            }
            if Shell_NotifyIconW(NIM_MODIFY, &data) == 0 {
                self.set_status("提醒已记录，可在“闹钟与专注”查看；Windows 通知暂不可用");
            }
            self.next_notice = Instant::now() + Duration::from_secs(8);
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
            data.hIcon = app_icon::load(self.dpi, false);
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
            let started = Instant::now();
            let Some(hwnd) = self.prepare_panel_at(anchor) else {
                return;
            };
            if IsWindowVisible(hwnd) != 0 {
                ShowWindow(hwnd, SW_HIDE);
                return;
            }
            let view = GetDlgItem(hwnd, dashboard::ID as i32);
            dashboard::update(view, &self.saved, &self.weather, self.weather_busy);
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
            SetFocus(GetDlgItem(view, 1102));
            self.schedule_tick();
            // Integer window property is inspectable without disk I/O on the click path.
            SetPropW(
                hwnd,
                wide("LiteTray.OpenMicros").as_ptr(),
                started.elapsed().as_micros().max(1) as usize as HANDLE,
            );
        }
    }
    unsafe fn prepare_panel_at(&mut self, anchor: popup::Rect) -> Option<HWND> {
        unsafe {
            if anchor.validate().is_err() {
                return None;
            }
            let rect = RECT {
                left: anchor.left,
                top: anchor.top,
                right: anchor.right,
                bottom: anchor.bottom,
            };
            let monitor = MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST);
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(monitor, &mut info) == 0 {
                return None;
            }
            if let Some(&panel) = self.panel_cache.iter().find(|&&p| {
                IsWindow(p) != 0 && MonitorFromWindow(p, MONITOR_DEFAULTTONEAREST) == monitor
            }) {
                if self.panel != panel && !self.panel.is_null() {
                    ShowWindow(self.panel, SW_HIDE);
                }
                let work = popup::Rect {
                    left: info.rcWork.left,
                    top: info.rcWork.top,
                    right: info.rcWork.right,
                    bottom: info.rcWork.bottom,
                };
                if let Ok(position) = popup::place(anchor, work, GetDpiForWindow(panel).max(96)) {
                    SetWindowPos(
                        panel,
                        null_mut(),
                        position.rect.left,
                        position.rect.top,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
                self.panel = panel;
                return Some(panel);
            }
            if !self.panel.is_null() {
                ShowWindow(self.panel, SW_HIDE);
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
                return None;
            }
            self.panel = hwnd;
            self.panel_cache.push(hwnd);
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
                    return None;
                }
            };
            let scale = |value: i32| {
                (i64::from(value) * i64::from(dpi) * i64::from(placement.scale_percent) / 9600)
                    .max(1) as i32
            };
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
            let view = dashboard::create(
                hwnd,
                design::rect(0, 0, scale(popup::WIDTH), scale(popup::HEIGHT)),
                self.dark(),
                true,
            );
            dashboard::bind_store(view, self.organizer.clone());
            app_icon::apply(hwnd, dpi as u32);
            let rounded = 2_i32;
            if self.build >= 22000 {
                DwmSetWindowAttribute(hwnd, 33, &rounded as *const _ as _, 4);
            }
            dashboard::update(view, &self.saved, &self.weather, self.weather_busy);
            Some(hwnd)
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
                212 => {
                    self.draft.tray.clock_enabled = true;
                    match self.save() {
                        Ok(()) => {
                            if let Some(service) = &self.taskbar {
                                let _ = service.retry(taskbar_now());
                            }
                        }
                        Err(error) => self.set_status(error),
                    }
                    self.update_views();
                    self.schedule_tick();
                }
                213 => {
                    // Disable first, even when another unsaved field is invalid.
                    let mut saved = self.saved.clone();
                    saved.tray.clock_enabled = false;
                    let restored = self
                        .taskbar
                        .as_ref()
                        .map(|s| s.configure(saved.clone(), taskbar_now()))
                        .transpose();
                    match config::save(&config::data_dir().join("config.json"), &saved) {
                        Ok(()) => {
                            self.saved = saved;
                            self.draft.tray.clock_enabled = false;
                            self.set_status(match restored {
                                Ok(_) => "已停用自定义时钟；图标复制按已保存设置运行".into(),
                                Err(error) => format!("已保存停用；当前恢复待确认：{error}"),
                            });
                        }
                        Err(error) => {
                            self.set_status(format!("本次运行已请求停用；停用偏好未保存：{error}"))
                        }
                    }
                    self.update_views();
                    self.schedule_tick();
                }
                42 => {
                    self.page = 1;
                    self.render();
                }
                805 => {
                    self.draft.clock.top_color.clear();
                    self.draft.clock.bottom_color.clear();
                    self.draft.clock.alignment = Default::default();
                    self.draft.clock.tooltip_line.clear();
                    self.draft.clock.top_style = Default::default();
                    self.draft.clock.bottom_style = Default::default();
                    self.status = "本页已重置；字体和字号沿用公共设置，点击保存后保留".into();
                    self.render();
                }
                307 => self.refresh_displays(),
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
                    self.update_views();
                    self.schedule_tick();
                }
                211 => {
                    if let Some(preview) = &self.preview {
                        preview.retry();
                    }
                    self.update_views();
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
                    if let Err(error) = self.stop_taskbar() {
                        self.set_status(format!("退出前恢复未完成：{error}"));
                        return false;
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
                    let result =
                        serde_json::to_vec_pretty(&diagnostics::collect_for(&self.saved.tray))
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
impl App {
    fn stop_taskbar(&mut self) -> Result<(), String> {
        if let Some(service) = &self.taskbar {
            service.stop(taskbar_now())?;
        }
        Ok(())
    }
}
fn taskbar_now() -> u64 {
    unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() }
}
/// Inspect only the owner of our profile-specific settings window. This uses
/// limited process metadata access, never shell injection or process memory.
unsafe fn running_executable(window: HWND) -> Result<String, String> {
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(window, &mut pid);
        if pid == 0 {
            return Err("无法识别正在运行的 LiteTray，请退出旧实例后重试。".into());
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return Err("无法核对已运行的 LiteTray 版本，请退出旧实例后重试。".into());
        }
        let mut path = vec![0u16; 32768];
        let mut length = path.len() as u32;
        let success = QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length) != 0;
        CloseHandle(process);
        if !success {
            return Err("无法读取已运行的 LiteTray 路径，请退出旧实例后重试。".into());
        }
        Ok(String::from_utf16_lossy(&path[..length as usize]))
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
        let created = TASKBAR_CREATED.with(|id| {
            if id.get() == 0 {
                id.set(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()));
            }
            id.get() != 0 && msg == id.get()
        });
        if created {
            TRAY_READD.with(|pending| pending.set(true));
            if let Some(service) = TASKBAR.with(|p| p.borrow().clone()) {
                service.invalidate();
            }
            SetTimer(hwnd, 2, 500, None);
        }
        if msg == WM_POWERBROADCAST
            && matches!(
                w as u32,
                PBT_APMSUSPEND
                    | PBT_APMRESUMEAUTOMATIC
                    | PBT_APMRESUMESUSPEND
                    | PBT_APMRESUMECRITICAL
            )
        {
            if let Some(service) = TASKBAR.with(|p| p.borrow().clone()) {
                service.invalidate();
            }
            SetTimer(hwnd, 2, 500, None);
        }
        if msg == WM_DISPLAYCHANGE
            || msg == WM_DEVICECHANGE
                && matches!(
                    w as u32,
                    DBT_DEVNODES_CHANGED | DBT_DEVICEARRIVAL | DBT_DEVICEREMOVECOMPLETE
                )
        {
            if let Some(service) = TASKBAR.with(|p| p.borrow().clone()) {
                service.invalidate();
            }
            // Debounce bursts; a modal settings borrow leaves this timer alive
            // so the next dispatch can refresh without dropping draft choices.
            SetTimer(hwnd, 2, 500, None);
            return 0;
        }
        if msg == WM_TIMER && w == 1 {
            let taskbar = TASKBAR.with(|p| p.borrow().clone());
            if let Some(taskbar) = taskbar {
                let _ = taskbar.tick(taskbar_now(), Local::now());
            }
            let preview = PREVIEW.with(|p| p.borrow().clone());
            if let Some(preview) = preview {
                let _ = preview.heartbeat();
            }
        }
        if msg == taskbar_clock::WM_ACTIVATED {
            CLOCK_RETRY.with(|pending| pending.set(Some(Instant::now() + Duration::from_secs(2))));
            SetTimer(hwnd, 3, 16, None);
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
                x: 1060 * dpi / 96,
                y: 820 * dpi / 96,
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
                TRAY_READD.with(|pending| pending.set(false));
                return Some(0);
            }
            match msg {
                agenda::CHANGED => {
                    app.organizer_tick();
                    app.update_views();
                    app.schedule_tick();
                    Some(0)
                }
                agenda::OPEN_DATE => {
                    app.hide_panel();
                    app.agenda_scope = chrono::NaiveDate::from_num_days_from_ce_opt(w as i32);
                    app.agenda_state[0] = None;
                    app.page = 8;
                    app.render();
                    if let Some(date) = app.agenda_scope {
                        agenda::open_date(app.field(agenda::ID), date);
                    }
                    ShowWindow(hwnd, SW_SHOW);
                    SetForegroundWindow(hwnd);
                    Some(0)
                }
                agenda::OPEN_ENTRY => {
                    app.hide_panel();
                    app.page = 8;
                    app.render();
                    agenda::open_entry(app.field(agenda::ID), w as u64);
                    ShowWindow(hwnd, SW_SHOW);
                    SetForegroundWindow(hwnd);
                    Some(0)
                }
                agenda::OPEN_ALARMS => {
                    app.page = 8;
                    app.render();
                    agenda::open_alarms(app.field(agenda::ID));
                    Some(0)
                }
                taskbar_clock::WM_ACTIVATED | WM_TIMER
                    if msg == taskbar_clock::WM_ACTIVATED || w == 3 =>
                {
                    if let Some(event) = app.taskbar.as_ref().and_then(|t| t.take_activation()) {
                        CLOCK_RETRY.with(|pending| pending.set(None));
                        app.show_panel_at(event.anchor);
                    }
                    if CLOCK_RETRY
                        .with(|pending| pending.get().is_none_or(|end| Instant::now() >= end))
                    {
                        CLOCK_RETRY.with(|pending| pending.set(None));
                        KillTimer(hwnd, 3);
                    }
                    Some(0)
                }
                WM_PREPARE_PANELS => {
                    app.prewarm_panels();
                    Some(0)
                }
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
                    if let Some(event) = app.pending_activation.take()
                        && app
                            .preview
                            .as_ref()
                            .is_some_and(|p| p.owns_activation(&event))
                    {
                        app.show_panel_at(event.anchor);
                    }
                    Some(0)
                }
                WM_COMMAND => {
                    let id = (w & 0xffff) as u16;
                    if app.page == 2 && (id == 301 || id == 305) && (w >> 16) == 1 {
                        match app.collect_page() {
                            Ok(()) => set_text(app.field(306), &app.monitor_summary()),
                            Err(error) => app.set_status(error),
                        }
                        return Some(0);
                    }
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
                WM_TIMER if w == 2 => {
                    KillTimer(hwnd, 2);
                    if TRAY_READD.with(|pending| pending.replace(false)) {
                        app.add_tray();
                    }
                    app.refresh_displays();
                    app.close_panel();
                    PostMessageW(hwnd, WM_PREPARE_PANELS, 0, 0);
                    Some(0)
                }
                WM_TIMER => {
                    app.organizer_tick();
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
                WM_SHOW_SECONDARY => {
                    let _ = app.collect_page();
                    if (1..=PAGES.len()).contains(&w) {
                        app.page = w - 1;
                    }
                    let snapshot = displays::collect();
                    if let Some(display) = snapshot.displays.iter().find(|d| !d.primary) {
                        let area = display.work;
                        let dpi = GetDpiForWindow(hwnd).max(96);
                        let margin = (32 * dpi / 96) as i32;
                        let width = app.scale(1120).min(area[2] - area[0] - margin * 2);
                        let height = app.scale(920).min(area[3] - area[1] - margin * 2);
                        SetWindowPos(
                            hwnd,
                            null_mut(),
                            area[0] + margin,
                            area[1] + margin,
                            width,
                            height,
                            SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                        let dpi = GetDpiForWindow(hwnd).max(96);
                        if dpi != app.dpi {
                            app.dpi = dpi;
                            app.recreate_resources();
                        }
                        app.render();
                        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                    }
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
                        NIN_BALLOONUSERCLICK => {
                            app.page = 9;
                            app.render();
                            ShowWindow(hwnd, SW_SHOW);
                            SetForegroundWindow(hwnd);
                        }
                        _ => {}
                    }
                    Some(0)
                }
                WM_SIZE => {
                    if w != SIZE_MINIMIZED as usize {
                        let fields = app.input_snapshot();
                        let _ = app.collect_page();
                        app.render();
                        app.restore_inputs(fields);
                    }
                    Some(0)
                }
                WM_DPICHANGED => {
                    let fields = app.input_snapshot();
                    let _ = app.collect_page();
                    app.dpi = (w & 0xffff) as u32;
                    app_icon::apply(hwnd, app.dpi);
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
                    app.restore_inputs(fields);
                    PostMessageW(hwnd, WM_PREPARE_PANELS, 0, 0);
                    Some(0)
                }
                WM_SETTINGCHANGE | WM_THEMECHANGED | WM_DWMCOLORIZATIONCOLORCHANGED => {
                    design::invalidate_theme();
                    {
                        let fields = app.input_snapshot();
                        let _ = app.collect_page();
                        app.recreate_resources();
                        app.render();
                        app.restore_inputs(fields);
                        PostMessageW(hwnd, WM_PREPARE_PANELS, 0, 0);
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
                _ => None,
            }
        });
        if quit {
            TASKBAR.with(|p| p.borrow_mut().take());
            PREVIEW.with(|p| p.borrow_mut().take());
            STATE.with(|s| {
                if let Some(app) = s.borrow_mut().as_mut() {
                    app.preview.take();
                    app.taskbar.take();
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
            SetTimer(GetWindow(hwnd, GW_OWNER), 2, 500, None);
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
                _ => None,
            }
        });
        handled.unwrap_or_else(|| DefWindowProcW(hwnd, msg, w, l))
    }
}
unsafe fn set_text(hwnd: HWND, text: &str) {
    unsafe {
        if !hwnd.is_null() && window_text(hwnd) != text {
            SetWindowTextW(hwnd, wide(text).as_ptr());
        }
    }
}
// Painting can re-enter synchronously while a page is being built. Keep its
// immutable data outside the mutably borrowed application state.
unsafe fn paint_message(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> Option<LRESULT> {
    unsafe {
        let p = PALETTE.with(Cell::get)?;
        match msg {
            WM_ERASEBKGND => Some(1),
            WM_PAINT | WM_PRINTCLIENT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let dc = if msg == WM_PRINTCLIENT {
                    w as HDC
                } else {
                    BeginPaint(hwnd, &mut ps)
                };
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut r);
                let t = design::Theme::new(p.dark);
                design::fill(dc, r, t.bg);
                if !GetPropW(hwnd, wide("LiteTray.SettingsSurface").as_ptr()).is_null() {
                    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
                    let s = |n: i32| n * dpi / 96;
                    design::fill(dc, design::rect(s(232), 0, s(1), r.bottom), t.border);
                    {
                        design::rounded(
                            dc,
                            design::rect(s(247), s(123), r.right - s(274), r.bottom - s(218)),
                            t.surface,
                            t.border,
                            s(16),
                        );
                    }
                    design::fill(
                        dc,
                        design::rect(s(248), r.bottom - s(84), r.right - s(276), s(1)),
                        t.border,
                    );
                    let mut child = GetWindow(hwnd, GW_CHILD);
                    while !child.is_null() {
                        if !GetPropW(child, wide("LiteTray.Field").as_ptr()).is_null() {
                            let mut bounds: RECT = std::mem::zeroed();
                            GetWindowRect(child, &mut bounds);
                            MapWindowPoints(
                                null_mut(),
                                hwnd,
                                &mut bounds as *mut RECT as *mut POINT,
                                2,
                            );
                            InflateRect(&mut bounds, s(2), s(2));
                            design::rounded(dc, bounds, t.field, t.border, s(8));
                            if GetFocus() == child {
                                design::fill(
                                    dc,
                                    design::rect(
                                        bounds.left + s(3),
                                        bounds.bottom - s(2),
                                        bounds.right - bounds.left - s(6),
                                        s(2),
                                    ),
                                    t.accent,
                                );
                            }
                        }
                        child = GetWindow(child, GW_HWNDNEXT);
                    }
                }
                if msg != WM_PRINTCLIENT {
                    EndPaint(hwnd, &ps);
                }
                Some(0)
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                let t = design::Theme::new(p.dark);
                let surface = !GetPropW(l as HWND, wide("LiteTray.Surface").as_ptr()).is_null();
                SetTextColor(w as HDC, p.text);
                SetBkColor(w as HDC, if surface { t.surface } else { p.bg });
                Some(if surface { p.surface } else { p.background } as isize)
            }
            WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
                SetTextColor(w as HDC, p.text);
                SetBkColor(w as HDC, p.edit_bg);
                Some(p.edit as isize)
            }
            WM_DRAWITEM if l != 0 => {
                let item = &*(l as *const DRAWITEMSTRUCT);
                if item.CtlType == ODT_COMBOBOX {
                    design::draw_combo(item, p.dark);
                    return Some(1);
                }
                if item.CtlType != ODT_BUTTON {
                    return None;
                }
                let t = design::Theme::new(p.dark);
                let nav = (NAV as u32..NAV as u32 + PAGES.len() as u32).contains(&item.CtlID);
                let selected = item.CtlID == p.active as u32;
                let primary = item.CtlID == SAVE as u32 || item.CtlID == 42;
                let pressed = item.itemState & ODS_SELECTED != 0;
                let hover = !GetPropW(item.hwndItem, wide("LiteTray.Hover").as_ptr()).is_null();
                let dpi = GetDpiForWindow(hwnd).max(96) as i32;
                let s = |n: i32| n * dpi / 96;
                let on_surface =
                    !GetPropW(item.hwndItem, wide("LiteTray.Surface").as_ptr()).is_null();
                let base = if on_surface { t.surface } else { t.bg };
                design::fill(item.hDC, item.rcItem, base);
                let color = if primary {
                    t.accent
                } else if selected || pressed || hover {
                    t.tint
                } else if nav {
                    t.bg
                } else {
                    t.surface
                };
                let border = if nav || primary { color } else { t.border };
                design::rounded(item.hDC, item.rcItem, color, border, s(10));
                let mut rect = item.rcItem;
                if nav {
                    let icons = [
                        "\u{e787}", "\u{e823}", "\u{e7f4}", "\u{e753}", "\u{e713}", "\u{e946}",
                        "\u{e8b5}", "\u{e790}", "\u{e8fd}", "\u{e916}",
                    ];
                    let icon_font = make_font(s(17), 400, "Segoe MDL2 Assets");
                    design::text(
                        item.hDC,
                        icons[(item.CtlID - NAV as u32) as usize],
                        design::rect(s(17), 0, s(26), rect.bottom),
                        icon_font,
                        if selected { t.accent } else { t.muted },
                        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                    );
                    DeleteObject(icon_font);
                    rect.left += s(52);
                    if selected {
                        design::rounded(
                            item.hDC,
                            design::rect(s(1), s(9), s(3), rect.bottom - s(18)),
                            t.accent,
                            t.accent,
                            s(3),
                        );
                    }
                }
                let font = SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as HFONT;
                design::text(
                    item.hDC,
                    &window_text(item.hwndItem),
                    rect,
                    font,
                    if item.itemState & ODS_DISABLED != 0 {
                        t.muted
                    } else if primary {
                        t.on_accent
                    } else if selected {
                        t.accent
                    } else {
                        t.text
                    },
                    if nav {
                        DT_LEFT | DT_VCENTER | DT_SINGLELINE
                    } else {
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE
                    },
                );
                if item.itemState & ODS_FOCUS != 0 {
                    let mut focus = item.rcItem;
                    InflateRect(&mut focus, -s(3), -s(3));
                    DrawFocusRect(item.hDC, &focus);
                }
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
