use crate::config::ClockConfig;
use chrono::{DateTime, Datelike, Local, Timelike};
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
            .any(|line| line.contains("%second%"))
}

/// Formatting belongs to the host timer, independent of settings visibility.
pub fn frame(
    config: &ClockConfig,
    weather: &str,
    now: DateTime<Local>,
) -> litetray_adapter_protocol::clock::Frame {
    use litetray_adapter_protocol::clock::{Frame, Style, Text};
    Frame {
        text: Text {
            top: format(&config.top_line, config, now, weather),
            bottom: format(&config.bottom_line, config, now, weather),
            tooltip: (!config.tooltip_line.is_empty())
                .then(|| format(&config.tooltip_line, config, now, weather)),
        },
        style: Style {
            font_family: config.font_family.clone(),
            font_size: config.font_size,
            width: config.width,
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
        if !TOKENS.contains(&token) {
            return Err(format!("暂不支持模板变量 %{token}%"));
        }
        rest = &tail[end + 1..];
    }
    Ok(())
}
pub fn format(template: &str, config: &ClockConfig, now: DateTime<Local>, weather: &str) -> String {
    let weekday = [
        "星期一",
        "星期二",
        "星期三",
        "星期四",
        "星期五",
        "星期六",
        "星期日",
    ][now.weekday().num_days_from_monday() as usize];
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
        match &tail[..end] {
            "date" => out.push_str(&now.format("%Y/%m/%d").to_string()),
            "time" => out.push_str(
                &now.format(if config.show_seconds {
                    "%H:%M:%S"
                } else {
                    "%H:%M"
                })
                .to_string(),
            ),
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
            _ => out.push_str(&rest[start..start + end + 2]),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}
#[cfg(test)]
mod tests {
    use super::*;
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
