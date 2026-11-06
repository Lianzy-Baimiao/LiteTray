//! Read-only popup diagnostics: class, owner, geometry; never window text/content.
#[cfg(windows)]
mod diagnostic {
    use std::{
        collections::BTreeMap,
        io::Write,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::Threading::*,
        UI::{HiDpi::*, Shell::*, WindowsAndMessaging::*},
    };

    unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> i32 {
        unsafe {
            let mut class = [0; 256];
            let len = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
            let class = String::from_utf16_lossy(&class[..len.max(0) as usize]);
            let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            let exstyle = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            let popup = matches!(
                class.as_str(),
                "tooltips_class32"
                    | "Xaml_WindowedPopupClass"
                    | "TopLevelWindowForOverflowXamlIsland"
            ) || class == "Chrome_WidgetWin_1";
            let mut pid = 0;
            let tid = GetWindowThreadProcessId(hwnd, &mut pid);
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            let mut path = [0; 1024];
            let mut len = path.len() as u32;
            let ok = !process.is_null()
                && QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut len) != 0;
            if !process.is_null() {
                CloseHandle(process);
            }
            if !ok {
                return 1;
            }
            let path = String::from_utf16_lossy(&path[..len as usize]);
            let name = path.rsplit('\\').next().unwrap_or("");
            if !["QQ.exe", "explorer.exe", "ShellHost.exe"]
                .iter()
                .any(|n| name.eq_ignore_ascii_case(n))
            {
                return 1;
            }
            if !name.eq_ignore_ascii_case("QQ.exe")
                && !popup
                && !matches!(class.as_str(), "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
            {
                return 1;
            }
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(hwnd, &mut r);
            let value = serde_json::json!({"hwnd": hwnd as usize, "pid": pid, "tid": tid,
            "process": name, "class": class, "popup": popup || name.eq_ignore_ascii_case("QQ.exe"), "visible": IsWindowVisible(hwnd) != 0,
            "owner": GetWindow(hwnd, GW_OWNER) as usize,
            "monitor": MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) as usize,
            "style": format!("{style:08x}"),
            "exstyle": format!("{exstyle:08x}"),
            "rect": [r.left, r.top, r.right, r.bottom]});
            (&mut *(data as *mut BTreeMap<usize, serde_json::Value>)).insert(hwnd as usize, value);
            1
        }
    }
    pub fn run() {
        unsafe {
            SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        let seconds: u64 = std::env::args()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let start = Instant::now();
        let mut previous = BTreeMap::new();
        loop {
            let mut current: BTreeMap<usize, serde_json::Value> = BTreeMap::new();
            let mut cursor = POINT { x: 0, y: 0 };
            unsafe {
                EnumWindows(Some(collect), &mut current as *mut _ as isize);
                GetCursorPos(&mut cursor);
            }
            for (hwnd, value) in &current {
                if previous.get(hwnd) != Some(value) {
                    let hover = current.values().find(|w| {
                        matches!(
                            w["class"].as_str(),
                            Some("Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
                        ) && w["rect"][0].as_i64().unwrap_or(0) <= cursor.x as i64
                            && w["rect"][2].as_i64().unwrap_or(0) > cursor.x as i64
                            && w["rect"][1].as_i64().unwrap_or(0) <= cursor.y as i64
                            && w["rect"][3].as_i64().unwrap_or(0) > cursor.y as i64
                    });
                    let verdict = hover
                        .filter(|_| value["popup"] == true && value["visible"] == true)
                        .map(|host| {
                            if host["monitor"] == value["monitor"] {
                                "same_monitor"
                            } else {
                                "wrong_monitor"
                            }
                        });
                    println!(
                        "{}",
                        serde_json::json!({"tick": unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() },
                    "ms": start.elapsed().as_millis(), "cursor": [cursor.x, cursor.y], "hover_taskbar": hover,
                    "verdict": verdict, "window": value})
                    );
                    let _ = std::io::stdout().flush();
                }
                if seconds == 0 && value["class"] == "Electron_NotifyIconHostWindow" {
                    for id in 0..4 {
                        let mut rect: RECT = unsafe { std::mem::zeroed() };
                        let identifier = NOTIFYICONIDENTIFIER {
                            cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
                            hWnd: *hwnd as HWND,
                            uID: id,
                            guidItem: windows_sys::core::GUID::from_u128(0),
                        };
                        let hr = unsafe { Shell_NotifyIconGetRect(&identifier, &mut rect) };
                        println!(
                            "{}",
                            serde_json::json!({"query_icon": hwnd, "id": id, "hr": hr,
                        "rect": [rect.left, rect.top, rect.right, rect.bottom]})
                        );
                    }
                }
            }
            if start.elapsed().as_secs() >= seconds {
                break;
            }
            previous = current;
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

#[cfg(windows)]
fn main() {
    diagnostic::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Tray diagnostics require Windows.");
}
