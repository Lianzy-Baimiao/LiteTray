//! Asynchronous local reminder audio. The UI thread owns its stop timer;
//! playback never changes the system volume and never blocks notification delivery.
use crate::organizer::SoundSettings;
use std::{cell::Cell, ptr::null_mut};
use windows_sys::Win32::{Foundation::*, Media::Audio::*, UI::WindowsAndMessaging::*};
thread_local! { static TIMER: Cell<usize> = const { Cell::new(0) }; }

pub fn stop() {
    unsafe {
        let timer = TIMER.with(|v| v.replace(0));
        if timer != 0 {
            KillTimer(null_mut(), timer);
        }
        PlaySoundW(std::ptr::null(), null_mut(), 0);
    }
}
pub fn play(settings: &SoundSettings, preview: bool) -> Result<(), String> {
    settings.validate()?;
    stop();
    if settings.tone == 0 {
        return Ok(());
    }
    let path = settings
        .path()
        .filter(|p| p.is_file())
        .ok_or("铃声文件不存在，请重新选择")?;
    let path = super::wide(&path.to_string_lossy());
    unsafe {
        if PlaySoundW(
            path.as_ptr(),
            null_mut(),
            SND_ASYNC | SND_FILENAME | SND_NODEFAULT | SND_LOOP,
        ) == 0
        {
            return Err("无法播放铃声，请选择 PCM 格式的 WAV 文件".into());
        }
        let timer = SetTimer(
            null_mut(),
            0,
            if preview {
                5000
            } else {
                settings.seconds * 1000
            },
            Some(expired),
        );
        if timer == 0 {
            stop();
            return Err("无法设置铃声停止时间".into());
        }
        TIMER.with(|v| v.set(timer));
    }
    Ok(())
}
unsafe extern "system" fn expired(_: HWND, _: u32, timer: usize, _: u32) {
    if TIMER.with(|v| v.get() == timer) {
        stop();
    }
}
