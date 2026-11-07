//! Physical-screen placement shared by clock and tray entry points.
pub use litetray_adapter_protocol::activation::Rect;
pub const WIDTH: i32 = 720;
pub const HEIGHT: i32 = 656;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub rect: Rect,
    pub scale_percent: u32,
}
pub fn place(anchor: Rect, work: Rect, dpi: u32) -> Result<Placement, String> {
    place_sized(anchor, work, dpi, WIDTH + 2, HEIGHT + 2)
}
pub fn place_sized(
    anchor: Rect,
    work: Rect,
    dpi: u32,
    logical_width: i32,
    logical_height: i32,
) -> Result<Placement, String> {
    anchor.validate()?;
    work.validate()?;
    if !(48..=768).contains(&dpi)
        || !(1..=4096).contains(&logical_width)
        || !(1..=4096).contains(&logical_height)
    {
        return Err("面板 DPI 无效".into());
    }
    let w = i64::from(work.right) - i64::from(work.left);
    let h = i64::from(work.bottom) - i64::from(work.top);
    let margin = (10 * i64::from(dpi) / 96).min(w / 20).min(h / 20);
    let preferred_w = (i64::from(logical_width) * i64::from(dpi) / 96).max(1);
    let preferred_h = (i64::from(logical_height) * i64::from(dpi) / 96).max(1);
    // Scale the entire native layout together on small work areas instead of
    // placing buttons outside the available display.
    let percent = 100_i64
        .min((w - 2 * margin) * 100 / preferred_w)
        .min((h - 2 * margin) * 100 / preferred_h)
        .max(1);
    let width = (preferred_w * percent / 100).max(1);
    let height = (preferred_h * percent / 100).max(1);
    let left = i64::from(work.left) + margin;
    let right = i64::from(work.right) - margin;
    let top = i64::from(work.top) + margin;
    let bottom = i64::from(work.bottom) - margin;
    let horizontal = anchor.bottom <= work.top || anchor.top >= work.bottom;
    let vertical = anchor.right <= work.left || anchor.left >= work.right;
    let x = if !horizontal && vertical {
        if anchor.right <= work.left {
            i64::from(anchor.right) + margin
        } else {
            i64::from(anchor.left) - margin - width
        }
    } else {
        i64::from(anchor.right) - width
    };
    let y = if anchor.bottom <= work.top {
        i64::from(anchor.bottom) + margin
    } else if !horizontal && vertical {
        i64::from(anchor.bottom) - height
    } else if i64::from(anchor.top) - margin - height >= top {
        i64::from(anchor.top) - margin - height
    } else {
        i64::from(anchor.bottom) + margin
    };
    let x = x.clamp(left, (right - width).max(left));
    let y = y.clamp(top, (bottom - height).max(top));
    Ok(Placement {
        rect: Rect {
            left: x as i32,
            top: y as i32,
            right: (x + width) as i32,
            bottom: (y + height) as i32,
        },
        scale_percent: percent as u32,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn anchors_to_each_edge_on_negative_monitor() {
        let work = Rect {
            left: -1920,
            top: -1080,
            right: 0,
            bottom: 0,
        };
        for anchor in [
            Rect {
                left: -200,
                top: 0,
                right: 0,
                bottom: 40,
            },
            Rect {
                left: -200,
                top: -1120,
                right: 0,
                bottom: -1080,
            },
            Rect {
                left: -1960,
                top: -700,
                right: -1920,
                bottom: -500,
            },
            Rect {
                left: 0,
                top: -700,
                right: 40,
                bottom: -500,
            },
        ] {
            let p = place(anchor, work, 144).unwrap().rect;
            assert!(
                p.left >= work.left
                    && p.top >= work.top
                    && p.right <= work.right
                    && p.bottom <= work.bottom
            );
            if anchor.bottom <= work.top {
                assert!(p.top >= anchor.bottom);
            }
            if anchor.top >= work.bottom {
                assert!(p.bottom <= anchor.top);
            }
            if anchor.right <= work.left {
                assert!(p.left >= anchor.right);
            }
            if anchor.left >= work.right {
                assert!(p.right <= anchor.left);
            }
        }
    }
    #[test]
    fn scales_small_work_areas_and_rejects_invalid_input() {
        let work = Rect {
            left: 4000,
            top: 0,
            right: 4800,
            bottom: 600,
        };
        let anchor = Rect {
            left: 4700,
            top: 600,
            right: 4800,
            bottom: 650,
        };
        let p = place(anchor, work, 192).unwrap();
        assert!(p.scale_percent < 100);
        assert!(p.rect.bottom <= work.bottom && p.rect.right <= work.right);
        assert!(place(anchor, work, 0).is_err());
    }
    #[test]
    fn compact_volume_feedback_stays_by_its_clock_on_every_edge_and_scale() {
        for dpi in [96, 120, 144, 192] {
            let work = Rect {
                left: -1920,
                top: -1080,
                right: 0,
                bottom: 0,
            };
            for anchor in [
                Rect {
                    left: -160,
                    top: 0,
                    right: 0,
                    bottom: 48,
                },
                Rect {
                    left: -160,
                    top: -1128,
                    right: 0,
                    bottom: -1080,
                },
                Rect {
                    left: -1968,
                    top: -500,
                    right: -1920,
                    bottom: -350,
                },
                Rect {
                    left: 0,
                    top: -500,
                    right: 48,
                    bottom: -350,
                },
            ] {
                let p = place_sized(anchor, work, dpi, 208, 72).unwrap().rect;
                assert!(
                    p.left >= work.left
                        && p.right <= work.right
                        && p.top >= work.top
                        && p.bottom <= work.bottom
                );
                assert!(
                    p.bottom <= anchor.top
                        || p.top >= anchor.bottom
                        || p.left >= anchor.right
                        || p.right <= anchor.left
                );
                assert_eq!(p.right - p.left, 208 * dpi as i32 / 96);
            }
        }
    }
}
