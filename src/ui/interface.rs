//! Appearance belongs to LiteTray's UI thread and owned windows. Backdrops
//! use DWM; the Explorer adapter does not receive these settings.
use super::*;
use config::{CornerStyle, InterfaceConfig, InterfaceStyle};

thread_local! {
    static CURRENT: RefCell<InterfaceConfig> = RefCell::new(InterfaceConfig::default());
    static SOLID_RENDER: Cell<u32> = const { Cell::new(0) };
}
const BACKDROP: &str = "LiteTray.GlassBackdrop";
/// Set on a settings window whose client replaces the system caption.
const FRAME: &str = "LiteTray.OwnFrame";

pub(super) fn configure(config: &InterfaceConfig) {
    let changed = CURRENT.with(|current| {
        let mut current = current.borrow_mut();
        if *current == *config {
            false
        } else {
            *current = config.clone();
            true
        }
    });
    if changed {
        design::invalidate_theme();
    }
}

pub(super) fn current() -> InterfaceConfig {
    CURRENT.with(|current| current.borrow().clone())
}

pub(super) fn style() -> InterfaceStyle {
    CURRENT.with(|current| current.borrow().style)
}

pub(super) fn radius(radius: i32) -> i32 {
    CURRENT.with(|current| match current.borrow().corners {
        CornerStyle::Rounded => radius,
        CornerStyle::Square => 0,
    })
}

pub(super) fn layout_dpi(dpi: u32) -> u32 {
    CURRENT.with(|current| (dpi * current.borrow().font_size + 7) / 14)
}

pub(super) fn text_pixels(logical: u32, dpi: u32) -> i32 {
    CURRENT.with(|current| (logical * dpi * current.borrow().font_size / (96 * 14)) as i32)
}

pub(super) fn font_override() -> Option<String> {
    CURRENT.with(|current| current.borrow().font_family.clone())
}

thread_local! {
    static DEFER_BACKDROP: Cell<bool> = const { Cell::new(false) };
    static PENDING_BACKDROP: Cell<Option<(usize, bool)>> = const { Cell::new(None) };
}

/// While a settings window is rebuilt for another style its old pixels stay
/// on screen. DWM reads them by the backdrop in force, so switching the
/// backdrop first shows them far too dark or too bright until the new frame
/// is painted. Under this guard `apply_window` leaves the backdrop alone;
/// `commit_backdrop` switches it right before that paint.
pub(super) struct DeferBackdrop;

impl DeferBackdrop {
    pub(super) fn begin() -> Self {
        DEFER_BACKDROP.with(|defer| defer.set(true));
        Self
    }
}

impl Drop for DeferBackdrop {
    fn drop(&mut self) {
        DEFER_BACKDROP.with(|defer| defer.set(false));
    }
}

/// False when the system refused the glass that the rebuild assumed; the
/// caller then applies the window again without deferring.
pub(super) unsafe fn commit_backdrop(hwnd: HWND) -> bool {
    unsafe {
        match PENDING_BACKDROP.with(Cell::take) {
            Some((window, requested)) if window == hwnd as usize => {
                set_backdrop(hwnd, requested) || !requested
            }
            _ => true,
        }
    }
}

/// A picture of the window as it is on screen, held above it while it is
/// rebuilt. Controls repaint one after another and DWM presents whatever is
/// finished at each refresh; under the cover none of that is visible, and
/// removing it shows the finished frame at once.
pub(super) struct FrameCover {
    window: HWND,
    bitmap: HBITMAP,
}

impl FrameCover {
    pub(super) unsafe fn show(hwnd: HWND) -> Self {
        unsafe {
            if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
                return Self {
                    window: null_mut(),
                    bitmap: null_mut(),
                };
            }
            let mut bounds: RECT = std::mem::zeroed();
            GetWindowRect(hwnd, &mut bounds);
            let (width, height) = (bounds.right - bounds.left, bounds.bottom - bounds.top);
            let screen = GetDC(null_mut());
            let memory = CreateCompatibleDC(screen);
            let bitmap = CreateCompatibleBitmap(screen, width.max(1), height.max(1));
            let old = SelectObject(memory, bitmap);
            BitBlt(
                memory,
                0,
                0,
                width,
                height,
                screen,
                bounds.left,
                bounds.top,
                SRCCOPY,
            );
            SelectObject(memory, old);
            DeleteDC(memory);
            ReleaseDC(null_mut(), screen);
            let window = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT,
                wide("STATIC").as_ptr(),
                null(),
                WS_POPUP | 0x0E, // SS_BITMAP
                bounds.left,
                bounds.top,
                width,
                height,
                hwnd,
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            SendMessageW(window, STM_SETIMAGE, IMAGE_BITMAP as usize, bitmap as isize);
            ShowWindow(window, SW_SHOWNOACTIVATE);
            UpdateWindow(window);
            Self { window, bitmap }
        }
    }
}

