//! The same weather fields feed taskbar text, tooltips and the settings preview.
//! Provider strings are substituted once and never interpreted as templates.
use crate::weather::{Weather, WeatherResult};
use chrono::{DateTime, Local};

pub const FIELDS: &[(&str, &str, bool)] = &[
    ("weather_condition", "天气状况", false),
    ("weather_temp", "温度 · °C", true),
    ("weather_feels", "体感温度 · °C", true),
    ("weather_humidity", "相对湿度 · %", true),
    ("weather_wind", "风速 · km/h", true),
    ("weather_wind_ms", "风速 · m/s", true),
    ("weather_wind_dir", "风向", false),
    ("weather_low", "今日最低温 · °C", true),
    ("weather_high", "今日最高温 · °C", true),
    ("weather_rain", "当前小时降水概率 · %", true),
    ("weather_pressure", "气压 · hPa", true),
    ("weather_visibility", "能见度 · km", true),
    ("weather_city", "城市", false),
    ("weather_source", "数据来源", false),
    ("weather_updated", "数据时间 · 时:分", false),
    ("weather_status", "数据状态", false),
];
pub const PRESETS: &[(&str, &str)] = &[
    ("简洁", "%weather_condition% %weather_temp%"),
    (
        "体感与湿度",
        "%weather_condition% %weather_temp% · 体感 %weather_feels% · 湿度 %weather_humidity%",
    ),
    (
        "温差与降雨",
        "%weather_condition% %weather_low%～%weather_high% · 降雨 %weather_rain%",
    ),
    (
        "风向与风速",
        "%weather_condition% %weather_temp% · %weather_wind_dir% %weather_wind_ms:1%",
    ),
];

fn field(token: &str) -> Option<(&str, usize)> {
    let (name, precision) = match token.split_once(':') {
        Some((name, "0")) => (name, Some(0)),
        Some((name, "1")) => (name, Some(1)),
        Some((name, "2")) => (name, Some(2)),
        Some(_) => return None,
        None => (token, None),
    };
    FIELDS
        .iter()
        .find(|(id, _, numeric)| *id == name && (precision.is_none() || *numeric))?;
    Some((name, precision.unwrap_or(0)))
}
pub(super) fn supports(token: &str) -> bool {
    field(token).is_some()
}
pub fn with_precision(template: &str, precision: usize) -> String {
    super::substitute(template, |token| {
        if let Some((name, _)) = field(token)
            && FIELDS
                .iter()
                .any(|(id, _, numeric)| *id == name && *numeric)
        {
            format!("%{name}:{}%", precision.min(2))
        } else {
            format!("%{token}%")
        }
    })
}
pub fn validate(template: &str) -> Result<(), String> {
    if template.trim().is_empty() || template.len() > 1024 || template.contains(['\0', '\r', '\n'])
    {
        return Err("天气格式须为非空单行文本，最长 1024 字节".into());
    }
    let mut rest = template;
    while let Some(start) = rest.find('%') {
        let tail = &rest[start + 1..];
        let end = tail.find('%').ok_or("天气格式中的 % 未闭合")?;
        let token = &tail[..end];
        if token != "percent" && !supports(token) {
            return Err(format!(
                "天气格式不支持 %{token}%；数值小数位可用 :0、:1、:2"
            ));
        }
        rest = &tail[end + 1..];
    }
    Ok(())
}

