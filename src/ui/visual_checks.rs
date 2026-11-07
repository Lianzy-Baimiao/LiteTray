//! Real native controls rendered to a memory DC. This creates only hidden
//! owned windows; it has no Explorer connection, profile, tray icon or network.
use super::*;
use chrono::Datelike;
use chrono::Days;

#[test]
fn weather_icons_have_smooth_edges_and_solid_strokes_at_actual_sizes() {
    on_isolated_desktop(|| unsafe {
        let mut failures = vec![];
        for dpi in [96, 120, 144, 192] {
            for logical in [22, 44] {
                let size = (logical * dpi + 48) / 96;
                let extent = size + 8;
                let dc = CreateCompatibleDC(null_mut());
                let mut info: BITMAPINFO = std::mem::zeroed();
                info.bmiHeader = BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: extent,
                    biHeight: -extent,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..std::mem::zeroed()
                };
                let mut bits = null_mut();
                let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
                assert!(!bitmap.is_null());
                let old = SelectObject(dc, bitmap);
                for (name, description) in [
                    ("sun", "晴"),
                    ("partly-cloudy", "晴间多云"),
                    ("cloud", "阴"),
                    ("rain", "小雨"),
                    ("thunder", "雷阵雨"),
                    ("snow", "雪"),
                    ("sleet", "雨夹雪"),
                    ("fog", "雾"),
                    ("hail", "冰雹"),
                    ("unknown", "未知"),
                ] {
                    for dark in [false, true] {
                        let (bg, fg) = if dark { (30, 240) } else { (250, 30) };
                        design::fill(dc, design::rect(0, 0, extent, extent), rgb(bg, bg, bg));
                        dashboard::weather_icon(
                            dc,
                            design::rect(4, 4, size, size),
                            rgb(fg, fg, fg),
                            description,
                        );
                        GdiFlush();
                        let pixels = std::slice::from_raw_parts(
                            bits.cast::<u8>(),
                            (extent * extent * 4) as usize,
                        );
                        let mut mixed = 0;
                        let mut solid = 0;
                        let mut spill = 0;
                        for (i, pixel) in pixels.as_chunks::<4>().0.iter().enumerate() {
                            assert!(
                                pixel[0] == pixel[1] && pixel[1] == pixel[2],
                                "colored antialias fringe"
                            );
                            if pixel[0] != bg as u8 {
                                let (x, y) = (i as i32 % extent, i as i32 / extent);
                                if x < 4 || x >= size + 4 || y < 4 || y >= size + 4 {
                                    spill += 1;
                                }
                                if pixel[0].abs_diff(fg as u8) <= 2 {
                                    solid += 1;
                                } else {
                                    mixed += 1;
                                }
                            }
                        }
                        if mixed < 8 || solid < 8 || spill != 0 {
                            failures.push(format!("{description} {logical}px {dpi}dpi dark={dark}: antialiased={mixed}, solid={solid}, outside={spill}"));
                        }
                        if let Some(dir) = std::env::var_os("LITETRAY_WEATHER_ICON_RENDER_DIR") {
                            let dir = std::path::PathBuf::from(dir);
                            std::fs::create_dir_all(&dir).unwrap();
                            let mut bytes = Vec::with_capacity(54 + pixels.len());
                            bytes.extend_from_slice(b"BM");
                            bytes.extend_from_slice(&(54u32 + pixels.len() as u32).to_le_bytes());
                            bytes.extend_from_slice(&[0; 4]);
                            bytes.extend_from_slice(&54u32.to_le_bytes());
                            bytes.extend_from_slice(&40u32.to_le_bytes());
                            bytes.extend_from_slice(&extent.to_le_bytes());
                            bytes.extend_from_slice(&(-extent).to_le_bytes());
                            bytes.extend_from_slice(&1u16.to_le_bytes());
                            bytes.extend_from_slice(&32u16.to_le_bytes());
                            bytes.extend_from_slice(&[0; 24]);
                            bytes.extend_from_slice(pixels);
                            std::fs::write(
                                dir.join(format!(
                                    "{name}-{logical}-{dpi}-{}.bmp",
                                    if dark { "dark" } else { "light" }
                                )),
                                bytes,
                            )
                            .unwrap();
                        }
                    }
                }
                SelectObject(dc, old);
                DeleteObject(bitmap);
                DeleteDC(dc);
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    });
}

#[test]
fn ui_text_has_neutral_antialiased_edges_at_monitor_scales() {
    on_isolated_desktop(|| unsafe {
        // Exercise the same font and drawing path as the calendar. Subpixel
        // RGB fringes are especially visible on its small Chinese labels and
        // do not remain neutral across different monitor pixel arrangements.
        let dc = CreateCompatibleDC(null_mut());
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: 720,
            biHeight: -64,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        let mut bits = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        assert!(!bitmap.is_null());
        let old = SelectObject(dc, bitmap);
        let face = design::font_face();
        let mut failures = vec![];
        let mut antialiased = 0;
        for dpi in [96, 120, 144, 192] {
            for size in [12, 14, 19] {
                for dark in [false, true] {
                    let (background, foreground) = if dark { (32, 255) } else { (255, 26) };
                    let color = |v| rgb(v, v, v);
                    let bounds = RECT {
                        left: 0,
                        top: 0,
                        right: 720,
                        bottom: 64,
                    };
                    design::fill(dc, bounds, color(background));
                    let font = make_font((size * dpi + 48) / 96, 400, &face);
                    design::text(
                        dc,
                        "星期六 · 立冬  农历九月廿九  天气 75%  12:34",
                        bounds,
                        font,
                        color(foreground),
                        DT_SINGLELINE | DT_VCENTER,
                    );
                    GdiFlush();
                    let pixels = std::slice::from_raw_parts(bits.cast::<u8>(), 720 * 64 * 4);
                    let mut fringe = 0;
                    let mut ink = 0;
                    for pixel in pixels.as_chunks::<4>().0 {
                        if pixel[0] != pixel[1] || pixel[1] != pixel[2] {
                            fringe += 1;
                        }
                        if pixel[0] != background as u8 {
                            ink += 1;
                            if pixel[0] != foreground as u8 {
                                antialiased += 1;
                            }
                        }
                    }
                    if fringe != 0 || ink == 0 {
                        failures.push(format!("{dpi} DPI, {size}px, dark={dark}: {fringe} colored edges, {ink} ink pixels"));
                    }
                    DeleteObject(font);
                }
            }
        }
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
        let mut smoothing = 0i32;
        if SystemParametersInfoW(SPI_GETFONTSMOOTHING, 0, &mut smoothing as *mut _ as _, 0) != 0
            && smoothing != 0
        {
            assert!(antialiased > 0, "text smoothing must not be disabled");
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        eprintln!("24 text samples: neutral edges at 100/125/150/200%, smoothing retained");
    });
}

#[test]
fn almanac_setting_survives_page_cache_and_saved_config() {
    on_isolated_desktop(|| unsafe {
        let path =
            std::env::temp_dir().join(format!("litetray-almanac-{}.json", std::process::id()));
        let mut app = app_for_check(false, 96, &path.with_extension("agenda"));
        app.page = 1;
        app.render();
        assert!(app.checked(220));
        SendMessageW(app.field(220), BM_SETCHECK, BST_UNCHECKED as usize, 0);
        app.command(NAV + 7);
        app.command(NAV + 1);
        assert!(!app.checked(220));
        app.collect_page().unwrap();
        config::save(&path, &app.draft).unwrap();
        let restored = config::load(&path).unwrap();
        assert!(!restored.clock.show_almanac);
        assert!(restored.clock.show_lunar);
        std::fs::remove_file(&path).unwrap();
        DestroyWindow(app.hwnd);
        app.release_resources();
    });
}

#[test]
#[ignore = "Explicit live network search on a non-input desktop, no profile writes"]
fn live_city_search_populates_native_choices_without_blocking_navigation() {
    on_isolated_desktop(|| unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-live-city-unused.json"),
        );
        app.page = 3;
        app.render();
        let saved = serde_json::to_value(&app.saved).unwrap();
        set_text(app.field(weather_location::QUERY), "深圳");
        let start = Instant::now();
        app.command(weather_location::SEARCH);
        assert!(
            start.elapsed() < Duration::from_millis(100),
            "location lookup blocked the UI thread"
        );
        app.command(NAV + 7);
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.location.active.is_some() && Instant::now() < deadline {
            app.receive();
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            app.location.active.is_none(),
            "location request did not finish"
        );
        assert!(!app.location.results.is_empty(), "{}", app.location.message);
        app.command(NAV + 3);
        assert!(SendMessageW(app.field(weather_location::CHOICE), CB_GETCOUNT, 0, 0) > 1);
        assert_eq!(serde_json::to_value(&app.saved).unwrap(), saved);
        eprintln!(
            "live native city search: {} choices, remained navigable",
            app.location.results.len()
        );
        DestroyWindow(app.hwnd);
        app.release_resources();
    });
}

