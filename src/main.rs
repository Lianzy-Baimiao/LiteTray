#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(windows)]
mod ui;

fn main() {
    if let Err(error) = run() {
        eprintln!("LiteTray: {error}");
        #[cfg(windows)]
        if !std::env::args().any(|a| {
            matches!(
                a.as_str(),
                "--diagnose"
                    | "--weather-check"
                    | "--check-updates"
                    | "--prepare-symbols"
                    | "--probe-taskbar"
                    | "--inspect-taskbar"
                    | "--self-test-clock"
                    | "--self-test-clock-owner"
                    | "--preview-windhawk"
            )
        }) {
            unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::*;
                MessageBoxW(
                    std::ptr::null_mut(),
                    litetray::platform::wide(&error).as_ptr(),
                    litetray::platform::wide("LiteTray").as_ptr(),
                    MB_OK | MB_ICONERROR,
                );
            }
        }
        std::process::exit(1);
    }
}
#[cfg(windows)]
fn run() -> Result<(), String> {
    use litetray::{config, http, platform, update, weather};
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    // Internal isolated-test child: no profile, tray discovery or network.
    if arguments.first().map(String::as_str) == Some("--self-test-clock-owner") {
        return platform::adapter::self_test_clock_owner();
    }
    let config = config::load(&config::data_dir().join("config.json"))?;
    match arguments.first().map(String::as_str) {
        Some("--preview-windhawk") => {
            use std::io::Write;
            let source = arguments.get(1).ok_or("请提供 Windhawk 导出文件路径")?;
            let report = litetray::migration::read(std::path::Path::new(source), &config)?;
            let text = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
            if let Some(path) = arguments.get(2) {
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                    .map_err(|e| format!("报告未保存（不覆盖已有文件）：{e}"))?;
                file.write_all(text.as_bytes())
                    .and_then(|_| file.sync_all())
                    .map_err(|e| e.to_string())?;
            }
            println!("{text}");
            Ok(())
        }
        Some("--self-test-clock") => {
            let result = platform::adapter::self_test_clock()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        Some("--inspect-taskbar") => {
            let result = platform::adapter::inspect()?;
            let text = serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?;
            if let Some(path) = arguments.get(1) {
                config::atomic_write(std::path::Path::new(path), text.as_bytes())?;
            }
            println!("{text}");
            Ok(())
        }
        Some("--prepare-symbols") => {
            let module = std::env::var_os("SystemRoot")
                .map(std::path::PathBuf::from)
                .ok_or("SystemRoot 不可用")?
                .join("System32/taskbar.dll");
            let import = arguments.get(1).map(std::path::Path::new);
            let result = platform::symbols::prepare(&module, import)?;
            let bytes = std::fs::read(&module).map_err(|e| e.to_string())?;
            let layout = platform::symbols::host_layout(&result, &bytes)?;
            config::atomic_write(
                &config::data_dir().join("taskbar-layout.json"),
                &serde_json::to_vec_pretty(&layout).map_err(|e| e.to_string())?,
            )?;
            println!(
                "{}",
                serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        Some("--probe-taskbar") => {
            let result = platform::probe::run()?;
            let text = serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?;
            if let Some(path) = arguments.get(1) {
                std::fs::write(path, &text).map_err(|e| e.to_string())?;
            }
            println!("{text}");
            Ok(())
        }
        Some("--diagnose") => {
            let text = serde_json::to_string_pretty(&platform::diagnostics::collect())
                .map_err(|e| e.to_string())?;
            if let Some(path) = arguments.get(1) {
                std::fs::write(path, &text).map_err(|e| e.to_string())?;
            }
            println!("{text}");
            Ok(())
        }
        Some("--weather-check") => {
            let result = weather::Engine::default().refresh(
                &http::Client::new(config.weather.timeout_seconds),
                &platform::credentials::WindowsSecrets,
                &config.weather,
                &config::data_dir().join("weather-cache.json"),
                chrono::Utc::now(),
                true,
            );
            let text = serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?;
            if let Some(path) = arguments.get(1) {
                std::fs::write(path, &text).map_err(|e| e.to_string())?;
            }
            println!("{text}");
            Ok(())
        }
        Some("--check-updates") => {
            let token = platform::credentials::read(update::CREDENTIAL_TARGET)?;
            let result = update::check(
                &http::Client::new(15),
                &config.updates,
                token.as_deref(),
                env!("CARGO_PKG_VERSION"),
            )?;
            println!("{}", result.text());
            Ok(())
        }
        Some("--help") => {
            println!(
                "LiteTray [--settings | --calendar | --background | --diagnose [file] | --weather-check [file] | --check-updates | --preview-windhawk <profile.json> [new-report.json] | --self-test-clock | --prepare-symbols [cache] | --probe-taskbar [file] | --inspect-taskbar [file]]"
            );
            Ok(())
        }
        None | Some("--settings") | Some("--calendar") | Some("--background") => ui::run(
            config,
            arguments
                .first()
                .map(String::as_str)
                .unwrap_or("--settings"),
        ),
        Some(_) => Err("未知参数，请使用 --help".into()),
    }
}
#[cfg(not(windows))]
fn run() -> Result<(), String> {
    Err("LiteTray 桌面程序仅支持 Windows".into())
}
