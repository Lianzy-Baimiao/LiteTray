use super::*;
use appearance_settings::{
    CLARITY, CORNERS, FIRST_STYLE, FONT, PAGE, RESET, SIZE, TBM_GETPOS, TRANSPARENCY,
};
use config::{CornerStyle, InterfaceConfig, InterfaceStyle};
use visual_checks::{app_for_check, audit_native_layout, on_isolated_desktop};

unsafe fn dispatch_appearance(hwnd: HWND) {
    unsafe {
        let mut message: MSG = std::mem::zeroed();
        while PeekMessageW(
            &mut message,
            hwnd,
            WM_APPLY_INTERFACE,
            WM_APPLY_INTERFACE,
            PM_REMOVE,
        ) != 0
        {
            DispatchMessageW(&message);
        }
    }
}

#[test]
fn glass_owner_draw_keeps_calendar_and_button_pixels_premultiplied() {
    on_isolated_desktop(|| unsafe {
        let mut app = app_for_check(
            true,
            96,
            &std::env::temp_dir().join("litetray-glass-pixels-unused.json"),
        );
        app.draft.interface.style = InterfaceStyle::LiquidGlass;
        app.draft.interface.transparency = 85;
        app.draft.interface.card_clarity = 30;
        app.recreate_resources();
        // Exercise the compositor's pixel contract even on test machines
        // whose DWM or accessibility policy disables the actual backdrop.
        SetPropW(
            app.hwnd,
            wide("LiteTray.GlassBackdrop").as_ptr(),
            1usize as HANDLE,
        );
        app.page = PAGE;
        app.render();
        app.prewarm_panels();
        let panel = *app.panel_cache.first().unwrap();
        SetPropW(
            panel,
            wide("LiteTray.GlassBackdrop").as_ptr(),
            1usize as HANDLE,
        );
        let calendar = GetDlgItem(panel, dashboard::ID as i32);
        let mut failures = vec![];
        for child in [
            app.field(FIRST_STYLE + 2),
            app.field(NAV + PAGE as u16),
            GetDlgItem(calendar, 1000),
            GetDlgItem(calendar, 1102),
            GetDlgItem(calendar, 40),
        ] {
            let mut bounds: RECT = std::mem::zeroed();
            GetClientRect(child, &mut bounds);
            let dc = CreateCompatibleDC(null_mut());
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: bounds.right,
                    biHeight: -bounds.bottom,
                    biPlanes: 1,
                    biBitCount: 32,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            assert!(!bitmap.is_null());
            let old = SelectObject(dc, bitmap);
            let item = DRAWITEMSTRUCT {
                CtlType: ODT_BUTTON,
                CtlID: GetDlgCtrlID(child) as u32,
                hwndItem: child,
                hDC: dc,
                rcItem: bounds,
                ..Default::default()
            };
            SendMessageW(GetParent(child), WM_DRAWITEM, 0, &item as *const _ as isize);
            GdiFlush();
            let pixels = std::slice::from_raw_parts(
                bits.cast::<u8>(),
                (bounds.right * bounds.bottom * 4) as usize,
            );
            let invalid = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[3] == 0 || pixel[..3].iter().any(|&value| value > pixel[3]))
                .count();
            if invalid != 0 {
                failures.push(format!(
                    "{}: {invalid} pixels would produce black or missing edges on Acrylic",
                    item.CtlID
                ));
            }
            SelectObject(dc, old);
            DeleteObject(bitmap);
            DeleteDC(dc);
        }
        app.close_panel();
        DestroyWindow(app.hwnd);
        app.release_resources();
        interface::configure(&InterfaceConfig::default());
        RoUninitialize();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    });
}

