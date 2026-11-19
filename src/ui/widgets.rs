//! Native settings controls: installed-font pickers, multiline editing and
//! clock previews. Input, selection and accessibility remain native controls.
use super::*;
use std::sync::OnceLock;

pub(super) unsafe fn divider(hwnd: HWND) {
    unsafe {
        SetWindowSubclass(hwnd, Some(divider_proc), 19, 0);
    }
}
unsafe extern "system" fn divider_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    _: usize,
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
                let t = design::Theme::new(PALETTE.with(Cell::get).is_some_and(|p| p.dark));
                design::fill(dc, r, t.border);
                if msg != WM_PRINTCLIENT {
                    EndPaint(hwnd, &ps);
                }
                return 0;
            }
            WM_ERASEBKGND => return 1,
            WM_NCDESTROY => {
                RemoveWindowSubclass(hwnd, Some(divider_proc), id);
            }
            _ => {}
        }
        DefSubclassProc(hwnd, msg, w, l)
    }
}

pub(super) unsafe fn draw_list(item: &DRAWITEMSTRUCT, dark: bool) {
    unsafe {
        let t = design::Theme::new(dark);
        let selected = item.itemState & ODS_SELECTED != 0;
        design::fill(
            item.hDC,
            item.rcItem,
            if selected { t.tint } else { t.field },
        );
        let count = SendMessageW(item.hwndItem, LB_GETTEXTLEN, item.itemID as usize, 0);
        if (0..=4096).contains(&count) {
            let mut value = vec![0u16; count as usize + 1];
            let len = SendMessageW(
                item.hwndItem,
                LB_GETTEXT,
                item.itemID as usize,
                value.as_mut_ptr() as isize,
            );
            if len >= 0 {
                let mut r = item.rcItem;
                r.left += 10;
                r.right -= 10;
                design::text(
                    item.hDC,
                    &String::from_utf16_lossy(&value[..len as usize]),
                    r,
                    SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as HFONT,
                    if selected { t.accent } else { t.text },
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
            }
        }
        if item.itemState & ODS_FOCUS != 0 {
            DrawFocusRect(item.hDC, &item.rcItem);
        }
    }
}

pub(super) fn fonts() -> &'static [String] {
    static FONTS: OnceLock<Vec<String>> = OnceLock::new();
    FONTS.get_or_init(|| unsafe {
        #[cfg(test)]
        let _timing = navigation_timing::scope(navigation_timing::Phase::FontEnumeration);
        unsafe extern "system" fn collect(
            font: *const LOGFONTW,
            _: *const TEXTMETRICW,
            _: u32,
            data: LPARAM,
        ) -> i32 {
            unsafe {
                let face = &(*font).lfFaceName;
                let value = String::from_utf16_lossy(
                    &face[..face.iter().position(|c| *c == 0).unwrap_or(face.len())],
                );
                if !value.is_empty() && !value.starts_with('@') {
                    (&mut *(data as *mut Vec<String>)).push(value);
                }
                1
            }
        }
        let mut result: Vec<String> = Vec::new();
        let dc = GetDC(null_mut());
        let mut font: LOGFONTW = std::mem::zeroed();
        font.lfCharSet = DEFAULT_CHARSET;
        EnumFontFamiliesExW(dc, &font, Some(collect), &mut result as *mut _ as isize, 0);
        ReleaseDC(null_mut(), dc);
        result.sort_by_key(|name| name.to_lowercase());
        result.dedup();
        result
    })
}

/// Enumerates the installed fonts off the UI thread, before a picker asks.
pub(super) fn prewarm_fonts() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        let _ = thread::Builder::new()
            .name("LiteTray fonts".into())
            .spawn(|| {
                fonts();
            });
    });
}

