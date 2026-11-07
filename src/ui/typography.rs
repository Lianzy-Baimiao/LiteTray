//! DirectWrite text on existing GDI surfaces. Render at physical pixel size;
//! the scratch bitmap is copied 1:1, never scaled. Native editable controls
//! retain their system text/input implementation.
use super::*;
use std::{collections::VecDeque, ffi::c_void};
use windows::{
    Win32::Graphics::DirectWrite::*,
    core::{BOOL, IUnknown, IUnknownImpl, Interface, PCWSTR, Ref, Result, implement},
};

const CACHE_LIMIT: usize = 256;
#[derive(Clone, PartialEq, Eq)]
struct Key {
    face: [u16; 32],
    height: i32,
    weight: i32,
    italic: bool,
    width: i32,
    lines: i32,
    flags: u32,
    text: String,
}
struct Text {
    factory: IDWriteFactory,
    target: IDWriteBitmapRenderTarget1,
    params: IDWriteRenderingParams,
    layouts: VecDeque<(Key, IDWriteTextLayout)>,
    width: i32,
    height: i32,
}
thread_local! {
    static TEXT: RefCell<Option<Text>> = const { RefCell::new(None) };
}
pub(super) fn invalidate() {
    TEXT.with(|state| {
        if let Ok(mut state) = state.try_borrow_mut() {
            *state = None;
        }
    });
}
unsafe fn font_key(value: &str, font: HFONT, width: i32, height: i32, flags: u32) -> Option<Key> {
    unsafe {
        let mut lf: LOGFONTW = std::mem::zeroed();
        if GetObjectW(font, size_of::<LOGFONTW>() as i32, &mut lf as *mut _ as _) == 0
            || lf.lfHeight >= 0
            || lf.lfWidth != 0
            || lf.lfEscapement != 0
            || lf.lfOrientation != 0
            || lf.lfUnderline != 0
            || lf.lfStrikeOut != 0
        {
            return None;
        }
        Some(Key {
            face: lf.lfFaceName,
            height: -lf.lfHeight,
            weight: lf.lfWeight,
            italic: lf.lfItalic != 0,
            width,
            lines: height,
            flags,
            text: value.into(),
        })
    }
}
pub(super) unsafe fn measure(value: &str, font: HFONT, width: i32) -> Option<i32> {
    unsafe {
        let key = font_key(value, font, width, 2048, DT_WORDBREAK)?;
        TEXT.with(|state| {
            let mut state = state.try_borrow_mut().ok()?;
            if state.is_none() {
                *state = Text::new().ok();
            }
            let layout = state.as_mut()?.layout(&key).ok()?;
            let mut metrics = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut metrics).ok()?;
            Some(metrics.height.ceil() as i32)
        })
    }
}

