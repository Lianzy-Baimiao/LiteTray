//! Original SVG weather symbols rasterized for their final physical pixel size.
//! An 8x coverage mask supplies edge alpha; solid interiors stay fully opaque.
//! AlphaBlend copies at 1:1, so neither DPI bitmap stretching nor blur is used.
use super::*;
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Symbol {
    Sun,
    PartlyCloudy,
    Cloud,
    Rain,
    Thunder,
    Snow,
    Sleet,
    Fog,
    Hail,
    Unknown,
}
impl Symbol {
    fn for_description(description: &str) -> Self {
        // Specific combinations precede general conditions: sleet isn't rain,
        // partly cloudy isn't clear, and a thunderstorm isn't just a cloud.
        if description.contains('雷') {
            Self::Thunder
        } else if description.contains("冰雹") {
            Self::Hail
        } else if description.contains('雨') && description.contains('雪') {
            Self::Sleet
        } else if description.contains('雪') {
            Self::Snow
        } else if description.contains('雨') {
            Self::Rain
        } else if description.contains(['雾', '霾', '沙', '尘']) {
            Self::Fog
        } else if description.contains('晴') && description.contains(['云', '阴']) {
            Self::PartlyCloudy
        } else if description.contains('晴') {
            Self::Sun
        } else if description.contains(['云', '阴']) {
            Self::Cloud
        } else {
            Self::Unknown
        }
    }
    fn source(self) -> &'static str {
        match self {
            Self::Sun => include_str!("../../assets/icons/weather/sun.svg"),
            Self::PartlyCloudy => include_str!("../../assets/icons/weather/partly-cloudy.svg"),
            Self::Cloud => include_str!("../../assets/icons/weather/cloud.svg"),
            Self::Rain => include_str!("../../assets/icons/weather/rain.svg"),
            Self::Thunder => include_str!("../../assets/icons/weather/thunder.svg"),
            Self::Snow => include_str!("../../assets/icons/weather/snow.svg"),
            Self::Sleet => include_str!("../../assets/icons/weather/sleet.svg"),
            Self::Fog => include_str!("../../assets/icons/weather/fog.svg"),
            Self::Hail => include_str!("../../assets/icons/weather/hail.svg"),
            Self::Unknown => include_str!("../../assets/icons/weather/unknown.svg"),
        }
    }
}

struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    size: i32,
}
impl Surface {
    unsafe fn new(size: i32) -> Option<Self> {
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            if dc.is_null() {
                return None;
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: size,
                    biHeight: -size,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..std::mem::zeroed()
                },
                ..std::mem::zeroed()
            };
            let mut bits = null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bitmap.is_null() {
                DeleteDC(dc);
                return None;
            }
            let old = SelectObject(dc, bitmap);
            std::ptr::write_bytes(bits.cast::<u8>(), 0, (size * size * 4) as usize);
            Some(Self {
                dc,
                bitmap,
                old,
                bits: bits.cast(),
                size,
            })
        }
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    symbol: Symbol,
    size: i32,
    color: u32,
}
struct Raster {
    key: Key,
    surface: Surface,
}
const CACHE_LIMIT: usize = 64;
thread_local! { static CACHE: RefCell<VecDeque<Raster>> = const { RefCell::new(VecDeque::new()) }; }

unsafe fn rasterize(key: Key) -> Option<Raster> {
    unsafe {
        const SAMPLES: i32 = 8;
        let high = Surface::new(key.size * SAMPLES)?;
        let output = Surface::new(key.size)?;
        let brush = LOGBRUSH {
            lbStyle: BS_SOLID,
            lbColor: rgb(255, 255, 255),
            lbHatch: 0,
        };
        // Small symbols need a visible stroke core, not a mostly translucent hairline.
        let stroke = (key.size as f64 * 1.8 / 24.).max(1.75);
        let pen = ExtCreatePen(
            (PS_GEOMETRIC | PS_SOLID | PS_ENDCAP_ROUND | PS_JOIN_ROUND) as u32,
            (stroke * SAMPLES as f64).round() as u32,
            &brush,
            0,
            null(),
        );
        if pen.is_null() {
            return None;
        }
        let old = SelectObject(high.dc, pen);
        icons::stroke_svg_path(high.dc, key.symbol.source(), high.size as f64 / 24.);
        SelectObject(high.dc, old);
        DeleteObject(pen);
        GdiFlush();
        let channel = [
            (key.color >> 16) & 255,
            (key.color >> 8) & 255,
            key.color & 255,
        ];
        let pixels = std::slice::from_raw_parts(high.bits, (high.size * high.size * 4) as usize);
        let target =
            std::slice::from_raw_parts_mut(output.bits, (key.size * key.size * 4) as usize);
        for y in 0..key.size {
            for x in 0..key.size {
                let mut covered = 0u32;
                for sy in 0..SAMPLES {
                    for sx in 0..SAMPLES {
                        let i = ((y * SAMPLES + sy) * high.size + x * SAMPLES + sx) as usize * 4;
                        covered += u32::from(pixels[i] != 0);
                    }
                }
                let alpha = (covered * 255 + 32) / 64;
                let i = (y * key.size + x) as usize * 4;
                for c in 0..3 {
                    target[i + c] = ((channel[c] * alpha + 127) / 255) as u8;
                }
                target[i + 3] = alpha as u8;
            }
        }
        Some(Raster {
            key,
            surface: output,
        })
    }
}
pub(super) unsafe fn draw(dc: HDC, bounds: RECT, color: u32, description: &str) {
    let size = (bounds.right - bounds.left).min(bounds.bottom - bounds.top);
    if !(8..=256).contains(&size) {
        return;
    }
    let key = Key {
        symbol: Symbol::for_description(description),
        size,
        color,
    };
    CACHE.with(|cache| unsafe {
        let mut cache = cache.borrow_mut();
        let index = if let Some(index) = cache.iter().position(|r| r.key == key) {
            index
        } else {
            let Some(raster) = rasterize(key) else {
                return;
            };
            if cache.len() == CACHE_LIMIT {
                cache.pop_front();
            }
            cache.push_back(raster);
            cache.len() - 1
        };
        let source = &cache[index].surface;
        AlphaBlend(
            dc,
            bounds.left + (bounds.right - bounds.left - size) / 2,
            bounds.top + (bounds.bottom - bounds.top - size) / 2,
            size,
            size,
            source.dc,
            0,
            0,
            size,
            size,
            BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            },
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn weather_conditions_choose_distinct_symbols_and_cache_is_bounded() {
        for (text, expected) in [
            ("晴", Symbol::Sun),
            ("晴间多云", Symbol::PartlyCloudy),
            ("阴", Symbol::Cloud),
            ("小雨", Symbol::Rain),
            ("雷雨伴冰雹", Symbol::Thunder),
            ("雨夹雪", Symbol::Sleet),
            ("小雪", Symbol::Snow),
            ("雾", Symbol::Fog),
            ("冰雹", Symbol::Hail),
            ("天气未知", Symbol::Unknown),
        ] {
            assert_eq!(Symbol::for_description(text), expected);
        }
        unsafe {
            let target = Surface::new(32).unwrap();
            for color in 0..80 {
                draw(target.dc, design::rect(0, 0, 22, 22), color, "晴");
            }
            CACHE.with(|cache| {
                assert_eq!(cache.borrow().len(), CACHE_LIMIT);
                let bitmap = cache.borrow().back().unwrap().surface.bitmap;
                draw(target.dc, design::rect(0, 0, 22, 22), 79, "晴");
                assert_eq!(cache.borrow().back().unwrap().surface.bitmap, bitmap);
            });
        }
    }
}