const FONT_LIST: &str = "LiteTray.FontList";
/// Marks a picker holding only its `head` entry and its current choice, and
/// adds the installed names when it is first focused, opened or queried.
/// Selection, keyboard access and UIA stay those of the native combo box.
pub(super) unsafe fn font_list(hwnd: HWND, head: bool) {
    unsafe {
        SetPropW(
            hwnd,
            wide(FONT_LIST).as_ptr(),
            (1 + usize::from(head)) as HANDLE,
        );
        SetWindowSubclass(hwnd, Some(font_list_proc), 31, 0);
    }
}
#[cfg(test)]
pub(super) unsafe fn font_list_pending(hwnd: HWND) -> bool {
    unsafe { !GetPropW(hwnd, wide(FONT_LIST).as_ptr()).is_null() }
}
unsafe fn fill_font_list(hwnd: HWND) {
    unsafe {
        let pending = wide(FONT_LIST);
        let kind = GetPropW(hwnd, pending.as_ptr()) as usize;
        if kind == 0 {
            return;
        }
        // Cleared first: the messages below pass through `font_list_proc`.
        RemovePropW(hwnd, pending.as_ptr());
        let head = (kind == 2).then(|| design::combo_text(hwnd, 0));
        let count = SendMessageW(hwnd, CB_GETCOUNT, 0, 0);
        let value = (count > isize::from(head.is_some()))
            .then(|| design::combo_text(hwnd, count as i32 - 1));
        let selected = design::combo_text(hwnd, SendMessageW(hwnd, CB_GETCURSEL, 0, 0) as i32);
        let installed = fonts();
        // A configured font that is no longer installed stays selectable.
        let names = head
            .iter()
            .chain(value.iter().filter(|value| !installed.contains(*value)))
            .chain(installed)
            .collect::<Vec<_>>();
        SendMessageW(hwnd, WM_SETREDRAW, 0, 0);
        SendMessageW(hwnd, CB_RESETCONTENT, 0, 0);
        let bytes = names
            .iter()
            .map(|name| (name.encode_utf16().count() + 1) * 2)
            .sum::<usize>();
        SendMessageW(hwnd, CB_INITSTORAGE, names.len(), bytes as isize);
        for name in &names {
            SendMessageW(hwnd, CB_ADDSTRING, 0, wide(name).as_ptr() as isize);
        }
        SendMessageW(
            hwnd,
            CB_SETCURSEL,
            names
                .iter()
                .position(|name| **name == selected)
                .unwrap_or(0),
            0,
        );
        SendMessageW(hwnd, WM_SETREDRAW, 1, 0);
        InvalidateRect(hwnd, null(), 0);
    }
}
unsafe extern "system" fn font_list_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        match msg {
            // Everything that can move the selection, show the list or
            // report its rows, including an accessibility client asking for
            // the control. Painting and reading the current choice do not
            // need the installed names.
            WM_SETFOCUS | WM_KEYDOWN | WM_SYSKEYDOWN | WM_CHAR | WM_LBUTTONDOWN
            | WM_LBUTTONDBLCLK | WM_MOUSEWHEEL | WM_GETOBJECT | CB_SHOWDROPDOWN | CB_GETCOUNT
            | CB_FINDSTRING | CB_FINDSTRINGEXACT | CB_SELECTSTRING => fill_font_list(hwnd),
            WM_NCDESTROY => {
                RemovePropW(hwnd, wide(FONT_LIST).as_ptr());
                RemoveWindowSubclass(hwnd, Some(font_list_proc), id);
            }
            _ => {}
        }
        DefSubclassProc(hwnd, msg, w, l)
    }
}