#[test]
fn native_appearance_controls_preview_preserve_clock_and_discard_the_draft() {
    on_isolated_desktop(|| unsafe {
        let path =
            std::env::temp_dir().join(format!("litetray-interface-{}.json", std::process::id()));
        let mut app = app_for_check(false, 96, &path.with_extension("agenda"));
        let saved = serde_json::to_value(&app.saved).unwrap();
        let clock = serde_json::to_value(&app.draft.clock).unwrap();
        app.page = PAGE;
        app.render();
        app.prewarm_panels();
        let popup = *app.panel_cache.first().expect("a flyout is prepared");
        let calendar = GetDlgItem(popup, dashboard::ID as i32);
        SendMessageW(calendar, WM_COMMAND, 1103, 0);
        let editor = GetDlgItem(calendar, quick_agenda::ID as i32);
        let title = GetDlgItem(editor, 4610);
        set_text(title, "字体切换时保留日程草稿");
        let hwnd = app.hwnd;
        let style = app.field(FIRST_STYLE + 2);
        assert!(
            !app.field(TRANSPARENCY).is_null(),
            "native slider must be registered"
        );
        assert_eq!(IsWindowEnabled(app.field(TRANSPARENCY)), 0);
        STATE.with(|state| *state.borrow_mut() = Some(app));

        SendMessageW(style, BM_CLICK, 0, 0);
        dispatch_appearance(hwnd);
        let (font, size, corners, transparency, clarity) = STATE.with(|state| {
            let state = state.borrow();
            let app = state.as_ref().unwrap();
            assert_eq!(app.draft.interface.style, InterfaceStyle::LiquidGlass);
            assert_ne!(IsWindowEnabled(app.field(TRANSPARENCY)), 0);
            (
                app.field(FONT),
                app.field(SIZE),
                app.field(CORNERS),
                app.field(TRANSPARENCY),
                app.field(CLARITY),
            )
        });
        let index = SendMessageW(
            font,
            CB_FINDSTRINGEXACT,
            usize::MAX,
            wide("Segoe UI").as_ptr() as isize,
        );
        assert!(
            index > 0,
            "font list includes Windows default and installed fonts"
        );
        SendMessageW(font, CB_SETCURSEL, index as usize, 0);
        SendMessageW(
            size,
            CB_SETCURSEL,
            (InterfaceConfig::MAX_FONT_SIZE - InterfaceConfig::MIN_FONT_SIZE) as usize,
            0,
        );
        SendMessageW(corners, CB_SETCURSEL, 1, 0);
        SendMessageW(transparency, TBM_SETPOS, 1, 82);
        SendMessageW(clarity, TBM_SETPOS, 1, 35);
        SendMessageW(
            hwnd,
            WM_COMMAND,
            (CBN_SELCHANGE as usize) << 16 | FONT as usize,
            font as isize,
        );
        dispatch_appearance(hwnd);

        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let app = state.as_mut().unwrap();
            let expected = InterfaceConfig {
                style: InterfaceStyle::LiquidGlass,
                corners: CornerStyle::Square,
                transparency: 82,
                card_clarity: 35,
                font_family: Some("Segoe UI".into()),
                font_size: 20,
            };
            assert_eq!(app.draft.interface, expected);
            assert_eq!(interface::current(), expected);
            assert_ne!(
                IsWindow(title),
                0,
                "editing flyout inputs must remain alive"
            );
            assert_eq!(window_text(title), "字体切换时保留日程草稿");
            let mut font: LOGFONTW = std::mem::zeroed();
            assert_ne!(
                GetObjectW(
                    SendMessageW(title, WM_GETFONT, 0, 0) as HFONT,
                    size_of::<LOGFONTW>() as i32,
                    &mut font as *mut _ as _
                ),
                0
            );
            let family = String::from_utf16_lossy(
                &font.lfFaceName[..font
                    .lfFaceName
                    .iter()
                    .position(|&value| value == 0)
                    .unwrap()],
            );
            assert_eq!(
                family, "Segoe UI",
                "inline editor must adopt the interface font"
            );
            assert_eq!(serde_json::to_value(&app.saved).unwrap(), saved);
            assert_eq!(serde_json::to_value(&app.draft.clock).unwrap(), clock);
            assert!(!path.exists(), "preview must not write a profile");
            config::save(&path, &app.draft).unwrap();
            assert_eq!(config::load(&path).unwrap().interface, expected);
            app.command(NAV + 7);
            app.command(NAV + PAGE as u16);
            assert_eq!(app.draft.interface, expected);
            assert_eq!(app.text(SIZE), "20");
            app.discard_settings_draft();
            assert_eq!(interface::current(), InterfaceConfig::default());
            assert_eq!(app.draft.interface, app.saved.interface);
            assert_eq!(app.text(SIZE), "14");
            assert_eq!(app.selected(FONT), 0);
        });
        STATE.with(|state| {
            let mut app = state.borrow_mut().take().unwrap();
            app.close_panel();
            DestroyWindow(hwnd);
            app.release_resources();
        });
        std::fs::remove_file(path).unwrap();
        RoUninitialize();
    });
}

