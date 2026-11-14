use super::*;
use config::{ProxyConfig, ProxyMode, ProxyProtocol};
use proxy_settings::{HOST, MODE, PAGE, PORT, PROTOCOL, TEST};

struct Profile {
    directory: std::path::PathBuf,
    parent: std::path::PathBuf,
    previous: Option<std::ffi::OsString>,
}
impl Profile {
    fn new() -> Self {
        let parent = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        let directory = parent.join(format!(
            "litetray-proxy-check-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir(&directory).unwrap();
        let profile = Self {
            directory,
            parent,
            previous: std::env::var_os("LITETRAY_DATA_DIR"),
        };
        // Every caller runs in the settings tests' isolated child process.
        unsafe {
            std::env::set_var("LITETRAY_DATA_DIR", &profile.directory);
        }
        profile
    }
}
impl Drop for Profile {
    fn drop(&mut self) {
        unsafe {
            if let Some(previous) = &self.previous {
                std::env::set_var("LITETRAY_DATA_DIR", previous);
            } else {
                std::env::remove_var("LITETRAY_DATA_DIR");
            }
        }
        if let Ok(resolved) = std::fs::canonicalize(&self.directory)
            && resolved == self.directory
            && resolved.parent() == Some(self.parent.as_path())
        {
            let _ = std::fs::remove_dir_all(resolved);
        }
    }
}

unsafe fn choose(hwnd: HWND, id: u16, index: usize) {
    unsafe {
        let field = STATE.with(|state| state.borrow().as_ref().unwrap().field(id));
        SendMessageW(field, CB_SETCURSEL, index, 0);
        SendMessageW(
            hwnd,
            WM_COMMAND,
            id as usize | ((CBN_SELCHANGE as usize) << 16),
            field as isize,
        );
    }
}

#[test]
fn proxy_page_switches_native_controls_preserves_drafts_and_validates_saves() {
    visual_checks::on_isolated_desktop(|| unsafe {
        let profile = Profile::new();
        let path = profile.directory.join("config.json");
        let mut app =
            visual_checks::app_for_check(false, 96, &profile.directory.join("agenda.json"));
        app.page = PAGE;
        app.render();
        assert_eq!(app.selected(MODE), 0);
        for id in [PROTOCOL, HOST, PORT] {
            assert_eq!(IsWindowEnabled(app.field(id)), 0);
        }
        assert_ne!(IsWindowEnabled(app.field(TEST)), 0);
        let hwnd = app.hwnd;
        let host = app.field(HOST);
        let port = app.field(PORT);
        STATE.with(|state| *state.borrow_mut() = Some(app));

        choose(hwnd, MODE, 2);
        STATE.with(|state| {
            let state = state.borrow();
            let app = state.as_ref().unwrap();
            for id in [PROTOCOL, HOST, PORT] {
                assert_ne!(IsWindowEnabled(app.field(id)), 0);
            }
        });
        choose(hwnd, PROTOCOL, 2);
        set_text(host, "proxy.example");
        set_text(port, "1080");
        choose(hwnd, MODE, 1);
        assert_eq!(IsWindowEnabled(host), 0);
        choose(hwnd, MODE, 2);
        assert_eq!(window_text(host), "proxy.example");
        assert_eq!(window_text(port), "1080");
        SendMessageW(hwnd, WM_COMMAND, (NAV + 4) as usize, 0);
        SendMessageW(hwnd, WM_COMMAND, (NAV + PAGE as u16) as usize, 0);
        STATE.with(|state| {
            let state = state.borrow();
            let app = state.as_ref().unwrap();
            assert_eq!(app.text(HOST), "proxy.example");
            assert_eq!(app.text(PORT), "1080");
            assert_eq!(app.selected(PROTOCOL), 2);
            assert_eq!(app.saved.proxy.mode, ProxyMode::System);
        });
        assert!(
            !path.exists(),
            "editing and navigating must not save a proxy"
        );
        SendMessageW(hwnd, WM_COMMAND, SAVE as usize, 0);
        let saved = config::load(&path).unwrap();
        assert_eq!(
            saved.proxy,
            ProxyConfig {
                mode: ProxyMode::Manual,
                protocol: ProxyProtocol::Socks5,
                host: "proxy.example".into(),
                port: 1080,
            }
        );
        let original = std::fs::read(&path).unwrap();
        let port = STATE.with(|state| state.borrow().as_ref().unwrap().field(PORT));
        let host = STATE.with(|state| state.borrow().as_ref().unwrap().field(HOST));
        for value in ["0", "65536", "invalid"] {
            set_text(port, value);
            SendMessageW(hwnd, WM_COMMAND, SAVE as usize, 0);
            assert_eq!(std::fs::read(&path).unwrap(), original);
            STATE
                .with(|state| assert!(state.borrow().as_ref().unwrap().status.contains("1–65535")));
        }
        set_text(port, "7890");
        set_text(host, "http://127.0.0.1");
        SendMessageW(hwnd, WM_COMMAND, SAVE as usize, 0);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        STATE.with(|state| {
            assert!(
                state
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .status
                    .contains("IP 地址或域名")
            )
        });

        let mut app = STATE.with(|state| state.borrow_mut().take().unwrap());
        DestroyWindow(hwnd);
        app.release_resources();
    });
}

#[test]
fn proxy_test_uses_draft_without_saving_and_ignores_changed_or_old_results() {
    visual_checks::on_isolated_desktop(|| unsafe {
        use std::{
            io::{BufRead, BufReader, Write},
            net::TcpListener,
        };
        let profile = Profile::new();
        let path = profile.directory.join("config.json");
        let mut app =
            visual_checks::app_for_check(false, 96, &profile.directory.join("agenda.json"));
        config::save(&path, &app.saved).unwrap();
        let original = std::fs::read(&path).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let proxy = ProxyConfig {
            mode: ProxyMode::Manual,
            port: listener.local_addr().unwrap().port(),
            ..Default::default()
        };
        app.draft.proxy = proxy.clone();
        app.page = PAGE;
        app.render();
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(3);
            while Instant::now() < deadline {
                if let Ok((mut stream, _)) = listener.accept() {
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    let mut request = String::new();
                    BufReader::new(&mut stream).read_line(&mut request).unwrap();
                    stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                    return request;
                }
                thread::sleep(Duration::from_millis(5));
            }
            panic!("draft proxy did not receive the connection test");
        });
        let hwnd = app.hwnd;
        let host = app.field(HOST);
        STATE.with(|state| *state.borrow_mut() = Some(app));
        SendMessageW(hwnd, WM_COMMAND, TEST as usize, 0);
        let serial = STATE.with(|state| {
            let state = state.borrow();
            let app = state.as_ref().unwrap();
            assert_eq!(IsWindowEnabled(app.field(TEST)), 0);
            app.proxy_test.active.as_ref().unwrap().0
        });
        // Even changing and reverting input invalidates the outstanding reply.
        set_text(host, "changed.example");
        set_text(host, "127.0.0.1");
        let mut app = STATE.with(|state| state.borrow_mut().take().unwrap());
        app.receive_proxy_test(serial + 1, proxy.clone(), Ok(Duration::from_millis(1)));
        assert!(
            app.proxy_test.active.is_some(),
            "an unrelated result cannot clear the pending test"
        );
        let deadline = Instant::now() + Duration::from_secs(4);
        while app.proxy_test.active.is_some() && Instant::now() < deadline {
            app.receive();
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            server
                .join()
                .unwrap()
                .starts_with("CONNECT api.open-meteo.com:443 ")
        );
        assert!(app.proxy_test.active.is_none());
        assert!(app.text(666).contains("请重新测试"));
        assert_ne!(IsWindowEnabled(app.field(TEST)), 0);
        assert_eq!(app.saved.proxy.mode, ProxyMode::System);
        assert_eq!(std::fs::read(&path).unwrap(), original);

        app.proxy_test = Default::default();
        app.proxy_test.active = Some((serial + 2, proxy.clone()));
        app.tx
            .send(Event::ProxyTest(
                serial + 2,
                proxy,
                Ok(Duration::from_millis(250)),
            ))
            .unwrap();
        app.receive();
        assert!(app.text(666).contains("连接成功"));
        assert!(app.text(666).contains("0.25 秒"));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        DestroyWindow(hwnd);
        app.release_resources();
    });
}

