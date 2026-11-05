#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(windows)]
mod ui;

fn main() {
    if let Err(error) = run() {
        eprintln!("LiteTray: {error}");
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::*;
            MessageBoxW(
                std::ptr::null_mut(),
                litetray::platform::wide(&error).as_ptr(),
                litetray::platform::wide("LiteTray").as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
        std::process::exit(1);
    }
}
#[cfg(windows)]
fn run() -> Result<(), String> {
    use litetray::{config, http, platform, update, weather};
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let config = config::load(&config::data_dir().join("config.json"))?;
    match arguments.first().map(String::as_str) {
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
                "LiteTray [--settings | --calendar | --background | --diagnose [file] | --weather-check [file] | --check-updates]"
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
