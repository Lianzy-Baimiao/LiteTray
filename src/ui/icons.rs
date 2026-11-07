//! Original 24-unit SVG assets. This renderer accepts only our M/L/C/Z path
//! subset, without file/network access or a web runtime. Supersampling keeps
//! these small monochrome paths smooth on the native GDI surfaces.
use super::*;
#[derive(Clone, Copy)]
pub(super) enum Icon {
    Settings,
    Detach,
    Close,
    Refresh,
    Plus,
    Timer,
    List,
    Left,
    Right,
    Volume,
    Muted,
}
impl Icon {
    fn source(self) -> &'static str {
        match self {
            Self::Settings => include_str!("../../assets/icons/settings.svg"),
            Self::Detach => include_str!("../../assets/icons/detach.svg"),
            Self::Close => include_str!("../../assets/icons/close.svg"),
            Self::Refresh => include_str!("../../assets/icons/refresh.svg"),
            Self::Plus => include_str!("../../assets/icons/plus.svg"),
            Self::Timer => include_str!("../../assets/icons/timer.svg"),
            Self::List => include_str!("../../assets/icons/list.svg"),
            Self::Left => include_str!("../../assets/icons/chevron-left.svg"),
            Self::Right => include_str!("../../assets/icons/chevron-right.svg"),
            Self::Volume => include_str!("../../assets/icons/volume.svg"),
            Self::Muted => include_str!("../../assets/icons/volume-muted.svg"),
        }
    }
}
pub(super) unsafe fn draw(dc: HDC, icon: Icon, bounds: RECT, foreground: u32, background: u32) {
    unsafe {
        let size = (bounds.right - bounds.left)
            .min(bounds.bottom - bounds.top)
            .max(1);
        let pixels = size * 3;
        let mem = CreateCompatibleDC(dc);
        let bitmap = CreateCompatibleBitmap(dc, pixels, pixels);
        if mem.is_null() || bitmap.is_null() {
            if !mem.is_null() {
                DeleteDC(mem);
            }
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
            return;
        }
        let old_bitmap = SelectObject(mem, bitmap);
        design::fill(mem, design::rect(0, 0, pixels, pixels), background);
        let scale = pixels as f64 / 24.;
        let brush = LOGBRUSH {
            lbStyle: BS_SOLID,
            lbColor: foreground,
            lbHatch: 0,
        };
        let pen = ExtCreatePen(
            (PS_GEOMETRIC | PS_SOLID | PS_ENDCAP_ROUND | PS_JOIN_ROUND) as u32,
            (1.6 * scale).round().max(1.) as u32,
            &brush,
            0,
            null(),
        );
        let old_pen = SelectObject(mem, pen);
        stroke_svg_path(mem, icon.source(), scale);
        let saved = SaveDC(dc);
        SetStretchBltMode(dc, HALFTONE);
        SetBrushOrgEx(dc, 0, 0, null_mut());
        StretchBlt(
            dc,
            bounds.left,
            bounds.top,
            size,
            size,
            mem,
            0,
            0,
            pixels,
            pixels,
            SRCCOPY,
        );
        RestoreDC(dc, saved);
        SelectObject(mem, old_pen);
        DeleteObject(pen);
        SelectObject(mem, old_bitmap);
        DeleteObject(bitmap);
        DeleteDC(mem);
    }
}
/// Internal, trusted bundled assets only; uses the caller's geometric pen.
pub(super) unsafe fn stroke_svg_path(dc: HDC, source: &str, scale: f64) {
    unsafe {
        BeginPath(dc);
        let path = source
            .split(" d=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        let mut tokens = path.split_whitespace();
        let coordinate = |tokens: &mut std::str::SplitWhitespace<'_>| {
            (tokens.next().unwrap().parse::<f64>().unwrap() * scale).round() as i32
        };
        while let Some(command) = tokens.next() {
            match command {
                "M" => {
                    let x = coordinate(&mut tokens);
                    let y = coordinate(&mut tokens);
                    MoveToEx(dc, x, y, null_mut());
                }
                "L" => {
                    let x = coordinate(&mut tokens);
                    let y = coordinate(&mut tokens);
                    LineTo(dc, x, y);
                }
                "C" => {
                    let points = std::array::from_fn::<_, 3, _>(|_| POINT {
                        x: coordinate(&mut tokens),
                        y: coordinate(&mut tokens),
                    });
                    PolyBezierTo(dc, points.as_ptr(), 3);
                }
                "Z" => {
                    CloseFigure(dc);
                }
                _ => unreachable!("unsupported bundled SVG command"),
            }
        }
        EndPath(dc);
        StrokePath(dc);
    }
}
pub(super) struct Tips {
    hwnd: HWND,
    labels: Vec<Vec<u16>>,
}
impl Tips {
    pub unsafe fn new(owner: HWND) -> Self {
        unsafe {
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                wide("tooltips_class32").as_ptr(),
                null(),
                WS_POPUP | TTS_ALWAYSTIP,
                0,
                0,
                0,
                0,
                owner,
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            SendMessageW(hwnd, TTM_SETMAXTIPWIDTH, 0, 300);
            Self {
                hwnd,
                labels: vec![],
            }
        }
    }
    pub unsafe fn add(&mut self, owner: HWND, button: HWND, label: &str) {
        unsafe {
            self.labels.push(wide(label));
            let mut info: TTTOOLINFOW = std::mem::zeroed();
            info.cbSize = size_of::<TTTOOLINFOW>() as u32;
            info.uFlags = TTF_IDISHWND | TTF_SUBCLASS;
            info.hwnd = owner;
            info.uId = button as usize;
            info.lpszText = self.labels.last_mut().unwrap().as_mut_ptr();
            SendMessageW(self.hwnd, TTM_ADDTOOLW, 0, &info as *const _ as isize);
        }
    }
}
impl Drop for Tips {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(self.hwnd) != 0 {
                DestroyWindow(self.hwnd);
            }
        }
    }
}