pub(super) const FIELD_PAD: &str = "LiteTray.FieldPad";
/// A single-line edit draws its text at the top of its client area. Shrink
/// the control to one line in the middle of its box; the parent, which
/// paints the box, extends it again by the recorded padding.
pub(super) unsafe fn center_edit(hwnd: HWND) {
    unsafe {
        if GetWindowLongW(hwnd, GWL_STYLE) as u32 & ES_MULTILINE as u32 != 0 {
            return;
        }
        let dc = GetDC(hwnd);
        let old = SelectObject(dc, SendMessageW(hwnd, WM_GETFONT, 0, 0) as HGDIOBJ);
        let mut metrics: TEXTMETRICW = std::mem::zeroed();
        GetTextMetricsW(dc, &mut metrics);
        SelectObject(dc, old);
        ReleaseDC(hwnd, dc);
        let parent = GetParent(hwnd);
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(hwnd, &mut r);
        MapWindowPoints(null_mut(), parent, &mut r as *mut RECT as *mut POINT, 2);
        let pad = (r.bottom - r.top - metrics.tmHeight) / 2;
        if pad <= 0 {
            return;
        }
        SetPropW(hwnd, wide(FIELD_PAD).as_ptr(), pad as usize as HANDLE);
        SetWindowPos(
            hwnd,
            null_mut(),
            r.left,
            r.top + pad,
            r.right - r.left,
            r.bottom - r.top - 2 * pad,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}
/// The box painted around a field: its window plus `center_edit` padding.
pub(super) unsafe fn field_box(child: HWND, parent: HWND) -> RECT {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        GetWindowRect(child, &mut r);
        MapWindowPoints(null_mut(), parent, &mut r as *mut RECT as *mut POINT, 2);
        let pad = GetPropW(child, wide(FIELD_PAD).as_ptr()) as i32;
        r.top -= pad;
        r.bottom += pad;
        r
    }
}

pub(super) unsafe fn style_edit(hwnd: HWND, dark: bool, dpi: u32) {
    unsafe {
        SetWindowTheme(
            hwnd,
            wide(if dark {
                "DarkMode_Explorer"
            } else {
                "Explorer"
            })
            .as_ptr(),
            null(),
        );
        SetWindowSubclass(hwnd, Some(edit_proc), 15, dpi as usize);
        format_edit(hwnd, dpi);
        glass_control(hwnd);
    }
}

pub(super) unsafe fn style_static(hwnd: HWND) {
    unsafe {
        SetWindowSubclass(hwnd, Some(static_proc), 25, 0);
    }
}

unsafe extern "system" fn static_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        if matches!(msg, WM_PAINT | WM_PRINTCLIENT) && interface::glass_active(hwnd) {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = if msg == WM_PRINTCLIENT {
                w as HDC
            } else {
                BeginPaint(hwnd, &mut ps)
            };
            let mut bounds: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut bounds);
            let buffer = design::PaintBuffer::new(dc, bounds);
            let dc = buffer.as_ref().map_or(dc, |buffer| buffer.dc);
            let theme =
                design::Theme::new(PALETTE.with(Cell::get).is_some_and(|palette| palette.dark));
            let surface = !GetPropW(hwnd, wide("LiteTray.Surface").as_ptr()).is_null();
            design::fill(dc, bounds, if surface { theme.surface } else { theme.bg });
            let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            let flags = match style & 3 {
                1 => DT_CENTER,
                2 => DT_RIGHT,
                _ => DT_LEFT,
            } | if style & 0x4000 != 0 {
                DT_SINGLELINE | DT_END_ELLIPSIS
            } else {
                DT_WORDBREAK
            };
            let color = if !GetPropW(hwnd, wide("LiteTray.Muted").as_ptr()).is_null() {
                theme.muted
            } else {
                theme.text
            };
            design::text(
                dc,
                &window_text(hwnd),
                bounds,
                SendMessageW(hwnd, WM_GETFONT, 0, 0) as HFONT,
                color,
                flags,
            );
            drop(buffer);
            if msg != WM_PRINTCLIENT {
                EndPaint(hwnd, &ps);
            }
            return 0;
        }
        if msg == WM_SETTEXT && interface::glass_active(hwnd) {
            // A static control draws its new text at once, outside WM_PAINT
            // and with an opaque background. Let only the glass paint draw.
            DefSubclassProc(hwnd, WM_SETREDRAW, 0, 0);
            let result = DefSubclassProc(hwnd, msg, w, l);
            DefSubclassProc(hwnd, WM_SETREDRAW, 1, 0);
            InvalidateRect(hwnd, null(), 0);
            return result;
        }
        if msg == WM_NCDESTROY {
            RemoveWindowSubclass(hwnd, Some(static_proc), id);
        }
        DefSubclassProc(hwnd, msg, w, l)
    }
}

/// Native editing, checkbox interaction and UIA remain in the original
/// control. Stage its paint so GDI's zero alpha cannot erase dark glyphs.
pub(super) unsafe fn glass_control(hwnd: HWND) {
    unsafe {
        SetWindowSubclass(hwnd, Some(glass_control_proc), 26, 0);
    }
}

