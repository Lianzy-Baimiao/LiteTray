use crate::config::ClockConfig;
use chrono::{DateTime, Datelike, Local, Timelike};
pub mod weather_format;
const TOKENS: [&str; 18] = [
    "date",
    "time",
    "weekday",
    "weather",
    "year",
    "month",
    "day",
    "newline",
    "year2",
    "month2",
    "day2",
    "hour",
    "hour12",
    "minute",
    "second",
    "ampm",
    "weekday_short",
    "percent",
];
pub fn needs_second_ticks(config: &ClockConfig) -> bool {
    config.show_seconds
        || [&config.top_line, &config.bottom_line, &config.tooltip_line]
            .iter()
            .any(|line| {
                line.contains("%second%")
                    || line.contains("%time%") && pattern_has_seconds(&config.time_format, false)
                    || line.contains("%date%") && pattern_has_seconds(&config.date_format, true)
            })
}

/// Separate date/time patterns make lower-case `yyyymmdd` unambiguous:
/// m is a month in the date field, and a minute in the time field.
/// Upper-case M is always a month. Quoted text and escaped characters are literal.
fn pattern_parts(pattern: &str, date: bool) -> Result<Vec<(String, Option<&'static str>)>, String> {
    if pattern.is_empty() || pattern.len() > 128 || pattern.contains(['\0', '\n', '\r']) {
        return Err("日期与时间格式须为 1–128 字符的单行文本".into());
    }
    let mut chars = pattern.chars().peekable();
    let mut parts = Vec::new();
    let mut month_context = date;
    while let Some(c) = chars.next() {
        if c == '\'' {
            let mut literal = String::new();
            loop {
                match chars.next() {
                    Some('\'') if chars.peek() == Some(&'\'') => {
                        chars.next();
                        literal.push('\'');
                    }
                    Some('\'') => break,
                    Some(c) => literal.push(c),
                    None => return Err("格式中的单引号未闭合".into()),
                }
            }
            parts.push((literal, None));
        } else if c == '\\' {
            parts.push((
                chars
                    .next()
                    .ok_or("格式末尾的转义符后缺少字符")?
                    .to_string(),
                None,
            ));
        } else if c.is_ascii_alphabetic() {
            let mut token = c.to_string();
            while chars.peek() == Some(&c) {
                token.push(chars.next().unwrap());
            }
            if matches!(c, 'H' | 'h') {
                month_context = false;
            }
            if c == 'y' {
                month_context = date;
            }
            let spec = match token.as_str() {
                "yyyy" => "%Y",
                "yy" => "%y",
                "M" => "%-m",
                "MM" => "%m",
                "m" if month_context => "%-m",
                "mm" if month_context => "%m",
                "d" => "%-d",
                "dd" => "%d",
                "ddd" => "weekday_short",
                "dddd" => "weekday",
                "H" => "%-H",
                "HH" => "%H",
                "h" => "%-I",
                "hh" => "%I",
                "m" => "%-M",
                "mm" => "%M",
                "s" => "%-S",
                "ss" => "%S",
                "t" | "tt" => "ampm",
                _ => return Err(format!("不支持格式 {token}；普通英文请用单引号包围")),
            };
            parts.push((token, Some(spec)));
        } else {
            parts.push((c.to_string(), None));
        }
    }
    Ok(parts)
}
pub fn validate_pattern(pattern: &str, date: bool) -> Result<(), String> {
    pattern_parts(pattern, date).map(|_| ())
}
pub fn effective_time_pattern(config: &ClockConfig) -> String {
    if config.show_seconds && !pattern_has_seconds(&config.time_format, false) {
        format!("{}:ss", config.time_format)
    } else {
        config.time_format.clone()
    }
}
fn pattern_has_seconds(pattern: &str, date: bool) -> bool {
    pattern_parts(pattern, date)
        .is_ok_and(|parts| parts.iter().any(|(_, s)| matches!(s, Some("%S" | "%-S"))))
}
fn format_pattern(pattern: &str, date: bool, now: DateTime<Local>) -> String {
    let Ok(parts) = pattern_parts(pattern, date) else {
        return pattern.into();
    };
    parts
        .into_iter()
        .map(|(literal, spec)| match spec {
            None => literal,
            Some("weekday") => [
                "星期一",
                "星期二",
                "星期三",
                "星期四",
                "星期五",
                "星期六",
                "星期日",
            ][now.weekday().num_days_from_monday() as usize]
                .into(),
            Some("weekday_short") => ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
                [now.weekday().num_days_from_monday() as usize]
                .into(),
            Some("ampm") => if now.hour12().0 { "下午" } else { "上午" }.into(),
            Some(spec) => now.format(spec).to_string(),
        })
        .collect()
}