impl Drop for FrameCover {
    fn drop(&mut self) {
        unsafe {
            if self.window.is_null() {
                return;
            }
            // Let DWM present the finished frame below before uncovering it.
            DwmFlush();
            DestroyWindow(self.window);
            DeleteObject(self.bitmap);
        }
    }
}

unsafe fn set_backdrop(hwnd: HWND, glass: bool) -> bool {
    unsafe {
        let backdrop = if glass {
            DWMSBT_TRANSIENTWINDOW
        } else {
            DWMSBT_NONE
        };
        let extended = MARGINS {
            cxLeftWidth: if glass { -1 } else { 0 },
            ..Default::default()
        };
        let attribute = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE as u32,
            &backdrop as *const _ as _,
            4,
        );
        let frame = DwmExtendFrameIntoClientArea(hwnd, &extended);
        attribute >= 0 && frame >= 0
    }
}

/// `flyout` selects which fills follow the window slider: the settings
/// window's backdrop, or the click flyout's single sheet.
pub(super) unsafe fn apply_window(hwnd: HWND, build: u32, dark: bool, flyout: bool) {
    unsafe {
        let config = current();
        let theme = design::Theme::new(dark);
        if build >= 17763 {
            let dark = i32::from(dark);
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
                &dark as *const _ as _,
                4,
            );
        }
        if build >= 22000 {
            let corners = match config.corners {
                CornerStyle::Rounded if !theme.high_contrast => DWMWCP_ROUND,
                _ => DWMWCP_DONOTROUND,
            };
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                &corners as *const _ as _,
                4,
            );
        }
        let was_glass = !RemovePropW(hwnd, wide(BACKDROP).as_ptr()).is_null();
        // Decide who owns the caption before anything below recalculates
        // the frame: that calculation reads this property.
        let was_owned = owns_frame(hwnd);
        let owned = !flyout && build >= 22000 && !theme.high_contrast;
        if owned {
            SetPropW(hwnd, wide(FRAME).as_ptr(), 1usize as HANDLE);
        } else {
            RemovePropW(hwnd, wide(FRAME).as_ptr());
        }
        let requested = build >= 22621
            && config.style == InterfaceStyle::LiquidGlass
            && !theme.high_contrast
            && windows::UI::ViewManagement::UISettings::new()
                .and_then(|settings| settings.AdvancedEffectsEnabled())
                .unwrap_or(false);
        // Solid windows that never used glass need no DWM backdrop calls.
        if build >= 22621 && (requested || was_glass) {
            let glass = if !flyout && DEFER_BACKDROP.with(Cell::get) {
                PENDING_BACKDROP.with(|pending| pending.set(Some((hwnd as usize, requested))));
                requested
            } else {
                requested && set_backdrop(hwnd, requested)
            };
            if requested && !glass {
                // A failed glass request must not leave transparent holes.
                let solid = DWMSBT_NONE;
                DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_SYSTEMBACKDROP_TYPE as u32,
                    &solid as *const _ as _,
                    4,
                );
                DwmExtendFrameIntoClientArea(hwnd, &MARGINS::default());
            }
            // DWM gives a system backdrop only to framed windows, and draws
            // it apart from the client under a system caption. Glass windows
            // therefore own their whole frame; see `frame_message`.
            let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            let framed = match (flyout, glass) {
                (true, true) => style | WS_CAPTION,
                (true, false) => style & !WS_DLGFRAME,
                // The settings frame is decided below, for every style.
                (false, _) => style,
            };
            if framed != style {
                SetWindowLongW(hwnd, GWL_STYLE, framed as i32);
            }
            if flyout {
                // A framed window would otherwise gain open/close animations.
                let still = i32::from(glass);
                DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_TRANSITIONS_FORCEDISABLED as u32,
                    &still as *const _ as _,
                    4,
                );
            }
            if glass {
                SetPropW(
                    hwnd,
                    wide(BACKDROP).as_ptr(),
                    (1 + usize::from(flyout)) as HANDLE,
                );
            }
            if framed != style || glass != was_glass {
                // Recalculate the client area for the new frame ownership.
                SetWindowPos(
                    hwnd,
                    null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }
            if glass {
                // DWM replaces the backdrop of an inactive frame with a solid
                // color. Glass keeps the frame active.
                SendMessageW(hwnd, WM_NCACTIVATE, 1, 0);
            }
        }
        if !flyout {
            // In every style the settings window is one surface from its top
            // edge down: no system caption sits apart from the content.
            // Without a system menu DWM draws no caption buttons; the window
            // paints its own. High contrast keeps the familiar system frame.
            let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            let framed = if owned {
                style & !WS_SYSMENU
            } else {
                style | WS_SYSMENU
            };
            if framed != style {
                SetWindowLongW(hwnd, GWL_STYLE, framed as i32);
            }
            if framed != style || owned != was_owned {
                SetWindowPos(
                    hwnd,
                    null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }
        }
        RedrawWindow(hwnd, null(), null_mut(), RDW_INVALIDATE | RDW_ALLCHILDREN);
    }
}