#[test]
fn weather_city_selection_uses_coordinates_and_keeps_other_drafts() {
    on_isolated_desktop(|| unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-city-ui-unused.json"),
        );
        app.page = 3;
        app.render();
        let old = app.saved.weather.clone();
        assert!(app.field(402).is_null() && app.field(403).is_null());
        set_text(app.field(weather_location::QUERY), "深圳");
        set_text(app.field(414), "17");
        set_text(app.field(411), "draft-key-no-network");
        app.collect_page().unwrap();
        let serial = weather_location::begin_for_check(&mut app, false);
        app.sync_location_controls();
        assert_eq!(IsWindowEnabled(app.field(weather_location::SEARCH)), 0);
        let places = vec![
            location::Place {
                name: "深圳".into(),
                region: "广东 · 中国".into(),
                latitude: 22.55,
                longitude: 114.06,
                source: "Open-Meteo / GeoNames",
            },
            location::Place {
                name: "深圳".into(),
                region: "福建 · 中国".into(),
                latitude: 24.79,
                longitude: 118.43,
                source: "Open-Meteo / GeoNames",
            },
        ];
        app.tx.send(Event::Location(serial, Ok(places))).unwrap();
        app.receive();
        assert_eq!(
            app.draft.weather.latitude, old.latitude,
            "search never guesses which same-name city the user wants"
        );
        assert_eq!(
            app.text(411),
            "draft-key-no-network",
            "network completions must not rebuild the form and lose credentials"
        );
        assert_eq!(app.text(414), "17");
        assert_ne!(IsWindowEnabled(app.field(weather_location::SEARCH)), 0);
        assert_eq!(
            SendMessageW(app.field(weather_location::CHOICE), CB_GETCOUNT, 0, 0),
            3
        );
        SendMessageW(app.field(weather_location::CHOICE), CB_SETCURSEL, 1, 0);
        app.choose_location();
        assert_eq!(
            (app.draft.weather.latitude, app.draft.weather.longitude),
            (22.55, 114.06)
        );
        assert_eq!(
            app.saved.weather.latitude, old.latitude,
            "choosing a place only updates the settings draft"
        );
        app.command(NAV + 4);
        app.command(NAV + 3);
        assert_eq!(app.text(weather_location::QUERY), "深圳");
        assert_eq!(app.draft.weather.refresh_minutes, 17);
        assert_eq!(
            SendMessageW(app.field(weather_location::CHOICE), CB_GETCURSEL, 0, 0),
            1
        );
        app.draft.validate().unwrap();
        let mut failures = vec![];
        audit_native_layout(app.hwnd, "location results", &mut failures);
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        DestroyWindow(app.hwnd);
        app.release_resources();
    });
}

#[test]
fn location_replies_do_not_overwrite_new_queries_or_positions() {
    on_isolated_desktop(|| unsafe {
        let mut app = app_for_check(
            true,
            96,
            &std::env::temp_dir().join("litetray-location-stale-unused.json"),
        );
        app.page = 3;
        app.render();
        let old = app.draft.weather.latitude;
        let place = location::Place {
            name: "香港".into(),
            region: "".into(),
            latitude: 22.2783,
            longitude: 114.1747,
            source: "IPWHOIS",
        };
        let serial = weather_location::begin_for_check(&mut app, true);
        app.location.query = "北京".into();
        app.receive_location(serial, Ok(vec![place.clone()]));
        assert_eq!(app.draft.weather.latitude, old);
        assert!(app.location.message.contains("忽略旧查询"));
        let next = weather_location::begin_for_check(&mut app, true);
        app.receive_location(serial, Ok(vec![place.clone()]));
        assert!(
            app.location.active.is_some(),
            "old delivery cannot complete the new request"
        );
        app.receive_location(next, Err("网络查询失败".into()));
        assert_eq!(app.draft.weather.latitude, old);
        assert!(app.location.active.is_none());
        let serial = weather_location::begin_for_check(&mut app, true);
        app.draft.weather.latitude = 35.;
        app.receive_location(serial, Ok(vec![place.clone()]));
        assert_eq!(app.draft.weather.latitude, 35.);
        let serial = weather_location::begin_for_check(&mut app, true);
        app.page = 4;
        app.render();
        app.receive_location(serial, Ok(vec![place]));
        assert_eq!(app.draft.weather.city, "香港");
        assert_ne!(app.saved.weather.city, "香港");
        app.page = 3;
        app.render();
        assert!(app.text(420).contains("近似位置"));
        assert!(app.text(420).contains("代理"));
        let mut failures = vec![];
        audit_native_layout(app.hwnd, "nearby result", &mut failures);
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        DestroyWindow(app.hwnd);
        app.release_resources();
    });
}

