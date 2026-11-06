//! Real native controls rendered to a memory DC. This creates only hidden
//! owned windows; it has no Explorer connection, profile, tray icon or network.
use super::*;
use chrono::Days;
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
            organizer: store,
            agenda_state: [None, None],
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
unsafe fn write_window(hwnd: HWND, path: &std::path::Path) {
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
fn hidden_calendar_cache_reuses_controls_and_tracks_new_anchor() {
    unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-panel-cache-unused.json"),
        );
        let foreground = GetForegroundWindow();
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
        assert_eq!(GetForegroundWindow(), foreground);
        app.close_panel();
        assert_eq!(IsWindow(panel), 0);
        DestroyWindow(app.hwnd);
        app.release_resources();
    }
}