fn number(value: Option<f64>, precision: usize, unit: &str, min: f64, max: f64) -> String {
    let Some(value) = value.filter(|v| v.is_finite() && (min..=max).contains(v)) else {
        return "—".into();
    };
    // Suppress -0°C and -0.0°C after rounding small negative temperatures.
    let scale = 10_f64.powi(precision as i32);
    let value = (value * scale).round() / scale;
    format!(
        "{:.*}{unit}",
        precision,
        if value == 0. { 0. } else { value }
    )
}
pub(super) fn value(token: &str, result: Option<&WeatherResult>, now: DateTime<Local>) -> String {
    let Some((name, precision)) = field(token) else {
        return format!("%{token}%");
    };
    let Some(result) = result else {
        return "—".into();
    };
    let Some(w) = &result.weather else {
        return "—".into();
    };
    let today = now.format("%Y-%m-%d").to_string();
    let day = || w.days.iter().find(|day| day.date == today);
    match name {
        "weather_condition" => crate::i18n::tr(&w.description).chars().take(128).collect(),
        "weather_temp" => number(Some(w.temperature_c), precision, "°C", -100., 65.),
        "weather_feels" => number(w.feels_like_c, precision, "°C", -150., 100.),
        "weather_humidity" => number(w.humidity_percent, precision, "%", 0., 100.),
        "weather_wind" => number(w.wind_kmh, precision, " km/h", 0., 500.),
        "weather_wind_ms" => number(
            w.wind_kmh.map(|v| v / 3.6),
            precision,
            " m/s",
            0.,
            500. / 3.6,
        ),
        "weather_pressure" => number(w.pressure_hpa, precision, " hPa", 100., 1200.),
        "weather_visibility" => number(w.visibility_km, precision, " km", 0., 1000.),
        "weather_wind_dir" => w
            .wind_degrees
            .filter(|v| v.is_finite() && (0. ..=360.).contains(v))
            .map(|v| {
                [
                    "北风",
                    "东北风",
                    "东风",
                    "东南风",
                    "南风",
                    "西南风",
                    "西风",
                    "西北风",
                ][((v / 45.).round() as usize) % 8]
                    .to_string()
            })
            .unwrap_or_else(|| "—".into()),
        "weather_low" => number(day().map(|day| day.low_c), precision, "°C", -100., 65.),
        "weather_high" => number(day().map(|day| day.high_c), precision, "°C", -100., 65.),
        "weather_rain" => {
            let now = now.with_timezone(&chrono::Utc);
            let hour = w
                .hourly
                .iter()
                .find(|hour| hour.at <= now && now < hour.at + chrono::Duration::hours(1));
            number(
                hour.and_then(|h| h.precipitation_probability),
                precision,
                "%",
                0.,
                100.,
            )
        }
        "weather_city" => w.city.chars().take(128).collect(),
        "weather_source" => w.provider.label().into(),
        "weather_updated" => w.data_at.with_timezone(&Local).format("%H:%M").to_string(),
        "weather_status" if result.cached => "离线缓存".into(),
        "weather_status" if result.fallback => "备用来源".into(),
        "weather_status" => w.kind.chars().take(64).collect(),
        _ => "—".into(),
    }
}
pub fn format(template: &str, result: Option<&WeatherResult>, now: DateTime<Local>) -> String {
    if result.and_then(|r| r.weather.as_ref()).is_none() {
        return "天气暂无数据".into();
    }
    super::substitute(template, |token| {
        if token == "percent" {
            "%".into()
        } else {
            value(token, result, now)
        }
    })
}

/// Explicitly labelled sample data used only by the settings preview.
pub fn sample(now: DateTime<Local>) -> WeatherResult {
    let now = now.with_timezone(&chrono::Utc);
    WeatherResult {
        weather: Some(Weather {
            provider: crate::config::Provider::OpenMeteo,
            city: "示例城市".into(),
            latitude: 0.,
            longitude: 0.,
            data_at: now,
            fetched_at: now,
            temperature_c: 23.4,
            feels_like_c: Some(25.1),
            humidity_percent: Some(68.),
            wind_kmh: Some(12.6),
            wind_degrees: Some(135.),
            pressure_hpa: Some(1013.),
            visibility_km: Some(16.),
            description: "晴".into(),
            kind: "示例天气".into(),
            days: vec![crate::weather::DailyForecast {
                date: now.with_timezone(&Local).format("%Y-%m-%d").to_string(),
                low_c: 19.,
                high_c: 27.,
                description: "晴".into(),
            }],
            hourly: vec![crate::weather::HourlyForecast {
                at: now,
                temperature_c: 23.4,
                description: "晴".into(),
                precipitation_probability: Some(20.),
            }],
        }),
        cached: false,
        fallback: false,
        attempts: vec![],
        cache_error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn weather_formats_fields_units_precision_and_literal_provider_text() {
        let now = Local::now();
        let mut result = sample(now);
        assert_eq!(
            format(
                "%weather_condition% %weather_temp:1% · 体感 %weather_feels% · 湿度 %weather_humidity% · %weather_wind_dir% %weather_wind_ms:1%",
                Some(&result),
                now
            ),
            "晴 23.4°C · 体感 25°C · 湿度 68% · 东南风 3.5 m/s"
        );
        for (token, _, _) in FIELDS {
            assert_ne!(value(token, Some(&result), now), "—", "{token}");
        }
        for (_, preset) in PRESETS {
            validate(preset).unwrap();
        }
        result.weather.as_mut().unwrap().description = "%time% 80%".into();
        assert_eq!(
            format("%weather_condition%", Some(&result), now),
            "%time% 80%"
        );
        for invalid in [
            "%weather%",
            "%weather_temp:8%",
            "%weather_city:1%",
            "%weather_temp",
            "",
            "a\nb",
        ] {
            assert!(validate(invalid).is_err(), "{invalid}");
        }
    }
    #[test]
    fn unavailable_invalid_and_expired_hour_fields_are_not_fabricated() {
        let now = Local::now();
        let mut result = sample(now);
        let w = result.weather.as_mut().unwrap();
        w.feels_like_c = None;
        w.wind_kmh = Some(f64::NAN);
        w.humidity_percent = Some(101.);
        w.temperature_c = -0.01;
        w.hourly[0].at -= chrono::Duration::hours(1);
        w.days[0].date = "2000-01-01".into();
        assert_eq!(
            format(
                "%weather_temp% %weather_feels% %weather_wind% %weather_humidity% %weather_rain% %weather_low%",
                Some(&result),
                now
            ),
            "0°C — — — — —"
        );
        assert_eq!(format("%weather_temp%", None, now), "天气暂无数据");
        result.cached = true;
        assert_eq!(value("weather_status", Some(&result), now), "离线缓存");
    }
}
