//! Real native controls rendered to a memory DC. This creates only hidden
//! owned windows; it has no Explorer connection, profile, tray icon or network.
use super::*;
use chrono::Days;

// Exercise real ShowWindow/restore and message routing without showing any
// test window on the user's input desktop. Never call SwitchDesktop.
fn on_isolated_desktop(check: impl FnOnce() + Send + 'static) {
    // WinRT/UISettings and registered native classes may keep process-wide
    // desktop state after earlier UI tests. Start a fresh process before
    // creating an alternate desktop instead of mixing both in one process.
    if std::env::var_os("LITETRAY_ISOLATED_SETTINGS_TEST").is_none() {
        use std::{
            os::windows::process::CommandExt,
            process::{Command, Stdio},
        };
        let name = thread::current().name().unwrap().to_owned();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &name, "--test-threads=1", "--nocapture"])
            .env("LITETRAY_ISOLATED_SETTINGS_TEST", "1")
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("isolated settings test timed out: {name}");
            }
            thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "isolated settings test failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    use windows_sys::Win32::System::StationsAndDesktops::*;
    unsafe {
        let name = wide(&format!(
            "LiteTray.SettingsCheck.{}.{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let desktop = CreateDesktopW(name.as_ptr(), null(), null(), 0, GENERIC_ALL, null());
        assert!(
            !desktop.is_null(),
            "cannot create isolated desktop: {}",
            GetLastError()
        );
        let handle = desktop as usize;
        let result = thread::spawn(move || {
            assert_ne!(SetThreadDesktop(handle as HDESK), 0);
            check();
        })
        .join();
        CloseDesktop(desktop);
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }
}
unsafe fn dispatch_settings_requests() {
    unsafe {
        // Only dispatch messages involved in settings activation. This fixture
        // must not run the production weather or holiday timer workers.
        for _ in 0..8 {
            let mut dispatched = false;
            for kind in [WM_COMMAND, WM_SHOW_SETTINGS] {
                let mut message: MSG = std::mem::zeroed();
                while PeekMessageW(&mut message, null_mut(), kind, kind, PM_REMOVE) != 0 {
                    DispatchMessageW(&message);
                    dispatched = true;
                }
            }
            if !dispatched {
                break;
            }
        }
    }
}
unsafe fn settings_first_click_case(busy: bool, minimized: bool) -> (bool, bool) {
    unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-settings-open-unused.json"),
        );
        app.render();
        app.prewarm_panels();
        let hwnd = app.hwnd;
        let popup = app.panel_cache[0];
        app.panel = popup;
        let calendar = GetDlgItem(popup, dashboard::ID as i32);
        let settings = GetDlgItem(calendar, 40);
        assert!(!settings.is_null());
        if minimized {
            ShowWindow(hwnd, SW_SHOWMINNOACTIVE);
            assert_ne!(IsIconic(hwnd), 0);
        }
        ShowWindow(hwnd, SW_HIDE);
        ShowWindow(popup, SW_SHOWNOACTIVATE);
        STATE.with(|s| *s.borrow_mut() = Some(app));
        // BM_CLICK follows the actual settings icon -> calendar -> popup ->
        // application message chain. Exactly one click, no second command.
        if busy {
            STATE.with(|s| {
                let _held = s.borrow_mut();
                SendMessageW(settings, BM_CLICK, 0, 0);
                dispatch_settings_requests();
            });
        } else {
            SendMessageW(settings, BM_CLICK, 0, 0);
            dispatch_settings_requests();
        }
        // Drain a deferred activation retry, if the implementation uses one.
        let until = Instant::now() + Duration::from_millis(120);
        while Instant::now() < until {
            dispatch_settings_requests();
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(&mut message, hwnd, WM_TIMER, WM_TIMER, PM_REMOVE) != 0 {
                if message.wParam != 1 && message.wParam != 2 && message.wParam != 3 {
                    DispatchMessageW(&message);
                }
            }
            thread::sleep(Duration::from_millis(4));
        }
        let result = (IsWindowVisible(hwnd) != 0, IsIconic(hwnd) == 0);
        STATE.with(|s| {
            let mut app = s.borrow_mut().take().unwrap();
            app.close_panel();
            DestroyWindow(hwnd);
            app.release_resources();
        });
        RoUninitialize();
        result
    }
}
#[test]
fn settings_open_first_click_survives_calendar_reentry() {
    on_isolated_desktop(|| unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-settings-calendar-unused.json"),
        );
        app.render();
        app.prewarm_panels();
        let hwnd = app.hwnd;
        let popup = app.panel_cache[0];
        app.panel = popup;
        let view = GetDlgItem(popup, dashboard::ID as i32);
        STATE.with(|s| *s.borrow_mut() = Some(app));
        dashboard::with_borrowed_view(view, || {
            SendMessageW(GetDlgItem(view, 40), BM_CLICK, 0, 0);
        });
        dispatch_settings_requests();
        let shown = IsWindowVisible(hwnd) != 0;
        STATE.with(|s| {
            let mut app = s.borrow_mut().take().unwrap();
            app.close_panel();
            DestroyWindow(hwnd);
            app.release_resources();
        });
        RoUninitialize();
        assert!(
            shown,
            "settings icon click was dropped while calendar view was updating"
        );
    });
}
#[test]
fn settings_open_first_click_from_hidden_window() {
    on_isolated_desktop(|| unsafe {
        let (shown, restored) = settings_first_click_case(false, false);
        assert!(
            shown && restored,
            "first settings click did not show a usable window: visible={shown}, restored={restored}"
        );
    });
}
#[test]
fn settings_open_first_click_survives_reentrant_dispatch() {
    on_isolated_desktop(|| unsafe {
        let (shown, restored) = settings_first_click_case(true, false);
        assert!(
            shown && restored,
            "first settings click was lost during nested message dispatch: visible={shown}, restored={restored}"
        );
    });
}
#[test]
fn settings_open_first_click_restores_a_previously_minimized_window() {
    on_isolated_desktop(|| unsafe {
        let (shown, restored) = settings_first_click_case(false, true);
        assert!(
            shown && restored,
            "first settings click left the window minimized: visible={shown}, restored={restored}"
        );
    });
}
#[test]
fn settings_open_handoff_keeps_source_until_settings_are_shown() {
    on_isolated_desktop(|| unsafe {
        struct Probe {
            source: HWND,
            shown: bool,
            source_visible_when_shown: bool,
        }
        unsafe extern "system" fn probe(
            hwnd: HWND,
            msg: u32,
            w: WPARAM,
            l: LPARAM,
            _id: usize,
            data: usize,
        ) -> LRESULT {
            unsafe {
                if msg == WM_SHOWWINDOW && w != 0 {
                    let probe = &mut *(data as *mut Probe);
                    probe.shown = true;
                    probe.source_visible_when_shown = IsWindowVisible(probe.source) != 0;
                }
                DefSubclassProc(hwnd, msg, w, l)
            }
        }
        for detached in [false, true] {
            let mut app = app_for_check(
                false,
                96,
                &std::env::temp_dir().join("litetray-settings-handoff-unused.json"),
            );
            app.render();
            app.prewarm_panels();
            let hwnd = app.hwnd;
            let popup = app.panel_cache[0];
            app.panel = popup;
            panel::set_detached(popup, detached);
            ShowWindow(popup, SW_SHOWNOACTIVATE);
            let mut log = Probe {
                source: popup,
                shown: false,
                source_visible_when_shown: false,
            };
            SetWindowSubclass(hwnd, Some(probe), 99, &mut log as *mut _ as usize);
            STATE.with(|s| *s.borrow_mut() = Some(app));
            SendMessageW(
                GetDlgItem(GetDlgItem(popup, dashboard::ID as i32), 40),
                BM_CLICK,
                0,
                0,
            );
            dispatch_settings_requests();
            let usable = IsWindowVisible(hwnd) != 0 && IsIconic(hwnd) == 0;
            let source_visible = IsWindowVisible(popup) != 0;
            RemoveWindowSubclass(hwnd, Some(probe), 99);
            STATE.with(|s| {
                let mut app = s.borrow_mut().take().unwrap();
                app.close_panel();
                DestroyWindow(hwnd);
                app.release_resources();
            });
            RoUninitialize();
            assert!(usable && log.shown);
            assert!(
                log.source_visible_when_shown,
                "settings handoff hides the source before showing its destination"
            );
            assert_eq!(
                source_visible, detached,
                "only the originating attached popup should close"
            );
        }
    });
}
unsafe fn wait_settings(hwnd: HWND) {
    unsafe {
        let deadline = Instant::now() + Duration::from_millis(200);
        while Instant::now() < deadline {
            dispatch_settings_requests();
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(&mut message, hwnd, WM_TIMER, WM_TIMER, PM_REMOVE) != 0 {
                if message.wParam == 74 {
                    DispatchMessageW(&message);
                }
            }
            if IsWindowVisible(hwnd) != 0 && IsIconic(hwnd) == 0 {
                return;
            }
            thread::sleep(Duration::from_millis(4));
        }
    }
}
#[test]
fn settings_open_single_request_waits_for_modal_owner_and_can_be_cancelled() {
    on_isolated_desktop(|| unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-settings-modal-unused.json"),
        );
        app.render();
        let hwnd = app.hwnd;
        STATE.with(|s| *s.borrow_mut() = Some(app));
        EnableWindow(hwnd, 0);
        SendMessageW(hwnd, WM_SHOW_SETTINGS, 0, 0);
        wait_settings(hwnd);
        assert_eq!(
            IsWindowVisible(hwnd),
            0,
            "do not activate a disabled modal owner"
        );
        EnableWindow(hwnd, 1);
        wait_settings(hwnd);
        assert_ne!(
            IsWindowVisible(hwnd),
            0,
            "one request must survive the entire modal loop"
        );
        assert_eq!(
            GetPropW(hwnd, wide("LiteTray.SettingsOpen.Completed").as_ptr()) as usize,
            1
        );
        ShowWindow(hwnd, SW_HIDE);
        STATE.with(|s| {
            let _held = s.borrow_mut();
            SendMessageW(hwnd, WM_SHOW_SETTINGS, 0, 0);
            SendMessageW(hwnd, WM_CLOSE, 0, 0);
        });
        wait_settings(hwnd);
        assert_eq!(
            IsWindowVisible(hwnd),
            0,
            "a cancelled pending open must not reappear later"
        );
        STATE.with(|s| {
            let mut app = s.borrow_mut().take().unwrap();
            DestroyWindow(hwnd);
            app.release_resources();
        });
        RoUninitialize();
    });
}
#[test]
fn settings_open_handles_one_hundred_first_clicks_across_all_settings_entries() {
    on_isolated_desktop(|| unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-settings-repeat-unused.json"),
        );
        app.page = 1;
        app.render();
        app.prewarm_panels();
        let hwnd = app.hwnd;
        let popup = app.panel_cache[0];
        app.panel = popup;
        let icon = GetDlgItem(GetDlgItem(popup, dashboard::ID as i32), 40);
        let template = app.field(201);
        set_text(template, "%time% · 未保存草稿");
        STATE.with(|s| *s.borrow_mut() = Some(app));
        for index in 0..100 {
            if index % 4 == 0 {
                ShowWindow(hwnd, SW_SHOWMINNOACTIVE);
            }
            ShowWindow(hwnd, SW_HIDE);
            ShowWindow(popup, SW_SHOWNOACTIVATE);
            let click = || {
                match index % 3 {
                    0 => {
                        SendMessageW(icon, BM_CLICK, 0, 0);
                    }
                    1 => {
                        PostMessageW(hwnd, WM_COMMAND, 40, 0);
                    }
                    _ => {
                        PostMessageW(hwnd, WM_SHOW_SETTINGS, 0, 0);
                    }
                }
                dispatch_settings_requests();
            };
            if index % 2 == 0 {
                STATE.with(|s| {
                    let _held = s.borrow_mut();
                    click();
                });
            } else {
                click();
            }
            wait_settings(hwnd);
            assert_ne!(IsWindowVisible(hwnd), 0, "click {index} was lost");
            assert_eq!(IsIconic(hwnd), 0, "click {index} failed to restore");
            assert_eq!(
                GetPropW(hwnd, wide("LiteTray.SettingsOpen.Completed").as_ptr()) as usize,
                index + 1,
                "one click must complete exactly once"
            );
            let draft = STATE.with(|s| s.borrow().as_ref().unwrap().text(201));
            assert_eq!(
                draft, "%time% · 未保存草稿",
                "native restore must preserve edits"
            );
        }
        STATE.with(|s| {
            let mut app = s.borrow_mut().take().unwrap();
            app.close_panel();
            DestroyWindow(hwnd);
            app.release_resources();
        });
        RoUninitialize();
    });
}
pub(super) unsafe fn assert_external_foreground() {
    unsafe {
        // A shared interactive desktop may change naturally during a test.
        // Fail if any window of the hidden test process took foreground.
        let mut process = 0;
        GetWindowThreadProcessId(GetForegroundWindow(), &mut process);
        assert_ne!(
            process,
            GetCurrentProcessId(),
            "hidden QA process took foreground"
        );
    }
}
#[test]
fn settings_switches_preserve_chrome_drafts_and_native_font_choices() {
    unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-settings-unused.json"),
        );
        app.page = 1;
        app.render();
        let navigation = app.field(NAV + 1);
        let title = app.field(901);
        assert!(
            SendMessageW(app.field(205), CB_GETCOUNT, 0, 0) > 1,
            "font picker must enumerate installed fonts"
        );
        set_text(app.field(201), "%time% · 自定义");
        app.command(NAV + 7);
        assert_eq!(
            app.field(NAV + 1),
            navigation,
            "page changes must not recreate navigation"
        );
        assert_eq!(app.field(901), title, "header survives page changes");
        assert_eq!(app.text(806), "沿用公共字体");
        let text = app.field(804);
        set_text(text, &"长备注\r\n".repeat(100));
        assert_ne!(
            GetWindowLongW(text, GWL_STYLE) as u32 & WS_VSCROLL,
            0,
            "overflow must remain scrollable"
        );
        set_text(text, "一行提示");
        assert_eq!(
            GetWindowLongW(text, GWL_STYLE) as u32 & WS_VSCROLL,
            0,
            "short text must not carry an empty scrollbar"
        );
        app.command(NAV + 1);
        assert_eq!(app.text(201), "%time% · 自定义");
        for index in 0..settings::PRESETS.len() {
            app.command(settings::PRESET + index as u16);
            app.collect_page().unwrap();
            app.draft.validate().unwrap();
            assert_eq!(app.draft.clock.width, 0);
        }
        assert_eq!(
            IsWindowVisible(app.hwnd),
            0,
            "hidden rendering cannot show or activate settings"
        );
        DestroyWindow(app.hwnd);
        app.release_resources();
    }
}
unsafe fn app_for_check(dark: bool, dpi: u32, path: &std::path::Path) -> App {
    unsafe {
        let _ = RoInitialize(RO_INIT_SINGLETHREADED);
        InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES,
        });
        let class = wide("LiteTray.VisualCheck.1");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: GetModuleHandleW(null()),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        });
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(panel_proc),
            hInstance: GetModuleHandleW(null()),
            lpszClassName: wide(PANEL_CLASS).as_ptr(),
            ..std::mem::zeroed()
        });
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            class.as_ptr(),
            wide("LiteTray visual fixture").as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            0,
            0,
            (1120 * dpi / 96) as i32,
            (920 * dpi / 96) as i32,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        assert!(!hwnd.is_null());
        // Native children query their actual monitor DPI. Use the same scale
        // for the parent layout so offscreen checks cannot mix two DPIs.
        let dpi = GetDpiForWindow(hwnd).max(96);
        SetWindowPos(
            hwnd,
            null_mut(),
            0,
            0,
            (1120 * dpi / 96) as i32,
            (920 * dpi / 96) as i32,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        SetPropW(
            hwnd,
            wide("LiteTray.SettingsSurface").as_ptr(),
            1usize as HANDLE,
        );
        let mut config = Config {
            appearance: if dark {
                Appearance::Dark
            } else {
                Appearance::Light
            },
            ..Default::default()
        };
        config.weather.enabled = false;
        let (tx, rx) = mpsc::channel();
        let store = Rc::new(RefCell::new(organizer::Store::open(path)));
        let today = Local::now().date_naive();
        for (kind, name, offset) in [
            (organizer::Kind::Todo, "阅读半小时", 0),
            (organizer::Kind::Event, "整理下周计划", 1),
            (organizer::Kind::Birthday, "家人的生日", 0),
            (organizer::Kind::Anniversary, "重要的纪念日", 12),
            (organizer::Kind::Countdown, "距离下次旅行", 30),
            (organizer::Kind::Alarm, "早间提醒", 1),
        ] {
            let mut entry = organizer::Entry::new(kind, today + Days::new(offset));
            entry.title = name.into();
            if kind == organizer::Kind::Alarm {
                entry.time = chrono::NaiveTime::from_hms_opt(8, 0, 0);
            }
            store.borrow_mut().data.put(entry).unwrap();
        }
        let mut app = App {
            hwnd,
            page_host: null_mut(),
            shell_layout: None,
            organizer: store,
            agenda_state: [None, None],
            holidays: Rc::new(RefCell::new(holidays::Catalog::default())),
            holiday_queue: Default::default(),
            holiday_active: None,
            agenda_scope: None,
            notices: Default::default(),
            next_notice: Instant::now(),
            popup_router: tray_popup::Router::default(),
            panel_cache: vec![],
            panel: null_mut(),
            preview: None,
            taskbar: None,
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
            draft: config.clone(),
            saved: config,
            status: "界面验证 · 示例日程 · 不触发提醒".into(),
            weather: None,
            update: None,
            migration_source: None,
            migration: None,
            monitor_snapshot: monitors::Snapshot {
                displays: vec![],
                warnings: vec![],
            },
            monitor_rows: vec![],
            tx,
            rx,
            weather_busy: false,
            update_busy: false,
            next_weather: Instant::now(),
            engine: Default::default(),
            taskbar_created: 0,
            build: 26300,
        };
        app.recreate_resources();
        app
    }
}
pub(super) unsafe fn write_window(hwnd: HWND, path: &std::path::Path) {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut r);
        let dc = CreateCompatibleDC(null_mut());
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: r.right,
            biHeight: -r.bottom,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        let mut bits = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        assert!(!bitmap.is_null());
        let old = SelectObject(dc, bitmap);
        SendMessageW(hwnd, WM_PRINTCLIENT, dc as usize, PRF_CLIENT as isize);
        // WM_PRINT on a top-level window offsets child painting by its title
        // bar even when PRF_NONCLIENT is absent. Draw direct children at their
        // actual client coordinates, in z-order, for an honest layout image.
        let first = GetWindow(hwnd, GW_CHILD);
        let mut child = GetWindow(first, GW_HWNDLAST);
        while !child.is_null() {
            if GetWindowLongW(child, GWL_STYLE) as u32 & WS_VISIBLE != 0 {
                let mut bounds: RECT = std::mem::zeroed();
                GetWindowRect(child, &mut bounds);
                MapWindowPoints(null_mut(), hwnd, &mut bounds as *mut RECT as *mut POINT, 2);
                let saved = SaveDC(dc);
                IntersectClipRect(dc, bounds.left, bounds.top, bounds.right, bounds.bottom);
                SetViewportOrgEx(dc, bounds.left, bounds.top, null_mut());
                SendMessageW(
                    child,
                    WM_PRINT,
                    dc as usize,
                    (PRF_CLIENT | PRF_CHILDREN | PRF_NONCLIENT | PRF_ERASEBKGND) as isize,
                );
                RestoreDC(dc, saved);
            }
            child = GetWindow(child, GW_HWNDPREV);
        }
        GdiFlush();
        let length = (r.right * r.bottom * 4) as usize;
        let mut bytes = vec![];
        bytes.extend_from_slice(b"BM");
        bytes.extend_from_slice(&((54 + length) as u32).to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend_from_slice(&54u32.to_le_bytes());
        bytes.extend_from_slice(&40u32.to_le_bytes());
        bytes.extend_from_slice(&r.right.to_le_bytes());
        bytes.extend_from_slice(&(-r.bottom).to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&32u16.to_le_bytes());
        bytes.extend_from_slice(&[0; 24]);
        bytes.extend_from_slice(std::slice::from_raw_parts(bits.cast::<u8>(), length));
        std::fs::write(path, bytes).unwrap();
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
    }
}
#[test]
#[ignore = "Renders hidden native settings controls; set LITETRAY_RENDER_DIR"]
fn render_settings_artifacts() {
    let output = std::path::PathBuf::from(
        std::env::var_os("LITETRAY_RENDER_DIR").expect("explicit render directory"),
    );
    std::fs::create_dir_all(&output).unwrap();
    unsafe {
        let previous_dpi = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        for dark in [false, true] {
            let mut app = app_for_check(dark, 120, &output.join("unused-agenda.json"));
            for page in 0..PAGES.len() {
                app.page = page;
                app.render();
                write_window(
                    app.hwnd,
                    &output.join(format!(
                        "settings-{}-{page}.bmp",
                        if dark { "dark" } else { "light" }
                    )),
                );
                if page >= 8 {
                    SendMessageW(app.page_host, WM_VSCROLL, SB_BOTTOM as usize, 0);
                    write_window(
                        app.hwnd,
                        &output.join(format!(
                            "settings-{}-{page}-bottom.bmp",
                            if dark { "dark" } else { "light" }
                        )),
                    );
                }
                if page == 9 {
                    for tab in 1..4 {
                        SendMessageW(app.field(agenda::ID), WM_COMMAND, 4300 + tab, 0);
                        write_window(
                            app.hwnd,
                            &output.join(format!(
                                "settings-{}-9-tab{tab}.bmp",
                                if dark { "dark" } else { "light" }
                            )),
                        );
                    }
                }
            }
            app.prewarm_panels();
            let popup = app.panel;
            write_window(
                popup,
                &output.join(format!(
                    "settings-{}-flyout.bmp",
                    if dark { "dark" } else { "light" }
                )),
            );
            panel::set_detached(popup, true);
            write_window(
                popup,
                &output.join(format!(
                    "settings-{}-detached.bmp",
                    if dark { "dark" } else { "light" }
                )),
            );
            app.close_panel();
            DestroyWindow(app.hwnd);
            app.release_resources();
        }
        SetThreadDpiAwarenessContext(previous_dpi);
    }
}