/// Logical height of the strip that drags the settings window and holds
/// its caption buttons.
pub(super) const CAPTION_HEIGHT: i32 = 30;

/// Frame behavior that keeps the system backdrop alive and lets the client
/// cover the whole window. Call first in the window procedure of every
/// top-level window passed to `apply_window`.
pub(super) unsafe fn frame_message(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> Option<LRESULT> {
    unsafe {
        let role = GetPropW(hwnd, wide(BACKDROP).as_ptr()) as usize;
        let owned = owns_frame(hwnd);
        if role == 0 && !owned {
            return None;
        }
        let dpi = GetDpiForWindow(hwnd).max(96);
        let resize = GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi)
            + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
        match msg {
            // The backdrop follows the non-client active state, not focus.
            WM_NCACTIVATE if w == 0 && role != 0 => Some(DefWindowProcW(hwnd, msg, 1, l)),
            // The framed flyout keeps exactly the client area of its
            // original one-pixel border, so its layout does not move.
            WM_NCCALCSIZE if role == 2 && l != 0 => {
                let client = &mut *(l as *mut RECT);
                let border = GetSystemMetrics(SM_CXBORDER).max(1);
                client.left += border;
                client.top += border;
                client.right -= border;
                client.bottom -= border;
                Some(0)
            }
            // The settings client replaces the caption and keeps the
            // system's resize borders on the other three sides.
            WM_NCCALCSIZE if owned && w != 0 && l != 0 => {
                let top = (*(l as *const RECT)).top;
                let result = DefWindowProcW(hwnd, msg, w, l);
                // A maximized window extends past its monitor by the frame.
                (*(l as *mut RECT)).top = top + if IsZoomed(hwnd) != 0 { resize } else { 0 };
                Some(result)
            }
            WM_NCHITTEST if owned => {
                let hit = DefWindowProcW(hwnd, msg, w, l);
                if hit != HTCLIENT as isize {
                    return Some(hit);
                }
                let mut bounds: RECT = std::mem::zeroed();
                GetWindowRect(hwnd, &mut bounds);
                let x = (l & 0xffff) as i16 as i32;
                let y = ((l >> 16) & 0xffff) as i16 as i32 - bounds.top;
                let zoomed = IsZoomed(hwnd) != 0;
                let caption =
                    CAPTION_HEIGHT * layout_dpi(dpi) as i32 / 96 + if zoomed { resize } else { 0 };
                Some(if !zoomed && y < resize {
                    if x < bounds.left + 2 * resize {
                        HTTOPLEFT
                    } else if x >= bounds.right - 2 * resize {
                        HTTOPRIGHT
                    } else {
                        HTTOP
                    }
                } else if y < caption {
                    HTCAPTION
                } else {
                    HTCLIENT
                } as isize)
            }
            _ => None,
        }
    }
}