pub(super) unsafe fn draw(
    dc: HDC,
    value: &str,
    bounds: RECT,
    font: HFONT,
    color: u32,
    flags: u32,
) -> bool {
    unsafe {
        let width = bounds.right - bounds.left;
        let height = bounds.bottom - bounds.top;
        if value.is_empty() || width <= 0 || height <= 0 {
            return true;
        }
        // Leave unsupported GDI transforms/fonts to the original renderer.
        if width > 4096
            || height > 2048
            || flags & DT_CALCRECT != 0
            || GetMapMode(dc) != MM_TEXT
            || GetGraphicsMode(dc) == GM_ADVANCED
        {
            return false;
        }
        let Some(key) = font_key(value, font, width, height, flags) else {
            return false;
        };
        TEXT.with(|state| {
            let Ok(mut state) = state.try_borrow_mut() else {
                return false;
            };
            if state.is_none() {
                *state = Text::new().ok();
            }
            let Some(text) = state.as_mut() else {
                return false;
            };
            text.draw(dc, bounds, color, key).is_ok()
        })
    }
}
impl Text {
    unsafe fn new() -> Result<Self> {
        unsafe {
            let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let target: IDWriteBitmapRenderTarget1 = factory
                .GetGdiInterop()?
                .CreateBitmapRenderTarget(None, 1, 1)?
                .cast()?;
            target.SetPixelsPerDip(1.)?;
            target.SetTextAntialiasMode(DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE)?;
            let params = factory.CreateRenderingParams()?;
            Ok(Self {
                factory,
                target,
                params,
                layouts: VecDeque::new(),
                width: 1,
                height: 1,
            })
        }
    }
    unsafe fn layout(&mut self, key: &Key) -> Result<IDWriteTextLayout> {
        unsafe {
            if let Some(index) = self.layouts.iter().position(|(k, _)| k == key) {
                let entry = self.layouts.remove(index).unwrap();
                let layout = entry.1.clone();
                self.layouts.push_back(entry);
                return Ok(layout);
            }
            let format = self.factory.CreateTextFormat(
                PCWSTR(key.face.as_ptr()),
                None,
                DWRITE_FONT_WEIGHT(key.weight),
                if key.italic {
                    DWRITE_FONT_STYLE_ITALIC
                } else {
                    DWRITE_FONT_STYLE_NORMAL
                },
                DWRITE_FONT_STRETCH_NORMAL,
                key.height as f32,
                windows::core::w!(""),
            )?;
            let value: Vec<u16> = key.text.encode_utf16().collect();
            let layout = self.factory.CreateGdiCompatibleTextLayout(
                &value,
                &format,
                key.width as f32,
                key.lines as f32,
                1.,
                None,
                true,
            )?;
            layout.SetTextAlignment(if key.flags & DT_CENTER != 0 {
                DWRITE_TEXT_ALIGNMENT_CENTER
            } else if key.flags & DT_RIGHT != 0 {
                DWRITE_TEXT_ALIGNMENT_TRAILING
            } else {
                DWRITE_TEXT_ALIGNMENT_LEADING
            })?;
            layout.SetParagraphAlignment(
                if key.flags & DT_SINGLELINE != 0 && key.flags & DT_VCENTER != 0 {
                    DWRITE_PARAGRAPH_ALIGNMENT_CENTER
                } else {
                    DWRITE_PARAGRAPH_ALIGNMENT_NEAR
                },
            )?;
            layout.SetWordWrapping(
                if key.flags & DT_WORDBREAK != 0 && key.flags & DT_SINGLELINE == 0 {
                    DWRITE_WORD_WRAPPING_WRAP
                } else {
                    DWRITE_WORD_WRAPPING_NO_WRAP
                },
            )?;
            if key.flags & DT_END_ELLIPSIS != 0 {
                let ellipsis = self.factory.CreateEllipsisTrimmingSign(&format)?;
                layout.SetTrimming(
                    &DWRITE_TRIMMING {
                        granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                        ..Default::default()
                    },
                    &ellipsis,
                )?;
            }
            if self.layouts.len() == CACHE_LIMIT {
                self.layouts.pop_front();
            }
            self.layouts.push_back((key.clone(), layout.clone()));
            Ok(layout)
        }
    }
    unsafe fn draw(&mut self, dc: HDC, r: RECT, color: u32, key: Key) -> Result<()> {
        unsafe {
            let layout = self.layout(&key)?;
            if key.width > self.width || key.lines > self.height {
                let width = self.width.max(key.width);
                let height = self.height.max(key.lines);
                self.target.Resize(width as u32, height as u32)?;
                self.width = width;
                self.height = height;
            }
            let memory = self.target.GetMemoryDC().0;
            // Blend glyphs into the actual destination background, including
            // rounded selection surfaces, then respect its original clip.
            if BitBlt(
                memory, 0, 0, key.width, key.lines, dc, r.left, r.top, SRCCOPY,
            ) == 0
            {
                return Err(windows::core::Error::from_thread());
            }
            let renderer: IDWriteTextRenderer = Glyphs {
                target: self.target.clone(),
                params: self.params.clone(),
                color,
            }
            .into();
            layout.Draw(None, &renderer, 0., 0.)?;
            if BitBlt(
                dc, r.left, r.top, key.width, key.lines, memory, 0, 0, SRCCOPY,
            ) == 0
            {
                return Err(windows::core::Error::from_thread());
            }
            Ok(())
        }
    }
}

