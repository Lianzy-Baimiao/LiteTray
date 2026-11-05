use crate::{
    config::{self, Provider, WeatherConfig},
    http::JsonHttp,
};
use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyForecast {
    pub date: String,
    pub low_c: f64,
    pub high_c: f64,
    pub description: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Weather {
    pub provider: Provider,
    pub city: String,
    pub latitude: f64,
    pub longitude: f64,
    pub data_at: DateTime<Utc>,
    pub fetched_at: DateTime<Utc>,
    pub temperature_c: f64,
    pub feels_like_c: Option<f64>,
    pub humidity_percent: Option<f64>,
    pub wind_kmh: Option<f64>,
    pub description: String,
    pub kind: String,
    pub days: Vec<DailyForecast>,
}
impl Weather {
    pub fn summary(&self) -> String {
        format!("{} {:.0}°C", self.description, self.temperature_c)
    }
    fn validate(&self, config: &WeatherConfig, now: DateTime<Utc>) -> Result<(), String> {
        if !self.temperature_c.is_finite() || !(-100.0..=65.0).contains(&self.temperature_c) {
            return Err("温度数据无效".into());
        }
        if self
            .humidity_percent
            .is_some_and(|h| !h.is_finite() || !(0.0..=100.0).contains(&h))
        {
            return Err("湿度数据无效".into());
        }
        if self.data_at > now + Duration::minutes(60) {
            return Err("数据时间异常，超过当前时间一小时".into());
        }
        if now - self.data_at > Duration::minutes(config.max_data_age_minutes) {
            return Err("来源数据已过期".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attempt {
    pub provider: Provider,
    pub error: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherResult {
    pub weather: Option<Weather>,
    pub cached: bool,
    pub fallback: bool,
    pub attempts: Vec<Attempt>,
    pub cache_error: Option<String>,
}
impl WeatherResult {
    pub fn status(&self) -> String {
        match &self.weather {
            Some(w) => format!(
                "{} · {}{} · 数据时间 {}",
                w.provider.label(),
                w.kind,
                if self.cached {
                    " · 离线缓存（非实时）"
                } else if self.fallback {
                    " · 备用来源"
                } else {
                    ""
                },
                w.data_at
                    .with_timezone(&chrono::Local)
                    .format("%m-%d %H:%M")
            ),
            None => "天气暂不可用，请检查数据源或网络".into(),
        }
    }
}
pub trait Secrets {
    fn get(&self, provider: Provider) -> Result<Option<String>, String>;
}
pub struct NoSecrets;
impl Secrets for NoSecrets {
    fn get(&self, _: Provider) -> Result<Option<String>, String> {
        Ok(None)
    }
}

#[derive(Default)]
pub struct Engine {
    cooldowns: HashMap<Provider, (u32, DateTime<Utc>)>,
}
impl Engine {
    pub fn refresh(
        &mut self,
        http: &dyn JsonHttp,
        secrets: &dyn Secrets,
        config: &WeatherConfig,
        cache_path: &Path,
        now: DateTime<Utc>,
        force: bool,
    ) -> WeatherResult {
        let mut attempts = Vec::new();
        for (index, provider) in config.providers.iter().copied().enumerate() {
            if !force
                && let Some((_, until)) = self.cooldowns.get(&provider)
                && *until > now
            {
                attempts.push(Attempt {
                    provider,
                    error: format!(
                        "失败退避中，{} 后重试",
                        until.with_timezone(&chrono::Local).format("%H:%M")
                    ),
                });
                continue;
            }
            let result = fetch(http, secrets, provider, config, now).and_then(|w| {
                w.validate(config, now)?;
                Ok(w)
            });
            match result {
                Ok(weather) => {
                    self.cooldowns.remove(&provider);
                    let cache_error = serde_json::to_vec(&weather)
                        .map_err(|e| e.to_string())
                        .and_then(|bytes| config::atomic_write(cache_path, &bytes))
                        .err();
                    return WeatherResult {
                        weather: Some(weather),
                        cached: false,
                        fallback: index > 0,
                        attempts,
                        cache_error,
                    };
                }
                Err(error) => {
                    let count = self
                        .cooldowns
                        .get(&provider)
                        .map(|(c, _)| *c)
                        .unwrap_or(0)
                        .saturating_add(1);
                    let minutes = (1_i64 << count.min(5)).min(30);
                    self.cooldowns
                        .insert(provider, (count, now + Duration::minutes(minutes)));
                    attempts.push(Attempt { provider, error });
                }
            }
        }
        let cached = load_cache(cache_path, config, now);
        WeatherResult {
            weather: cached,
            cached: true,
            fallback: false,
            attempts,
            cache_error: None,
        }
    }
}
pub fn load_cache(path: &Path, config: &WeatherConfig, now: DateTime<Utc>) -> Option<Weather> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() > 2 * 1024 * 1024 {
        return None;
    }
    let w: Weather = serde_json::from_slice(&bytes).ok()?;
    if (w.latitude - config.latitude).abs() > 0.0001
        || (w.longitude - config.longitude).abs() > 0.0001
        || w.city != config.city
        || !config.providers.contains(&w.provider)
    {
        return None;
    }
    if w.fetched_at > now + Duration::minutes(5)
        || now - w.data_at > Duration::hours(config.cache_max_age_hours)
        || w.data_at > now + Duration::hours(1)
    {
        return None;
    }
    if !w.temperature_c.is_finite() || !(-100.0..=65.0).contains(&w.temperature_c) {
        return None;
    }
    Some(w)
}
fn fetch(
    http: &dyn JsonHttp,
    secrets: &dyn Secrets,
    provider: Provider,
    c: &WeatherConfig,
    now: DateTime<Utc>,
) -> Result<Weather, String> {
    let mut url = match provider {
        Provider::OpenMeteo => url::Url::parse("https://api.open-meteo.com/v1/forecast").unwrap(),
        Provider::MetNorway => {
            url::Url::parse("https://api.met.no/weatherapi/locationforecast/2.0/compact").unwrap()
        }
        Provider::QWeather => {
            config::validate_qweather_host(&c.qweather_host)?;
            url::Url::parse(&format!("https://{}/v7/weather/now", c.qweather_host))
                .map_err(|_| "和风 Host 无效")?
        }
        Provider::OpenWeather => {
            url::Url::parse("https://api.openweathermap.org/data/2.5/weather").unwrap()
        }
        Provider::Wttr => url::Url::parse(&format!(
            "https://wttr.in/{:.4},{:.4}",
            c.latitude, c.longitude
        ))
        .unwrap(),
    };
    let token = secrets.get(provider)?;
    let mut headers = Vec::new();
    match provider {
        Provider::OpenMeteo => {
            url.query_pairs_mut().append_pair("latitude",&c.latitude.to_string()).append_pair("longitude",&c.longitude.to_string()).append_pair("current","temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,wind_speed_10m").append_pair("daily","temperature_2m_max,temperature_2m_min,weather_code").append_pair("timezone","auto").append_pair("forecast_days","3");
        }
        Provider::MetNorway => {
            url.query_pairs_mut()
                .append_pair("lat", &format!("{:.4}", c.latitude))
                .append_pair("lon", &format!("{:.4}", c.longitude));
        }
        Provider::QWeather => {
            let secret = token
                .as_deref()
                .filter(|s| !s.is_empty())
                .ok_or("请在设置中保存和风 API Key")?;
            headers.push(("X-QW-Api-Key", secret));
            url.query_pairs_mut()
                .append_pair("location", &format!("{:.4},{:.4}", c.longitude, c.latitude))
                .append_pair("lang", "zh");
        }
        Provider::OpenWeather => {
            let secret = token
                .as_deref()
                .filter(|s| !s.is_empty())
                .ok_or("请在设置中保存 OpenWeather API Key")?;
            url.query_pairs_mut()
                .append_pair("lat", &c.latitude.to_string())
                .append_pair("lon", &c.longitude.to_string())
                .append_pair("units", "metric")
                .append_pair("lang", "zh_cn")
                .append_pair("appid", secret);
        }
        Provider::Wttr => {
            url.query_pairs_mut()
                .append_pair("format", "j1")
                .append_pair("lang", "zh-cn");
        }
    }
    let r = http.get(url.as_str(), &headers)?;
    if r.status != 200 {
        return Err(match r.status {
            401 | 403 => "访问受限，请检查 API Key、额度或网络".into(),
            429 => "服务限流，已切换备用来源".into(),
            _ => format!("服务返回 HTTP {}", r.status),
        });
    }
    parse(provider, &r.value, c, now)
}
fn number(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str()?.parse::<f64>().ok())
        .filter(|v| v.is_finite())
}
fn required(v: &Value, name: &str) -> Result<f64, String> {
    number(v).ok_or_else(|| format!("缺少有效字段：{name}"))
}
fn timestamp(v: &Value) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(v.as_str().ok_or("缺少数据时间")?)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| "数据时间格式无效".into())
}
pub fn parse(
    provider: Provider,
    v: &Value,
    c: &WeatherConfig,
    now: DateTime<Utc>,
) -> Result<Weather, String> {
    let mut w = Weather {
        provider,
        city: c.city.clone(),
        latitude: c.latitude,
        longitude: c.longitude,
        data_at: now,
        fetched_at: now,
        temperature_c: 0.,
        feels_like_c: None,
        humidity_percent: None,
        wind_kmh: None,
        description: String::new(),
        kind: "当前观测".into(),
        days: Vec::new(),
    };
    match provider {
        Provider::OpenMeteo => {
            let x = &v["current"];
            let local = x["time"].as_str().ok_or("缺少数据时间")?;
            let parsed = NaiveDateTime::parse_from_str(local, "%Y-%m-%dT%H:%M")
                .or_else(|_| NaiveDateTime::parse_from_str(local, "%Y-%m-%dT%H:%M:%S"))
                .map_err(|_| "数据时间无效")?;
            let offset = v["utc_offset_seconds"].as_i64().ok_or("缺少时区偏移")?;
            if !(-86400..=86400).contains(&offset) {
                return Err("时区偏移无效".into());
            }
            w.data_at = parsed.and_utc() - Duration::seconds(offset);
            w.temperature_c = required(&x["temperature_2m"], "temperature_2m")?;
            w.feels_like_c = number(&x["apparent_temperature"]);
            w.humidity_percent = number(&x["relative_humidity_2m"]);
            w.wind_kmh = number(&x["wind_speed_10m"]);
            w.description = wmo(x["weather_code"].as_i64().unwrap_or(-1)).into();
            w.kind = "当前预估".into();
            if let Some(dates) = v["daily"]["time"].as_array() {
                for (i, date) in dates.iter().take(7).enumerate() {
                    if let (Some(date), Some(low), Some(high)) = (
                        date.as_str(),
                        number(&v["daily"]["temperature_2m_min"][i]),
                        number(&v["daily"]["temperature_2m_max"][i]),
                    ) {
                        w.days.push(DailyForecast {
                            date: date.into(),
                            low_c: low,
                            high_c: high,
                            description: wmo(v["daily"]["weather_code"][i].as_i64().unwrap_or(-1))
                                .into(),
                        });
                    }
                }
            }
        }
        Provider::MetNorway => {
            let entries = v["properties"]["timeseries"]
                .as_array()
                .ok_or("缺少预报数据")?;
            let (time, x) = entries
                .iter()
                .filter_map(|x| timestamp(&x["time"]).ok().map(|t| (t, x)))
                .min_by_key(|(t, _)| (*t - now).num_seconds().abs())
                .ok_or("无有效预报时间")?;
            let d = &x["data"]["instant"]["details"];
            w.data_at = time;
            w.temperature_c = required(&d["air_temperature"], "air_temperature")?;
            w.humidity_percent = number(&d["relative_humidity"]);
            w.wind_kmh = number(&d["wind_speed"]).map(|x| x * 3.6);
            w.kind = "逐小时预报".into();
            let symbol = x["data"]["next_1_hours"]["summary"]["symbol_code"]
                .as_str()
                .or_else(|| x["data"]["next_6_hours"]["summary"]["symbol_code"].as_str())
                .unwrap_or("unknown");
            w.description = met_symbol(symbol).into();
            // Reject forecasts from an outdated model run, even when its timeline includes now.
            let updated = timestamp(&v["properties"]["meta"]["updated_at"])?;
            if now - updated > Duration::hours(12) {
                return Err("预报模型更新时间超过 12 小时".into());
            }
        }
        Provider::QWeather => {
            if v["code"].as_str() != Some("200") {
                return Err("和风服务报告失败，请检查 Key、Host 和额度".into());
            }
            let x = &v["now"];
            w.data_at = timestamp(&x["obsTime"])?;
            w.temperature_c = required(&x["temp"], "temp")?;
            w.feels_like_c = number(&x["feelsLike"]);
            w.humidity_percent = number(&x["humidity"]);
            w.wind_kmh = number(&x["windSpeed"]);
            w.description = x["text"].as_str().unwrap_or("天气未知").into();
        }
        Provider::OpenWeather => {
            w.data_at = Utc
                .timestamp_opt(v["dt"].as_i64().ok_or("缺少观测时间")?, 0)
                .single()
                .ok_or("观测时间无效")?;
            w.temperature_c = required(&v["main"]["temp"], "temp")?;
            w.feels_like_c = number(&v["main"]["feels_like"]);
            w.humidity_percent = number(&v["main"]["humidity"]);
            w.wind_kmh = number(&v["wind"]["speed"]).map(|x| x * 3.6);
            w.description = v["weather"][0]["description"]
                .as_str()
                .unwrap_or("天气未知")
                .into();
        }
        Provider::Wttr => {
            let x = &v["current_condition"][0];
            w.temperature_c = required(&x["temp_C"], "temp_C")?;
            w.feels_like_c = number(&x["FeelsLikeC"]);
            w.humidity_percent = number(&x["humidity"]);
            w.wind_kmh = number(&x["windspeedKmph"]);
            w.description = x["lang_zh-cn"][0]["value"]
                .as_str()
                .or_else(|| x["weatherDesc"][0]["value"].as_str())
                .unwrap_or("天气未知")
                .into();
            // wttr's observation_time is UTC without a date; use the most recent plausible day.
            let time = chrono::NaiveTime::parse_from_str(
                x["observation_time"].as_str().ok_or("缺少观测时间")?,
                "%I:%M %p",
            )
            .map_err(|_| "观测时间无效")?;
            w.data_at = now.date_naive().and_time(time).and_utc();
            if w.data_at > now + Duration::minutes(30) {
                w.data_at -= Duration::days(1);
            }
        }
    }
    Ok(w)
}
pub fn wmo(code: i64) -> &'static str {
    match code {
        0 => "晴",
        1 => "晴间多云",
        2 => "多云",
        3 => "阴",
        45 | 48 => "雾",
        51 | 53 | 55 => "毛毛雨",
        56 | 57 => "冻毛毛雨",
        61 => "小雨",
        63 => "中雨",
        65 => "大雨",
        66 | 67 => "冻雨",
        71 => "小雪",
        73 => "中雪",
        75 | 77 => "大雪",
        80..=82 => "阵雨",
        85 | 86 => "阵雪",
        95 => "雷雨",
        96 | 99 => "雷雨伴冰雹",
        _ => "天气未知",
    }
}
fn met_symbol(symbol: &str) -> &'static str {
    let s = symbol.split('_').next().unwrap_or(symbol);
    match s {
        "clearsky" => "晴",
        "fair" => "晴间多云",
        "partlycloudy" => "多云",
        "cloudy" => "阴",
        "fog" => "雾",
        s if s.contains("thunder") => "雷雨",
        s if s.contains("snow") => "雪",
        s if s.contains("sleet") => "雨夹雪",
        s if s.contains("heavyrain") => "大雨",
        s if s.contains("rain") => "雨",
        _ => "天气未知",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::JsonResponse;
    use std::sync::Mutex;
    struct Mock {
        responses: Mutex<Vec<Result<JsonResponse, String>>>,
    }
    impl JsonHttp for Mock {
        fn get(&self, _: &str, _: &[(&str, &str)]) -> Result<JsonResponse, String> {
            self.responses.lock().unwrap().remove(0)
        }
    }
    fn instant() -> DateTime<Utc> {
        "2026-11-06T00:00:00Z".parse().unwrap()
    }
    fn openmeteo(time: &str) -> Value {
        serde_json::json!({"utc_offset_seconds":28800,"current":{"time":time,"temperature_2m":24.2,"relative_humidity_2m":80,"weather_code":2}})
    }
    #[test]
    fn converts_source_timezone() {
        let w = parse(
            Provider::OpenMeteo,
            &openmeteo("2026-11-06T08:00"),
            &WeatherConfig::default(),
            instant(),
        )
        .unwrap();
        assert_eq!(w.data_at, instant());
        assert_eq!(w.description, "多云");
    }
    #[test]
    fn rejects_old_and_future_data() {
        let c = WeatherConfig::default();
        for time in ["2026-11-05T08:00", "2026-11-07T08:00"] {
            let w = parse(Provider::OpenMeteo, &openmeteo(time), &c, instant()).unwrap();
            assert!(w.validate(&c, instant()).is_err());
        }
    }
    #[test]
    fn fails_over_without_masking_primary_failure() {
        let c = WeatherConfig {
            providers: vec![Provider::MetNorway, Provider::OpenMeteo],
            ..WeatherConfig::default()
        };
        let h = Mock {
            responses: Mutex::new(vec![
                Err("timeout".into()),
                Ok(JsonResponse {
                    status: 200,
                    value: openmeteo("2026-11-06T08:00"),
                    etag: None,
                }),
            ]),
        };
        let p =
            std::env::temp_dir().join(format!("litetray-weather-test-{}.json", std::process::id()));
        let result = Engine::default().refresh(&h, &NoSecrets, &c, &p, instant(), true);
        assert!(result.fallback);
        assert_eq!(result.attempts.len(), 1);
        assert_eq!(
            result.weather.as_ref().unwrap().provider,
            Provider::OpenMeteo
        );
        assert!(load_cache(&p, &c, instant() + Duration::hours(30)).is_none());
        let moved = WeatherConfig { latitude: 40., ..c };
        assert!(load_cache(&p, &moved, instant()).is_none());
        let _ = std::fs::remove_file(p);
    }
    #[test]
    fn does_not_report_auth_response_as_weather() {
        let v = serde_json::json!({"code":"401"});
        assert!(parse(Provider::QWeather, &v, &WeatherConfig::default(), instant()).is_err());
    }
}