#[test]
fn selected_calendar_day_has_antialiased_corners() {
    on_isolated_desktop(|| unsafe {
        for dark in [false, true] {
            let mut app = app_for_check(
                dark,
                96,
                &std::env::temp_dir().join("litetray-edge-check.json"),
            );
            app.render();
            let dashboard = app.field(dashboard::ID);
            let selection = dashboard::selection(dashboard).unwrap();
            let index = selection
                .grid()
                .iter()
                .position(|d| *d == selection.selected)
                .unwrap();
            let day = GetDlgItem(dashboard, 1000 + index as i32);
            let path = std::env::temp_dir().join(format!("litetray-calendar-edge-{dark}.bmp"));
            write_window(day, &path);
            let bmp = std::fs::read(&path).unwrap();
            let width = i32::from_le_bytes(bmp[18..22].try_into().unwrap()) as usize;
            let t = design::Theme::new(dark);
            let mut intermediate = 0;
            for y in 0..5 {
                for x in 0..5 {
                    let i = 54 + (y * width + x) * 4;
                    let c = rgb(bmp[i + 2] as u32, bmp[i + 1] as u32, bmp[i] as u32);
                    if c != t.surface && c != t.accent {
                        intermediate += 1;
                    }
                }
            }
            assert!(
                intermediate >= 3,
                "date corner has only {intermediate} blended edge pixels; hard GDI stair steps"
            );
            DestroyWindow(app.hwnd);
            app.release_resources();
        }
    });
}

