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
        assert_eq!(window_text(GetDlgItem(view, 4011)), "日程");
        assert_eq!(window_text(GetDlgItem(view, 4015)), "准时");
        for (index, kind) in Kind::ALL.into_iter().enumerate() {
            SendMessageW(view, WM_COMMAND, 4001, 0);
            SetWindowTextW(
                GetDlgItem(view, 4010),
                wide(&format!("测试{}", kind.label())).as_ptr(),
            );
            SendMessageW(GetDlgItem(view, 4011), CB_SETCURSEL, index, 0);
            SendMessageW(view, WM_COMMAND, 4011 | ((CBN_SELCHANGE as usize) << 16), 0);
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
        assert_eq!(GetWindow(view, GW_CHILD), GetDlgItem(view, 4131));
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
        DestroyWindow(app.hwnd);
        app.release_resources();
        std::fs::remove_file(file).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
