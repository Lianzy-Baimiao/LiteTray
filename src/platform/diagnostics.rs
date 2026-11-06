use serde::Serialize;
use windows_sys::Win32::{Foundation::*, System::LibraryLoader::*, UI::WindowsAndMessaging::*};

pub use crate::monitors::Display as Monitor;
#[derive(Debug, Clone, Serialize)]
pub struct Taskbar {
    pub class: String,
    pub hwnd: usize,
    pub process_id: u32,
    pub thread_id: u32,
    pub rect: [i32; 4],
    pub display_device: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostics {
    pub build: u32,
    pub family: String,
    pub monitors: Vec<Monitor>,
    pub monitor_warnings: Vec<String>,
    pub monitor_selection: Option<crate::monitors::Selection>,
    pub taskbars: Vec<Taskbar>,
    pub adapter_status: String,
}
fn rectangle(r: RECT) -> [i32; 4] {
    [r.left, r.top, r.right, r.bottom]
}
pub fn collect() -> Diagnostics {
    unsafe {
        let build = windows_build();
        let monitors = super::displays::collect();
        let mut taskbars = Vec::new();
        EnumWindows(Some(window_callback), &mut taskbars as *mut _ as isize);
        Diagnostics {
            build,
            family: if build >= 22000 {
                "Windows 11 / XAML 任务栏"
            } else if build >= 10240 {
                "Windows 10 / 经典任务栏"
            } else {
                "未知或未支持的 Windows"
            }
            .into(),
            monitors: monitors.displays,
            monitor_warnings: monitors.warnings,
            monitor_selection: None,
            taskbars,
            adapter_status: "独立原生程序已就绪；任务栏接管适配尚未启用".into(),
        }
    }
}
pub fn collect_for(tray: &crate::config::TrayConfig) -> Diagnostics {
    let mut result = collect();
    result.monitor_selection = Some(crate::monitors::resolve(
        tray,
        &crate::monitors::Snapshot {
            displays: result.monitors.clone(),
            warnings: result.monitor_warnings.clone(),
        },
    ));
    result
}
unsafe extern "system" fn window_callback(hwnd: HWND, data: LPARAM) -> i32 {
    unsafe {
        let mut class = [0; 128];
        let n = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
        let class = String::from_utf16_lossy(&class[..n.max(0) as usize]);
        if class == "Shell_TrayWnd" || class == "Shell_SecondaryTrayWnd" {
            let mut process_id = 0;
            let thread_id = GetWindowThreadProcessId(hwnd, &mut process_id);
            let mut r = std::mem::zeroed();
            GetWindowRect(hwnd, &mut r);
            (&mut *(data as *mut Vec<Taskbar>)).push(Taskbar {
                class,
                hwnd: hwnd as usize,
                process_id,
                thread_id,
                rect: rectangle(r),
                display_device: super::displays::device_for_window(hwnd),
            });
        }
        1
    }
}
pub fn windows_build() -> u32 {
    unsafe {
        #[repr(C)]
        struct Version {
            size: u32,
            major: u32,
            minor: u32,
            build: u32,
            platform: u32,
            service_pack: [u16; 128],
        }
        let module = GetModuleHandleW(super::wide("ntdll.dll").as_ptr());
        let Some(function) = GetProcAddress(module, c"RtlGetVersion".as_ptr().cast()) else {
            return 0;
        };
        let function: unsafe extern "system" fn(*mut Version) -> i32 =
            std::mem::transmute(function);
        let mut version: Version = std::mem::zeroed();
        version.size = std::mem::size_of::<Version>() as u32;
        if function(&mut version) == 0 {
            version.build
        } else {
            0
        }
    }
}