unsafe extern "system" fn glass_control_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        if matches!(msg, WM_PAINT | WM_PRINTCLIENT) && interface::glass_active(hwnd) {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = if msg == WM_PRINTCLIENT {
                w as HDC
            } else {
                BeginPaint(hwnd, &mut ps)
            };
            let mut bounds: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut bounds);
            let buffer = design::PaintBuffer::new(dc, bounds);
            let memory = buffer.as_ref().map_or(dc, |buffer| buffer.dc);
            let theme =
                design::Theme::new(PALETTE.with(Cell::get).is_some_and(|palette| palette.dark));
            let field = !GetPropW(hwnd, wide("LiteTray.Field").as_ptr()).is_null();
            design::fill(
                memory,
                bounds,
                if field { theme.field } else { theme.surface },
            );
            let result =
                DefSubclassProc(hwnd, WM_PRINTCLIENT, memory as usize, PRF_CLIENT as isize);
            drop(buffer);
            if msg != WM_PRINTCLIENT {
                EndPaint(hwnd, &ps);
            }
            return result;
        }
        if msg == WM_NCDESTROY {
            RemoveWindowSubclass(hwnd, Some(glass_control_proc), id);
        }
        DefSubclassProc(hwnd, msg, w, l)
    }
}

pub(super) unsafe fn slider(hwnd: HWND) {
    unsafe {
        SetWindowSubclass(hwnd, Some(slider_proc), 27, 0);
    }
}

unsafe extern "system" fn slider_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        if matches!(msg, WM_PAINT | WM_PRINTCLIENT) {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = if msg == WM_PRINTCLIENT {
                w as HDC
            } else {
                BeginPaint(hwnd, &mut ps)
            };
            let mut bounds: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut bounds);
            let buffer = design::PaintBuffer::new(dc, bounds);
            let dc = buffer.as_ref().map_or(dc, |buffer| buffer.dc);
            let theme =
                design::Theme::new(PALETTE.with(Cell::get).is_some_and(|palette| palette.dark));
            let scale = interface::layout_dpi(GetDpiForWindow(hwnd).max(96)) as i32;
            let s = |value: i32| (value * scale / 96).max(1);
            design::fill(dc, bounds, theme.surface);
            let mut channel: RECT = std::mem::zeroed();
            let mut thumb: RECT = std::mem::zeroed();
            SendMessageW(hwnd, TBM_GETCHANNELRECT, 0, &mut channel as *mut _ as isize);
            SendMessageW(hwnd, TBM_GETTHUMBRECT, 0, &mut thumb as *mut _ as isize);
            let y = bounds.bottom / 2;
            let channel = design::rect(channel.left, y - s(2), channel.right - channel.left, s(4));
            design::rounded(dc, channel, theme.border, theme.border, s(4));
            let center = (thumb.left + thumb.right) / 2;
            let progress = design::rect(
                channel.left,
                channel.top,
                (center - channel.left).max(0),
                channel.bottom - channel.top,
            );
            let enabled = IsWindowEnabled(hwnd) != 0;
            design::rounded(
                dc,
                progress,
                if enabled { theme.accent } else { theme.muted },
                if enabled { theme.accent } else { theme.muted },
                s(4),
            );
            // A round handle with an accent core, kept inside the control.
            let radius = s(9).min(bounds.bottom / 2);
            let center = center.clamp(radius, (bounds.right - radius).max(radius));
            let handle = design::rect(center - radius, y - radius, 2 * radius, 2 * radius);
            design::rounded(
                dc,
                handle,
                theme.control,
                if GetFocus() == hwnd {
                    theme.accent
                } else {
                    theme.control_border
                },
                2 * radius,
            );
            let core = radius - s(4).min(radius - 1);
            let color = if enabled { theme.accent } else { theme.muted };
            design::rounded(
                dc,
                design::rect(center - core, y - core, 2 * core, 2 * core),
                color,
                color,
                2 * core,
            );
            drop(buffer);
            if msg != WM_PRINTCLIENT {
                EndPaint(hwnd, &ps);
            }
            return 0;
        }
        if msg == WM_ERASEBKGND {
            return 1;
        }
        if msg == WM_NCDESTROY {
            RemoveWindowSubclass(hwnd, Some(slider_proc), id);
        }
        let result = DefSubclassProc(hwnd, msg, w, l);
        if matches!(
            msg,
            WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE | TBM_SETPOS | WM_MOUSEMOVE | WM_KEYDOWN
        ) {
            InvalidateRect(hwnd, null(), 0);
        }
        result
    }
}
unsafe fn format_edit(hwnd: HWND, dpi: u32) {
    unsafe {
        if GetWindowLongW(hwnd, GWL_STYLE) as u32 & ES_MULTILINE as u32 == 0 {
            return;
        }
        if !GetPropW(hwnd, wide("LiteTray.EditLayout").as_ptr()).is_null() {
            return;
        }
        SetPropW(hwnd, wide("LiteTray.EditLayout").as_ptr(), 1usize as HANDLE);
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut r);
        let dc = GetDC(hwnd);
        let old = SelectObject(dc, SendMessageW(hwnd, WM_GETFONT, 0, 0) as HGDIOBJ);
        let mut metrics: TEXTMETRICW = std::mem::zeroed();
        GetTextMetricsW(dc, &mut metrics);
        SelectObject(dc, old);
        ReleaseDC(hwnd, dc);
        let margin = (8 * dpi / 96) as i32;
        let lines = SendMessageW(hwnd, EM_GETLINECOUNT, 0, 0) as i32;
        ShowScrollBar(
            hwnd,
            SB_VERT,
            (lines * metrics.tmHeight > r.bottom - margin * 2) as i32,
        );
        GetClientRect(hwnd, &mut r);
        InflateRect(&mut r, -margin, -margin);
        SendMessageW(hwnd, EM_SETRECTNP, 0, &r as *const _ as isize);
        RemovePropW(hwnd, wide("LiteTray.EditLayout").as_ptr());
    }
}
unsafe extern "system" fn edit_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    dpi: usize,
) -> LRESULT {
    unsafe {
        let result = DefSubclassProc(hwnd, msg, w, l);
        match msg {
            WM_SETTEXT | WM_SIZE | WM_CHAR | WM_PASTE | WM_CUT | WM_CLEAR | EM_REPLACESEL
            | WM_SETFONT => format_edit(hwnd, dpi as u32),
            WM_SETFOCUS | WM_KILLFOCUS => {
                InvalidateRect(GetParent(hwnd), null(), 0);
            }
            WM_NCDESTROY => {
                RemovePropW(hwnd, wide(FIELD_PAD).as_ptr());
                RemoveWindowSubclass(hwnd, Some(edit_proc), id);
            }
            _ => {}
        }
        result
    }
}