#[test]
fn appearance_layout_fits_small_and_large_fonts_in_every_material() {
    on_isolated_desktop(|| unsafe {
        let mut failures = vec![];
        for dark in [false, true] {
            for style in InterfaceStyle::ALL {
                for (size, corners) in [(12, CornerStyle::Rounded), (20, CornerStyle::Square)] {
                    let mut app = app_for_check(
                        dark,
                        96,
                        &std::env::temp_dir().join("litetray-interface-layout-unused.json"),
                    );
                    app.draft.interface.style = style;
                    app.draft.interface.corners = corners;
                    app.draft.interface.font_size = size;
                    app.draft.interface.font_family = Some("Microsoft YaHei UI".into());
                    app.saved.interface = app.draft.interface.clone();
                    app.recreate_resources();
                    SetWindowPos(
                        app.hwnd,
                        null_mut(),
                        0,
                        0,
                        app.scale(1060),
                        app.scale(820),
                        SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER,
                    );
                    // Windows caps a window at the size of the display. On
                    // one too small for this font size's minimum window the
                    // layout cannot be judged; say so instead of auditing a
                    // truncated window.
                    let mut placed = std::mem::zeroed();
                    GetWindowRect(app.hwnd, &mut placed);
                    if placed.right - placed.left < app.scale(1060)
                        || placed.bottom - placed.top < app.scale(820)
                    {
                        eprintln!(
                            "skipped {style:?} size={size} dark={dark}: display allows only {}x{}, layout needs {}x{}",
                            placed.right - placed.left,
                            placed.bottom - placed.top,
                            app.scale(1060),
                            app.scale(820)
                        );
                        DestroyWindow(app.hwnd);
                        app.release_resources();
                        interface::configure(&InterfaceConfig::default());
                        continue;
                    }
                    app.page = PAGE;
                    app.render();
                    audit_native_layout(
                        app.hwnd,
                        &format!("{style:?} {corners:?} size={size} dark={dark}"),
                        &mut failures,
                    );
                    for page in [8, 9] {
                        app.page = page;
                        app.render();
                        let view = app.field(agenda::ID);
                        for tab in 0..if page == 9 { 4 } else { 6 } {
                            SendMessageW(
                                view,
                                WM_COMMAND,
                                if page == 9 { 4300 } else { 4200 } + tab,
                                0,
                            );
                            audit_native_layout(
                                app.hwnd,
                                &format!(
                                    "agenda {style:?} size={size} page={page} tab={tab} dark={dark}"
                                ),
                                &mut failures,
                            );
                        }
                    }
                    assert_eq!(
                        interface::radius(12),
                        if corners == CornerStyle::Square {
                            0
                        } else {
                            12
                        }
                    );
                    app.prewarm_panels();
                    for &popup in &app.panel_cache {
                        audit_native_layout(
                            popup,
                            &format!("flyout {style:?} size={size} dark={dark}"),
                            &mut failures,
                        );
                    }
                    app.close_panel();
                    DestroyWindow(app.hwnd);
                    app.release_resources();
                    interface::configure(&InterfaceConfig::default());
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        RoUninitialize();
    });
}

#[test]
fn slider_preview_keeps_native_controls_and_keyboard_navigation_alive() {
    on_isolated_desktop(|| unsafe {
        let mut app = app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-interface-slider-unused.json"),
        );
        app.draft.interface.style = InterfaceStyle::LiquidGlass;
        app.recreate_resources();
        app.page = PAGE;
        app.render();
        let slider = app.field(TRANSPARENCY);
        let clarity = app.field(CLARITY);
        SendMessageW(slider, WM_KEYDOWN, VK_RIGHT as usize, 0);
        app.interface_input_changed(true);
        assert_eq!(app.draft.interface.transparency, 61);
        app.apply_interface_preview();
        assert_eq!(
            app.field(TRANSPARENCY),
            slider,
            "opacity changes must preserve capture and focus"
        );
        assert_eq!(app.field(CLARITY), clarity);
        assert_eq!(app.draft.interface.card_clarity, 35);
        assert_eq!(app.saved.interface.transparency, 60);
        app.discard_settings_draft();
        DestroyWindow(app.hwnd);
        app.release_resources();
        interface::configure(&InterfaceConfig::default());
        RoUninitialize();
    });
}

#[test]
fn restoring_interface_defaults_moves_every_control_and_survives_leaving_the_page() {
    on_isolated_desktop(|| unsafe {
        // Start in the colour mode Windows uses, so that returning to
        // "follow Windows" changes no colour and rebuilds nothing by itself.
        let system = system_dark();
        let mut app = app_for_check(
            system,
            96,
            &std::env::temp_dir().join("litetray-interface-reset-unused.json"),
        );
        app.page = PAGE;
        app.render();
        let hwnd = app.hwnd;
        STATE.with(|state| *state.borrow_mut() = Some(app));
        let field = |id: u16| STATE.with(|state| state.borrow().as_ref().unwrap().field(id));
        let changed = |id: u16| {
            SendMessageW(
                hwnd,
                WM_COMMAND,
                (CBN_SELCHANGE as usize) << 16 | id as usize,
                field(id) as isize,
            );
            dispatch_appearance(hwnd);
        };
        let click = |id: u16| {
            SendMessageW(hwnd, WM_COMMAND, id as usize, field(id) as isize);
            dispatch_appearance(hwnd);
        };
        let shows_defaults = |context: &str| {
            STATE.with(|state| {
                let state = state.borrow();
                let app = state.as_ref().unwrap();
                let defaults = InterfaceConfig::default();
                assert_eq!(app.page, PAGE, "{context}");
                assert_eq!(app.draft.interface, defaults, "{context}");
                assert_eq!(app.draft.appearance, Appearance::System, "{context}");
                assert_eq!(interface::current(), defaults, "{context}");
                assert_eq!(app.selected(501), 0, "{context}: colour mode");
                assert_eq!(app.selected(CORNERS), 0, "{context}: corners");
                assert_eq!(app.selected(FONT), 0, "{context}: font");
                assert_eq!(
                    app.text(SIZE),
                    defaults.font_size.to_string(),
                    "{context}: size"
                );
                for (id, value) in [
                    (TRANSPARENCY, defaults.transparency),
                    (CLARITY, defaults.card_clarity),
                ] {
                    assert_eq!(
                        SendMessageW(app.field(id), TBM_GETPOS, 0, 0),
                        value as isize,
                        "{context}: slider {id}"
                    );
                }
                for index in 0..3 {
                    assert_eq!(
                        !GetPropW(
                            app.field(FIRST_STYLE + index),
                            wide("LiteTray.Selected").as_ptr()
                        )
                        .is_null(),
                        index == 0,
                        "{context}: style card {index}"
                    );
                }
            });
        };
        let leave_and_return = || {
            STATE.with(|state| {
                let mut state = state.borrow_mut();
                let app = state.as_mut().unwrap();
                // Leaving reads the controls back into the draft.
                app.command(NAV + 7);
                app.command(NAV + PAGE as u16);
            });
        };

        // Choices that repaint the window without rebuilding its controls.
        SendMessageW(field(TRANSPARENCY), TBM_SETPOS, 1, 82);
        SendMessageW(field(CLARITY), TBM_SETPOS, 1, 71);
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.as_mut().unwrap().interface_input_changed(true);
        });
        STATE.with(|state| {
            let state = state.borrow();
            let app = state.as_ref().unwrap();
            assert_eq!(app.draft.interface.transparency, 82);
            assert_eq!(app.draft.interface.card_clarity, 71);
            assert_eq!(app.selected(501), if system { 2 } else { 1 });
        });
        let slider = field(TRANSPARENCY);
        click(RESET);
        assert_eq!(
            field(TRANSPARENCY),
            slider,
            "restoring these values needs no new controls"
        );
        shows_defaults("after restoring values that need no rebuild");
        leave_and_return();
        shows_defaults("after leaving a page restored without a rebuild");

        // Choices that rebuild the whole window, and resize it.
        click(FIRST_STYLE + 2);
        let font = field(FONT);
        let index = SendMessageW(
            font,
            CB_FINDSTRINGEXACT,
            usize::MAX,
            wide("Segoe UI").as_ptr() as isize,
        );
        assert!(index > 0);
        SendMessageW(font, CB_SETCURSEL, index as usize, 0);
        SendMessageW(
            field(SIZE),
            CB_SETCURSEL,
            (InterfaceConfig::MAX_FONT_SIZE - InterfaceConfig::MIN_FONT_SIZE) as usize,
            0,
        );
        SendMessageW(field(CORNERS), CB_SETCURSEL, 1, 0);
        SendMessageW(field(501), CB_SETCURSEL, if system { 1 } else { 2 }, 0);
        SendMessageW(field(TRANSPARENCY), TBM_SETPOS, 1, 18);
        SendMessageW(field(CLARITY), TBM_SETPOS, 1, 90);
        changed(FONT);
        STATE.with(|state| {
            let state = state.borrow();
            let app = state.as_ref().unwrap();
            assert_eq!(
                app.draft.interface,
                InterfaceConfig {
                    style: InterfaceStyle::LiquidGlass,
                    corners: CornerStyle::Square,
                    transparency: 18,
                    card_clarity: 90,
                    font_family: Some("Segoe UI".into()),
                    font_size: InterfaceConfig::MAX_FONT_SIZE,
                }
            );
            assert_eq!(app.dark(), !system);
        });
        click(RESET);
        shows_defaults("after restoring values that rebuild the window");
        leave_and_return();
        shows_defaults("after leaving a page restored with a rebuild");

        STATE.with(|state| {
            let mut app = state.borrow_mut().take().unwrap();
            app.discard_settings_draft();
            DestroyWindow(hwnd);
            app.release_resources();
        });
        interface::configure(&InterfaceConfig::default());
        RoUninitialize();
    });
}

