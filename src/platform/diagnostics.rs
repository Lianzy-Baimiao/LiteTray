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
    pub target_plan: Option<litetray_adapter_protocol::routing::Plan>,
    pub target_plan_error: Option<String>,
    pub taskbar_discovery_error: Option<String>,
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
        let (taskbars, taskbar_discovery_error) = match discover_taskbars() {
            Ok(taskbars) => (taskbars, None),
            Err(error) => (Vec::new(), Some(error)),
        };
        Diagnostics {
            build,
            family: if build >= 22000 {
                "Windows 11（任务栏实现需单独检查）"
            } else if build >= 10240 {
                "Windows 10（任务栏实现需单独检查）"
            } else {
                "未知或未支持的 Windows"
            }
            .into(),
            monitors: monitors.displays,
            monitor_warnings: monitors.warnings,
            monitor_selection: None,
            target_plan: None,
            target_plan_error: taskbar_discovery_error.clone(),
            taskbar_discovery_error,
            taskbars,
            adapter_status: "支持范围由精确模块资料和运行时检查决定；磁盘匹配不代表接管或交互已通过。各项实测范围见兼容性页签，此诊断不代表当前连接状态。".into(),
        }
    }
}
impl Diagnostics {
    pub fn primary_taskbar(&self) -> Result<&Taskbar, String> {
        if let Some(error) = &self.taskbar_discovery_error {
            return Err(error.clone());
        }
        let mut primaries = self
            .taskbars
            .iter()
            .filter(|bar| bar.class == "Shell_TrayWnd");
        let primary = primaries.next().ok_or("未找到唯一的原生主任务栏，未接管")?;
        if primaries.next().is_some() {
            return Err("未找到唯一的原生主任务栏，未接管".into());
        }
        Ok(primary)
    }
}
/// Explicit export only. Hashing images is not part of display enumeration or
/// the settings render path. The UI invokes this on a worker thread.
pub fn export(tray: &crate::config::TrayConfig) -> Result<serde_json::Value, String> {
    let diagnostics = collect_for(tray);
    let root = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .ok_or("SystemRoot 不可用")?;
    let compatibility = crate::compatibility::collect(&root, Some(diagnostics.build))?;
    let mut value = serde_json::to_value(diagnostics).map_err(|e| e.to_string())?;
    value["compatibility"] = serde_json::to_value(compatibility).map_err(|e| e.to_string())?;
    value["recent_events"] = crate::event_log::tail(40);
    Ok(value)
}
pub fn collect_for(tray: &crate::config::TrayConfig) -> Diagnostics {
    let mut result = collect();
    let selection = crate::monitors::resolve(
        tray,
        &crate::monitors::Snapshot {
            displays: result.monitors.clone(),
            warnings: result.monitor_warnings.clone(),
        },
    );
    if result.taskbar_discovery_error.is_some() {
        result.monitor_selection = Some(selection);
        return result;
    }
    let routes = result
        .taskbars
        .iter()
        .map(|t| {
            Ok(litetray_adapter_protocol::routing::Route {
                window: t.hwnd as u64,
                process_id: t.process_id,
                thread_id: t.thread_id,
                secondary: t.class == "Shell_SecondaryTrayWnd",
                device: t
                    .display_device
                    .clone()
                    .ok_or("任务栏显示器映射缺失，稍后重新枚举")?,
            })
        })
        .collect::<Result<Vec<_>, String>>();
    match routes.and_then(|routes| crate::routing::plan(tray, &selection, &routes)) {
        Ok(plan) => result.target_plan = Some(plan),
        Err(error) => result.target_plan_error = Some(error),
    }
    result.monitor_selection = Some(selection);
    result
}
// EnumWindows/EnumDesktopWindows omit the real Shell_TrayWnd on the tested
// 25H2 desktop. Exact-class FindWindowEx queries discover it without guessing
// a class, OS range, process, or private layout. Keep every distinct candidate
// so duplicate primaries remain an error rather than choosing the first one.
const MAX_TASKBARS: usize = 64;
fn class_handles(
    mut next: impl FnMut(usize) -> Result<usize, String>,
) -> Result<Vec<usize>, String> {
    let mut handles = Vec::new();
    let mut previous = 0;
    loop {
        let current = next(previous)?;
        if current == 0 {
            return Ok(handles);
        }
        if handles.len() >= MAX_TASKBARS || handles.contains(&current) {
            return Err("任务栏窗口查询超限或重复，稍后重新检测".into());
        }
        handles.push(current);
        previous = current;
    }
}
unsafe fn discover_taskbars() -> Result<Vec<Taskbar>, String> {
    unsafe {
        let mut taskbars: Vec<Taskbar> = Vec::new();
        for expected in ["Shell_TrayWnd", "Shell_SecondaryTrayWnd"] {
            let name = super::wide(expected);
            let handles = class_handles(|previous| {
                SetLastError(0);
                let hwnd = FindWindowExW(
                    std::ptr::null_mut(),
                    previous as HWND,
                    name.as_ptr(),
                    std::ptr::null(),
                );
                let error = GetLastError();
                if hwnd.is_null() && error != 0 {
                    Err(format!("任务栏窗口查询失败：{error}"))
                } else {
                    Ok(hwnd as usize)
                }
            })?;
            for handle in handles {
                let hwnd = handle as HWND;
                let mut class = [0; 128];
                let n = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
                let class = String::from_utf16_lossy(&class[..n.max(0) as usize]);
                let mut process_id = 0;
                let thread_id = GetWindowThreadProcessId(hwnd, &mut process_id);
                let mut rect = std::mem::zeroed();
                if class != expected
                    || process_id == 0
                    || thread_id == 0
                    || GetAncestor(hwnd, GA_ROOT) != hwnd
                    || GetAncestor(hwnd, GA_PARENT) != GetDesktopWindow()
                    || GetWindowRect(hwnd, &mut rect) == 0
                    || taskbars.iter().any(|bar| bar.hwnd == handle)
                    || taskbars.len() >= MAX_TASKBARS
                {
                    return Err("任务栏窗口在查询期间变化或不属于桌面顶层，稍后重新检测".into());
                }
                taskbars.push(Taskbar {
                    class,
                    hwnd: handle,
                    process_id,
                    thread_id,
                    rect: rectangle(rect),
                    display_device: super::displays::device_for_window(hwnd),
                });
            }
        }
        Ok(taskbars)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_class_query_retains_every_candidate_until_the_end() {
        let mut visited = Vec::new();
        let handles = class_handles(|previous| {
            visited.push(previous);
            Ok(match previous {
                0 => 11,
                11 => 22,
                22 => 0,
                _ => panic!("unexpected predecessor"),
            })
        })
        .unwrap();
        assert_eq!(handles, [11, 22]);
        assert_eq!(visited, [0, 11, 22]);
        assert!(class_handles(|_| Ok(0)).unwrap().is_empty());
    }

    #[test]
    fn cycling_and_unbounded_queries_never_return_partial_candidates() {
        assert!(class_handles(|previous| Ok(if previous == 11 { 22 } else { 11 })).is_err());
        let mut calls = 0;
        assert!(
            class_handles(|previous| {
                calls += 1;
                Ok(previous + 1)
            })
            .is_err()
        );
        assert_eq!(calls, MAX_TASKBARS + 1);
        assert!(
            class_handles(|previous| if previous == 0 {
                Ok(11)
            } else {
                Err("query failed".into())
            })
            .is_err()
        );
    }

    #[test]
    fn primary_selection_rejects_ambiguity_and_incomplete_discovery() {
        let mut snapshot = Diagnostics {
            build: 26200,
            family: String::new(),
            monitors: vec![],
            monitor_warnings: vec![],
            monitor_selection: None,
            target_plan: None,
            target_plan_error: None,
            taskbar_discovery_error: None,
            taskbars: vec![],
            adapter_status: String::new(),
        };
        let bar = |hwnd, class: &str| Taskbar {
            class: class.into(),
            hwnd,
            process_id: 10,
            thread_id: 20,
            rect: [0, 720, 1024, 768],
            display_device: Some("DISPLAY1".into()),
        };
        snapshot.taskbars.push(bar(2, "Shell_SecondaryTrayWnd"));
        assert!(snapshot.primary_taskbar().is_err());
        snapshot.taskbars.push(bar(1, "Shell_TrayWnd"));
        assert_eq!(snapshot.primary_taskbar().unwrap().hwnd, 1);
        snapshot.taskbars.push(bar(3, "Shell_TrayWnd"));
        assert!(snapshot.primary_taskbar().is_err());
        snapshot.taskbars.pop();
        snapshot.taskbar_discovery_error = Some("incomplete discovery".into());
        assert_eq!(
            snapshot.primary_taskbar().unwrap_err(),
            "incomplete discovery"
        );
    }
}
