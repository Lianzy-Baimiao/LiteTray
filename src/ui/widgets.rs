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
                RemoveWindowSubclass(hwnd, Some(edit_proc), id);
            }
            _ => {}
        }
        result
    }
}

pub(super) unsafe fn heading(hwnd: HWND, dpi: u32) {
    unsafe {
        let font = make_font((16 * dpi / 96) as i32, 600, &design::font_face());
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
                make_font((12 * dpi / 96) as i32, 400, &design::font_face()),
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
                "任务栏时钟预览",
                design::rect(s(18), s(10), s(190), s(23)),
                data.fonts[0],
                t.muted,
                DT_LEFT | DT_SINGLELINE,
            );
            design::text(
                dc,
                "示例天气 · 保存后应用",
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
