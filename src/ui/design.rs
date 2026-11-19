//! Shared native drawing primitives. All dimensions are logical pixels.
use super::*;
use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};

#[derive(Clone, Copy)]
pub(super) struct Theme {
    pub bg: u32,
    pub surface: u32,
    pub field: u32,
    pub control: u32,
    pub control_border: u32,
    pub selection: u32,
    pub text: u32,
    pub muted: u32,
    pub border: u32,
    pub accent: u32,
    pub tint: u32,
    pub on_accent: u32,
    pub high_contrast: bool,
}
thread_local! { static THEMES: Cell<[Option<Theme>; 6]> = const { Cell::new([None; 6]) }; }
pub(super) fn invalidate_theme() {
    THEMES.with(|cache| cache.set([None; 6]));
    typography::invalidate();
}
impl Theme {
    pub fn new(dark: bool) -> Self {
        Self::for_style(dark, interface::style())
    }
    pub fn for_style(dark: bool, style: config::InterfaceStyle) -> Self {
        let index = usize::from(dark) * 3 + style as usize;
        THEMES.with(|cache| {
            let mut values = cache.get();
            if let Some(theme) = values[index] {
                return theme;
            }
            let theme = Self::read(dark, style);
            values[index] = Some(theme);
            cache.set(values);
            theme
        })
    }
    fn read(dark: bool, style: config::InterfaceStyle) -> Self {
        unsafe {
            let mut contrast: HIGHCONTRASTW = std::mem::zeroed();
            contrast.cbSize = size_of::<HIGHCONTRASTW>() as u32;
            if SystemParametersInfoW(
                SPI_GETHIGHCONTRAST,
                contrast.cbSize,
                &mut contrast as *mut _ as _,
                0,
            ) != 0
                && contrast.dwFlags & HCF_HIGHCONTRASTON != 0
            {
                return Self {
                    bg: GetSysColor(COLOR_WINDOW),
                    surface: GetSysColor(COLOR_WINDOW),
                    field: GetSysColor(COLOR_BTNFACE),
                    control: GetSysColor(COLOR_BTNFACE),
                    control_border: GetSysColor(COLOR_WINDOWTEXT),
                    selection: GetSysColor(COLOR_HIGHLIGHT),
                    text: GetSysColor(COLOR_WINDOWTEXT),
                    muted: GetSysColor(COLOR_WINDOWTEXT),
                    border: GetSysColor(COLOR_WINDOWTEXT),
                    accent: GetSysColor(COLOR_HIGHLIGHT),
                    tint: GetSysColor(COLOR_BTNFACE),
                    on_accent: GetSysColor(COLOR_HIGHLIGHTTEXT),
                    high_contrast: true,
                };
            }
        }
        let mut theme = if dark {
            Self {
                bg: rgb(32, 32, 32),
                surface: rgb(43, 43, 43),
                field: rgb(51, 51, 51),
                control: rgb(55, 55, 55),
                control_border: rgb(70, 70, 70),
                selection: rgb(48, 48, 48),
                text: rgb(255, 255, 255),
                muted: rgb(195, 195, 195),
                border: rgb(66, 66, 66),
                accent: rgb(146, 185, 255),
                tint: rgb(43, 62, 89),
                on_accent: rgb(20, 37, 62),
                high_contrast: false,
            }
        } else {
            Self {
                bg: rgb(243, 243, 243),
                surface: rgb(255, 255, 255),
                field: rgb(250, 250, 250),
                control: rgb(255, 255, 255),
                control_border: rgb(212, 212, 212),
                selection: rgb(234, 234, 234),
                text: rgb(26, 26, 26),
                muted: rgb(96, 96, 96),
                border: rgb(225, 225, 225),
                accent: rgb(35, 95, 187),
                tint: rgb(231, 238, 249),
                on_accent: rgb(255, 255, 255),
                high_contrast: false,
            }
        };
        // One layout serves every material. A style only replaces these
        // tokens, so cards, fields and buttons keep the same relationships.
        let pick = |light: u32, dark_value: u32| if dark { dark_value } else { light };
        let gray = |light: u32, dark_value: u32| {
            let value = pick(light, dark_value);
            rgb(value, value, value)
        };
        match style {
            config::InterfaceStyle::RefinedNative => {}
            config::InterfaceStyle::SoftSkeuomorphic => {
                theme.bg = pick(rgb(239, 235, 228), rgb(35, 33, 31));
                theme.surface = pick(rgb(250, 247, 242), rgb(46, 44, 41));
                theme.field = pick(rgb(255, 253, 249), rgb(56, 53, 50));
                theme.control = pick(rgb(255, 254, 251), rgb(62, 59, 55));
                theme.border = pick(rgb(226, 219, 208), rgb(64, 60, 55));
                theme.control_border = pick(rgb(209, 200, 187), rgb(26, 24, 22));
                theme.selection = pick(rgb(238, 231, 220), rgb(66, 62, 56));
                theme.text = pick(rgb(38, 33, 27), rgb(240, 236, 228));
                theme.muted = pick(rgb(104, 94, 82), rgb(190, 182, 170));
            }
            config::InterfaceStyle::LiquidGlass => {
                theme.bg = gray(240, 28);
                theme.surface = gray(250, 40);
                theme.field = gray(255, 50);
                theme.control = gray(254, 54);
                // Hairlines become light rims once the fills turn translucent.
                theme.control_border = gray(200, 112);
                theme.border = gray(216, 92);
                theme.selection = gray(234, 60);
                theme.text = gray(26, 245);
                theme.muted = gray(88, 196);
            }
        }
        use windows::UI::ViewManagement::{UIColorType, UISettings};
        if let Ok(color) = UISettings::new().and_then(|ui| {
            ui.GetColorValue(if dark {
                UIColorType::AccentLight2
            } else {
                UIColorType::AccentDark1
            })
        }) {
            theme.accent = rgb(color.R as u32, color.G as u32, color.B as u32);
        }
        if style == config::InterfaceStyle::SoftSkeuomorphic {
            theme.accent = pick(rgb(170, 72, 26), rgb(240, 164, 112));
        }
        let luminance = |c: u32| {
            let c = c as f64 / 255.;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let l = 0.2126 * luminance(theme.accent & 255)
            + 0.7152 * luminance((theme.accent >> 8) & 255)
            + 0.0722 * luminance((theme.accent >> 16) & 255);
        theme.on_accent = if l > 0.179 {
            rgb(0, 0, 0)
        } else {
            rgb(255, 255, 255)
        };
        theme.tint = if style == config::InterfaceStyle::RefinedNative {
            blend(theme.surface, theme.accent, if dark { 18 } else { 10 })
        } else {
            blend(theme.selection, theme.accent, if dark { 10 } else { 6 })
        };
        theme
    }
}
pub(super) fn blend(base: u32, color: u32, percent: u32) -> u32 {
    let component = |shift: u32| {
        (((base >> shift) & 255u32) * (100 - percent) + ((color >> shift) & 255u32) * percent) / 100
    };
    rgb(component(0), component(8), component(16))
}
pub(super) fn font_face() -> String {
    if let Some(face) = interface::font_override() {
        return face;
    }
    unsafe {
        let mut metrics: NONCLIENTMETRICSW = std::mem::zeroed();
        metrics.cbSize = size_of::<NONCLIENTMETRICSW>() as u32;
        if SystemParametersInfoW(
            SPI_GETNONCLIENTMETRICS,
            metrics.cbSize,
            &mut metrics as *mut _ as _,
            0,
        ) != 0
        {
            let name = metrics.lfMessageFont.lfFaceName;
            return String::from_utf16_lossy(
                &name[..name.iter().position(|c| *c == 0).unwrap_or(name.len())],
            );
        }
    }
    "Segoe UI".into()
}
pub(super) unsafe fn style_combo(hwnd: HWND, dark: bool, dpi: u32) {
    unsafe {
        if dark {
            SetPropW(
                hwnd,
                wide("LiteTray.ControlDark").as_ptr(),
                1usize as HANDLE,
            );
        } else {
            RemovePropW(hwnd, wide("LiteTray.ControlDark").as_ptr());
        }
        // Keep the native combo's input, keyboard navigation, list and UIA;
        // paint its closed surface and rows with the same system palette.
        SetWindowTheme(hwnd, wide("").as_ptr(), wide("").as_ptr());
        SendMessageW(hwnd, CB_SETITEMHEIGHT, usize::MAX, (28 * dpi / 96) as isize);
        SendMessageW(hwnd, CB_SETITEMHEIGHT, 0, (28 * dpi / 96) as isize);
        SetWindowSubclass(hwnd, Some(combo_proc), 4, 0);
    }
}
pub(super) unsafe fn combo_text(hwnd: HWND, index: i32) -> String {
    unsafe {
        if index < 0 {
            return String::new();
        }
        let len = SendMessageW(hwnd, CB_GETLBTEXTLEN, index as usize, 0);
        if !(0..=4096).contains(&len) {
            return String::new();
        }
        let mut text = vec![0u16; len as usize + 1];
        let count = SendMessageW(
            hwnd,
            CB_GETLBTEXT,
            index as usize,
            text.as_mut_ptr() as isize,
        );
        if count < 0 {
            return String::new();
        }
        String::from_utf16_lossy(&text[..count as usize])
    }
}
pub(super) unsafe fn draw_combo(item: &DRAWITEMSTRUCT, dark: bool) {
    unsafe {
        let t = Theme::new(dark);
        let selected = item.itemState & ODS_SELECTED != 0;
        fill(
            item.hDC,
            item.rcItem,
            if selected { t.tint } else { t.field },
        );
        let mut r = item.rcItem;
        r.left += 8;
        text(
            item.hDC,
            &combo_text(item.hwndItem, item.itemID as i32),
            r,
            SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as HFONT,
            if item.itemState & ODS_DISABLED != 0 {
                t.muted
            } else {
                t.text
            },
            DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
        );
        if item.itemState & ODS_FOCUS != 0 {
            DrawFocusRect(item.hDC, &item.rcItem);
        }
    }
}
unsafe extern "system" fn combo_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    _data: usize,
) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT | WM_PRINTCLIENT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let dc = if msg == WM_PRINTCLIENT {
                    w as HDC
                } else {
                    BeginPaint(hwnd, &mut ps)
                };
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut r);
                let buffer = PaintBuffer::for_window(dc, r, hwnd);
                let dc = buffer.as_ref().map_or(dc, |b| b.dc);
                let dark = !GetPropW(hwnd, wide("LiteTray.ControlDark").as_ptr()).is_null();
                let t = Theme::new(dark);
                let dpi = interface::layout_dpi(GetDpiForWindow(hwnd).max(96)) as i32;
                let s = |n| n * dpi / 96;
                fill(dc, r, t.surface);
                card(
                    dc,
                    r,
                    t.field,
                    if GetFocus() == hwnd {
                        t.accent
                    } else {
                        t.control_border
                    },
                    s(6),
                );
                let mut label = r;
                label.left += s(10);
                label.right -= s(30);
                let value = combo_text(hwnd, SendMessageW(hwnd, CB_GETCURSEL, 0, 0) as i32);
                text(
                    dc,
                    &value,
                    label,
                    SendMessageW(hwnd, WM_GETFONT, 0, 0) as HFONT,
                    if IsWindowEnabled(hwnd) != 0 {
                        t.text
                    } else {
                        t.muted
                    },
                    DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
                );
                let pen = CreatePen(PS_SOLID, s(1).max(1), t.muted);
                let old = SelectObject(dc, pen);
                let x = r.right - s(20);
                let y = r.bottom / 2 - s(2);
                MoveToEx(dc, x, y, null_mut());
                LineTo(dc, x + s(4), y + s(4));
                LineTo(dc, x + s(8), y);
                SelectObject(dc, old);
                DeleteObject(pen);
                drop(buffer);
                if msg != WM_PRINTCLIENT {
                    EndPaint(hwnd, &ps);
                }
                return 0;
            }
            WM_ERASEBKGND => return 1,
            WM_SETFOCUS | WM_KILLFOCUS | CB_SETCURSEL | WM_ENABLE => {
                let result = DefSubclassProc(hwnd, msg, w, l);
                InvalidateRect(hwnd, null(), 0);
                return result;
            }
            WM_NCDESTROY => {
                RemovePropW(hwnd, wide("LiteTray.ControlDark").as_ptr());
                RemoveWindowSubclass(hwnd, Some(combo_proc), id);
            }
            _ => {}
        }
        DefSubclassProc(hwnd, msg, w, l)
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
        let radius = if Theme::new(PALETTE.with(Cell::get).is_some_and(|p| p.dark)).high_contrast {
            0
        } else {
            interface::radius(radius)
        };
        if radius > 0 && smooth_rounded(dc, r, color, border, radius) {
            return;
        }
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

