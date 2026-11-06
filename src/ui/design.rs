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
                bg: rgb(28, 29, 32),
                surface: rgb(37, 39, 43),
                field: rgb(48, 50, 56),
                text: rgb(241, 242, 244),
                muted: rgb(177, 181, 191),
                border: rgb(65, 68, 76),
                accent: rgb(146, 185, 255),
                tint: rgb(43, 62, 89),
                on_accent: rgb(20, 37, 62),
            }
        } else {
            Self {
                bg: rgb(243, 243, 243),
                surface: rgb(250, 251, 252),
                field: rgb(240, 242, 245),
                text: rgb(32, 34, 37),
                muted: rgb(98, 103, 115),
                border: rgb(222, 226, 232),
                accent: rgb(35, 95, 187),
                tint: rgb(231, 238, 249),
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