// Exercise real ShowWindow/restore and message routing without showing any
// test window on the user's input desktop. Never call SwitchDesktop.
pub(super) fn on_isolated_desktop(check: impl FnOnce() + Send + 'static) {
    // WinRT/UISettings and registered native classes may keep process-wide
    // desktop state after earlier UI tests. Start a fresh process before
    // creating an alternate desktop instead of mixing both in one process.
    if std::env::var_os("LITETRAY_ISOLATED_SETTINGS_TEST").is_none() {
        use std::{
            os::windows::process::CommandExt,
            process::{Command, Stdio},
        };
        let name = thread::current().name().unwrap().to_owned();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", &name, "--test-threads=1", "--nocapture"]);
        if std::env::args().any(|arg| arg == "--ignored") {
            command.arg("--ignored");
        }
        let mut child = command
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
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "isolated test did not execute: {name}"
        );
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        return;
    }
    isolated_desktop_check(check);
}
fn isolated_desktop_check(check: impl FnOnce() + Send + 'static) {
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
// Match Start-Process -WindowStyle Hidden, including the first ShowWindow's
// STARTUPINFO override. CREATE_NO_WINDOW alone does not set that override.
fn on_startup_desktop(show: SHOW_WINDOW_CMD, check: impl FnOnce() + Send + 'static) {
    const CHILD: &str = "--skip=litetray-hidden-startup-child";
    if std::env::args().any(|arg| arg == CHILD) {
        unsafe {
            let mut startup: STARTUPINFOW = std::mem::zeroed();
            GetStartupInfoW(&mut startup);
            assert_ne!(startup.dwFlags & STARTF_USESHOWWINDOW, 0);
            assert_eq!(startup.wShowWindow, show as u16);
        }
        isolated_desktop_check(check);
        return;
    }
    use windows_sys::Win32::{Security::SECURITY_ATTRIBUTES, Storage::FileSystem::*};
    unsafe {
        let name = thread::current().name().unwrap().to_owned();
        let executable = std::env::current_exe().unwrap();
        let log = std::env::temp_dir().join(format!(
            "litetray-hidden-startup-{}-{}.log",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let security = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: null_mut(),
            bInheritHandle: 1,
        };
        let output = CreateFileW(
            wide(&log.to_string_lossy()).as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_READ,
            &security,
            CREATE_ALWAYS,
            FILE_ATTRIBUTE_TEMPORARY,
            null_mut(),
        );
        assert_ne!(output, INVALID_HANDLE_VALUE);
        let startup = STARTUPINFOW {
            cb: size_of::<STARTUPINFOW>() as u32,
            dwFlags: STARTF_USESHOWWINDOW | STARTF_USESTDHANDLES,
            wShowWindow: show as u16,
            hStdOutput: output,
            hStdError: output,
            ..std::mem::zeroed()
        };
        let mut process: PROCESS_INFORMATION = std::mem::zeroed();
        let mut command = wide(&format!(
            "\"{}\" --exact {name} --test-threads=1 --nocapture {CHILD}",
            executable.display()
        ));
        let created = CreateProcessW(
            wide(&executable.to_string_lossy()).as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            1,
            CREATE_NO_WINDOW,
            null(),
            null(),
            &startup,
            &mut process,
        );
        CloseHandle(output);
        assert_ne!(created, 0, "cannot launch hidden startup test");
        CloseHandle(process.hThread);
        let completed = WaitForSingleObject(process.hProcess, 20_000) == WAIT_OBJECT_0;
        if !completed {
            TerminateProcess(process.hProcess, 1);
            WaitForSingleObject(process.hProcess, 2_000);
        }
        let mut code = 1;
        GetExitCodeProcess(process.hProcess, &mut code);
        CloseHandle(process.hProcess);
        let output = std::fs::read_to_string(&log).unwrap_or_default();
        let _ = std::fs::remove_file(log);
        assert!(
            completed && code == 0,
            "hidden startup test failed: {output}"
        );
    }
}
unsafe fn dispatch_settings_requests() {
    unsafe {
        // Only dispatch messages involved in settings activation. This fixture
        // must not run the production weather or holiday timer workers.
        for _ in 0..8 {
            let mut dispatched = false;
            for kind in [
                WM_COMMAND,
                WM_SHOW_SETTINGS,
                agenda::OPEN_DATE,
                agenda::OPEN_ENTRY,
                agenda::OPEN_ALARMS,
                settings_open::OPEN_FOCUS,
                WM_TRAY,
            ] {
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
unsafe fn startup_settings_case(from_calendar: bool) {
    unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-settings-startup-unused.json"),
        );
        app.render();
        let hwnd = app.hwnd;
        let mut icon = null_mut();
        if from_calendar {
            app.prewarm_panels();
            let popup = app.panel_cache[0];
            app.panel = popup;
            // Production opens the calendar with SetWindowPos. Do not pre-hide
            // settings via ShowWindow: that would consume the startup override.
            panel::open(
                popup,
                popup::Rect {
                    left: 0,
                    top: 0,
                    right: 100,
                    bottom: 40,
                },
                false,
            );
            assert_ne!(IsWindowVisible(popup), 0);
            icon = GetDlgItem(GetDlgItem(popup, dashboard::ID as i32), 40);
        }
        assert_eq!(IsWindowVisible(hwnd), 0);
        STATE.with(|s| *s.borrow_mut() = Some(app));
        if from_calendar {
            SendMessageW(icon, BM_CLICK, 0, 0);
        } else {
            SendMessageW(hwnd, WM_SHOW_SETTINGS, 0, 0);
        }
        dispatch_settings_requests();
        wait_settings(hwnd);
        let shown = IsWindowVisible(hwnd) != 0;
        let restored = IsIconic(hwnd) == 0;
        let requested = GetPropW(hwnd, wide("LiteTray.SettingsOpen.Requested").as_ptr()) as usize;
        let completed = GetPropW(hwnd, wide("LiteTray.SettingsOpen.Completed").as_ptr()) as usize;
        STATE.with(|s| {
            let mut app = s.borrow_mut().take().unwrap();
            app.close_panel();
            DestroyWindow(hwnd);
            app.release_resources();
        });
        RoUninitialize();
        assert!(
            shown && restored && requested == 1 && completed == 1,
            "first request after process startup failed: visible={shown}, restored={restored}, requested={requested}, completed={completed}"
        );
    }
}
#[test]
fn settings_open_first_click_after_hidden_process_startup() {
    on_startup_desktop(SW_HIDE, || unsafe { startup_settings_case(true) });
}
unsafe fn startup_organizer_case(entry: u32, busy: u8) {
    unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-organizer-open-unused.json"),
        );
        app.render();
        app.prewarm_panels();
        let hwnd = app.hwnd;
        let popup = app.panel_cache[0];
        app.panel = popup;
        panel::open(
            popup,
            popup::Rect {
                left: 0,
                top: 0,
                right: 100,
                bottom: 40,
            },
            false,
        );
        let view = GetDlgItem(popup, dashboard::ID as i32);
        STATE.with(|s| *s.borrow_mut() = Some(app));
        let click = || {
            if entry == agenda::OPEN_DATE || entry == settings_open::OPEN_FOCUS {
                SendMessageW(
                    GetDlgItem(
                        view,
                        if entry == agenda::OPEN_DATE {
                            1106
                        } else {
                            1104
                        },
                    ),
                    BM_CLICK,
                    0,
                    0,
                );
            } else {
                SendMessageW(
                    hwnd,
                    entry,
                    0,
                    if entry == WM_TRAY {
                        NIN_BALLOONUSERCLICK as isize
                    } else {
                        0
                    },
                );
            }
            dispatch_settings_requests();
        };
        if busy == 1 {
            STATE.with(|s| {
                let _held = s.borrow_mut();
                click();
            });
        } else if busy == 2 {
            dashboard::with_borrowed_view(view, click);
            // The command was retained by the view's retry timer.
            SendMessageW(view, WM_TIMER, 1106, 0);
        } else {
            click();
        }
        wait_settings(hwnd);
        let shown = IsWindowVisible(hwnd) != 0 && IsIconic(hwnd) == 0;
        let page = STATE.with(|s| s.borrow().as_ref().unwrap().page);
        STATE.with(|s| {
            let mut app = s.borrow_mut().take().unwrap();
            app.close_panel();
            DestroyWindow(hwnd);
            app.release_resources();
        });
        RoUninitialize();
        assert!(
            shown,
            "one organizer click failed to open usable window: entry={entry}, busy={busy}, page={page}"
        );
        assert_eq!(
            page,
            if entry == WM_TRAY || entry == settings_open::OPEN_FOCUS {
                9
            } else {
                8
            }
        );
    }
}
#[test]
fn main_open_manage_first_click_after_hidden_process_startup() {
    on_startup_desktop(SW_HIDE, || unsafe {
        startup_organizer_case(agenda::OPEN_DATE, 0)
    });
}
#[test]
fn main_open_manage_first_click_survives_reentrant_dispatch() {
    on_startup_desktop(SW_HIDE, || unsafe {
        startup_organizer_case(agenda::OPEN_DATE, 1)
    });
}
#[test]
fn main_open_alarms_first_click_after_hidden_process_startup() {
    on_startup_desktop(SW_HIDE, || unsafe {
        startup_organizer_case(agenda::OPEN_ALARMS, 0)
    });
}
#[test]
fn main_open_entry_first_click_after_hidden_process_startup() {
    on_startup_desktop(SW_HIDE, || unsafe {
        startup_organizer_case(agenda::OPEN_ENTRY, 0)
    });
}
#[test]
fn main_open_notification_first_click_after_hidden_process_startup() {
    on_startup_desktop(SW_HIDE, || unsafe { startup_organizer_case(WM_TRAY, 0) });
}
#[test]
fn main_open_manage_survives_calendar_borrow() {
    on_startup_desktop(SW_HIDE, || unsafe {
        startup_organizer_case(agenda::OPEN_DATE, 2)
    });
}
#[test]
fn main_open_focus_first_click_after_hidden_process_startup() {
    on_startup_desktop(SW_HIDE, || unsafe {
        startup_organizer_case(settings_open::OPEN_FOCUS, 1)
    });
}
#[test]
fn main_open_manage_restores_minimized_startup() {
    on_startup_desktop(SW_SHOWMINIMIZED, || unsafe {
        startup_organizer_case(agenda::OPEN_DATE, 0)
    });
}
#[test]
fn main_open_mixed_entries_restore_once_and_preserve_settings_drafts() {
    on_isolated_desktop(|| unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-mixed-open-unused.json"),
        );
        app.page = 1;
        app.render();
        set_text(app.field(201), "%time% · 跨入口草稿");
        let hwnd = app.hwnd;
        STATE.with(|s| *s.borrow_mut() = Some(app));
        for (index, msg) in [
            agenda::OPEN_DATE,
            agenda::OPEN_ENTRY,
            agenda::OPEN_ALARMS,
            settings_open::OPEN_FOCUS,
            WM_SHOW_SETTINGS,
        ]
        .into_iter()
        .cycle()
        .take(35)
        .enumerate()
        {
            ShowWindow(hwnd, SW_SHOWMINNOACTIVE);
            ShowWindow(hwnd, SW_HIDE);
            STATE.with(|s| {
                let _held = s.borrow_mut();
                SendMessageW(
                    hwnd,
                    msg,
                    if msg == agenda::OPEN_DATE {
                        Local::now().date_naive().num_days_from_ce() as usize
                    } else {
                        0
                    },
                    0,
                );
            });
            wait_settings(hwnd);
            assert_ne!(IsWindowVisible(hwnd), 0, "entry {index} lost");
            assert_eq!(IsIconic(hwnd), 0);
            assert_eq!(
                GetPropW(hwnd, wide("LiteTray.SettingsOpen.Completed").as_ptr()) as usize,
                index + 1
            );
        }
        STATE.with(|s| {
            let mut app = s.borrow_mut().take().unwrap();
            app.command(NAV + 1);
            assert_eq!(app.text(201), "%time% · 跨入口草稿");
            DestroyWindow(hwnd);
            app.release_resources();
        });
        RoUninitialize();
    });
}
#[test]
fn volume_settings_switch_independently_and_survive_cache_and_save() {
    on_isolated_desktop(|| unsafe {
        let path =
            std::env::temp_dir().join(format!("litetray-volume-{}.json", std::process::id()));
        let mut app = app_for_check(false, 96, &path.with_extension("agenda"));
        app.page = 1;
        app.render();
        for (scroll, mute) in [(true, false), (false, true), (false, false), (true, true)] {
            SendMessageW(
                app.field(settings::VOLUME_SCROLL),
                BM_SETCHECK,
                scroll as usize,
                0,
            );
            SendMessageW(
                app.field(settings::VOLUME_MUTE),
                BM_SETCHECK,
                mute as usize,
                0,
            );
            app.command(NAV + 7);
            app.command(NAV + 1);
            assert_eq!(app.checked(settings::VOLUME_SCROLL), scroll);
            assert_eq!(app.checked(settings::VOLUME_MUTE), mute);
            config::save(&path, &app.draft).unwrap();
            let saved = config::load(&path).unwrap();
            assert_eq!(
                (saved.clock.volume_scroll, saved.clock.volume_mute),
                (scroll, mute)
            );
        }
        if let Some(output) = std::env::var_os("LITETRAY_VOLUME_RENDER_DIR") {
            let output = std::path::PathBuf::from(output);
            std::fs::create_dir_all(&output).unwrap();
            for dark in [false, true] {
                app.draft.appearance = if dark {
                    Appearance::Dark
                } else {
                    Appearance::Light
                };
                app.recreate_resources();
                app.render();
                write_window(
                    app.hwnd,
                    &output.join(format!(
                        "volume-{}.bmp",
                        if dark { "dark" } else { "light" }
                    )),
                );
            }
        }
        std::fs::remove_file(path).unwrap();
        DestroyWindow(app.hwnd);
        app.release_resources();
    });
}
#[test]
fn settings_open_first_request_after_hidden_process_startup() {
    on_startup_desktop(SW_HIDE, || unsafe { startup_settings_case(false) });
}
#[test]
fn settings_open_first_click_after_normal_process_startup() {
    on_startup_desktop(SW_SHOWNORMAL, || unsafe { startup_settings_case(true) });
}
#[test]
fn settings_open_first_request_after_minimized_process_startup() {
    on_startup_desktop(SW_SHOWMINIMIZED, || unsafe { startup_settings_case(false) });
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
fn settings_clock_navigation_stays_responsive() {
    on_isolated_desktop(|| unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-navigation-latency-unused.json"),
        );
        app.page = 1;
        app.render();
        ShowWindow(app.hwnd, SW_SHOWNOACTIVATE);
        UpdateWindow(app.hwnd);
        let cold_start = Instant::now();
        app.command(NAV + 7);
        RedrawWindow(
            app.hwnd,
            null(),
            null_mut(),
            RDW_UPDATENOW | RDW_ALLCHILDREN,
        );
        let cold = cold_start.elapsed().as_micros();
        app.command(NAV + 1);
        let mut samples = vec![];
        for index in 0..16 {
            let start = Instant::now();
            app.command(NAV + if index % 2 == 0 { 7 } else { 1 });
            RedrawWindow(
                app.hwnd,
                null(),
                null_mut(),
                RDW_UPDATENOW | RDW_ALLCHILDREN,
            );
            samples.push(start.elapsed().as_micros());
        }
        samples.sort_unstable();
        let p95 = samples[15];
        let median = samples[8];
        let fonts = widgets::fonts().len();
        DestroyWindow(app.hwnd);
        app.release_resources();
        RoUninitialize();
        eprintln!(
            "clock/settings navigation: fonts={fonts}, cold={cold}us, median={median}us, p95={p95}us"
        );
        assert!(
            cold < 100_000,
            "first clock page switch blocked UI for {cold}us"
        );
        assert!(
            p95 < 50_000,
            "clock page switching blocked UI for {p95}us (median {median}us, fonts {fonts})"
        );
    });
}