/// A raised surface. Tactile material adds a one-pixel top light and bottom
/// shade inside the border; the other styles stay flat.
pub(super) unsafe fn card(dc: HDC, r: RECT, color: u32, border: u32, radius: i32) {
    unsafe {
        rounded(dc, r, color, border, radius);
        relief(dc, r, color, radius, false);
    }
}

pub(super) unsafe fn button_surface(
    dc: HDC,
    r: RECT,
    color: u32,
    border: u32,
    radius: i32,
    pressed: bool,
) {
    unsafe {
        rounded(dc, r, color, border, radius);
        relief(dc, r, color, radius, pressed);
    }
}

unsafe fn relief(dc: HDC, r: RECT, color: u32, radius: i32, pressed: bool) {
    unsafe {
        let t = Theme::new(PALETTE.with(Cell::get).is_some_and(|p| p.dark));
        let style = interface::style();
        if t.high_contrast || style == config::InterfaceStyle::RefinedNative {
            return;
        }
        let inset = (interface::radius(radius) / 2).max(2);
        let width = r.right - r.left - 2 * inset;
        if width <= 0 || r.bottom - r.top < 8 {
            return;
        }
        if style == config::InterfaceStyle::LiquidGlass {
            // Glass catches light along its upper edge only.
            if !pressed {
                let light = blend(color, rgb(255, 255, 255), 28);
                fill(dc, rect(r.left + inset, r.top + 1, width, 1), light);
            }
            return;
        }
        // Light paper catches a bright edge; dark material only a faint one
        // above a deeper shade.
        let dark = t.text & 255 > t.surface & 255;
        let (top, bottom) = if pressed {
            (blend(color, rgb(0, 0, 0), 10), color)
        } else if dark {
            (
                blend(color, rgb(255, 255, 255), 7),
                blend(color, rgb(0, 0, 0), 22),
            )
        } else {
            (
                blend(color, rgb(255, 255, 255), 45),
                blend(color, rgb(0, 0, 0), 7),
            )
        };
        fill(dc, rect(r.left + inset, r.top + 1, width, 1), top);
        fill(dc, rect(r.left + inset, r.bottom - 2, width, 1), bottom);
    }
}

