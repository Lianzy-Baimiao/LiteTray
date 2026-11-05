use serde::Serialize;
use windows_sys::Win32::{
    Foundation::*, Graphics::Gdi::*, System::LibraryLoader::*, UI::WindowsAndMessaging::*,
};

#[derive(Debug, Clone, Serialize)]
pub struct Monitor {
    pub device: String,
    pub primary: bool,
    pub rect: [i32; 4],
    pub work: [i32; 4],
}
#[derive(Debug, Clone, Serialize)]
pub struct Taskbar {
    pub class: String,
    pub hwnd: usize,
    pub process_id: u32,
    pub thread_id: u32,
    pub rect: [i32; 4],
}
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostics {
    pub build: u32,
    pub family: String,
    pub monitors: Vec<Monitor>,
    pub taskbars: Vec<Taskbar>,
    pub adapter_status: String,
}
fn rectangle(r: RECT) -> [i32; 4] {
    [r.left, r.top, r.right, r.bottom]
}
pub fn collect() -> Diagnostics {
    unsafe {
        let build = windows_build();
        let mut monitors = Vec::new();
        let mut taskbars = Vec::new();
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(monitor_callback),
            &mut monitors as *mut _ as isize,
        );
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
            monitors,
            taskbars,
            adapter_status: "独立原生程序已就绪；任务栏接管适配尚未启用".into(),
        }
    }
}
unsafe extern "system" fn monitor_callback(
    monitor: HMONITOR,
    _: HDC,
    _: *mut RECT,
    data: LPARAM,
) -> i32 {
    unsafe {
        let list = &mut *(data as *mut Vec<Monitor>);
        let mut info: MONITORINFOEXW = std::mem::zeroed();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(monitor, (&mut info as *mut MONITORINFOEXW).cast()) != 0 {
            let end = info
                .szDevice
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(info.szDevice.len());
            list.push(Monitor {
                device: String::from_utf16_lossy(&info.szDevice[..end]),
                primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
                rect: rectangle(info.monitorInfo.rcMonitor),
                work: rectangle(info.monitorInfo.rcWork),
            });
        }
        1
    }
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
