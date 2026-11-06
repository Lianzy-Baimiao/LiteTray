//! A real scrolling content viewport. Controls use page-local coordinates;
//! navigation and the save/status footer never scroll with the form.
use super::*;
const CLASS: &str = "LiteTray.Page.1";
pub(super) unsafe fn create(parent: HWND, bounds: RECT) -> HWND {
    unsafe {
        let class = wide(CLASS);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: GetModuleHandleW(null()),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            class.as_ptr(),
            wide("页面内容").as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_VSCROLL,
            bounds.left,
            bounds.top,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            parent,
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        SetWindowTheme(
            hwnd,
            wide(if PALETTE.with(Cell::get).is_some_and(|p| p.dark) {
                "DarkMode_Explorer"
            } else {
                "Explorer"
            })
            .as_ptr(),
            null(),
        );
        hwnd
    }
}
pub(super) unsafe fn finish(hwnd: HWND) {
    unsafe {
        if hwnd.is_null() {
            return;
        }
        let mut client: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut client);
        let mut bottom = 0;
        let mut child = GetWindow(hwnd, GW_CHILD);
        while !child.is_null() {
            let mut r: RECT = std::mem::zeroed();
            GetWindowRect(child, &mut r);
            MapWindowPoints(null_mut(), hwnd, &mut r as *mut RECT as *mut POINT, 2);
            bottom = bottom.max(r.bottom);
            child = GetWindow(child, GW_HWNDNEXT);
        }
        let info = SCROLLINFO {
            cbSize: size_of::<SCROLLINFO>() as u32,
            fMask: SIF_RANGE | SIF_PAGE,
            nMin: 0,
            nMax: bottom + (20 * GetDpiForWindow(hwnd).max(96) / 96) as i32,
            nPage: client.bottom.max(1) as u32,
            ..std::mem::zeroed()
        };
        SetScrollInfo(hwnd, SB_VERT, &info, 1);
    }
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_COMMAND | WM_NOTIFY | WM_DRAWITEM | WM_CTLCOLORSTATIC | WM_CTLCOLORBTN
            | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => SendMessageW(GetParent(hwnd), msg, w, l),
            WM_ERASEBKGND => 1,
            WM_PAINT | WM_PRINTCLIENT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let dc = if msg == WM_PRINTCLIENT {
                    w as HDC
                } else {
                    BeginPaint(hwnd, &mut ps)
                };
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut r);
                let theme = design::Theme::new(PALETTE.with(Cell::get).is_some_and(|p| p.dark));
                design::fill(dc, r, theme.surface);
                paint_fields(hwnd, dc, theme);
                if msg != WM_PRINTCLIENT {
                    EndPaint(hwnd, &ps);
                }
                0
            }
            WM_MOUSEWHEEL | WM_VSCROLL => {
                if GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_VSCROLL == 0 {
                    return 0;
                }
                let mut info: SCROLLINFO = std::mem::zeroed();
                info.cbSize = size_of::<SCROLLINFO>() as u32;
                info.fMask = SIF_ALL;
                GetScrollInfo(hwnd, SB_VERT, &mut info);
                let old = info.nPos;
                let line = (40 * GetDpiForWindow(hwnd).max(96) / 96) as i32;
                let next = if msg == WM_MOUSEWHEEL {
                    old - (w >> 16) as i16 as i32 * line / WHEEL_DELTA as i32
                } else {
                    match (w & 0xffff) as i32 {
                        SB_LINEUP => old - line,
                        SB_LINEDOWN => old + line,
                        SB_PAGEUP => old - info.nPage as i32,
                        SB_PAGEDOWN => old + info.nPage as i32,
                        SB_THUMBTRACK | SB_THUMBPOSITION => info.nTrackPos,
                        SB_TOP => 0,
                        SB_BOTTOM => info.nMax,
                        _ => old,
                    }
                };
                info.fMask = SIF_POS;
                info.nPos = next;
                SetScrollInfo(hwnd, SB_VERT, &info, 1);
                GetScrollInfo(hwnd, SB_VERT, &mut info);
                ScrollWindowEx(
                    hwnd,
                    0,
                    old - info.nPos,
                    null(),
                    null(),
                    null_mut(),
                    null_mut(),
                    SW_SCROLLCHILDREN | SW_INVALIDATE | SW_ERASE,
                );
                0
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }
}
pub(super) unsafe fn paint_fields(hwnd: HWND, dc: HDC, theme: design::Theme) {
    unsafe {
        let scale = GetDpiForWindow(hwnd).max(96) as i32;
        let mut child = GetWindow(hwnd, GW_CHILD);
        while !child.is_null() {
            if GetWindowLongW(child, GWL_STYLE) as u32 & WS_VISIBLE != 0
                && !GetPropW(child, wide("LiteTray.Field").as_ptr()).is_null()
            {
                let mut r: RECT = std::mem::zeroed();
                GetWindowRect(child, &mut r);
                MapWindowPoints(null_mut(), hwnd, &mut r as *mut RECT as *mut POINT, 2);
                InflateRect(&mut r, 2 * scale / 96, 2 * scale / 96);
                design::rounded(
                    dc,
                    r,
                    theme.field,
                    theme.border,
                    if theme.high_contrast {
                        0
                    } else {
                        8 * scale / 96
                    },
                );
                if GetFocus() == child {
                    design::fill(
                        dc,
                        design::rect(
                            r.left,
                            r.bottom - 2 * scale / 96,
                            r.right - r.left,
                            2 * scale / 96,
                        ),
                        theme.accent,
                    );
                }
            }
            child = GetWindow(child, GW_HWNDNEXT);
        }
    }
}