#[test]
fn settings_pages_fit_the_minimum_window_without_outer_scrollbars() {
    on_isolated_desktop(|| unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-settings-fit-unused.json"),
        );
        SetWindowPos(
            app.hwnd,
            null_mut(),
            0,
            0,
            app.scale(1060),
            app.scale(820),
            SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER,
        );
        let mut failures = vec![];
        for page in [1, 2, 3, 4, 5, 6, 7] {
            app.page = page;
            app.render();
            let mut bounds: RECT = std::mem::zeroed();
            GetClientRect(app.page_host, &mut bounds);
            if GetWindowLongW(app.page_host, GWL_STYLE) as u32 & WS_VSCROLL != 0 {
                failures.push(format!(
                    "{}: outer scrollbar, viewport={}",
                    PAGES[page], bounds.bottom
                ));
            }
            let mut child = GetWindow(app.page_host, GW_CHILD);
            while !child.is_null() {
                if GetWindowLongW(child, GWL_STYLE) as u32 & WS_VISIBLE != 0 {
                    let mut r: RECT = std::mem::zeroed();
                    GetWindowRect(child, &mut r);
                    MapWindowPoints(null_mut(), app.page_host, &mut r as *mut _ as *mut POINT, 2);
                    if r.bottom > bounds.bottom || r.top < 0 || r.left < 0 || r.right > bounds.right
                    {
                        failures.push(format!(
                            "{}: control {} [{},{},{},{}] outside {}x{}",
                            PAGES[page],
                            GetDlgCtrlID(child),
                            r.left,
                            r.top,
                            r.right,
                            r.bottom,
                            bounds.right,
                            bounds.bottom
                        ));
                    }
                }
                child = GetWindow(child, GW_HWNDNEXT);
            }
        }
        DestroyWindow(app.hwnd);
        app.release_resources();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    });
}

#[test]
fn ui_audit_text_and_geometry_across_pages_and_timer_tabs() {
    on_isolated_desktop(|| unsafe {
        let mut failures = vec![];
        for context in [
            DPI_AWARENESS_CONTEXT_UNAWARE,
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        ] {
            let previous = SetThreadDpiAwarenessContext(context);
            for dark in [false, true] {
                let mut app = app_for_check(
                    dark,
                    96,
                    &std::env::temp_dir().join("litetray-deep-ui-unused.json"),
                );
                app.draft.clock.top_color = "#AABBCC".into();
                app.draft.clock.bottom_color = "#112233".into();
                for (width, height) in [(1060, 820), (1120, 920)] {
                    SetWindowPos(
                        app.hwnd,
                        null_mut(),
                        0,
                        0,
                        app.scale(width),
                        app.scale(height),
                        SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER,
                    );
                    for (page, name) in PAGES.iter().enumerate() {
                        app.page = page;
                        app.render();
                        let tag = format!("{} {width}x{height} dpi={} dark={dark}", name, app.dpi);
                        if GetWindowLongW(app.page_host, GWL_STYLE) as u32 & WS_VSCROLL != 0 {
                            failures.push(format!("{tag}: unnecessary outer scrollbar"));
                        }
                        audit_native_layout(app.hwnd, &tag, &mut failures);
                        if matches!(page, 8 | 9) {
                            for tab in 0..if page == 9 { 4 } else { 6 } {
                                let view = app.field(agenda::ID);
                                SendMessageW(
                                    view,
                                    WM_COMMAND,
                                    if page == 9 { 4300 } else { 4200 } + tab,
                                    0,
                                );
                                audit_native_layout(
                                    app.hwnd,
                                    &format!("{tag} tab={tab}"),
                                    &mut failures,
                                );
                            }
                        }
                    }
                    app.prewarm_panels();
                    for &popup in &app.panel_cache {
                        let calendar = GetDlgItem(popup, dashboard::ID as i32);
                        let tag = format!("flyout dpi={} dark={dark}", GetDpiForWindow(popup));
                        audit_native_layout(popup, &tag, &mut failures);
                        SendMessageW(calendar, WM_COMMAND, 1103, 0);
                        let editor = GetDlgItem(calendar, quick_agenda::ID as i32);
                        for kind in 0..6 {
                            SendMessageW(editor, WM_COMMAND, 4600 + kind, 0);
                            audit_native_layout(
                                popup,
                                &format!("{tag} editor kind={kind}"),
                                &mut failures,
                            );
                        }
                    }
                    app.close_panel();
                }
                DestroyWindow(app.hwnd);
                app.release_resources();
            }
            SetThreadDpiAwarenessContext(previous);
        }
        failures.sort();
        failures.dedup();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    });
}

