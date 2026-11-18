//! A single draft controls the settings shell and the click flyout. Preview
//! does not write the profile; Save and discard use the normal transaction.
use super::*;
use config::{CornerStyle, InterfaceConfig, InterfaceStyle};

pub(super) const PAGE: usize = 13;
pub(super) const FIRST_STYLE: u16 = 550;
pub(super) const CORNERS: u16 = 553;
pub(super) const FONT: u16 = 554;
pub(super) const SIZE: u16 = 555;
pub(super) const TRANSPARENCY: u16 = 556;
pub(super) const CLARITY: u16 = 557;
pub(super) const PREVIEW: u16 = 558;
pub(super) const RESET: u16 = 559;
const TRANSPARENCY_LABEL: u16 = 560;
const CLARITY_LABEL: u16 = 561;
const EFFECT_STATUS: u16 = 562;
pub(super) const PREVIEW_TIMER: usize = 87;
// TBM_GETPOS is WM_USER itself; Windows metadata omits zero-offset messages.
const TBM_GETPOS: u32 = WM_USER;
const STYLE_CARD_HEIGHT: i32 = 104;
const ROW: i32 = 40;

/// Draws one style choice as a miniature of that material in its own
/// palette, so the three cards can be compared before any is applied.
pub(super) unsafe fn draw_style_card(item: &DRAWITEMSTRUCT, dark: bool) -> bool {
    unsafe {
        let Some(style) = (item.CtlID as usize)
            .checked_sub(FIRST_STYLE as usize)
            .and_then(|index| InterfaceStyle::ALL.get(index).copied())
        else {
            return false;
        };
        let t = design::Theme::new(dark);
        let device_dpi = GetDpiForWindow(item.hwndItem).max(96);
        let dpi = interface::layout_dpi(device_dpi) as i32;
        let s = |n: i32| n * dpi / 96;
        let selected = !GetPropW(item.hwndItem, wide("LiteTray.Selected").as_ptr()).is_null();
        let hover = !GetPropW(item.hwndItem, wide("LiteTray.Hover").as_ptr()).is_null();
        let pressed = item.itemState & ODS_SELECTED != 0;
        let focus = item.itemState & (ODS_FOCUS | ODS_NOFOCUSRECT) == ODS_FOCUS;
        let r = item.rcItem;
        design::fill(item.hDC, r, t.surface);
        let face = if pressed || hover { t.tint } else { t.control };
        let outline = if selected || focus {
            t.accent
        } else {
            t.control_border
        };
        design::rounded(item.hDC, r, face, outline, s(12));
        if selected && !t.high_contrast {
            // A two-pixel ring reads as chosen without recoloring the card.
            let mut inner = r;
            InflateRect(&mut inner, -1, -1);
            design::rounded(item.hDC, inner, face, t.accent, s(12) - 2);
        }

        // Miniature: backdrop, a navigation rail and one card with content.
        let m = design::Theme::for_style(dark, style);
        let glass = style == InterfaceStyle::LiquidGlass && !m.high_contrast;
        let mini = design::rect(
            r.left + s(10),
            r.top + s(10),
            r.right - r.left - s(20),
            s(44),
        );
        let backdrop = if glass {
            design::blend(m.bg, m.accent, if dark { 22 } else { 14 })
        } else {
            m.bg
        };
        design::rounded(item.hDC, mini, backdrop, m.border, s(8));
        for line in 0..3 {
            let color = if line == 0 { m.accent } else { m.border };
            design::rounded(
                item.hDC,
                design::rect(mini.left + s(8), mini.top + s(9 + line * 11), s(26), s(4)),
                color,
                color,
                s(4),
            );
        }
        let card = design::rect(
            mini.left + s(44),
            mini.top + s(7),
            mini.right - mini.left - s(52),
            s(30),
        );
        let surface = if glass {
            design::blend(m.surface, backdrop, 30)
        } else {
            m.surface
        };
        design::rounded(item.hDC, card, surface, m.border, s(6));
        if style == InterfaceStyle::SoftSkeuomorphic && !m.high_contrast {
            let width = card.right - card.left - s(8);
            design::fill(
                item.hDC,
                design::rect(card.left + s(4), card.top + 1, width, 1),
                design::blend(surface, rgb(255, 255, 255), 45),
            );
            design::fill(
                item.hDC,
                design::rect(card.left + s(4), card.bottom - 2, width, 1),
                design::blend(surface, rgb(0, 0, 0), 9),
            );
        }
        let available = card.right - card.left - s(20);
        for (line, length, strength) in [(0, 70, 70), (1, 45, 45)] {
            let ink = if line == 0 { m.text } else { m.muted };
            let color = design::blend(surface, ink, strength);
            design::rounded(
                item.hDC,
                design::rect(
                    card.left + s(10),
                    card.top + s(8 + line * 10),
                    (available * length / 100).max(s(8)),
                    s(4),
                ),
                color,
                color,
                s(4),
            );
        }

        design::text(
            item.hDC,
            &window_text(item.hwndItem),
            design::rect(
                r.left + s(12),
                r.top + s(59),
                r.right - r.left - s(24),
                s(22),
            ),
            SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as HFONT,
            if selected { t.accent } else { t.text },
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        let caption = make_font(
            interface::text_pixels(12, device_dpi),
            400,
            &design::font_face(),
        );
        design::text(
            item.hDC,
            tr(match style {
                InterfaceStyle::RefinedNative => "清晰分区，贴近 Windows",
                InterfaceStyle::SoftSkeuomorphic => "暖色纸感，细微浮起",
                InterfaceStyle::LiquidGlass => "透出桌面，文字清晰",
            }),
            design::rect(
                r.left + s(12),
                r.top + s(80),
                r.right - r.left - s(24),
                s(18),
            ),
            caption,
            t.muted,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        DeleteObject(caption);
        true
    }
}

impl App {
    pub(super) unsafe fn interface_settings(&mut self, c: &Config, width: i32) {
        unsafe {
            let total = width - 260;
            let input = 224 + total - 16;
            self.section("界面风格", 224, 88, total - 336);
            // Page actions sit beside the first heading, as on other pages.
            self.button("预览点击弹层", PREVIEW, 224 + total - 324, 84, 156);
            self.button("恢复界面默认", RESET, 224 + total - 156, 84, 156);
            let third = (total - 24) / 3;
            for (index, style) in InterfaceStyle::ALL.into_iter().enumerate() {
                let hwnd = self.control(
                    "BUTTON",
                    style.label(),
                    FIRST_STYLE + index as u16,
                    WS_TABSTOP | BS_OWNERDRAW as u32,
                    224 + index as i32 * (third + 12),
                    122,
                    third,
                    STYLE_CARD_HEIGHT,
                );
                if c.interface.style == style {
                    SetPropW(hwnd, wide("LiteTray.Selected").as_ptr(), 1usize as HANDLE);
                }
            }

            // One setting per row: its name on the left, its input on the right.
            self.section("形状与文字", 224, 242, total);
            let group = self.control("STATIC", "", 0, SS_LEFT as u32, 224, 274, total, 4 * ROW);
            widgets::card_group(group, 4);
            let row = |index: i32| 274 + index * ROW;
            self.label("颜色模式", 240, row(0) + 8, 200, 24);
            self.combo(
                &["跟随 Windows", "浅色", "深色"],
                match c.appearance {
                    Appearance::System => 0,
                    Appearance::Light => 1,
                    Appearance::Dark => 2,
                },
                501,
                input - 220,
                row(0) + 3,
                220,
            );
            self.label("边角形状", 240, row(1) + 8, 200, 24);
            self.combo(
                &["圆角", "直角"],
                usize::from(c.interface.corners == CornerStyle::Square),
                CORNERS,
                input - 220,
                row(1) + 3,
                220,
            );
            self.label("界面字体", 240, row(2) + 8, 200, 24);
            let mut fonts = widgets::fonts().to_vec();
            if let Some(face) = &c.interface.font_family
                && !fonts.contains(face)
            {
                fonts.insert(0, face.clone());
            }
            fonts.insert(0, tr("跟随系统字体").into());
            let selected = c
                .interface
                .font_family
                .as_ref()
                .and_then(|face| fonts.iter().position(|name| name == face))
                .unwrap_or(0);
            self.combo(
                &fonts.iter().map(String::as_str).collect::<Vec<_>>(),
                selected,
                FONT,
                input - 220,
                row(2) + 3,
                220,
            );
            SendMessageW(
                self.field(FONT),
                CB_SETDROPPEDWIDTH,
                self.scale(320) as usize,
                0,
            );
            SendMessageW(self.field(FONT), CB_SETMINVISIBLE, 10, 0);
            self.label("界面字号", 240, row(3) + 8, 200, 24);
            let sizes = (InterfaceConfig::MIN_FONT_SIZE..=InterfaceConfig::MAX_FONT_SIZE)
                .map(|size| size.to_string())
                .collect::<Vec<_>>();
            self.combo(
                &sizes.iter().map(String::as_str).collect::<Vec<_>>(),
                (c.interface.font_size - InterfaceConfig::MIN_FONT_SIZE) as usize,
                SIZE,
                input - 220,
                row(3) + 3,
                220,
            );

            let glass = row(4) + 14;
            self.section("玻璃效果", 224, glass, total);
            let group = self.control(
                "STATIC",
                "",
                0,
                SS_LEFT as u32,
                224,
                glass + 32,
                total,
                2 * ROW,
            );
            widgets::card_group(group, 2);
            for (index, (id, label_id, value, label)) in [
                (
                    TRANSPARENCY,
                    TRANSPARENCY_LABEL,
                    c.interface.transparency,
                    "透明程度",
                ),
                (
                    CLARITY,
                    CLARITY_LABEL,
                    c.interface.card_clarity,
                    "控件清晰度",
                ),
            ]
            .into_iter()
            .enumerate()
            {
                let y = glass + 32 + index as i32 * ROW;
                // A native trackbar takes its accessible name from the
                // preceding label. Keep each label next to its own control
                // in creation order as well as in the visual layout.
                self.control(
                    "STATIC",
                    "",
                    label_id,
                    SS_LEFT as u32,
                    240,
                    y + 8,
                    total - 300,
                    24,
                );
                let slider = self.control(
                    "msctls_trackbar32",
                    label,
                    id,
                    WS_TABSTOP | TBS_NOTICKS,
                    input - 220,
                    y + 6,
                    220,
                    28,
                );
                SendMessageW(slider, TBM_SETRANGE, 0, 100 << 16);
                SendMessageW(slider, TBM_SETPAGESIZE, 0, 10);
                SendMessageW(slider, TBM_SETPOS, 1, value as isize);
                widgets::slider(slider);
            }
            let footer = glass + 32 + 2 * ROW;
            self.control(
                "STATIC",
                "",
                EFFECT_STATUS,
                SS_LEFT as u32,
                224,
                footer + 6,
                total,
                18,
            );
            SetPropW(
                self.field(EFFECT_STATUS),
                wide("LiteTray.Muted").as_ptr(),
                1usize as HANDLE,
            );
            widgets::caption_font(self.field(EFFECT_STATUS), self.dpi);
            self.update_interface_controls();
        }
    }

    pub(super) fn collect_interface(&self) -> Result<InterfaceConfig, String> {
        let family = self.text(FONT);
        let config = InterfaceConfig {
            style: self.draft.interface.style,
            corners: if self.selected(CORNERS) == 1 {
                CornerStyle::Square
            } else {
                CornerStyle::Rounded
            },
            transparency: unsafe {
                SendMessageW(self.field(TRANSPARENCY), TBM_GETPOS, 0, 0).clamp(0, 100) as u32
            },
            card_clarity: unsafe {
                SendMessageW(self.field(CLARITY), TBM_GETPOS, 0, 0).clamp(0, 100) as u32
            },
            font_family: (self.selected(FONT) != 0).then_some(family),
            font_size: self.text(SIZE).parse().map_err(|_| "界面字号必须是整数")?,
        };
        config.validate()?;
        Ok(config)
    }

    pub(super) unsafe fn update_interface_controls(&self) {
        unsafe {
            if self.page != PAGE {
                return;
            }
            let glass = self.draft.interface.style == InterfaceStyle::LiquidGlass;
            for id in [TRANSPARENCY, CLARITY] {
                EnableWindow(self.field(id), i32::from(glass));
            }
            set_text(
                self.field(TRANSPARENCY_LABEL),
                &format!(
                    "{} · {}%",
                    tr("透明程度"),
                    self.draft.interface.transparency
                ),
            );
            set_text(
                self.field(CLARITY_LABEL),
                &format!(
                    "{} · {}%",
                    tr("控件清晰度"),
                    self.draft.interface.card_clarity
                ),
            );
            set_text(
                self.field(EFFECT_STATUS),
                if !glass {
                    "选择液态玻璃后，可调节透明程度和控件清晰度。"
                } else if interface::glass_active(self.hwnd) {
                    "背景越透明，越能看见桌面；控件越清晰，按钮和输入框越醒目。"
                } else {
                    "系统透明效果不可用，当前使用清晰的玻璃配色。"
                },
            );
        }
    }

    pub(super) unsafe fn interface_input_changed(&mut self, slider: bool) {
        unsafe {
            match self.collect_page() {
                Ok(()) => {
                    self.update_interface_controls();
                    if slider {
                        // Coalesce thumb tracking without rebuilding controls
                        // while the native slider owns mouse capture.
                        SetTimer(self.hwnd, PREVIEW_TIMER, 50, None);
                    } else {
                        PostMessageW(self.hwnd, WM_APPLY_INTERFACE, 0, 0);
                    }
                }
                Err(error) => self.set_status(error),
            }
        }
    }

    pub(super) unsafe fn apply_interface_preview(&mut self) {
        unsafe {
            KillTimer(self.hwnd, PREVIEW_TIMER);
            let previous = interface::current();
            let rebuild = previous.style != self.draft.interface.style
                || previous.corners != self.draft.interface.corners
                || previous.font_family != self.draft.interface.font_family
                || previous.font_size != self.draft.interface.font_size
                || PALETTE
                    .with(Cell::get)
                    .is_some_and(|palette| palette.dark != self.dark());
            if rebuild {
                let old_dpi = interface::layout_dpi(self.dpi);
                // Every control is destroyed and created again. Keep the old
                // frame on screen until the new one is complete, then show
                // it in one paint; otherwise each stage of the rebuild flashes.
                let cover = interface::FrameCover::show(self.hwnd);
                SendMessageW(self.hwnd, WM_SETREDRAW, 0, 0);
                let defer = interface::DeferBackdrop::begin();
                self.recreate_resources();
                if previous.font_size != self.draft.interface.font_size {
                    self.resize_for_interface(old_dpi);
                }
                self.render();
                drop(defer);
                if !interface::commit_backdrop(self.hwnd) {
                    self.recreate_resources();
                    self.render();
                }
                SendMessageW(self.hwnd, WM_SETREDRAW, 1, 0);
                RedrawWindow(
                    self.hwnd,
                    null(),
                    null_mut(),
                    RDW_INVALIDATE | RDW_ERASE | RDW_FRAME | RDW_ALLCHILDREN | RDW_UPDATENOW,
                );
                drop(cover);
                PostMessageW(self.hwnd, WM_PREPARE_PANELS, 0, 0);
            } else {
                interface::configure(&self.draft.interface);
                interface::apply_window(self.hwnd, self.build, self.dark(), false);
                for &panel in &self.panel_cache {
                    interface::apply_window(panel, self.build, self.dark(), true);
                }
                RedrawWindow(
                    self.hwnd,
                    null(),
                    null_mut(),
                    RDW_INVALIDATE | RDW_ALLCHILDREN,
                );
            }
            self.update_interface_controls();
            self.set_status(tr("外观预览中 · 点击保存后保留设置"));
        }
    }

    pub(super) unsafe fn resize_for_interface(&self, old_dpi: u32) {
        unsafe {
            let mut bounds: RECT = std::mem::zeroed();
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = size_of::<MONITORINFO>() as u32;
            if GetWindowRect(self.hwnd, &mut bounds) == 0
                || GetMonitorInfoW(
                    MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST),
                    &mut info,
                ) == 0
            {
                return;
            }
            let dpi = interface::layout_dpi(self.dpi) as i32;
            let width = ((bounds.right - bounds.left) * dpi / old_dpi.max(1) as i32)
                .max(self.scale(1060))
                .min(info.rcWork.right - info.rcWork.left);
            let height = ((bounds.bottom - bounds.top) * dpi / old_dpi.max(1) as i32)
                .max(self.scale(820))
                .min(info.rcWork.bottom - info.rcWork.top);
            let left = bounds
                .left
                .clamp(info.rcWork.left, info.rcWork.right - width);
            let top = bounds
                .top
                .clamp(info.rcWork.top, info.rcWork.bottom - height);
            SetWindowPos(
                self.hwnd,
                null_mut(),
                left,
                top,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    pub(super) unsafe fn discard_settings_draft(&mut self) {
        unsafe {
            KillTimer(self.hwnd, PREVIEW_TIMER);
            let previous = interface::current();
            let old_dpi = interface::layout_dpi(self.dpi);
            self.draft = self.saved.clone();
            for page in self.settings_cache.drain(..) {
                DestroyWindow(page.window);
            }
            if previous != self.draft.interface
                || PALETTE
                    .with(Cell::get)
                    .is_some_and(|palette| palette.dark != self.dark())
            {
                self.recreate_resources();
                if previous.font_size != self.draft.interface.font_size {
                    self.resize_for_interface(old_dpi);
                }
                PostMessageW(self.hwnd, WM_PREPARE_PANELS, 0, 0);
            }
            self.render();
        }
    }
}
