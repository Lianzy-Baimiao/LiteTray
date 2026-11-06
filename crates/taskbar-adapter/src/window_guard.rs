//! Same-process, same-thread window lifetime and monitor snapshot. A unique
//! window property distinguishes a recycled HWND; no subclass or hook is used.
use crate::wide;
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::Threading::*,
    UI::{HiDpi::GetDpiForWindow, WindowsAndMessaging::*},
};

#[derive(PartialEq, Eq)]
struct Monitor {
    handle: usize,
    device: [u16; 32],
    rect: [i32; 4],
    work: [i32; 4],
    dpi: u32,
}
pub(crate) struct WindowGuard {
    window: HWND,
    property: Vec<u16>,
    token: Box<u8>,
    monitor: Monitor,
}
fn local_window(window: HWND) -> bool {
    unsafe {
        let mut pid = 0;
        let tid = GetWindowThreadProcessId(window, &mut pid);
        tid == GetCurrentThreadId() && pid == GetCurrentProcessId()
    }
}
fn monitor(window: HWND) -> Result<Monitor, String> {
    unsafe {
        let handle = MonitorFromWindow(window, MONITOR_DEFAULTTONULL);
        let mut info: MONITORINFOEXW = std::mem::zeroed();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        let dpi = GetDpiForWindow(window);
        if handle.is_null()
            || dpi == 0
            || GetMonitorInfoW(handle, (&mut info as *mut MONITORINFOEXW).cast()) == 0
        {
            return Err("目标窗口的显示器或 DPI 不可用".into());
        }
        let rect = |r: RECT| [r.left, r.top, r.right, r.bottom];
        Ok(Monitor {
            handle: handle as usize,
            device: info.szDevice,
            rect: rect(info.monitorInfo.rcMonitor),
            work: rect(info.monitorInfo.rcWork),
            dpi,
        })
    }
}
impl WindowGuard {
    pub(crate) fn capture(window: HWND) -> Result<Self, String> {
        if !local_window(window) {
            return Err("窗口生命周期检查只接受当前进程和线程的窗口".into());
        }
        let token = Box::new(0u8);
        let property = wide(&format!(
            "LiteTray.WindowLease.{:x}",
            &*token as *const u8 as usize
        ));
        let snapshot = monitor(window)?;
        unsafe {
            if !GetPropW(window, property.as_ptr()).is_null() {
                return Err("窗口生命周期标记重复".into());
            }
            if SetPropW(
                window,
                property.as_ptr(),
                (&*token as *const u8).cast_mut().cast(),
            ) == 0
            {
                return Err("无法保存窗口生命周期标记".into());
            }
        }
        let guard = Self {
            window,
            property,
            token,
            monitor: snapshot,
        };
        guard.validate()?;
        Ok(guard)
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !local_window(self.window)
            || unsafe { GetPropW(self.window, self.property.as_ptr()) }
                != (&*self.token as *const u8).cast_mut().cast()
        {
            return Err("目标窗口已销毁、重用或生命周期标记已变化".into());
        }
        if monitor(self.window)? != self.monitor {
            return Err("任务栏显示器、工作区或 DPI 已变化，需要重新识别".into());
        }
        Ok(())
    }
}
impl Drop for WindowGuard {
    fn drop(&mut self) {
        unsafe {
            if local_window(self.window)
                && GetPropW(self.window, self.property.as_ptr())
                    == (&*self.token as *const u8).cast_mut().cast()
            {
                RemovePropW(self.window, self.property.as_ptr());
            }
        }
    }
}
pub(crate) fn self_test() -> Result<Vec<&'static str>, String> {
    use std::ptr::{null, null_mut};
    unsafe {
        let window = CreateWindowExW(
            0,
            wide("STATIC").as_ptr(),
            wide("LiteTray isolated window lifetime").as_ptr(),
            WS_POPUP,
            0,
            0,
            32,
            32,
            null_mut(),
            null_mut(),
            windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(null()),
            null(),
        );
        if window.is_null() {
            return Err("无法创建隔离窗口检查".into());
        }
        struct Window(HWND);
        impl Drop for Window {
            fn drop(&mut self) {
                if !self.0.is_null() {
                    unsafe {
                        DestroyWindow(self.0);
                    }
                }
            }
        }
        let mut window = Window(window);
        for _ in 0..12 {
            let guard = WindowGuard::capture(window.0)?;
            guard.validate()?;
            let property = guard.property.clone();
            drop(guard);
            if !GetPropW(window.0, property.as_ptr()).is_null() {
                return Err("窗口标记未回收".into());
            }
        }
        let guard = WindowGuard::capture(window.0)?;
        let property = guard.property.clone();
        let competing = Box::new(3u8);
        let value = (&*competing as *const u8).cast_mut().cast();
        if SetPropW(window.0, property.as_ptr(), value) == 0 {
            return Err("无法构造窗口标记竞争".into());
        }
        if guard.validate().is_ok() {
            return Err("未拒绝变更的窗口标记".into());
        }
        drop(guard);
        if GetPropW(window.0, property.as_ptr()) != value {
            return Err("清理覆盖了外部窗口标记".into());
        }
        RemovePropW(window.0, property.as_ptr());
        let guard = WindowGuard::capture(window.0)?;
        if DestroyWindow(window.0) == 0 {
            return Err("无法结束隔离窗口".into());
        }
        window.0 = null_mut();
        if guard.validate().is_ok() {
            return Err("已销毁窗口未使会话检查失效".into());
        }
        drop(guard);
        Ok(vec![
            "window_guard_native_marker_cleanup_and_competing_write",
            "window_guard_destroyed_window_rejected",
        ])
    }
}