/// Formatting belongs to the host timer, independent of settings visibility.
pub fn frame(
    config: &ClockConfig,
    weather: &str,
    now: DateTime<Local>,
) -> litetray_adapter_protocol::clock::Frame {
    frame_using(config, |template| format(template, config, now, weather))
}
pub fn frame_with_weather(
    config: &ClockConfig,
    weather: Option<&crate::weather::WeatherResult>,
    now: DateTime<Local>,
) -> litetray_adapter_protocol::clock::Frame {
    let summary = weather_format::format(&config.weather_format, weather, now);
    frame_using(config, |template| format_using(template, config, now, &summary, weather))
}
fn frame_using(config: &ClockConfig, render: impl Fn(&str) -> String) -> litetray_adapter_protocol::clock::Frame {
    use litetray_adapter_protocol::clock::{Frame, Style, Text};
    Frame {
        text: Text {
            top: render(&config.top_line),
            bottom: render(&config.bottom_line),
            tooltip: (!config.tooltip_line.is_empty())
                .then(|| render(&config.tooltip_line)),
        },
        style: Style {
            font_family: config.font_family.clone(),
            font_size: config.font_size,
            width: config.width,
            compact_two_lines: config.compact_two_lines,
            top_color: (!config.top_color.is_empty()).then(|| config.top_color.clone()),
            bottom_color: (!config.bottom_color.is_empty()).then(|| config.bottom_color.clone()),
            alignment: config.alignment,
            top_style: config.top_style.clone(),
            bottom_style: config.bottom_style.clone(),
        },
    }
}