unsafe fn audit_native_layout(parent: HWND, tag: &str, failures: &mut Vec<String>) {
    unsafe {
        let mut bounds: RECT = std::mem::zeroed();
        GetClientRect(parent, &mut bounds);
        let mut siblings: Vec<(i32, String, RECT)> = vec![];
        let mut parent_class = [0u16; 128];
        let parent_len = GetClassNameW(parent, parent_class.as_mut_ptr(), 128) as usize;
        let parent_class = String::from_utf16_lossy(&parent_class[..parent_len]);
        let mut child = GetWindow(parent, GW_CHILD);
        while !child.is_null() {
            let style = GetWindowLongW(child, GWL_STYLE) as u32;
            if style & WS_VISIBLE != 0 {
                let mut r: RECT = std::mem::zeroed();
                GetWindowRect(child, &mut r);
                MapWindowPoints(null_mut(), parent, &mut r as *mut _ as *mut POINT, 2);
                let mut class = [0u16; 128];
                let len = GetClassNameW(child, class.as_mut_ptr(), 128) as usize;
                let class = String::from_utf16_lossy(&class[..len]);
                let text = window_text(child);
                let id = GetDlgCtrlID(child);
                if !class.starts_with("LiteTray.") {
                    for (other_id, other_text, other_rect) in &siblings {
                        let overlap_w = r.right.min(other_rect.right) - r.left.max(other_rect.left);
                        let overlap_h = r.bottom.min(other_rect.bottom) - r.top.max(other_rect.top);
                        let empty_overlay = (id == 4000 && matches!(*other_id, 4005 | 4131))
                            || (*other_id == 4000 && matches!(id, 4005 | 4131));
                        if overlap_w > 0 && overlap_h > 0 && !empty_overlay {
                            failures.push(format!("{tag}: overlap {id} '{text}' / {other_id} '{other_text}' {overlap_w}x{overlap_h}"));
                        }
                    }
                    siblings.push((id, text.clone(), r));
                }
                let preview = id == widgets::PREVIEW as i32;
                if r.left < -1
                    || r.top < -1
                    || r.right > bounds.right + 1
                    || r.bottom > bounds.bottom + 1
                {
                    failures.push(format!(
                        "{tag}: {class}/{id} [{},{},{},{}] outside {}x{}",
                        r.left, r.top, r.right, r.bottom, bounds.right, bounds.bottom
                    ));
                }
                // Owner-drawn calendar buttons use accessible names that are
                // longer than their SVG/date glyphs. Measure normal controls.
                let custom_calendar = parent_class == "LiteTray.Dashboard.2"
                    || parent_class == "LiteTray.QuickAgenda.1" && id == 4622;
                let is_static = class.eq_ignore_ascii_case("static") && !preview;
                let is_button = class.eq_ignore_ascii_case("button") && !custom_calendar;
                let is_combo = class.eq_ignore_ascii_case("combobox");
                let color_edit = class.eq_ignore_ascii_case("edit") && matches!(id, 801 | 802);
                if class.eq_ignore_ascii_case("edit")
                    && text.is_empty()
                    && style & ES_MULTILINE as u32 != 0
                    && style & WS_VSCROLL != 0
                {
                    failures.push(format!(
                        "{tag}: empty multiline field {id} has an unnecessary scrollbar"
                    ));
                }
                if (is_static || is_button || is_combo || color_edit) && !text.is_empty() {
                    let dc = GetDC(child);
                    let font = SendMessageW(child, WM_GETFONT, 0, 0) as HGDIOBJ;
                    let old = SelectObject(dc, font);
                    let mut client: RECT = std::mem::zeroed();
                    GetClientRect(child, &mut client);
                    let scale = GetDpiForWindow(child).max(96) as i32;
                    let padding =
                        if parent_class == "LiteTray.QuickAgenda.1" && (4600..4606).contains(&id) {
                            8
                        } else if is_combo {
                            40
                        } else if color_edit {
                            20
                        } else if is_button {
                            if (100..111).contains(&id) { 58 } else { 20 }
                        } else {
                            0
                        };
                    let available = client.right - padding * scale / 96;
                    let mut measured = design::rect(0, 0, available.max(1), 0);
                    let mut value = wide(&text);
                    DrawTextW(
                        dc,
                        value.as_mut_ptr(),
                        (value.len() - 1) as i32,
                        &mut measured,
                        DT_CALCRECT
                            | DT_NOPREFIX
                            | if is_static {
                                DT_WORDBREAK
                            } else {
                                DT_SINGLELINE
                            },
                    );
                    SelectObject(dc, old);
                    ReleaseDC(child, dc);
                    if measured.bottom > client.bottom + 1
                        || (!is_static && measured.right > available + 1)
                    {
                        failures.push(format!(
                            "{tag}: clipped {class}/{id} '{text}': text {}x{}, available {}x{}",
                            measured.right, measured.bottom, available, client.bottom
                        ));
                    }
                }
                if class.starts_with("LiteTray.") {
                    audit_native_layout(child, tag, failures);
                }
            }
            child = GetWindow(child, GW_HWNDNEXT);
        }
    }
}

#[test]
fn ui_audit_maximum_clock_font_is_not_clipped_in_preview() {
    on_isolated_desktop(|| unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-large-clock-ui-unused.json"),
        );
        app.page = 7;
        app.draft.clock.top_style.font_size = Some(32);
        app.draft.clock.bottom_style.font_size = Some(32);
        app.render();
        let (available, heights) = widgets::preview_line_heights(app.field(widgets::PREVIEW));
        DestroyWindow(app.hwnd);
        app.release_resources();
        assert!(
            heights.iter().all(|&height| height <= available),
            "large clock preview clips text: line heights={heights:?}, available={available}"
        );
    });
}