/// Whether this settings window draws its own caption strip and buttons.
pub(super) unsafe fn owns_frame(hwnd: HWND) -> bool {
    unsafe { !GetPropW(hwnd, wide(FRAME).as_ptr()).is_null() }
}

pub(super) unsafe fn glass_active(hwnd: HWND) -> bool {
    unsafe { glass_role(hwnd) != 0 }
}

/// 0 = solid, 1 = settings window, 2 = click flyout.
unsafe fn glass_role(hwnd: HWND) -> usize {
    unsafe {
        let root = GetAncestor(hwnd, GA_ROOT);
        if root.is_null() {
            0
        } else {
            GetPropW(root, wide(BACKDROP).as_ptr()) as usize
        }
    }
}

/// One background fill of the opaque rendering and its glass replacement.
#[derive(Clone, Copy)]
struct GlassLayer {
    key: u32,
    /// Premultiplied red, green, blue.
    tint: [u32; 3],
    alpha: u32,
}

impl GlassLayer {
    /// `fill` of the theme color under a white `sheen`. The sheen is what
    /// reads as glass: at low clarity a surface is a bright veil over the
    /// backdrop instead of a gray slab.
    fn new(color: u32, fill: u32, sheen: u32) -> Self {
        let under = fill * (255 - sheen) / 255;
        Self {
            key: color & 0x00ff_ffff,
            tint: [16, 8, 0].map(|shift| (255 * sheen + ((color >> shift) & 255) * under) / 255),
            alpha: sheen + under,
        }
    }
}

/// GDI and DirectWrite paint opaque RGB. Known background fills become
/// translucent glass layers chosen by the two sliders; every other pixel is
/// treated as content covering the background of its row, so antialiased
/// glyphs, corners and hairlines fade continuously instead of leaving an
/// opaque fringe.
#[derive(Clone, Copy)]
pub(super) struct GlassPalette {
    layers: [GlassLayer; 4],
    /// The layer under content that leaves none of its background visible.
    ground: usize,
    text: u32,
}

impl GlassPalette {
    fn new(theme: design::Theme, config: &InterfaceConfig, flyout: bool) -> Self {
        let dark = theme.text & 255 > theme.bg & 255;
        let window = u32::from(config.background_alpha());
        // A control is never clearer than the sheet it lies on; it would
        // be a hole in a window the user made opaque.
        let card = u32::from(config.card_alpha()).max(window);
        // Inputs stay easier to read than buttons.
        let input = card + (255 - card) / 5;
        let (veil, raised) = if dark { (22, 34) } else { (96, 128) };
        Self {
            layers: if flyout {
                // The flyout is one sheet. Only the inputs laid on it, such as
                // the inline event editor's fields, follow the card slider.
                [
                    GlassLayer::new(theme.surface, window, 0),
                    GlassLayer::new(theme.bg, window, 0),
                    GlassLayer::new(theme.field, card, veil),
                    GlassLayer::new(theme.control, card, raised),
                ]
            } else {
                [
                    GlassLayer::new(theme.bg, window, 0),
                    // The settings window is one sheet as well: its content
                    // area is the same glass as the navigation beside it.
                    GlassLayer {
                        key: theme.surface & 0x00ff_ffff,
                        ..GlassLayer::new(theme.bg, window, 0)
                    },
                    GlassLayer::new(theme.control, card, raised),
                    GlassLayer::new(theme.field, input, veil),
                ]
            },
            ground: 0,
            text: theme.text & 0x00ff_ffff,
        }
    }