#[test]
fn native_agenda_editor_persists_six_types_and_timer_actions() {
    use litetray::organizer::{Kind, Store};
    unsafe {
        let directory = std::env::temp_dir().join(format!(
            "litetray-ui-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let file = directory.join("agenda.json");
        let mut app = app_for_check(false, 96, &file);
        app.organizer.borrow_mut().data.entries.clear();
        app.page = 8;
        app.render();
        let view = app.field(agenda::ID);
        assert_eq!(GetWindow(view, GW_CHILD), GetDlgItem(view, 4005));
        assert_eq!(
            GetWindowLongW(GetDlgItem(view, 4000), GWL_STYLE) as u32 & WS_VISIBLE,
            0
        );
        assert!(GetDlgItem(view, 4011).is_null());
        assert_eq!(window_text(GetDlgItem(view, 4201)), "日程");
        assert_eq!(window_text(GetDlgItem(view, 4015)), "准时");
        SetWindowTextW(GetDlgItem(view, 4010), wide("未保存的日程").as_ptr());
        SendMessageW(view, WM_COMMAND, 4200, 0);
        SetWindowTextW(GetDlgItem(view, 4010), wide("未保存的待办").as_ptr());
        SendMessageW(view, WM_COMMAND, 4201, 0);
        assert_eq!(window_text(GetDlgItem(view, 4010)), "未保存的日程");
        SendMessageW(view, WM_COMMAND, 4200, 0);
        assert_eq!(window_text(GetDlgItem(view, 4010)), "未保存的待办");
        for (index, kind) in Kind::ALL.into_iter().enumerate() {
            SendMessageW(view, WM_COMMAND, 4200 + index, 0);
            SendMessageW(view, WM_COMMAND, 4001, 0);
            SetWindowTextW(
                GetDlgItem(view, 4010),
                wide(&format!("测试{}", kind.label())).as_ptr(),
            );
            SetWindowTextW(
                GetDlgItem(view, 4012),
                wide(&(Local::now().date_naive() + Days::new(1)).to_string()).as_ptr(),
            );
            SendMessageW(view, WM_COMMAND, 4020, 0);
            assert_eq!(
                app.organizer.borrow().data.entries.len(),
                index + 1,
                "{}",
                window_text(GetDlgItem(view, 4022))
            );
        }
        let restored = Store::open(&file);
        assert!(restored.error.is_none());
        assert_eq!(restored.data.entries.len(), 6);
        app.page = 9;
        app.render();
        let view = app.field(agenda::ID);
        SendMessageW(view, WM_COMMAND, 4301, 0);
        assert_eq!(
            GetWindowLongW(GetDlgItem(view, 4000), GWL_STYLE) as u32 & WS_VISIBLE,
            0
        );
        SendMessageW(view, WM_COMMAND, 4122, 0);
        assert!(app.organizer.borrow().data.timers[0].due.is_some());
        SendMessageW(view, WM_COMMAND, 4123, 0);
        assert!(app.organizer.borrow().data.timers[0].due.is_none());
        SendMessageW(view, WM_COMMAND, 4123, 0);
        assert!(app.organizer.borrow().data.timers[0].due.is_some());
        SendMessageW(view, WM_COMMAND, 4102, 0);
        assert_eq!(app.organizer.borrow().data.timers.len(), 2);
        SendMessageW(view, WM_COMMAND, 4303, 0);
        // A local WAV also works on CI hosts without Windows' bundled Media
        // files. This exercises selection/persistence without playing audio.
        let sound_file = directory.join("test.wav");
        std::fs::write(&sound_file, b"RIFF\x24\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x40\x1f\0\0\x80\x3e\0\0\x02\0\x10\0data\0\0\0\0").unwrap();
        SendMessageW(GetDlgItem(view, 4400), CB_SETCURSEL, 11, 0);
        SetWindowTextW(
            GetDlgItem(view, 4401),
            wide(&sound_file.to_string_lossy()).as_ptr(),
        );
        SetWindowTextW(GetDlgItem(view, 4402), wide("45").as_ptr());
        SendMessageW(view, WM_COMMAND, 4405, 0);
        let restored = Store::open(&file);
        assert!(restored.error.is_none());
        assert_eq!(restored.data.sound.tone, 11);
        assert_eq!(
            restored.data.sound.custom_file,
            sound_file.to_string_lossy()
        );
        assert_eq!(restored.data.sound.seconds, 45);
        DestroyWindow(app.hwnd);
        app.release_resources();
        std::fs::remove_file(file).unwrap();
        std::fs::remove_file(sound_file).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}

#[test]
fn organizer_pages_fit_the_viewport_without_page_scrollbars() {
    unsafe {
        let mut app = app_for_check(
            true,
            96,
            &std::env::temp_dir().join("litetray-layout-check-unused.json"),
        );
        for (page, height) in [(8, 820), (9, 820), (8, 920), (9, 920)] {
            SetWindowPos(
                app.hwnd,
                null_mut(),
                0,
                0,
                app.scale(1060),
                app.scale(height),
                SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER,
            );
            app.page = page;
            app.render();
            let mut viewport: RECT = std::mem::zeroed();
            let mut editor: RECT = std::mem::zeroed();
            GetClientRect(app.page_host, &mut viewport);
            GetClientRect(app.field(agenda::ID), &mut editor);
            SendMessageW(app.page_host, WM_VSCROLL, SB_BOTTOM as usize, 0);
            let mut origin: RECT = std::mem::zeroed();
            GetWindowRect(app.field(agenda::ID), &mut origin);
            MapWindowPoints(
                null_mut(),
                app.page_host,
                &mut origin as *mut _ as *mut POINT,
                2,
            );
            assert_eq!(origin.top, 0, "a fitted page must ignore outer scrolling");
            let fits = editor.bottom <= viewport.bottom;
            if !fits {
                DestroyWindow(app.hwnd);
                app.release_resources();
            }
            assert!(
                fits,
                "page {page} content height {} exceeds viewport {}",
                editor.bottom, viewport.bottom
            );
            assert_eq!(
                GetWindowLongW(app.page_host, GWL_STYLE) as u32 & WS_VSCROLL,
                0
            );
            for tab in 0..if page == 9 { 4 } else { 6 } {
                let view = app.field(agenda::ID);
                SendMessageW(
                    view,
                    WM_COMMAND,
                    if page == 9 { 4300 } else { 4200 } + tab,
                    0,
                );
                let mut child = GetWindow(view, GW_CHILD);
                while !child.is_null() {
                    if GetWindowLongW(child, GWL_STYLE) as u32 & WS_VISIBLE != 0 {
                        let mut r: RECT = std::mem::zeroed();
                        GetWindowRect(child, &mut r);
                        MapWindowPoints(null_mut(), view, &mut r as *mut _ as *mut POINT, 2);
                        assert!(
                            r.top >= 0
                                && r.bottom <= editor.bottom
                                && r.left >= 0
                                && r.right <= editor.right,
                            "page {page}, tab {tab}, height {height}, control {} extends outside viewport: [{},{},{},{}] / [{},{}]",
                            GetDlgCtrlID(child),
                            r.left,
                            r.top,
                            r.right,
                            r.bottom,
                            editor.right,
                            editor.bottom
                        );
                    }
                    child = GetWindow(child, GW_HWNDNEXT);
                }
            }
        }
        DestroyWindow(app.hwnd);
        app.release_resources();
    }
}

#[test]
fn inline_draft_survives_hidden_flyout_theme_refresh_and_failed_save() {
    unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-inline-unused.json"),
        );
        app.prewarm_panels();
        let popup = app.panel_cache[0];
        let view = GetDlgItem(popup, dashboard::ID as i32);
        SendMessageW(view, WM_COMMAND, 1103, 0);
        let editor = GetDlgItem(view, quick_agenda::ID as i32);
        let title = GetDlgItem(editor, 4610);
        set_text(title, "未保存的弹层草稿");
        panel::hide(popup);
        app.draft.appearance = Appearance::Dark;
        app.recreate_resources();
        assert_ne!(
            IsWindow(popup),
            0,
            "theme refresh must preserve an editing flyout"
        );
        assert_eq!(
            GetDlgItem(
                GetDlgItem(popup, dashboard::ID as i32),
                quick_agenda::ID as i32
            ),
            editor
        );
        assert_eq!(window_text(title), "未保存的弹层草稿");
        let count = app.organizer.borrow().data.entries.len();
        app.organizer.borrow_mut().error = Some("日程文件读取失败，原文件已保留".into());
        SendMessageW(editor, WM_COMMAND, 4620, 0);
        assert_eq!(app.organizer.borrow().data.entries.len(), count);
        assert_eq!(window_text(title), "未保存的弹层草稿");
        assert!(dashboard::editing(view));
        // Esc dismisses the editor first, including in a detached popup.
        panel::set_detached(popup, true);
        SendMessageW(popup, WM_COMMAND, IDCANCEL as usize, 0);
        let mut message: MSG = std::mem::zeroed();
        while PeekMessageW(
            &mut message,
            view,
            quick_agenda::CLOSED,
            quick_agenda::SAVED,
            PM_REMOVE,
        ) != 0
        {
            DispatchMessageW(&message);
        }
        assert!(!dashboard::editing(view));
        assert!(panel::detached(popup));
        assert_eq!(IsWindowVisible(popup), 0);
        app.close_panel();
        DestroyWindow(app.hwnd);
        app.release_resources();
        assert_external_foreground();
    }
}

#[test]
fn hidden_calendar_cache_reuses_controls_and_tracks_new_anchor() {
    unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-panel-cache-unused.json"),
        );

        let cold = Instant::now();
        app.prewarm_panels();
        let cold = cold.elapsed();
        assert!(!app.panel_cache.is_empty());
        let panel = app.panel_cache[0];
        let view = GetDlgItem(panel, dashboard::ID as i32);
        let date = GetDlgItem(view, 1000);
        assert!(!date.is_null());
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(
            MonitorFromWindow(panel, MONITOR_DEFAULTTONEAREST),
            &mut info,
        );
        let anchor = popup::Rect {
            left: info.rcWork.right - 180,
            top: info.rcWork.bottom,
            right: info.rcWork.right - 100,
            bottom: info.rcWork.bottom + 40,
        };
        let mut samples = vec![];
        for _ in 0..100 {
            let start = Instant::now();
            assert_eq!(app.prepare_panel_at(anchor), Some(panel));
            dashboard::update(view, &app.saved, &app.weather, false);
            samples.push(start.elapsed().as_micros());
            assert_eq!(GetDlgItem(view, 1000), date);
            assert_eq!(IsWindowVisible(panel), 0);
        }
        let expected = popup::place(
            anchor,
            popup::Rect {
                left: info.rcWork.left,
                top: info.rcWork.top,
                right: info.rcWork.right,
                bottom: info.rcWork.bottom,
            },
            GetDpiForWindow(panel).max(96),
        )
        .unwrap();
        let mut bounds: RECT = std::mem::zeroed();
        GetWindowRect(panel, &mut bounds);
        assert_eq!(
            (bounds.left, bounds.top),
            (expected.rect.left, expected.rect.top)
        );
        samples.sort();
        eprintln!(
            "calendar prewarm={:.2}ms; cached prepare/update p95={}us",
            cold.as_secs_f64() * 1000.,
            samples[95]
        );
        assert!(samples[95] < 50_000);
        assert_external_foreground();
        app.close_panel();
        assert_eq!(IsWindow(panel), 0);
        DestroyWindow(app.hwnd);
        app.release_resources();
    }
}

