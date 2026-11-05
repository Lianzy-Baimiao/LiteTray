use crate::config::ClockConfig;
use chrono::{DateTime, Datelike, Local};
const TOKENS: [&str; 8] = [
    "date", "time", "weekday", "weather", "year", "month", "day", "newline",
];

pub fn validate_template(template: &str) -> Result<(), String> {
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
}