#[implement(IDWriteTextRenderer)]
struct Glyphs {
    target: IDWriteBitmapRenderTarget1,
    params: IDWriteRenderingParams,
    color: u32,
}
impl IDWritePixelSnapping_Impl for Glyphs_Impl {
    fn IsPixelSnappingDisabled(&self, _: *const c_void) -> Result<BOOL> {
        Ok(false.into())
    }
    fn GetPixelsPerDip(&self, _: *const c_void) -> Result<f32> {
        Ok(1.)
    }
    fn GetCurrentTransform(&self, _: *const c_void, transform: *mut DWRITE_MATRIX) -> Result<()> {
        unsafe {
            *transform = DWRITE_MATRIX {
                m11: 1.,
                m22: 1.,
                ..Default::default()
            };
        }
        Ok(())
    }
}
impl IDWriteTextRenderer_Impl for Glyphs_Impl {
    fn DrawGlyphRun(
        &self,
        _: *const c_void,
        x: f32,
        y: f32,
        mode: DWRITE_MEASURING_MODE,
        glyphs: *const DWRITE_GLYPH_RUN,
        _: *const DWRITE_GLYPH_RUN_DESCRIPTION,
        _: Ref<IUnknown>,
    ) -> Result<()> {
        unsafe {
            self.target.DrawGlyphRun(
                x,
                y,
                mode,
                glyphs,
                &self.params,
                windows::Win32::Foundation::COLORREF(self.color),
                None,
            )
        }
    }
    fn DrawInlineObject(
        &self,
        context: *const c_void,
        x: f32,
        y: f32,
        object: Ref<IDWriteInlineObject>,
        sideways: BOOL,
        rtl: BOOL,
        effect: Ref<IUnknown>,
    ) -> Result<()> {
        unsafe {
            if let Some(object) = object.as_ref() {
                object.Draw(
                    Some(context),
                    &self.to_interface::<IDWriteTextRenderer>(),
                    x,
                    y,
                    sideways.as_bool(),
                    rtl.as_bool(),
                    effect.as_ref(),
                )?;
            }
            Ok(())
        }
    }
    fn DrawUnderline(
        &self,
        _: *const c_void,
        _: f32,
        _: f32,
        _: *const DWRITE_UNDERLINE,
        _: Ref<IUnknown>,
    ) -> Result<()> {
        Ok(())
    }
    fn DrawStrikethrough(
        &self,
        _: *const c_void,
        _: f32,
        _: f32,
        _: *const DWRITE_STRIKETHROUGH,
        _: Ref<IUnknown>,
    ) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_text_preserves_background_viewport_and_clip() {
        super::super::visual_checks::on_isolated_desktop(|| unsafe {
            let dc = CreateCompatibleDC(null_mut());
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: 180,
                    biHeight: -96,
                    biPlanes: 1,
                    biBitCount: 32,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            assert!(!bitmap.is_null());
            let old = SelectObject(dc, bitmap);
            let font = make_font(16, 400, &design::font_face());
            let bounds = design::rect(10, 8, 140, 62);
            let value = "农历九月廿九 · 小时天气 · 足够长的文字验证省略号";
            let flags = DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS;
            let mut frames = vec![];
            // A cache hit must not bring pixels from the previous surface or
            // color. A fresh layout must produce exactly the same final frame.
            for (background, foreground, reset) in
                [(255, 26, true), (32, 255, false), (32, 255, true)]
            {
                if reset {
                    invalidate();
                }
                design::fill(
                    dc,
                    design::rect(0, 0, 180, 96),
                    rgb(background, background, background),
                );
                GdiFlush();
                let before = std::slice::from_raw_parts(bits.cast::<u8>(), 180 * 96 * 4).to_vec();
                let saved = SaveDC(dc);
                IntersectClipRect(dc, 24, 16, 120, 60);
                SetViewportOrgEx(dc, 7, 5, null_mut());
                assert!(draw(
                    dc,
                    value,
                    bounds,
                    font,
                    rgb(foreground, foreground, foreground),
                    flags
                ));
                RestoreDC(dc, saved);
                GdiFlush();
                let after = std::slice::from_raw_parts(bits.cast::<u8>(), before.len()).to_vec();
                let mut changed = 0;
                for (i, (a, b)) in before
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(after.as_chunks::<4>().0)
                    .enumerate()
                {
                    if a[..3] != b[..3] {
                        let (x, y) = (i % 180, i / 180);
                        assert!(
                            (24..120).contains(&x) && (16..60).contains(&y),
                            "text escaped clip at {x},{y}"
                        );
                        changed += 1;
                    }
                }
                assert!(changed > 30, "text was not rendered");
                frames.push(after);
            }
            assert_eq!(
                frames[1], frames[2],
                "cache hit changed the rendered result"
            );
            assert!(measure(value, font, 90).unwrap() > measure(value, font, 900).unwrap());
            TEXT.with(|state| {
                let mut state = state.borrow_mut();
                let text = state.as_mut().unwrap();
                for i in 0..CACHE_LIMIT + 10 {
                    text.layout(&font_key(&format!("日期 {i}"), font, 120, 24, flags).unwrap())
                        .unwrap();
                }
                assert_eq!(text.layouts.len(), CACHE_LIMIT);
            });
            DeleteObject(font);
            SelectObject(dc, old);
            DeleteObject(bitmap);
            DeleteDC(dc);
            invalidate();
        });
    }
}