// Only the corner pixels need antialiasing. Cache small premultiplied
// coverage masks and draw the flat interior with GDI; this avoids allocating
// or rasterizing a full-sized surface for every date/button on each paint.
#[derive(PartialEq, Eq, Clone, Copy)]
struct CornerKey {
    diameter: i32,
    color: u32,
    border: u32,
}
struct CornerMask {
    key: CornerKey,
    bitmap: HBITMAP,
}
impl Drop for CornerMask {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.bitmap);
        }
    }
}
thread_local! { static CORNERS: RefCell<std::collections::VecDeque<CornerMask>> = const { RefCell::new(std::collections::VecDeque::new()) }; }
unsafe fn corner_mask(key: CornerKey) -> Option<CornerMask> {
    unsafe {
        let n = (key.diameter + 1) / 2;
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: n * 2,
            biHeight: -n * 2,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        let mut bits = null_mut();
        let bitmap = CreateDIBSection(null_mut(), &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if bitmap.is_null() {
            return None;
        }
        let pixels = std::slice::from_raw_parts_mut(bits.cast::<u32>(), (4 * n * n) as usize);
        let radius = key.diameter as f32 / 2.;
        for y in 0..n {
            for x in 0..n {
                let mut channels = [0u32; 4];
                for sy in 0..8 {
                    for sx in 0..8 {
                        let px = x as f32 + (sx as f32 + 0.5) / 8.;
                        let py = y as f32 + (sy as f32 + 0.5) / 8.;
                        let distance = (px - radius).powi(2) + (py - radius).powi(2);
                        if distance <= radius * radius {
                            let inside = px >= 1.
                                && py >= 1.
                                && radius > 1.
                                && distance <= (radius - 1.).powi(2);
                            let color = if inside { key.color } else { key.border };
                            channels[0] += color & 255;
                            channels[1] += color >> 8 & 255;
                            channels[2] += color >> 16 & 255;
                            channels[3] += 255;
                        }
                    }
                }
                let pixel = (channels[3] / 64) << 24
                    | (channels[0] / 64) << 16
                    | (channels[1] / 64) << 8
                    | (channels[2] / 64);
                for (dx, dy) in [
                    (x, y),
                    (2 * n - 1 - x, y),
                    (x, 2 * n - 1 - y),
                    (2 * n - 1 - x, 2 * n - 1 - y),
                ] {
                    pixels[(dy * 2 * n + dx) as usize] = pixel;
                }
            }
        }
        Some(CornerMask { key, bitmap })
    }
}
unsafe fn smooth_rounded(dc: HDC, r: RECT, color: u32, border: u32, diameter: i32) -> bool {
    unsafe {
        let width = r.right - r.left;
        let height = r.bottom - r.top;
        let diameter = diameter.min(width).min(height);
        let n = (diameter + 1) / 2;
        if !(1..=64).contains(&n) || 2 * n > width || 2 * n > height {
            return false;
        }
        let key = CornerKey {
            diameter,
            color,
            border,
        };
        CORNERS.with(|cache| {
            let mut cache = cache.borrow_mut();
            let mask = if let Some(index) = cache.iter().position(|c| c.key == key) {
                cache.remove(index).unwrap()
            } else if let Some(mask) = corner_mask(key) {
                mask
            } else {
                return false;
            };
            let source = CreateCompatibleDC(dc);
            if source.is_null() {
                return false;
            }
            let old = SelectObject(source, mask.bitmap);
            fill(dc, rect(r.left + n, r.top, width - 2 * n, height), color);
            fill(dc, rect(r.left, r.top + n, width, height - 2 * n), color);
            fill(dc, rect(r.left + n, r.top, width - 2 * n, 1), border);
            fill(dc, rect(r.left + n, r.bottom - 1, width - 2 * n, 1), border);
            fill(dc, rect(r.left, r.top + n, 1, height - 2 * n), border);
            fill(dc, rect(r.right - 1, r.top + n, 1, height - 2 * n), border);
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            for (x, y, sx, sy) in [
                (r.left, r.top, 0, 0),
                (r.right - n, r.top, n, 0),
                (r.left, r.bottom - n, 0, n),
                (r.right - n, r.bottom - n, n, n),
            ] {
                AlphaBlend(dc, x, y, n, n, source, sx, sy, n, n, blend);
            }
            SelectObject(source, old);
            DeleteDC(source);
            if cache.len() == 32 {
                cache.pop_front();
            }
            cache.push_back(mask);
            true
        })
    }
}
/// Buffer a complete painted surface in CPU memory. DirectWrite can read its
/// background without synchronously reading pixels back from a window DC.
pub(super) struct PaintBuffer {
    pub dc: HDC,
    destination: HDC,
    bounds: RECT,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    pixels: *mut u8,
    glass: Option<interface::GlassPalette>,
}
thread_local! {
    static STAGED_DCS: RefCell<Vec<HDC>> = const { RefCell::new(Vec::new()) };
}
impl PaintBuffer {
    pub unsafe fn new(destination: HDC, bounds: RECT) -> Option<Self> {
        unsafe {
            if GetObjectType(destination) == OBJ_MEMDC as u32 {
                return None;
            }
            Self::stage(destination, bounds, interface::glass_palette(destination))
        }
    }
    pub unsafe fn for_window(destination: HDC, bounds: RECT, hwnd: HWND) -> Option<Self> {
        unsafe {
            let glass = interface::glass_palette_for_window(hwnd);
            if GetObjectType(destination) == OBJ_MEMDC as u32
                && (glass.is_none() || STAGED_DCS.with(|dcs| dcs.borrow().contains(&destination)))
            {
                return None;
            }
            Self::stage(destination, bounds, glass)
        }
    }
    unsafe fn stage(
        destination: HDC,
        bounds: RECT,
        glass: Option<interface::GlassPalette>,
    ) -> Option<Self> {
        unsafe {
            let width = bounds.right - bounds.left;
            let height = bounds.bottom - bounds.top;
            if width <= 0 || height <= 0 {
                return None;
            }
            let dc = CreateCompatibleDC(destination);
            if dc.is_null() {
                return None;
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bitmap.is_null() {
                DeleteDC(dc);
                return None;
            }
            let old = SelectObject(dc, bitmap);
            SetViewportOrgEx(dc, -bounds.left, -bounds.top, null_mut());
            STAGED_DCS.with(|dcs| dcs.borrow_mut().push(dc));
            Some(Self {
                dc,
                destination,
                bounds,
                bitmap,
                old,
                pixels: bits.cast(),
                glass,
            })
        }
    }
    pub unsafe fn item(item: &DRAWITEMSTRUCT) -> (Option<Self>, DRAWITEMSTRUCT) {
        unsafe {
            let buffer = Self::for_window(item.hDC, item.rcItem, item.hwndItem);
            let buffered = DRAWITEMSTRUCT {
                hDC: buffer.as_ref().map_or(item.hDC, |b| b.dc),
                ..*item
            };
            (buffer, buffered)
        }
    }
}
impl Drop for PaintBuffer {
    fn drop(&mut self) {
        unsafe {
            let r = self.bounds;
            if let Some(glass) = self.glass {
                GdiFlush();
                glass.prepare(
                    std::slice::from_raw_parts_mut(
                        self.pixels,
                        ((r.right - r.left) * (r.bottom - r.top) * 4) as usize,
                    ),
                    (r.right - r.left) as usize,
                );
            }
            BitBlt(
                self.destination,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                self.dc,
                r.left,
                r.top,
                SRCCOPY,
            );
            SelectObject(self.dc, self.old);
            STAGED_DCS.with(|dcs| dcs.borrow_mut().retain(|&dc| dc != self.dc));
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}
pub(super) unsafe fn text(dc: HDC, value: &str, r: RECT, font: HFONT, color: u32, flags: u32) {
    unsafe {
        if typography::draw(dc, value, r, font, color, flags) {
            return;
        }
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