    pub(super) fn prepare(&self, pixels: &mut [u8], width: usize) {
        if width == 0 {
            return;
        }
        let channels = |color: u32| [(color >> 16) & 255, (color >> 8) & 255, color & 255];
        let text = channels(self.text);
        let layer = |pixel: &[u8; 4]| {
            let color = u32::from(pixel[2]) | u32::from(pixel[1]) << 8 | u32::from(pixel[0]) << 16;
            self.layers.iter().copied().find(|layer| layer.key == color)
        };
        // A row covered from edge to edge, such as a full-width hairline or
        // the middle of a selection, lies on the background of the rows
        // around it. Alone it would turn opaque between translucent rows.
        let mut around = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .find_map(layer)
            .unwrap_or(self.layers[self.ground]);
        for row in pixels.chunks_exact_mut(width * 4) {
            // Content before the first background run belongs to that run.
            let mut under = row
                .as_chunks::<4>()
                .0
                .iter()
                .find_map(layer)
                .unwrap_or(around);
            for pixel in row.as_chunks_mut::<4>().0 {
                if let Some(found) = layer(pixel) {
                    under = found;
                    pixel[..3].copy_from_slice(&found.tint.map(|value| value as u8));
                    pixel[3] = found.alpha as u8;
                    continue;
                }
                let background = channels(under.key);
                let mut distance = 0;
                let mut range = 1;
                for index in 0..3 {
                    distance = distance.max(u32::from(pixel[index]).abs_diff(background[index]));
                    range = range.max(text[index].abs_diff(background[index]));
                }
                let coverage = (distance * 255 / range).min(255);
                let rest = 255 - coverage;
                let result = coverage + under.alpha * rest / 255;
                for index in 0..3 {
                    // Swap the uncovered share of the opaque background for
                    // the same share of its glass layer.
                    let content =
                        (u32::from(pixel[index]) * 255).saturating_sub(background[index] * rest);
                    let value = (content + under.tint[index] * rest) / 255;
                    pixel[index] = value.min(result) as u8;
                }
                pixel[3] = result as u8;
            }
            around = under;
        }
    }
}

pub(super) unsafe fn glass_palette(dc: HDC) -> Option<GlassPalette> {
    unsafe { glass_palette_for_window(WindowFromDC(dc)) }
}

pub(super) unsafe fn glass_palette_for_window(hwnd: HWND) -> Option<GlassPalette> {
    unsafe {
        if SOLID_RENDER.with(Cell::get) != 0 || hwnd.is_null() {
            return None;
        }
        let role = glass_role(hwnd);
        if role == 0 {
            return None;
        }
        let dark = PALETTE.with(Cell::get).is_some_and(|palette| palette.dark);
        let mut palette = GlassPalette::new(design::Theme::new(dark), &current(), role == 2);
        if role == 1 && on_card(hwnd) {
            palette.ground = 1;
        }
        Some(palette)
    }
}

/// Settings controls lie on the window or on the page card inside it.
unsafe fn on_card(hwnd: HWND) -> bool {
    unsafe {
        let surface = wide("LiteTray.Surface");
        let root = GetAncestor(hwnd, GA_ROOT);
        let mut window = hwnd;
        while !window.is_null() && window != root {
            if !GetPropW(window, surface.as_ptr()).is_null() {
                return true;
            }
            window = GetAncestor(window, GA_PARENT);
        }
        false
    }
}

#[cfg(test)]
pub(super) fn glass_preview(pixels: &mut [u8], width: usize, flyout: bool) {
    let dark = PALETTE.with(Cell::get).is_some_and(|palette| palette.dark);
    GlassPalette::new(design::Theme::new(dark), &current(), flyout).prepare(pixels, width);
}

