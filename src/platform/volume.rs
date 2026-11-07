//! Core Audio runs outside both Explorer and the UI message loop. Resolve the
//! current default output for every gesture, including after device changes.
use litetray_adapter_protocol::activation::{Input, Rect};
use std::{
    sync::{
        Mutex, OnceLock,
        mpsc::{SyncSender, sync_channel},
    },
    time::{Duration, Instant},
};
use windows::Win32::{
    Media::Audio::{
        Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator, eMultimedia,
        eRender,
    },
    System::Com::*,
};

pub const WM_FEEDBACK: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 13;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Level {
    pub percent: u32,
    pub muted: bool,
}
#[derive(Clone, Copy)]
pub struct Feedback {
    pub anchor: Rect,
    /// No output device / failed operation must not display a made-up value.
    pub level: Option<Level>,
    pub created: Instant,
}
struct Request {
    input: Input,
    window: usize,
    anchor: Rect,
}
static FEEDBACK: Mutex<Option<(usize, Feedback)>> = Mutex::new(None);

pub fn take_feedback(window: usize) -> Option<Feedback> {
    let mut latest = FEEDBACK.lock().ok()?;
    if latest.as_ref()?.0 != window {
        return None;
    }
    let (_, feedback) = latest.take()?;
    (feedback.created.elapsed() < Duration::from_secs(2)).then_some(feedback)
}
fn publish(request: &Request, level: Option<Level>) {
    if request.window == 0 {
        return;
    }
    let Ok(mut latest) = FEEDBACK.lock() else {
        return;
    };
    // Coalesce fast scrolling to the actual latest value. Posted messages carry
    // no heap pointers; a destroyed owner cannot leak a queued allocation.
    *latest = Some((
        request.window,
        Feedback {
            anchor: request.anchor,
            level,
            created: Instant::now(),
        },
    ));
    unsafe {
        if windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
            request.window as _,
            WM_FEEDBACK,
            0,
            0,
        ) == 0
        {
            *latest = None;
        }
    }
}
pub fn dispatch(input: Input, window: usize, anchor: Rect) -> bool {
    if !matches!(input, Input::VolumeWheel(_) | Input::VolumeMute) {
        return false;
    }
    if anchor.validate().is_err() {
        return false;
    }
    static WORKER: OnceLock<Option<SyncSender<Request>>> = OnceLock::new();
    WORKER
        .get_or_init(|| {
            let (tx, rx) = sync_channel::<Request>(256);
            std::thread::Builder::new()
                .name("clock-volume".into())
                .spawn(move || unsafe {
                    if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
                        return;
                    }
                    let mut wheel = Wheel::default();
                    while let Ok(request) = rx.recv() {
                        // A missing/unplugged device affects only this gesture. The
                        // next gesture resolves the default endpoint again.
                        let result = apply(request.input, &mut wheel);
                        if let Err(error) = &result {
                            eprintln!("System volume: {error}");
                        }
                        publish(&request, result.ok());
                    }
                    CoUninitialize();
                })
                .ok()
                .map(|_| tx)
        })
        .as_ref()
        .is_some_and(|tx| {
            tx.try_send(Request {
                input,
                window,
                anchor,
            })
            .is_ok()
        })
}
#[derive(Default)]
struct Wheel {
    remainder: i32,
}
impl Wheel {
    fn steps(&mut self, delta: i32) -> i32 {
        self.remainder += delta.clamp(-32768, 32767);
        let steps = self.remainder / 120;
        self.remainder %= 120;
        steps
    }
}
fn adjusted(level: f32, steps: i32) -> f32 {
    (level + steps as f32 * 0.02).clamp(0., 1.)
}
unsafe fn apply(input: Input, wheel: &mut Wheel) -> windows_core::Result<Level> {
    unsafe {
        let steps = if let Input::VolumeWheel(delta) = input {
            wheel.steps(delta)
        } else {
            0
        };
        let devices: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER)?;
        let device = devices.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
        let volume: IAudioEndpointVolume = device.Activate(CLSCTX_INPROC_SERVER, None)?;
        match input {
            Input::VolumeWheel(_) if steps != 0 => volume.SetMasterVolumeLevelScalar(
                adjusted(volume.GetMasterVolumeLevelScalar()?, steps),
                std::ptr::null(),
            ),
            Input::VolumeMute => volume.SetMute(!volume.GetMute()?.as_bool(), std::ptr::null()),
            _ => Ok(()),
        }?;
        // Read back from Core Audio, including rounding/clamping performed by
        // the endpoint and changes made by other volume controls.
        Ok(Level {
            percent: (volume.GetMasterVolumeLevelScalar()?.clamp(0., 1.) * 100.).round() as u32,
            muted: volume.GetMute()?.as_bool(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn high_resolution_wheel_accumulates_signed_steps_and_clamps_volume() {
        let mut wheel = Wheel::default();
        assert_eq!(wheel.steps(60), 0);
        assert_eq!(wheel.steps(60), 1);
        assert_eq!(wheel.steps(-30), 0);
        assert_eq!(wheel.steps(-90), -1);
        assert_eq!(wheel.steps(360), 3);
        assert!((adjusted(0.5, 1) - 0.52).abs() < 0.0001);
        assert_eq!(adjusted(0.01, -1), 0.);
        assert_eq!(adjusted(0.99, 1), 1.);
    }
    #[test]
    #[ignore = "Explicit Core Audio verification: lowers output by 2%, toggles mute and restores original state"]
    fn live_audio_worker_changes_default_endpoint_and_restores_it() {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok().unwrap();
            let devices: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER).unwrap();
            let device = devices
                .GetDefaultAudioEndpoint(eRender, eMultimedia)
                .unwrap();
            let volume: IAudioEndpointVolume = device.Activate(CLSCTX_INPROC_SERVER, None).unwrap();
            let before = volume.GetMasterVolumeLevelScalar().unwrap();
            let muted = volume.GetMute().unwrap().as_bool();
            struct Restore(IAudioEndpointVolume, f32, bool);
            impl Drop for Restore {
                fn drop(&mut self) {
                    unsafe {
                        let _ = self.0.SetMasterVolumeLevelScalar(self.1, std::ptr::null());
                        let _ = self.0.SetMute(self.2, std::ptr::null());
                    }
                }
            }
            let restore = Restore(volume.clone(), before, muted);
            use windows_sys::Win32::{
                System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*,
            };
            let window = CreateWindowExW(
                0,
                super::super::wide("STATIC").as_ptr(),
                std::ptr::null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            );
            assert!(!window.is_null());
            let wait = |ready: &dyn Fn() -> bool| {
                for _ in 0..100 {
                    if ready() {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                assert!(ready(), "audio worker failed to apply action");
            };
            let anchor = Rect {
                left: 0,
                top: 100,
                right: 120,
                bottom: 140,
            };
            let feedback = || {
                let until = Instant::now() + Duration::from_millis(800);
                loop {
                    if let Some(feedback) = take_feedback(window as usize) {
                        return feedback;
                    }
                    assert!(Instant::now() < until, "audio feedback never arrived");
                    std::thread::sleep(Duration::from_millis(5));
                }
            };
            assert!(dispatch(Input::VolumeWheel(-120), window as usize, anchor));
            let result = feedback();
            assert_eq!(result.anchor, anchor);
            assert_eq!(
                result.level,
                Some(Level {
                    percent: (adjusted(before, -1) * 100.).round() as u32,
                    muted
                })
            );
            wait(&|| {
                (volume.GetMasterVolumeLevelScalar().unwrap() - adjusted(before, -1)).abs() < 0.001
            });
            assert!(dispatch(Input::VolumeMute, window as usize, anchor));
            assert_eq!(feedback().level.unwrap().muted, !muted);
            wait(&|| volume.GetMute().unwrap().as_bool() != muted);
            assert!(dispatch(Input::VolumeMute, window as usize, anchor));
            assert_eq!(feedback().level.unwrap().muted, muted);
            wait(&|| volume.GetMute().unwrap().as_bool() == muted);
            drop(restore);
            DestroyWindow(window);
            assert!((volume.GetMasterVolumeLevelScalar().unwrap() - before).abs() < 0.001);
            assert_eq!(volume.GetMute().unwrap().as_bool(), muted);
            eprintln!(
                "Core Audio worker: wheel -2%, mute twice, restored level={before:.3} muted={muted}"
            );
            drop(volume);
            drop(device);
            drop(devices);
            CoUninitialize();
        }
    }
    #[test]
    fn feedback_keeps_latest_clock_value_rejects_wrong_owner_and_expired_values() {
        let anchor = Rect {
            left: -240,
            top: 100,
            right: -40,
            bottom: 140,
        };
        let feedback = Feedback {
            anchor,
            level: Some(Level {
                percent: 100,
                muted: false,
            }),
            created: Instant::now(),
        };
        *FEEDBACK.lock().unwrap() = Some((7, feedback));
        assert!(take_feedback(8).is_none());
        assert_eq!(take_feedback(7).unwrap().level, feedback.level);
        assert!(take_feedback(7).is_none());
        *FEEDBACK.lock().unwrap() = Some((
            7,
            Feedback {
                created: Instant::now() - Duration::from_secs(3),
                ..feedback
            },
        ));
        assert!(take_feedback(7).is_none());
    }
}