#[test]
fn weather_reply_from_previous_proxy_cannot_replace_current_weather() {
    visual_checks::on_isolated_desktop(|| unsafe {
        let mut app = visual_checks::app_for_check(
            false,
            96,
            &std::env::temp_dir().join("litetray-proxy-weather-unused.json"),
        );
        let result = || weather::WeatherResult {
            weather: None,
            cached: false,
            fallback: false,
            attempts: vec![],
            cache_error: None,
        };
        app.weather_busy = true;
        app.tx
            .send(Event::Weather(Box::new((
                app.saved.weather.clone(),
                ProxyConfig {
                    mode: ProxyMode::Direct,
                    ..Default::default()
                },
                result(),
            ))))
            .unwrap();
        app.receive();
        assert!(app.weather.is_none());
        assert!(!app.weather_busy);
        app.tx
            .send(Event::Weather(Box::new((
                app.saved.weather.clone(),
                app.saved.proxy.clone(),
                result(),
            ))))
            .unwrap();
        app.receive();
        assert!(app.weather.is_some());
        DestroyWindow(app.hwnd);
        app.release_resources();
    });
}

#[test]
fn proxy_page_labels_and_test_results_follow_language_and_theme() {
    visual_checks::on_isolated_desktop(|| unsafe {
        for dark in [false, true] {
            let mut app = visual_checks::app_for_check(
                dark,
                96,
                &std::env::temp_dir().join("litetray-proxy-labels-unused.json"),
            );
            app.saved.language = Language::En;
            app.draft.language = Language::En;
            app.draft.proxy.mode = ProxyMode::Manual;
            app.recreate_resources();
            app.page = PAGE;
            app.render();
            assert_eq!(app.text(901), "Proxy settings");
            assert_eq!(app.text(TEST), "Test connection");
            app.proxy_test.active = Some((1, app.draft.proxy.clone()));
            app.receive_proxy_test(
                1,
                app.draft.proxy.clone(),
                Err("网络请求失败或超时，请检查网络与代理设置".into()),
            );
            assert!(
                app.text(666)
                    .starts_with("Connection failed: The network request failed")
            );
            if let Some(output) = std::env::var_os("LITETRAY_PROXY_RENDER_DIR") {
                let output = std::path::PathBuf::from(output);
                std::fs::create_dir_all(&output).unwrap();
                for language in [Language::En, Language::ZhCn] {
                    app.saved.language = language;
                    app.draft.language = language;
                    app.proxy_test = Default::default();
                    app.recreate_resources();
                    app.render();
                    visual_checks::write_window(
                        app.hwnd,
                        &output.join(format!(
                            "proxy-{}-{}.bmp",
                            if language == Language::En { "en" } else { "zh" },
                            if dark { "dark" } else { "light" }
                        )),
                    );
                }
            }
            DestroyWindow(app.hwnd);
            app.release_resources();
        }
    });
}