// RGB exports do not pass through DWM. The same native painters can render
// them without premultiplication, including nested owner-drawn controls.
#[cfg(test)]
pub(super) struct SolidRender;
#[cfg(test)]
impl SolidRender {
    pub(super) fn begin() -> Self {
        SOLID_RENDER.with(|depth| depth.set(depth.get() + 1));
        Self
    }
}
#[cfg(test)]
impl Drop for SolidRender {
    fn drop(&mut self) {
        SOLID_RENDER.with(|depth| depth.set(depth.get() - 1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixels(colors: &[u32]) -> Vec<u8> {
        colors
            .iter()
            .flat_map(|&color| [(color >> 16) as u8, (color >> 8) as u8, color as u8, 0])
            .collect()
    }

    #[test]
    fn glass_backgrounds_are_independent_and_glyphs_remain_opaque() {
        for dark in [false, true] {
            let config = InterfaceConfig {
                style: InterfaceStyle::LiquidGlass,
                transparency: 85,
                card_clarity: 25,
                ..Default::default()
            };
            configure(&config);
            let theme = design::Theme::new(dark);
            let mut row = pixels(&[theme.bg, theme.surface, theme.field, theme.text]);
            GlassPalette::new(theme, &config, false).prepare(&mut row, 4);
            assert_eq!(row[3], config.background_alpha());
            assert_eq!(row[..4], row[4..8], "content area and window are one sheet");
            assert!(row[11] > row[7]);
            assert_eq!(row[15], 255);
            for pixel in row.as_chunks::<4>().0 {
                assert!(pixel[..3].iter().all(|&value| value <= pixel[3]));
            }
            // The flyout sheet follows the window slider; its inputs the card.
            let mut row = pixels(&[theme.surface, theme.field]);
            GlassPalette::new(theme, &config, true).prepare(&mut row, 2);
            assert_eq!(row[3], config.background_alpha());
            assert!(row[7] > config.card_alpha());
        }
        configure(&InterfaceConfig::default());
    }

    #[test]
    fn glass_edges_fade_between_background_and_content() {
        let config = InterfaceConfig {
            style: InterfaceStyle::LiquidGlass,
            transparency: 80,
            card_clarity: 20,
            ..Default::default()
        };
        configure(&config);
        for dark in [false, true] {
            let theme = design::Theme::new(dark);
            // A glyph edge: card, then 25/50/75% text coverage, then text.
            let colors = [0, 25, 50, 75, 100].map(|k| design::blend(theme.surface, theme.text, k));
            let mut row = pixels(&colors);
            GlassPalette::new(theme, &config, false).prepare(&mut row, 5);
            let card = [0, 1, 2].map(|channel| (u32::from(row[channel]), u32::from(row[3])));
            let mut previous = u32::from(row[3]);
            for (index, pixel) in row.as_chunks::<4>().0.iter().enumerate() {
                assert!(u32::from(pixel[3]) >= previous, "alpha rises with coverage");
                previous = u32::from(pixel[3]);
                // Over a backdrop equal to the card color the composite must
                // reproduce the opaque rendering: no dark or light halo.
                for (channel, shift) in [16, 8, 0].into_iter().enumerate() {
                    assert!(pixel[channel] <= pixel[3]);
                    // Choose the backdrop over which the bare card equals its
                    // opaque color; every edge pixel must then match as well.
                    let surface = (theme.surface >> shift) & 255;
                    let (tint, alpha) = card[channel];
                    let backdrop = (surface.saturating_sub(tint)) * 255 / (255 - alpha);
                    let expected = (colors[index] >> shift) & 255;
                    let composite =
                        u32::from(pixel[channel]) + backdrop * (255 - u32::from(pixel[3])) / 255;
                    assert!(composite.abs_diff(expected) <= 4, "{composite} {expected}");
                }
            }
            assert_eq!(row[19], 255);
        }
        configure(&InterfaceConfig::default());
    }

    #[test]
    fn covered_rows_are_as_translucent_as_the_rows_around_them() {
        let config = InterfaceConfig {
            style: InterfaceStyle::LiquidGlass,
            ..Default::default()
        };
        configure(&config);
        for (dark, flyout) in [(false, false), (false, true), (true, false), (true, true)] {
            let theme = design::Theme::new(dark);
            let mut palette = GlassPalette::new(theme, &config, flyout);
            let ground = if flyout { theme.surface } else { theme.bg };
            // A rounded selection shows its background only beside the
            // corners; a hairline may span the whole width.
            let mut rows = pixels(
                &[
                    [ground, theme.tint, theme.tint, ground],
                    [theme.tint; 4],
                    [theme.border; 4],
                    [ground, theme.border, theme.tint, ground],
                ]
                .concat(),
            );
            palette.prepare(&mut rows, 4);
            let pixel = |index: usize| &rows[index * 4..index * 4 + 4];
            assert!(pixel(1)[3] < 255, "a selection stays glass");
            for covered in 4..8 {
                assert_eq!(pixel(covered), pixel(1));
            }
            for covered in 8..12 {
                assert_eq!(pixel(covered), pixel(13));
            }
            // Square corners leave no background at all: the control's own
            // ground decides, the window or the card inside it.
            for (index, under) in [(0, ground), (1, theme.surface)] {
                if flyout && index == 1 {
                    continue;
                }
                palette.ground = index;
                let mut bare = pixels(&[theme.tint; 2]);
                palette.prepare(&mut bare, 2);
                let mut beside = pixels(&[under, theme.tint]);
                palette.prepare(&mut beside, 2);
                assert_eq!(bare[..4], beside[4..]);
                assert_eq!(bare[4..], beside[4..]);
            }
        }
        configure(&InterfaceConfig::default());
    }
}
