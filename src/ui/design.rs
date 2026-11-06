//! Shared native drawing primitives. All dimensions are logical pixels.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct Theme {
    pub bg: u32,
    pub surface: u32,
    pub field: u32,
    pub text: u32,
    pub muted: u32,
    pub border: u32,
    pub accent: u32,
    pub tint: u32,
    pub on_accent: u32,
}
impl Theme {
    pub fn new(dark: bool) -> Self {
        if dark {
            Self {
                bg: rgb(24, 27, 33),
                surface: rgb(33, 37, 45),
                field: rgb(41, 46, 56),
                text: rgb(240, 243, 249),
                muted: rgb(157, 168, 186),
                border: rgb(53, 60, 72),
                accent: rgb(146, 185, 255),
                tint: rgb(43, 62, 89),
                on_accent: rgb(20, 37, 62),
            }
        } else {
            Self {
                bg: rgb(242, 245, 250),
                surface: rgb(255, 255, 255),
                field: rgb(246, 248, 252),
                text: rgb(28, 39, 58),
                muted: rgb(106, 119, 139),
                border: rgb(225, 231, 240),
                accent: rgb(44, 100, 219),
                tint: rgb(230, 239, 255),
                on_accent: rgb(255, 255, 255),
            }
        }
    }
}
pub(super) fn rect(x: i32, y: i32, w: i32, h: i32) -> RECT {
    RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}
pub(super) unsafe fn fill(dc: HDC, r: RECT, color: u32) {
    unsafe {
        let brush = CreateSolidBrush(color);
        FillRect(dc, &r, brush);
        DeleteObject(brush);
    }
}
pub(super) unsafe fn rounded(dc: HDC, r: RECT, color: u32, border: u32, radius: i32) {
    unsafe {
        let brush = CreateSolidBrush(color);
        let pen = CreatePen(PS_SOLID, 1, border);
        let ob = SelectObject(dc, brush);
        let op = SelectObject(dc, pen);
        RoundRect(dc, r.left, r.top, r.right, r.bottom, radius, radius);
        SelectObject(dc, ob);
        SelectObject(dc, op);
        DeleteObject(brush);
        DeleteObject(pen);
    }
}
pub(super) unsafe fn text(dc: HDC, value: &str, r: RECT, font: HFONT, color: u32, flags: u32) {
    unsafe {
        let old = SelectObject(dc, font);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, color);
        let mut bounds = r;
        let mut value = wide(value);
        DrawTextW(dc, value.as_mut_ptr(), -1, &mut bounds, flags | DT_NOPREFIX);
        SelectObject(dc, old);
    }
}
pub(super) unsafe extern "system" fn hover_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    _data: usize,
) -> LRESULT {
    unsafe {
        match msg {
            WM_MOUSEMOVE => {
                if GetPropW(hwnd, wide("LiteTray.Hover").as_ptr()).is_null() {
                    SetPropW(hwnd, wide("LiteTray.Hover").as_ptr(), 1usize as HANDLE);
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    TrackMouseEvent(&mut track);
                    InvalidateRect(hwnd, null(), 0);
                }
            }
            WM_MOUSELEAVE => {
                RemovePropW(hwnd, wide("LiteTray.Hover").as_ptr());
                InvalidateRect(hwnd, null(), 0);
            }
            WM_NCDESTROY => {
                RemovePropW(hwnd, wide("LiteTray.Hover").as_ptr());
                RemoveWindowSubclass(hwnd, Some(hover_proc), id);
            }
            _ => {}
        }
        DefSubclassProc(hwnd, msg, w, l)
    }
}