pub(super) unsafe fn heading(hwnd: HWND, dpi: u32) {
    unsafe {
        let font = make_font(interface::text_pixels(16, dpi), 600, &design::font_face());
        SendMessageW(hwnd, WM_SETFONT, font as usize, 0);
        SetWindowSubclass(hwnd, Some(font_proc), 16, font as usize);
    }
}
unsafe extern "system" fn font_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    font: usize,
) -> LRESULT {
    unsafe {
        let result = DefSubclassProc(hwnd, msg, w, l);
        if msg == WM_NCDESTROY {
            RemoveWindowSubclass(hwnd, Some(font_proc), id);
            DeleteObject(font as HGDIOBJ);
        }
        result
    }
}

pub(super) unsafe fn caption_font(hwnd: HWND, dpi: u32) {
    unsafe {
        let font = make_font(interface::text_pixels(12, dpi), 400, &design::font_face());
        SendMessageW(hwnd, WM_SETFONT, font as usize, 0);
        SetWindowSubclass(hwnd, Some(font_proc), 16, font as usize);
    }
}

pub(super) const CARD_GROUP: &str = "LiteTray.CardGroup";
/// Marks an invisible placeholder whose bounds the page paints as one
/// outlined group of `rows` equal settings rows. The group has no window
/// surface of its own, so the labels and inputs above it never flicker.
pub(super) unsafe fn card_group(hwnd: HWND, rows: usize) {
    unsafe {
        ShowWindow(hwnd, SW_HIDE);
        SetPropW(hwnd, wide(CARD_GROUP).as_ptr(), rows.max(1) as HANDLE);
        SetWindowSubclass(hwnd, Some(card_group_proc), 29, 0);
    }
}
unsafe extern "system" fn card_group_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        if msg == WM_NCDESTROY {
            RemovePropW(hwnd, wide(CARD_GROUP).as_ptr());
            RemoveWindowSubclass(hwnd, Some(card_group_proc), id);
        }
        DefSubclassProc(hwnd, msg, w, l)
    }
}
pub(super) unsafe fn paint_card_groups(hwnd: HWND, dc: HDC, theme: design::Theme) {
    unsafe {
        let scale = interface::layout_dpi(GetDpiForWindow(hwnd).max(96)) as i32;
        let mut child = GetWindow(hwnd, GW_CHILD);
        while !child.is_null() {
            let rows = GetPropW(child, wide(CARD_GROUP).as_ptr()) as i32;
            if rows > 0 {
                let mut r: RECT = std::mem::zeroed();
                GetWindowRect(child, &mut r);
                MapWindowPoints(null_mut(), hwnd, &mut r as *mut RECT as *mut POINT, 2);
                // On glass the rows are parted by hairlines alone.
                if !interface::glass_active(hwnd) {
                    design::card(dc, r, theme.surface, theme.border, 12 * scale / 96);
                }
                for row in 1..rows {
                    design::fill(
                        dc,
                        design::rect(
                            r.left + 1,
                            r.top + (r.bottom - r.top) * row / rows,
                            r.right - r.left - 2,
                            1,
                        ),
                        theme.border,
                    );
                }
            }
            child = GetWindow(child, GW_HWNDNEXT);
        }
    }
}