#[test]
fn opening_focus_bounce_does_not_hide_calendar_and_popup_is_topmost() {
    unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-panel-focus-unused.json"),
        );
        app.prewarm_panels();
        let panel = app.panel;
        // Visible-style lifecycle test stays outside every desktop and never
        // asks for foreground, focus or physical input.
        SetWindowPos(
            panel,
            null_mut(),
            -30000,
            -30000,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );

        SendMessageW(panel, WM_ACTIVATE, WA_INACTIVE as usize, 0);
        let visible = IsWindowVisible(panel) != 0;
        let topmost = GetWindowLongW(panel, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST != 0;
        assert_external_foreground();
        app.close_panel();
        DestroyWindow(app.hwnd);
        app.release_resources();
        assert!(
            visible,
            "a delayed opening deactivation immediately hid the new calendar"
        );
        assert!(topmost, "the taskbar popup must be topmost");
    }
}

#[test]
fn detached_calendar_keeps_its_position_and_lifetime_until_close() {
    unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-panel-detach-unused.json"),
        );
        app.prewarm_panels();
        let popup = app.panel;
        let view = GetDlgItem(popup, dashboard::ID as i32);

        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(
            MonitorFromWindow(popup, MONITOR_DEFAULTTONEAREST),
            &mut info,
        );
        let anchor = popup::Rect {
            left: info.rcWork.right - 100,
            top: info.rcWork.bottom,
            right: info.rcWork.right,
            bottom: info.rcWork.bottom + 1,
        };
        SetWindowPos(
            popup,
            HWND_TOPMOST,
            -30000,
            -30000,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        SendMessageW(popup, WM_COMMAND, panel::DETACH as usize, 0);
        assert!(panel::detached(popup));
        assert_eq!(
            GetWindowLongW(GetDlgItem(view, panel::DETACH as i32), GWL_STYLE) as u32 & WS_VISIBLE,
            0
        );
        assert_ne!(
            GetWindowLongW(GetDlgItem(view, panel::CLOSE as i32), GWL_STYLE) as u32 & WS_VISIBLE,
            0
        );
        for _ in 0..100 {
            SendMessageW(popup, WM_ACTIVATE, WA_INACTIVE as usize, 0);
            SendMessageW(popup, WM_KEYDOWN, VK_ESCAPE as usize, 0);
        }
        assert_ne!(IsWindowVisible(popup), 0);
        let point = ((-29970i32 as u16 as usize) << 16) | (-29970i32 as u16 as usize);
        assert_eq!(
            SendMessageW(popup, WM_NCHITTEST, 0, point as isize),
            HTCAPTION as isize
        );
        assert_eq!(
            SendMessageW(view, WM_NCHITTEST, 0, point as isize),
            HTTRANSPARENT as isize
        );
        let attached = app.prepare_panel_at(anchor).unwrap();
        assert_ne!(attached, popup);
        app.hide_panel();
        app.recreate_resources();
        assert_ne!(IsWindow(popup), 0);
        assert_ne!(IsWindowVisible(popup), 0);
        assert!(panel::detached(popup));
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(popup, &mut r);
        assert_eq!((r.left, r.top), (-30000, -30000));
        SetWindowPos(
            popup,
            HWND_TOPMOST,
            -29950,
            -29950,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE,
        );
        GetWindowRect(popup, &mut r);
        assert_eq!((r.left, r.top), (-29950, -29950));
        SendMessageW(popup, WM_COMMAND, panel::CLOSE as usize, 0);
        assert_eq!(IsWindowVisible(popup), 0);
        assert!(!panel::detached(popup));
        assert_external_foreground();
        app.close_panel();
        DestroyWindow(app.hwnd);
        app.release_resources();
    }
}
