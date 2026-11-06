//! All surfaces use the icon embedded in the executable. LR_SHARED handles
//! belong to the module and must not be destroyed on a window/DPI change.
use windows_sys::Win32::{
    Foundation::HWND,
    Graphics::Gdi::*,
    System::LibraryLoader::*,
    UI::{HiDpi::GetSystemMetricsForDpi, WindowsAndMessaging::*},
};

#[expect(
    clippy::manual_dangling_ptr,
    reason = "Win32 MAKEINTRESOURCEW(1), not a pointer to memory"
)]
pub fn load(dpi: u32, large: bool) -> HICON {
    unsafe {
        let size = GetSystemMetricsForDpi(if large { SM_CXICON } else { SM_CXSMICON }, dpi);
        LoadImageW(
            GetModuleHandleW(std::ptr::null()),
            1usize as *const u16,
            IMAGE_ICON,
            size,
            size,
            LR_SHARED,
        ) as HICON
    }
}
pub fn verify() -> Result<serde_json::Value, String> {
    unsafe {
        let module = GetModuleHandleW(std::ptr::null());
        #[expect(
            clippy::manual_dangling_ptr,
            reason = "Win32 integer resource identifier"
        )]
        let group = FindResourceW(module, 1usize as *const u16, RT_GROUP_ICON);
        if group.is_null() {
            return Err("EXE 缺少程序图标资源".into());
        }
        let bytes = SizeofResource(module, group);
        let memory = LoadResource(module, group);
        let data = LockResource(memory) as *const u8;
        if data.is_null() || bytes < 6 {
            return Err("程序图标资源无效".into());
        }
        let bytes = std::slice::from_raw_parts(data, bytes as usize);
        let count = u16::from_le_bytes([bytes[4], bytes[5]]);
        if count < 9 {
            return Err("程序图标缺少 DPI 尺寸".into());
        }
        let mut sizes = vec![];
        for (dpi, large) in [
            (96, false),
            (120, false),
            (144, false),
            (192, false),
            (96, true),
            (120, true),
            (192, true),
        ] {
            let icon = load(dpi, large);
            let mut info: ICONINFO = std::mem::zeroed();
            if icon.is_null() || GetIconInfo(icon, &mut info) == 0 {
                return Err("图标载入失败".into());
            }
            let mut bitmap: BITMAP = std::mem::zeroed();
            GetObjectW(
                info.hbmColor,
                std::mem::size_of::<BITMAP>() as i32,
                &mut bitmap as *mut _ as _,
            );
            DeleteObject(info.hbmColor);
            DeleteObject(info.hbmMask);
            let expected = GetSystemMetricsForDpi(if large { SM_CXICON } else { SM_CXSMICON }, dpi);
            if bitmap.bmWidth != expected || bitmap.bmHeight != expected {
                return Err("图标像素尺寸不符合系统 DPI".into());
            }
            sizes.push(serde_json::json!({"dpi":dpi,"large":large,"pixels":bitmap.bmWidth}));
        }
        Ok(serde_json::json!({"passed":true,"resource_frames":count,"loaded_sizes":sizes}))
    }
}

/// # Safety
/// `hwnd` must be a live window owned by the calling UI thread.
pub unsafe fn apply(hwnd: HWND, dpi: u32) {
    unsafe {
        SendMessageW(
            hwnd,
            WM_SETICON,
            ICON_BIG as usize,
            load(dpi, true) as isize,
        );
        SendMessageW(
            hwnd,
            WM_SETICON,
            ICON_SMALL as usize,
            load(dpi, false) as isize,
        );
    }
}
