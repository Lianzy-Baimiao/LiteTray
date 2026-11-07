//! In-card forecast pages. Controls remain alive when switching; no page
//! rebuild or network request is needed to browse the downloaded hours.
use super::*;
use chrono::{DateTime, Utc};
use design::{Theme, rect, text};
pub(super) const HOURLY: u16 = 1120;
pub(super) const DAILY: u16 = 1121;
pub(super) const PREVIOUS: u16 = 1122;
pub(super) const NEXT: u16 = 1123;
pub(super) const PER_PAGE: usize = 4;
#[derive(Default)]
pub(super) struct State {
    pub daily: bool,
    pub page: usize,
    pub count: usize,
    first: Option<DateTime<Utc>>,
}
impl State {
    pub fn sync(&mut self, weather: Option<&weather::Weather>, now: DateTime<Utc>) -> bool {
        let hours = weather.map(|w| w.upcoming_hours(now)).unwrap_or_default();
        let first = hours.first().map(|h| h.at);
        let before = (self.count, self.first, self.page);
        self.count = hours.len();
        self.first = first;
        self.page = self.page.min(self.count.saturating_sub(1) / PER_PAGE);
        before != (self.count, self.first, self.page)
    }
    pub fn command(&mut self, id: u16) {
        match id {
            HOURLY => self.daily = false,
            DAILY => self.daily = true,
            PREVIOUS => self.page = self.page.saturating_sub(1),
            NEXT => self.page = (self.page + 1).min(self.count.saturating_sub(1) / PER_PAGE),
            _ => {}
        }
    }
    pub fn selected(&self, id: u16) -> bool {
        if id == DAILY { self.daily } else { !self.daily }
    }
    pub unsafe fn render(
        &self,
        dc: HDC,
        w: &weather::Weather,
        now: DateTime<Utc>,
        factor: f64,
        fonts: [HFONT; 5],
        dark: bool,
    ) {
        unsafe {
            let s = |n: i32| (n as f64 * factor).round() as i32;
            let r = |x, y, width, height| rect(s(x), s(y), s(width), s(height));
            let t = Theme::new(dark);
            if self.daily {
                if w.days.is_empty() {
                    text(
                        dc,
                        tr("此来源暂无三日预报，可切回逐小时查看。"),
                        r(402, 340, 276, 60),
                        fonts[1],
                        t.muted,
                        DT_LEFT | DT_WORDBREAK,
                    );
                }
                for (i, day) in w.days.iter().take(3).enumerate() {
                    let y = 334 + i as i32 * 38;
                    let label = chrono::NaiveDate::parse_from_str(&day.date, "%Y-%m-%d")
                        .ok()
                        .map(|d| d.format("%-m/%-d").to_string())
                        .unwrap_or_default();
                    text(
                        dc,
                        &label,
                        r(402, y, 48, 24),
                        fonts[1],
                        t.muted,
                        DT_LEFT | DT_SINGLELINE | DT_VCENTER,
                    );
                    text(
                        dc,
                        tr(&day.description),
                        r(456, y, 126, 24),
                        fonts[1],
                        t.text,
                        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
                    );
                    text(
                        dc,
                        &format!("{:.0}° / {:.0}°", day.low_c, day.high_c),
                        r(592, y, 86, 24),
                        fonts[1],
                        t.text,
                        DT_RIGHT | DT_SINGLELINE | DT_VCENTER,
                    );
                }
                return;
            }
            let hours = w.upcoming_hours(now);
            let page = self.page.min(hours.len().saturating_sub(1) / PER_PAGE);
            let shown: Vec<_> = hours.iter().skip(page * PER_PAGE).take(PER_PAGE).collect();
            if shown.is_empty() {
                text(
                    dc,
                    tr("暂无有效小时预报。Open-Meteo、MET Norway 均无需 Key；可刷新或切换来源。"),
                    r(402, 340, 276, 88),
                    fonts[1],
                    t.muted,
                    DT_LEFT | DT_WORDBREAK,
                );
                return;
            }
            for (i, hour) in shown.iter().enumerate() {
                let x = 400 + i as i32 * 70;
                let center = |value: &str, y, height, font, color| {
                    text(
                        dc,
                        value,
                        r(x, y, 66, height),
                        fonts[font],
                        color,
                        DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
                    )
                };
                center(
                    &hour.at.with_timezone(&Local).format("%H:%M").to_string(),
                    330,
                    22,
                    1,
                    t.muted,
                );
                dashboard::weather_icon(dc, r(x + 23, 358, 22, 22), t.accent, &hour.description);
                center(tr(&hour.description), 382, 20, 1, t.text);
                center(&format!("{:.0}°", hour.temperature_c), 402, 26, 2, t.text);
                center(
                    &format!(
                        "{} {}",
                        tr("降水"),
                        hour.precipitation_probability
                            .map(|p| format!("{p:.0}%"))
                            .unwrap_or_else(|| "—".into())
                    ),
                    434,
                    20,
                    1,
                    t.muted,
                );
            }
            let first = shown.first().unwrap().at.with_timezone(&Local);
            let last = shown.last().unwrap().at.with_timezone(&Local);
            let date = if first.date_naive() == last.date_naive() {
                first.format("%-m/%-d").to_string()
            } else {
                format!("{}–{}", first.format("%-m/%-d"), last.format("%-m/%-d"))
            };
            text(
                dc,
                &format!(
                    "{date} · {} · {}/{}",
                    tr("本机时间"),
                    page + 1,
                    hours.len().div_ceil(PER_PAGE)
                ),
                r(402, 457, 276, 16),
                fonts[1],
                t.muted,
                DT_LEFT | DT_SINGLELINE | DT_VCENTER,
            );
        }
    }
}
