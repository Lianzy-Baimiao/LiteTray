//! Core Audio runs outside both Explorer and the UI message loop. Resolve the
//! current default output for every gesture, including after device changes.
use litetray_adapter_protocol::activation::Input;
use std::sync::{
    OnceLock,
    mpsc::{SyncSender, sync_channel},
};
use windows::Win32::{
    Media::Audio::{
        Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator, eMultimedia,
        eRender,
    },
    System::Com::*,
};

pub fn dispatch(input: Input) -> bool {
    if !matches!(input, Input::VolumeWheel(_) | Input::VolumeMute) {
        return false;
    }
    static WORKER: OnceLock<Option<SyncSender<Input>>> = OnceLock::new();
    WORKER
        .get_or_init(|| {
            let (tx, rx) = sync_channel(256);
            std::thread::Builder::new()
                .name("clock-volume".into())
                .spawn(move || unsafe {
                    if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
                        return;
                    }
                    let mut wheel = Wheel::default();
                    while let Ok(input) = rx.recv() {
                        // A missing/unplugged device affects only this gesture. The
                        // next gesture resolves the default endpoint again.
                        if let Err(error) = apply(input, &mut wheel) {
                            eprintln!("System volume: {error}");
                        }
                    }
                    CoUninitialize();
                })
                .ok()
                .map(|_| tx)
        })
        .as_ref()
        .is_some_and(|tx| tx.try_send(input).is_ok())
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
unsafe fn apply(input: Input, wheel: &mut Wheel) -> windows_core::Result<()> {
    unsafe {
        let steps = if let Input::VolumeWheel(delta) = input {
            let steps = wheel.steps(delta);
            if steps == 0 {
                return Ok(());
            }
            steps
        } else {
            0
        };
        let devices: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_INPROC_SERVER)?;
        let device = devices.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
        let volume: IAudioEndpointVolume = device.Activate(CLSCTX_INPROC_SERVER, None)?;
        match input {
            Input::VolumeWheel(_) => volume.SetMasterVolumeLevelScalar(
                adjusted(volume.GetMasterVolumeLevelScalar()?, steps),
                std::ptr::null(),
            ),
            Input::VolumeMute => volume.SetMute(!volume.GetMute()?.as_bool(), std::ptr::null()),
            _ => Ok(()),
        }
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
            let wait = |ready: &dyn Fn() -> bool| {
                for _ in 0..100 {
                    if ready() {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                assert!(ready(), "audio worker failed to apply action");
            };
            assert!(dispatch(Input::VolumeWheel(-120)));
            wait(&|| {
                (volume.GetMasterVolumeLevelScalar().unwrap() - adjusted(before, -1)).abs() < 0.001
            });
            assert!(dispatch(Input::VolumeMute));
            wait(&|| volume.GetMute().unwrap().as_bool() != muted);
            assert!(dispatch(Input::VolumeMute));
            wait(&|| volume.GetMute().unwrap().as_bool() == muted);
            drop(restore);
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
}