pub(super) const PREVIEW: u16 = 820;
struct PreviewData {
    clock: config::ClockConfig,
    dark: bool,
    dpi: u32,
    fonts: [HFONT; 3],
}
impl Drop for PreviewData {
    fn drop(&mut self) {
        unsafe {
            for font in self.fonts {
                DeleteObject(font);
            }
        }
    }
}
pub(super) unsafe fn preview(hwnd: HWND, config: &config::ClockConfig, dark: bool, dpi: u32) {
    unsafe {
        let mut data = Box::new(PreviewData {
            clock: config.clone(),
            dark,
            dpi,
            fonts: [
                make_font(interface::text_pixels(12, dpi), 400, &design::font_face()),
                make_font(
                    (config.top_style.font_size.unwrap_or(config.font_size) * dpi / 96) as i32,
                    400,
                    config
                        .top_style
                        .font_family
                        .as_deref()
                        .unwrap_or(&config.font_family),
                ),
                make_font(
                    (config.bottom_style.font_size.unwrap_or(config.font_size) * dpi / 96) as i32,
                    400,
                    config
                        .bottom_style
                        .font_family
                        .as_deref()
                        .unwrap_or(&config.font_family),
                ),
            ],
        });
        // Fit both lines by the same ratio so large configured fonts remain
        // legible in the compact sample without clipping their ascenders.
        let dc = GetDC(hwnd);
        let mut heights = [
            (config.top_style.font_size.unwrap_or(config.font_size) * dpi / 96) as i32,
            (config.bottom_style.font_size.unwrap_or(config.font_size) * dpi / 96) as i32,
        ];
        let families = [
            config
                .top_style
                .font_family
                .as_deref()
                .unwrap_or(&config.font_family),
            config
                .bottom_style
                .font_family
                .as_deref()
                .unwrap_or(&config.font_family),
        ];
        let available = (28 * dpi / 96) as i32;
        for _ in 0..3 {
            let mut largest = 0;
            for font in &data.fonts[1..] {
                let old = SelectObject(dc, *font);
                let mut metrics: TEXTMETRICW = std::mem::zeroed();
                GetTextMetricsW(dc, &mut metrics);
                SelectObject(dc, old);
                largest = largest.max(metrics.tmHeight);
            }
            if largest <= available {
                break;
            }
            for index in 0..2 {
                heights[index] = (heights[index] * available / largest).max(1);
                let replacement = make_font(heights[index], 400, families[index]);
                DeleteObject(std::mem::replace(&mut data.fonts[index + 1], replacement));
            }
        }
        ReleaseDC(hwnd, dc);
        SetWindowSubclass(hwnd, Some(preview_proc), 17, Box::into_raw(data) as usize);
    }
}
pub(super) unsafe fn update_preview(hwnd: HWND, config: &config::ClockConfig) {
    unsafe {
        let mut pointer = 0;
        if GetWindowSubclass(hwnd, Some(preview_proc), 17, &mut pointer) == 0 {
            return;
        }
        let previous = Box::from_raw(pointer as *mut PreviewData);
        let dark = previous.dark;
        let dpi = previous.dpi;
        // Replace data while no paint is running; SetWindowSubclass retains the
        // same callback and changes only its owned per-control state pointer.
        preview(hwnd, config, dark, dpi);
        drop(previous);
        InvalidateRect(hwnd, null(), 0);
    }
}
#[cfg(test)]
pub(super) unsafe fn preview_clock(hwnd: HWND) -> config::ClockConfig {
    unsafe {
        let mut data = 0;
        assert_ne!(
            GetWindowSubclass(hwnd, Some(preview_proc), 17, &mut data),
            0
        );
        (*(data as *const PreviewData)).clock.clone()
    }
}
#[cfg(test)]
pub(super) unsafe fn preview_line_heights(hwnd: HWND) -> (i32, [i32; 2]) {
    unsafe {
        let mut data = 0;
        assert_ne!(
            GetWindowSubclass(hwnd, Some(preview_proc), 17, &mut data),
            0
        );
        let data = &*(data as *const PreviewData);
        let dc = GetDC(hwnd);
        let heights = [1, 2].map(|line| {
            let old = SelectObject(dc, data.fonts[line]);
            let mut metrics: TEXTMETRICW = std::mem::zeroed();
            GetTextMetricsW(dc, &mut metrics);
            SelectObject(dc, old);
            metrics.tmHeight
        });
        ReleaseDC(hwnd, dc);
        ((28 * data.dpi / 96) as i32, heights)
    }
}
unsafe extern "system" fn preview_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT {
    unsafe {
        if msg == WM_NCDESTROY {
            RemoveWindowSubclass(hwnd, Some(preview_proc), id);
            drop(Box::from_raw(data as *mut PreviewData));
        } else if msg == WM_PAINT || msg == WM_PRINTCLIENT {
            let data = &*(data as *const PreviewData);
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = if msg == WM_PRINTCLIENT {
                w as HDC
            } else {
                BeginPaint(hwnd, &mut ps)
            };
            let t = design::Theme::new(data.dark);
            let mut r: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut r);
            let buffer = design::PaintBuffer::new(dc, r);
            let dc = buffer.as_ref().map_or(dc, |b| b.dc);
            let s = |n| n * data.dpi as i32 / 96;
            design::fill(dc, r, t.surface);
            design::rounded(dc, r, t.bg, t.border, s(10));
            design::text(
                dc,
                tr("任务栏时钟预览"),
                design::rect(s(18), s(10), s(190), s(23)),
                data.fonts[0],
                t.muted,
                DT_LEFT | DT_SINGLELINE,
            );
            design::text(
                dc,
                tr("示例天气 · 保存后应用"),
                design::rect(s(18), s(38), s(220), s(22)),
                data.fonts[0],
                t.muted,
                DT_LEFT | DT_SINGLELINE,
            );
            let now = Local::now();
            let frame = clock::frame_with_weather(
                &data.clock,
                Some(&clock::weather_format::sample(now)),
                now,
            );
            let color = |value: &str| {
                if t.high_contrast {
                    return t.text;
                }
                let hex = value.trim_start_matches('#');
                u32::from_str_radix(if hex.len() == 8 { &hex[2..] } else { hex }, 16)
                    .map(|v| rgb(v >> 16 & 255, v >> 8 & 255, v & 255))
                    .unwrap_or(t.text)
            };
            let alignment = |line: &litetray_adapter_protocol::clock::LineStyle| match line
                .alignment
                .unwrap_or(data.clock.alignment)
            {
                litetray_adapter_protocol::clock::Alignment::Left => DT_LEFT,
                litetray_adapter_protocol::clock::Alignment::Center => DT_CENTER,
                _ => DT_RIGHT,
            };
            let area = design::rect(s(250), s(8), r.right - s(274), s(28));
            design::text(
                dc,
                &frame.text.top,
                area,
                data.fonts[1],
                color(&data.clock.top_color),
                alignment(&data.clock.top_style) | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
            );
            design::text(
                dc,
                &frame.text.bottom,
                design::rect(area.left, s(36), area.right - area.left, s(28)),
                data.fonts[2],
                color(&data.clock.bottom_color),
                alignment(&data.clock.bottom_style) | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
            );
            drop(buffer);
            if msg != WM_PRINTCLIENT {
                EndPaint(hwnd, &ps);
            }
            return 0;
        } else if msg == WM_ERASEBKGND {
            return 1;
        }
        DefSubclassProc(hwnd, msg, w, l)
    }
}
