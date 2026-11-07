//! Mouse button packets delivered to our visible flyout by Windows Raw Input.
//! Registration is local to this process; legacy input continues to its target.
use super::*;
use windows_sys::Win32::UI::Input::*;

pub(super) const LEFT_DOWN: u16 = 0x0001;
const BUTTONS_DOWN: u16 = 0x0155; // left/right/middle/X1/X2 down, from winuser.h
thread_local! { static OWNER: Cell<HWND> = const { Cell::new(null_mut()) }; }

unsafe fn registered_mouse() -> Result<Option<RAWINPUTDEVICE>, ()> {
    unsafe {
        let size = size_of::<RAWINPUTDEVICE>() as u32;
        let mut count = 0;
        if GetRegisteredRawInputDevices(null_mut(), &mut count, size) == u32::MAX || count > 128 {
            return Err(());
        }
        if count == 0 {
            return Ok(None);
        }
        let mut devices = vec![RAWINPUTDEVICE::default(); count as usize];
        let read = GetRegisteredRawInputDevices(devices.as_mut_ptr(), &mut count, size);
        if read == u32::MAX || read as usize > devices.len() {
            return Err(());
        }
        Ok(devices[..read as usize]
            .iter()
            .copied()
            .find(|d| d.usUsagePage == 1 && d.usUsage == 2))
    }
}
pub(super) fn active(hwnd: HWND) -> bool {
    OWNER.with(Cell::get) == hwnd
}
pub(super) unsafe fn start(hwnd: HWND) -> bool {
    unsafe {
        if active(hwnd) {
            return true;
        }
        let previous = OWNER.with(Cell::get);
        if !previous.is_null() {
            stop(previous);
        }
        // Do not replace a registration made by another component in our
        // process. Focus dismissal + the old sampler remain a fallback.
        if !matches!(registered_mouse(), Ok(None)) {
            return false;
        }
        let device = RAWINPUTDEVICE {
            usUsagePage: 1,
            usUsage: 2,
            dwFlags: RIDEV_INPUTSINK,
            hwndTarget: hwnd,
        };
        if RegisterRawInputDevices(&device, 1, size_of::<RAWINPUTDEVICE>() as u32) == 0 {
            return false;
        }
        OWNER.with(|owner| owner.set(hwnd));
        true
    }
}
pub(super) unsafe fn stop(hwnd: HWND) {
    unsafe {
        if !active(hwnd) {
            return;
        }
        OWNER.with(|owner| owner.set(null_mut()));
        if registered_mouse()
            .ok()
            .flatten()
            .is_some_and(|d| d.hwndTarget == hwnd)
        {
            let device = RAWINPUTDEVICE {
                usUsagePage: 1,
                usUsage: 2,
                dwFlags: RIDEV_REMOVE,
                hwndTarget: null_mut(),
            };
            RegisterRawInputDevices(&device, 1, size_of::<RAWINPUTDEVICE>() as u32);
        }
    }
}
fn button_packet(raw: &RAWINPUT, bytes: usize) -> Option<u16> {
    let required = std::mem::offset_of!(RAWINPUT, data) + size_of::<RAWMOUSE>();
    if raw.header.dwType != 0 || bytes < required || (raw.header.dwSize as usize) < required {
        return None;
    }
    let down = unsafe { raw.data.mouse.Anonymous.Anonymous.usButtonFlags } & BUTTONS_DOWN;
    (down != 0).then_some(down)
}
pub(super) unsafe fn receive(hwnd: HWND, handle: HRAWINPUT) {
    unsafe {
        if !active(hwnd) {
            return;
        }
        let mut raw = RAWINPUT::default();
        let mut bytes = size_of::<RAWINPUT>() as u32;
        let read = GetRawInputData(
            handle,
            RID_INPUT,
            &mut raw as *mut _ as _,
            &mut bytes,
            size_of::<RAWINPUTHEADER>() as u32,
        );
        if read == u32::MAX {
            return;
        }
        let Some(buttons) = button_packet(&raw, read as usize) else {
            return;
        };
        // Queue-time position/time, not the cursor position at some later
        // timer tick. A quick down/up pair therefore cannot disappear.
        let position = GetMessagePos();
        let point = POINT {
            x: position as u16 as i16 as i32,
            y: (position >> 16) as u16 as i16 as i32,
        };
        pointer_down(hwnd, point, buttons, GetMessageTime() as u32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_registration_lives_only_with_its_panel_and_preserves_other_owners() {
        super::super::super::visual_checks::on_isolated_desktop(|| unsafe {
            let make = || {
                CreateWindowExW(
                    0,
                    wide("STATIC").as_ptr(),
                    null(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    HWND_MESSAGE,
                    null_mut(),
                    GetModuleHandleW(null()),
                    null(),
                )
            };
            let first = make();
            let second = make();
            assert!(!first.is_null() && !second.is_null());
            assert!(registered_mouse().unwrap().is_none());
            assert!(start(first));
            let registration = registered_mouse().unwrap().unwrap();
            assert_eq!(registration.hwndTarget, first);
            assert_eq!(
                registration.dwFlags, RIDEV_INPUTSINK,
                "must not suppress legacy input or capture the mouse"
            );
            assert!(start(second));
            stop(first);
            assert_eq!(registered_mouse().unwrap().unwrap().hwndTarget, second);
            stop(second);
            assert!(registered_mouse().unwrap().is_none());
            let other = RAWINPUTDEVICE {
                usUsagePage: 1,
                usUsage: 2,
                dwFlags: RIDEV_INPUTSINK,
                hwndTarget: first,
            };
            assert_ne!(
                RegisterRawInputDevices(&other, 1, size_of::<RAWINPUTDEVICE>() as u32),
                0
            );
            assert!(
                !start(second),
                "must not replace another component's registration"
            );
            stop(second);
            assert_eq!(registered_mouse().unwrap().unwrap().hwndTarget, first);
            let remove = RAWINPUTDEVICE {
                dwFlags: RIDEV_REMOVE,
                hwndTarget: null_mut(),
                ..other
            };
            assert_ne!(
                RegisterRawInputDevices(&remove, 1, size_of::<RAWINPUTDEVICE>() as u32),
                0
            );
            DestroyWindow(second);
            DestroyWindow(first);
        });
    }
    #[test]
    fn packets_distinguish_button_presses_from_release_wheel_and_other_devices() {
        let mut raw = RAWINPUT::default();
        raw.header.dwSize = size_of::<RAWINPUT>() as u32;
        for flags in [1, 4, 16, 64, 256] {
            raw.data.mouse.Anonymous.Anonymous.usButtonFlags = flags;
            assert_eq!(button_packet(&raw, size_of::<RAWINPUT>()), Some(flags));
        }
        for flags in [0, 2, 8, 32, 128, 512, 1024, 2048] {
            raw.data.mouse.Anonymous.Anonymous.usButtonFlags = flags;
            assert_eq!(button_packet(&raw, size_of::<RAWINPUT>()), None);
        }
        raw.data.mouse.Anonymous.Anonymous.usButtonFlags = LEFT_DOWN;
        assert!(button_packet(&raw, size_of::<RAWINPUTHEADER>()).is_none());
        raw.header.dwType = 1;
        assert!(button_packet(&raw, size_of::<RAWINPUT>()).is_none());
    }
}
