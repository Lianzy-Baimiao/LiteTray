//! Read-only Windows display topology. No SetDisplayConfig, registry writes,
//! Explorer injection, or persisted HWND/HMONITOR/adapter source identifiers.
use crate::monitors::{Display, Snapshot, Target, same_id, stable_id};
use std::{
    mem::size_of,
    ptr::{null, null_mut},
};
use windows_sys::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;
use windows_sys::Win32::{Devices::Display::*, Foundation::*, Graphics::Gdi::*};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Route {
    device: String,
    target: Target,
}
pub fn collect() -> Snapshot {
    // Enumeration can race a hotplug. Read the active source/target mapping
    // on both sides of monitor enumeration and accept only a stable mapping.
    let mut snapshot = Snapshot::default();
    for _ in 0..3 {
        let before = paths();
        snapshot.displays.clear();
        snapshot.warnings.clear();
        unsafe {
            if EnumDisplayMonitors(
                null_mut(),
                null(),
                Some(monitor_callback),
                &mut snapshot as *mut _ as isize,
            ) == 0
            {
                snapshot
                    .warnings
                    .push(format!("枚举显示器失败：{}", GetLastError()));
                return snapshot;
            }
        }
        match (before, paths()) {
            (Ok(before), Ok(after)) if before == after => {
                for display in &mut snapshot.displays {
                    display.targets = after
                        .iter()
                        .filter(|r| same_id(&r.device, &display.device))
                        .map(|r| r.target.clone())
                        .collect();
                    if display.targets.is_empty() {
                        snapshot
                            .warnings
                            .push(format!("{} 尚无稳定设备标识", display.device));
                    }
                }
                snapshot
                    .displays
                    .sort_by_key(|m| (m.rect[0], m.rect[1], m.device.to_ascii_lowercase()));
                return snapshot;
            }
            (Err(error), _) | (_, Err(error)) => {
                snapshot.warnings.push(error);
                return snapshot;
            }
            _ => {}
        }
    }
    snapshot
        .warnings
        .push("显示器连接正在变化，请稍后刷新；此次不匹配已保存的屏幕".into());
    snapshot
}
fn paths() -> Result<Vec<Route>, String> {
    unsafe {
        for _ in 0..3 {
            let (mut path_count, mut mode_count) = (0, 0);
            let status = GetDisplayConfigBufferSizes(
                QDC_ONLY_ACTIVE_PATHS,
                &mut path_count,
                &mut mode_count,
            );
            if status != ERROR_SUCCESS {
                return Err(format!("读取显示器拓扑大小失败：{status}"));
            }
            if path_count > 128 || mode_count > 512 {
                return Err("显示器拓扑超过支持数量".into());
            }
            let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
            let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
            let status = QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                null_mut(),
            );
            if status == ERROR_INSUFFICIENT_BUFFER {
                continue;
            }
            if status != ERROR_SUCCESS {
                return Err(format!("读取活动显示器失败：{status}"));
            }
            if path_count as usize > paths.len() || mode_count as usize > modes.len() {
                return Err("Windows 返回的显示器数量越界".into());
            }
            let mut result = Vec::new();
            for path in paths.into_iter().take(path_count as usize) {
                let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
                    header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                        r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                        size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                        adapterId: path.sourceInfo.adapterId,
                        id: path.sourceInfo.id,
                    },
                    ..Default::default()
                };
                let status = DisplayConfigGetDeviceInfo(&mut source.header);
                if status != 0 {
                    return Err(format!("读取桌面显示源失败：{status}"));
                }
                let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
                    header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                        r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                        size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                        adapterId: path.targetInfo.adapterId,
                        id: path.targetInfo.id,
                    },
                    ..Default::default()
                };
                let status = DisplayConfigGetDeviceInfo(&mut target.header);
                if status != 0 {
                    return Err(format!("读取显示器设备标识失败：{status}"));
                }
                let id = text(&target.monitorDevicePath)?.to_ascii_lowercase();
                if !stable_id(&id) {
                    return Err("Windows 未返回可保存的显示器设备路径".into());
                }
                let name = text(&target.monitorFriendlyDeviceName)?;
                result.push(Route {
                    device: text(&source.viewGdiDeviceName)?,
                    target: Target {
                        id,
                        name: if name.trim().is_empty() {
                            "未命名显示器".into()
                        } else {
                            name
                        },
                    },
                });
            }
            result.sort_by(|a, b| (&a.device, &a.target.id).cmp(&(&b.device, &b.target.id)));
            result.dedup();
            return Ok(result);
        }
    }
    Err("显示器拓扑持续变化，无法取得一致快照".into())
}
fn text(value: &[u16]) -> Result<String, String> {
    let len = value
        .iter()
        .position(|c| *c == 0)
        .ok_or("显示器名称未终止")?;
    let value = String::from_utf16(&value[..len]).map_err(|_| "显示器名称编码无效")?;
    if value.chars().any(char::is_control) {
        return Err("显示器名称包含控制字符".into());
    }
    Ok(value)
}
unsafe extern "system" fn monitor_callback(
    monitor: HMONITOR,
    _: HDC,
    _: *mut RECT,
    data: LPARAM,
) -> i32 {
    unsafe {
        let snapshot = &mut *(data as *mut Snapshot);
        if snapshot.displays.len() >= 64 {
            snapshot
                .warnings
                .push("显示器数量超过 64，列表已截断".into());
            return 0;
        }
        match monitor_info(monitor) {
            Ok(display) => snapshot.displays.push(display),
            Err(error) => snapshot.warnings.push(error),
        }
        1
    }
}
unsafe fn monitor_info(monitor: HMONITOR) -> Result<Display, String> {
    unsafe {
        let mut info: MONITORINFOEXW = std::mem::zeroed();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(monitor, (&mut info as *mut MONITORINFOEXW).cast()) == 0 {
            return Err(format!("读取显示器区域失败：{}", GetLastError()));
        }
        let rect = |r: RECT| [r.left, r.top, r.right, r.bottom];
        Ok(Display {
            device: text(&info.szDevice)?,
            primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
            rect: rect(info.monitorInfo.rcMonitor),
            work: rect(info.monitorInfo.rcWork),
            targets: Vec::new(),
        })
    }
}
/// Runtime mapping for a taskbar HWND. Call again after topology changes.
/// # Safety
/// `window` must be an HWND obtained from Windows, never an arbitrary pointer.
pub unsafe fn device_for_window(window: HWND) -> Option<String> {
    unsafe {
        let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONULL);
        if monitor.is_null() {
            None
        } else {
            monitor_info(monitor).ok().map(|m| m.device)
        }
    }
}