#[test]
#[ignore = "Renders native appearance and flyout artifacts; set LITETRAY_INTERFACE_RENDER_DIR"]
fn render_interface_appearance_artifacts() {
    on_isolated_desktop(|| unsafe {
        let output = std::path::PathBuf::from(
            std::env::var_os("LITETRAY_INTERFACE_RENDER_DIR").expect("explicit render directory"),
        );
        std::fs::create_dir_all(&output).unwrap();
        for dark in [false, true] {
            for style in InterfaceStyle::ALL {
                let mut app = app_for_check(dark, 96, &output.join("unused-agenda.json"));
                app.draft.interface.style = style;
                app.saved.interface = app.draft.interface.clone();
                app.recreate_resources();
                app.page = PAGE;
                app.render();
                let tag = format!("{style:?}-{}", if dark { "dark" } else { "light" });
                visual_checks::write_window(app.hwnd, &output.join(format!("settings-{tag}.bmp")));
                // The shared materials must also hold on ordinary pages.
                for page in [0, 1, 4, 8, 12] {
                    app.page = page;
                    app.render();
                    visual_checks::write_window(
                        app.hwnd,
                        &output.join(format!("page{page}-{tag}.bmp")),
                    );
                }
                app.page = PAGE;
                app.render();
                let glass = style == InterfaceStyle::LiquidGlass;
                if glass {
                    visual_checks::write_window_glass(
                        app.hwnd,
                        &output.join(format!("settings-{tag}-composited.bmp")),
                        false,
                    );
                }
                app.prewarm_panels();
                if let Some(&popup) = app.panel_cache.first() {
                    let view = GetDlgItem(popup, dashboard::ID as i32);
                    visual_checks::write_window(view, &output.join(format!("flyout-{tag}.bmp")));
                    if glass {
                        visual_checks::write_window_glass(
                            view,
                            &output.join(format!("flyout-{tag}-composited.bmp")),
                            true,
                        );
                    }
                }
                app.close_panel();
                DestroyWindow(app.hwnd);
                app.release_resources();
                interface::configure(&InterfaceConfig::default());
            }
        }
        RoUninitialize();
    });
}