#[test]
fn ui_audit_long_content_stays_inside_fields_and_messages_remain_readable() {
    on_isolated_desktop(|| unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-long-content-unused.json"),
        );
        SetWindowPos(
            app.hwnd,
            null_mut(),
            0,
            0,
            app.scale(1060),
            app.scale(820),
            SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER,
        );
        app.page = 7;
        app.render();
        let note = "包含多行文字的悬停提示\r\n".repeat(30);
        set_text(app.field(804), &note);
        assert_ne!(
            GetWindowLongW(app.field(804), GWL_STYLE) as u32 & WS_VSCROLL,
            0
        );
        app.command(NAV + 5);
        app.update=Some(UpdateStatus::RepositoryUnavailable("暂时无法公开访问此仓库，请核对仓库名称。开发中的私有仓库会返回此状态；转为公开后即可直接检查更新，无需密钥。".into()));
        app.render();
        let mut failures = vec![];
        audit_native_layout(app.hwnd, "long update message", &mut failures);
        app.command(NAV + 7);
        assert_eq!(app.text(804), note);
        assert_eq!(
            GetWindowLongW(app.page_host, GWL_STYLE) as u32 & WS_VSCROLL,
            0
        );
        app.command(NAV + 8);
        let view = app.field(agenda::ID);
        let field = GetDlgItem(view, 4016);
        set_text(field, &"很长的日程备注\r\n".repeat(100));
        assert_ne!(GetWindowLongW(field, GWL_STYLE) as u32 & WS_VSCROLL, 0);
        assert_eq!(
            GetWindowLongW(app.page_host, GWL_STYLE) as u32 & WS_VSCROLL,
            0
        );
        audit_native_layout(app.hwnd, "long agenda note", &mut failures);
        app.command(NAV + 3);
        app.draft.weather.providers = Provider::ALL.to_vec();
        app.render();
        let providers = app.field(404);
        SendMessageW(providers, LB_SETCURSEL, 4, 0);
        assert_eq!(SendMessageW(providers, LB_GETCURSEL, 0, 0), 4);
        assert_eq!(SendMessageW(providers, LB_GETCOUNT, 0, 0), 5);
        assert_eq!(
            GetWindowLongW(app.page_host, GWL_STYLE) as u32 & WS_VSCROLL,
            0
        );
        audit_native_layout(app.hwnd, "five weather providers", &mut failures);
        DestroyWindow(app.hwnd);
        app.release_resources();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    });
}

#[test]
fn ui_audit_font_dropdown_fits_installed_names() {
    on_isolated_desktop(|| unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-font-width-unused.json"),
        );
        app.page = 7;
        app.render();
        let mut failures = vec![];
        for id in [806, 809] {
            let field = app.field(id);
            let width = SendMessageW(field, CB_GETDROPPEDWIDTH, 0, 0) as i32;
            let dc = GetDC(field);
            let old = SelectObject(dc, SendMessageW(field, WM_GETFONT, 0, 0) as HGDIOBJ);
            for name in widgets::fonts() {
                let name_w = wide(name);
                let mut extent: SIZE = std::mem::zeroed();
                GetTextExtentPoint32W(dc, name_w.as_ptr(), (name_w.len() - 1) as i32, &mut extent);
                if extent.cx + app.scale(32) > width {
                    failures.push(format!(
                        "font {name}: needs {}, dropdown={width}",
                        extent.cx + app.scale(32)
                    ));
                }
            }
            SelectObject(dc, old);
            ReleaseDC(field, dc);
        }
        DestroyWindow(app.hwnd);
        app.release_resources();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    });
}

#[test]
fn ui_audit_keyboard_tabs_only_visit_active_page_and_native_choices_work() {
    on_isolated_desktop(|| unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-keyboard-ui-unused.json"),
        );
        app.page = 1;
        app.render();
        app.command(NAV + 7);
        app.command(NAV + 1);
        ShowWindow(app.hwnd, SW_SHOWNOACTIVATE);
        for page in [7, 1, 3, 5, 8, 9, 7, 1] {
            app.command(NAV + page);
            let mut visited = std::collections::HashSet::new();
            let mut next = GetNextDlgTabItem(app.hwnd, null_mut(), 0);
            while !next.is_null() && visited.insert(next as usize) {
                assert_ne!(IsWindowVisible(next), 0, "Tab reached hidden page {page}");
                assert_ne!(
                    IsWindowEnabled(next),
                    0,
                    "Tab reached disabled control {page}"
                );
                assert!(
                    !app.settings_cache
                        .iter()
                        .any(|cache| IsChild(cache.window, next) != 0),
                    "Tab reached cached inactive controls"
                );
                next = GetNextDlgTabItem(app.hwnd, next, 0);
            }
            assert!(!visited.is_empty());
            for (&id, &field) in &app.fields {
                if IsWindowVisible(field) != 0
                    && IsWindowEnabled(field) != 0
                    && GetWindowLongW(field, GWL_STYLE) as u32 & WS_TABSTOP != 0
                {
                    assert!(
                        visited.contains(&(field as usize)),
                        "visible control {id} on page {page} missing from Tab order"
                    );
                }
            }
        }
        app.command(NAV + 7);
        let font = app.field(205);
        let selected = SendMessageW(font, CB_GETCURSEL, 0, 0);
        SendMessageW(font, WM_KEYDOWN, VK_HOME as usize, 0);
        SendMessageW(font, WM_KEYDOWN, VK_DOWN as usize, 0);
        assert_eq!(SendMessageW(font, CB_GETCURSEL, 0, 0), 1);
        SendMessageW(font, CB_SETCURSEL, selected as usize, 0);
        SendMessageW(font, CB_SHOWDROPDOWN, 1, 0);
        let mut info: COMBOBOXINFO = std::mem::zeroed();
        info.cbSize = size_of::<COMBOBOXINFO>() as u32;
        assert_ne!(GetComboBoxInfo(font, &mut info), 0);
        assert_ne!(IsWindowVisible(info.hwndList), 0);
        assert!(SendMessageW(font, CB_GETCOUNT, 0, 0) > 1);
        SendMessageW(font, CB_SHOWDROPDOWN, 0, 0);
        DestroyWindow(app.hwnd);
        app.release_resources();
        RoUninitialize();
    });
}

#[test]
fn settings_cached_pages_preserve_edits_and_invalidate_after_presets_theme_and_resize() {
    on_isolated_desktop(|| unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-cached-settings-unused.json"),
        );
        app.page = 1;
        app.render();
        app.command(NAV + 7);
        let style = app.page_host;
        app.command(NAV + 1);
        let clock = app.page_host;
        app.command(NAV + 7);
        assert_eq!(
            app.page_host, style,
            "unchanged style page must reuse native controls"
        );
        app.command(NAV + 1);
        assert_eq!(
            app.page_host, clock,
            "unchanged clock page must reuse native controls"
        );
        set_text(app.field(201), "%time% · 保留草稿");
        app.command(NAV + 7);
        set_text(app.field(804), "悬停说明\r\n第二行");
        app.command(NAV + 1);
        assert_eq!(app.text(201), "%time% · 保留草稿");
        app.command(NAV + 7);
        assert_eq!(app.text(804), "悬停说明\r\n第二行");
        app.command(NAV + 5);
        app.command(NAV + 7);
        assert_eq!(app.text(804), "悬停说明\r\n第二行");
        app.command(settings::PRESET + 1);
        app.command(NAV + 1);
        assert_eq!(app.text(201), settings::PRESETS[1].1);
        assert_eq!(app.text(202), settings::PRESETS[1].2);
        app.command(NAV + 7);
        app.command(NAV + 1);
        assert!(!app.settings_cache.is_empty());
        app.saved.appearance = Appearance::Dark;
        app.draft.appearance = Appearance::Dark;
        app.recreate_resources();
        assert!(
            app.settings_cache.is_empty(),
            "theme changes must discard old native resources"
        );
        app.render();
        app.command(NAV + 7);
        assert!(!GetPropW(app.field(806), wide("LiteTray.ControlDark").as_ptr()).is_null());
        SetWindowPos(
            app.hwnd,
            null_mut(),
            0,
            0,
            app.scale(1060),
            app.scale(820),
            SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER,
        );
        app.render();
        assert!(
            app.settings_cache.is_empty(),
            "resize must invalidate other page layouts"
        );
        app.command(NAV + 1);
        assert_eq!(app.text(201), settings::PRESETS[1].1);
        assert!(app.settings_cache.len() <= 2);
        DestroyWindow(app.hwnd);
        app.release_resources();
    });
}