pub fn validate_template(template: &str) -> Result<(), String> {
    if template.contains('\0') {
        return Err("模板不能包含空字符".into());
    }
    let mut rest = template;
    while let Some(start) = rest.find('%') {
        let tail = &rest[start + 1..];
        let end = tail.find('%').ok_or("模板中的 % 未闭合")?;
        let token = &tail[..end];
        if !TOKENS.contains(&token) && !weather_format::supports(token) {
            return Err(format!("暂不支持模板变量 %{token}%"));
        }
        rest = &tail[end + 1..];
    }
    Ok(())
}
pub fn format(template: &str, config: &ClockConfig, now: DateTime<Local>, weather: &str) -> String {
    format_using(template, config, now, weather, None)
}
fn format_using(template: &str, config: &ClockConfig, now: DateTime<Local>, weather: &str,
    result: Option<&crate::weather::WeatherResult>) -> String {
    let weekday = [
        "星期一",
        "星期二",
        "星期三",
        "星期四",
        "星期五",
        "星期六",
        "星期日",
    ][now.weekday().num_days_from_monday() as usize];
    substitute(template, |token| {
        let mut out = String::new();
        match token {
            "date" => out.push_str(&format_pattern(&config.date_format, true, now)),
            "time" => {
                out.push_str(&format_pattern(&config.time_format, false, now));
                if config.show_seconds && !pattern_has_seconds(&config.time_format, false) {
                    out.push_str(&now.format(":%S").to_string());
                }
            }
            "weekday" => out.push_str(weekday),
            "weather" => out.push_str(weather),
            "year" => out.push_str(&now.year().to_string()),
            "month" => out.push_str(&now.month().to_string()),
            "day" => out.push_str(&now.day().to_string()),
            "year2" => out.push_str(&format!("{:02}", now.year().rem_euclid(100))),
            "month2" => out.push_str(&format!("{:02}", now.month())),
            "day2" => out.push_str(&format!("{:02}", now.day())),
            "hour" => out.push_str(&format!("{:02}", now.hour())),
            "hour12" => out.push_str(&format!("{:02}", now.hour12().1)),
            "minute" => out.push_str(&format!("{:02}", now.minute())),
            "second" => out.push_str(&format!("{:02}", now.second())),
            "ampm" => out.push_str(if now.hour12().0 { "下午" } else { "上午" }),
            "weekday_short" => out.push_str(
                ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
                    [now.weekday().num_days_from_monday() as usize],
            ),
            "percent" => out.push('%'),
            "newline" => out.push('\n'),
            token if weather_format::supports(token) => out.push_str(&weather_format::value(token, result, now)),
            _ => out.push_str(&format!("%{token}%")),
        }
        out
    })
}
fn substitute(template: &str, mut value: impl FnMut(&str) -> String) -> String {
    // A single pass keeps percent signs in provider text from becoming template code.
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        let Some(end) = tail.find('%') else {
            out.push_str(&rest[start..]);
            return out;
        };
        out.push_str(&value(&tail[..end]));
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detailed_patterns_reach_the_taskbar_frame() {
        use chrono::TimeZone;
        let c: ClockConfig = serde_json::from_value(serde_json::json!({
            "date_format": "yyyymmdd", "time_format": "hh:mm:ss tt",
            "top_line": "%date%", "bottom_line": "%time%", "show_seconds": false
        }))
        .unwrap();
        let now = Local
            .with_ymd_and_hms(2026, 1, 2, 0, 5, 9)
            .single()
            .unwrap();
        let output = frame(&c, "", now);
        assert_eq!(output.text.top, "20260102");
        assert_eq!(output.text.bottom, "12:05:09 上午");
        assert!(needs_second_ticks(&c));
    }
    #[test]
    fn patterns_distinguish_months_minutes_literals_and_leap_days() {
        use chrono::TimeZone;
        let now = Local
            .with_ymd_and_hms(2024, 2, 29, 23, 7, 4)
            .single()
            .unwrap();
        assert_eq!(
            format_pattern("yyyy年M月d日 dddd", true, now),
            "2024年2月29日 星期四"
        );
        assert_eq!(format_pattern("yy-MM-dd ddd", true, now), "24-02-29 周四");
        assert_eq!(
            format_pattern("HH:mm:ss / h:m:s tt", false, now),
            "23:07:04 / 11:7:4 下午"
        );
        assert_eq!(
            format_pattern("'Date:' yyyymmdd 'it''s'", true, now),
            "Date: 20240229 it's"
        );
        assert!(validate_pattern("yyy-MM-dd", true).is_err());
        assert!(validate_pattern("yyyy 'unfinished", true).is_err());
        assert!(validate_pattern("", false).is_err());
        assert!(validate_pattern("HH:mm\nss", false).is_err());
        let mut c = ClockConfig {
            show_seconds: false,
            top_line: "%time%".into(),
            time_format: "HH:mm 'ss'".into(),
            ..Default::default()
        };
        assert!(
            !needs_second_ticks(&c),
            "literal seconds must not affect cadence"
        );
        c.tooltip_line = "%date%".into();
        c.date_format = "yyyy/MM/dd HH:mm:ss".into();
        assert!(needs_second_ticks(&c));
        assert_eq!(format("%date%", &c, now, ""), "2024/02/29 23:07:04");
    }
    #[test]
    fn unknown_fields_are_not_silently_lost() {
        assert!(validate_template("%cpu%").is_err());
        assert!(validate_template("%time").is_err());
        assert!(validate_template("%date%  %time%").is_ok());
    }
    #[test]
    fn weather_is_literal() {
        let c = ClockConfig::default();
        assert_eq!(
            format("%weather%", &c, Local::now(), "湿度 80% %time%"),
            "湿度 80% %time%"
        );
    }
    #[test]
    fn padded_fields_and_periods_preserve_time_boundaries() {
        use chrono::TimeZone;
        let c = ClockConfig::default();
        let midnight = Local
            .with_ymd_and_hms(2026, 1, 2, 0, 5, 9)
            .single()
            .unwrap();
        assert_eq!(
            format(
                "%year2%-%month2%-%day2% %hour%:%minute%:%second% %hour12% %ampm%",
                &c,
                midnight,
                ""
            ),
            "26-01-02 00:05:09 12 上午"
        );
        let noon = midnight + chrono::Duration::hours(12);
        assert_eq!(
            format("%hour12% %ampm% 80%percent%", &c, noon, ""),
            "12 下午 80%"
        );
    }
    #[test]
    fn explicit_second_fields_keep_the_preview_updating() {
        let mut c = ClockConfig {
            show_seconds: false,
            top_line: "%hour%:%minute%:%second%".into(),
            ..ClockConfig::default()
        };
        assert!(needs_second_ticks(&c));
        c.top_line = "%time%".into();
        assert!(!needs_second_ticks(&c));
        c.tooltip_line = "%second%".into();
        assert!(needs_second_ticks(&c));
    }
}