#[test]
fn settings_switches_preserve_chrome_drafts_and_native_font_choices() {
    on_isolated_desktop(|| unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-settings-unused.json"),
        );
        app.page = 1;
        app.render();
        let navigation = app.field(NAV + 1);
        let title = app.field(901);
        set_text(app.field(201), "%time% · 自定义");
        app.command(NAV + 7);
        assert_eq!(
            app.field(NAV + 1),
            navigation,
            "page changes must not recreate navigation"
        );
        assert_eq!(app.field(901), title, "header survives page changes");
        assert!(
            SendMessageW(app.field(205), CB_GETCOUNT, 0, 0) > 1,
            "font picker must enumerate installed fonts"
        );
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
    });
}

#[test]
fn weather_format_controls_preserve_selection_precision_and_saved_draft() {
    on_isolated_desktop(|| unsafe {
        let path = std::env::temp_dir().join(format!(
            "litetray-weather-format-{}.json",
            std::process::id()
        ));
        let mut app = app_for_check(false, 96, &path.with_extension("agenda"));
        app.page = 1;
        app.render();
        app.command(settings::OPEN_WEATHER_FORMAT);
        assert_eq!(app.page, 10);
        assert_eq!(
            app.text(settings::WEATHER_FORMAT),
            app.saved.clock.weather_format
        );
        SendMessageW(app.field(settings::WEATHER_PRESET), CB_SETCURSEL, 2, 0);
        app.choose_weather_format(settings::WEATHER_PRESET);
        assert!(
            app.draft
                .clock
                .weather_format
                .contains("%weather_humidity%")
        );
        SendMessageW(app.field(settings::WEATHER_PRECISION), CB_SETCURSEL, 2, 0);
        app.choose_weather_format(settings::WEATHER_PRECISION);
        assert!(app.draft.clock.weather_format.contains("%weather_temp:1%"));
        set_text(app.field(settings::WEATHER_FORMAT), "前 后");
        SendMessageW(app.field(settings::WEATHER_FORMAT), EM_SETSEL, 1, 2);
        SendMessageW(app.field(settings::WEATHER_FIELD), CB_SETCURSEL, 2, 0);
        app.choose_weather_format(settings::WEATHER_FIELD);
        assert_eq!(app.draft.clock.weather_format, "前%weather_temp:1%后");
        app.command(NAV + 7);
        app.command(NAV + 10);
        assert_eq!(app.text(settings::WEATHER_FORMAT), "前%weather_temp:1%后");
        config::save(&path, &app.draft).unwrap();
        assert_eq!(
            config::load(&path).unwrap().clock.weather_format,
            "前%weather_temp:1%后"
        );
        set_text(app.field(settings::WEATHER_FORMAT), "%weather%");
        app.collect_page().unwrap();
        assert!(app.draft.validate().is_err());
        assert!(
            clock::validate_template("%weather_temp:1% %weather_rain% %weather_source%").is_ok()
        );
        std::fs::remove_file(path).unwrap();
        if let Some(output) = std::env::var_os("LITETRAY_WEATHER_FORMAT_RENDER_DIR") {
            let output = std::path::PathBuf::from(output);
            std::fs::create_dir_all(&output).unwrap();
            for dark in [false, true] {
                app.draft.appearance = if dark {
                    Appearance::Dark
                } else {
                    Appearance::Light
                };
                app.draft.clock.weather_format = clock::weather_format::PRESETS[1].1.into();
                app.recreate_resources();
                app.render();
                write_window(
                    app.hwnd,
                    &output.join(format!(
                        "weather-format-{}.bmp",
                        if dark { "dark" } else { "light" }
                    )),
                );
            }
        }
        DestroyWindow(app.hwnd);
        app.release_resources();
    });
}
#[test]
fn settings_date_patterns_preview_validate_and_survive_navigation() {
    on_isolated_desktop(|| unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-pattern-ui-unused.json"),
        );
        app.page = 1;
        app.render();
        app.collect_page().unwrap();
        assert_eq!(
            serde_json::to_value(&app.draft).unwrap(),
            serde_json::to_value(&app.saved).unwrap(),
            "opening formats must not mark an old configuration as edited"
        );
        set_text(app.field(214), "yyyymmdd");
        set_text(app.field(215), "HH:mm:ss");
        app.collect_page().unwrap();
        app.update_clock_samples();
        assert!(
            app.text(216)
                .ends_with(&Local::now().format("%Y%m%d").to_string())
        );
        app.command(NAV + 7);
        app.command(NAV + 1);
        assert_eq!(app.text(214), "yyyymmdd");
        assert_eq!(app.text(215), "HH:mm:ss");
        set_text(app.field(215), "HH:mm");
        app.collect_page().unwrap();
        assert!(!clock::needs_second_ticks(&app.draft.clock));
        set_text(app.field(214), "yyy-MM-dd");
        app.collect_page().unwrap();
        app.update_clock_samples();
        assert!(app.draft.validate().is_err());
        assert!(app.text(216).contains("请检查格式"));
        assert!(app.text(218).contains("不支持格式"));
        app.command(settings::PRESET + 3);
        assert_eq!(app.text(214), "yyyymmdd");
        app.draft.validate().unwrap();
        DestroyWindow(app.hwnd);
        app.release_resources();
    });
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
            settings_key: None,
            settings_cache: vec![],
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
            taskbar_status: None,
            weather: None,
            location: Default::default(),
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
                let (width, height) = (1060, 820);
                SetWindowPos(
                    app.hwnd,
                    null_mut(),
                    0,
                    0,
                    app.scale(width),
                    app.scale(height),
                    SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER,
                );
                app.page = page;
                app.render();
                write_window(
                    app.hwnd,
                    &output.join(format!(
                        "settings-{}-{page}.bmp",
                        if dark { "dark" } else { "light" }
                    )),
                );
                if matches!(page, 8 | 9) {
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
    on_isolated_desktop(|| {
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
    });
}

#[test]
fn organizer_pages_fit_the_viewport_without_page_scrollbars() {
    on_isolated_desktop(|| unsafe {
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
    });
}

#[test]
fn inline_draft_survives_hidden_flyout_theme_refresh_and_failed_save() {
    on_isolated_desktop(|| {
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
    });
}

#[test]
fn hidden_calendar_cache_reuses_controls_and_tracks_new_anchor() {
    on_isolated_desktop(|| unsafe {
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
    });
}

#[test]
fn opening_focus_bounce_does_not_hide_calendar_and_popup_is_topmost() {
    on_isolated_desktop(|| {
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
    });
}

#[test]
fn detached_calendar_keeps_its_position_and_lifetime_until_close() {
    on_isolated_desktop(|| unsafe {
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
    });
}
