use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HourlyForecast {
    pub at: DateTime<Utc>,
    pub temperature_c: f64,
    pub description: String,
    pub precipitation_probability: Option<f64>,
}
pub(super) fn meteo_time(value: &Value, offset: i64) -> Option<DateTime<Utc>> {
    if let Some(seconds) = value.as_i64() {
        return Utc.timestamp_opt(seconds, 0).single();
    }
    let value = value.as_str()?;
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S"))
        .ok()?
        .and_utc()
        .checked_sub_signed(Duration::seconds(offset))
}
pub(super) fn open_meteo(value: &Value, offset: i64) -> Vec<HourlyForecast> {
    let data = &value["hourly"];
    let Some(times) = data["time"].as_array() else {
        return vec![];
    };
    times
        .iter()
        .take(168)
        .enumerate()
        .filter_map(|(i, time)| {
            Some(HourlyForecast {
                at: meteo_time(time, offset)?,
                temperature_c: number(&data["temperature_2m"][i])?,
                description: wmo(data["weather_code"][i].as_i64().unwrap_or(-1)).into(),
                precipitation_probability: number(&data["precipitation_probability"][i]),
            })
        })
        .collect()
}
pub(super) fn met_norway(entries: &[Value]) -> Vec<HourlyForecast> {
    entries
        .iter()
        .take(168)
        .filter_map(|value| {
            // Later parts of MET's timeline can be six-hourly. Do not present
            // those entries as one-hour forecasts or invent interpolated values.
            let next = &value["data"]["next_1_hours"];
            if !next.is_object() {
                return None;
            }
            Some(HourlyForecast {
                at: timestamp(&value["time"]).ok()?,
                temperature_c: number(&value["data"]["instant"]["details"]["air_temperature"])?,
                description: met_symbol(
                    next["summary"]["symbol_code"].as_str().unwrap_or("unknown"),
                )
                .into(),
                precipitation_probability: number(&next["details"]["probability_of_precipitation"]),
            })
        })
        .collect()
}
pub(super) fn sanitize(hours: &mut Vec<HourlyForecast>, now: DateTime<Utc>) {
    hours.retain(|h| {
        h.temperature_c.is_finite()
            && (-100.0..=65.0).contains(&h.temperature_c)
            && h.at > now - Duration::hours(1)
            && h.at < now + Duration::hours(72)
    });
    hours.sort_by_key(|h| h.at);
    hours.dedup_by_key(|h| h.at);
    hours.truncate(72);
    for hour in hours {
        if hour
            .precipitation_probability
            .is_some_and(|p| !p.is_finite() || !(0.0..=100.0).contains(&p))
        {
            hour.precipitation_probability = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn open_meteo_requests_hourly_weather_in_the_existing_keyless_call() {
        struct Request;
        impl JsonHttp for Request {
            fn get(
                &self,
                address: &str,
                headers: &[(&str, &str)],
            ) -> Result<crate::http::JsonResponse, String> {
                let url = url::Url::parse(address).unwrap();
                assert_eq!(url.host_str(), Some("api.open-meteo.com"));
                assert!(headers.is_empty());
                let query: HashMap<_, _> = url.query_pairs().collect();
                assert_eq!(
                    query.get("hourly").unwrap(),
                    "temperature_2m,weather_code,precipitation_probability"
                );
                assert_eq!(query.get("timeformat").unwrap(), "unixtime");
                assert!(!query.contains_key("apikey"));
                Ok(crate::http::JsonResponse {
                    status: 200,
                    value: meteo(),
                    etag: None,
                })
            }
        }
        let w = fetch(
            &Request,
            &NoSecrets,
            Provider::OpenMeteo,
            &WeatherConfig::default(),
            now(),
        )
        .unwrap();
        assert_eq!(w.hourly.len(), 3);
    }
    #[test]
    #[ignore = "Explicit public weather request, no key and no profile writes"]
    fn live_keyless_hourly_forecast() {
        let now = Utc::now();
        let config = WeatherConfig::default();
        let http = crate::http::Client::new(15);
        let w = fetch(&http, &NoSecrets, Provider::OpenMeteo, &config, now).unwrap();
        w.validate(&config, now).unwrap();
        let hours = w.upcoming_hours(now);
        assert_eq!(hours.len(), 24);
        assert!(
            hours
                .windows(2)
                .all(|pair| pair[1].at - pair[0].at == Duration::hours(1))
        );
        assert!(hours.iter().any(|h| h.precipitation_probability.is_some()));
        assert_eq!(w.days.len(), 3);
        eprintln!(
            "keyless hourly: provider={}, hours={}, days={}, first={}, last={}",
            w.provider.label(),
            hours.len(),
            w.days.len(),
            hours[0].at,
            hours[23].at
        );
        let met = fetch(&http, &NoSecrets, Provider::MetNorway, &config, now).unwrap();
        met.validate(&config, now).unwrap();
        let hours = met.upcoming_hours(now);
        assert_eq!(hours.len(), 24);
        assert!(
            hours
                .windows(2)
                .all(|pair| pair[1].at - pair[0].at == Duration::hours(1))
        );
        eprintln!(
            "keyless hourly fallback: provider={}, hours={}",
            met.provider.label(),
            hours.len()
        );
    }
    fn now() -> DateTime<Utc> {
        "2026-11-07T15:30:00Z".parse().unwrap()
    }
    fn meteo() -> Value {
        serde_json::json!({"utc_offset_seconds":28800,"current":{"time":now().timestamp(),"temperature_2m":22,"weather_code":2},
            "daily":{"time":[1793980800i64],"temperature_2m_min":[17],"temperature_2m_max":[25],"weather_code":[2]},
            "hourly":{"time":["2026-11-07T23:00","2026-11-08T00:00","2026-11-08T01:00","2026-11-08T02:00"],
            "temperature_2m":[22,21,null,19],"weather_code":[2,61,3,0],"precipitation_probability":[0,75,10,140]}})
    }
    #[test]
    fn hourly_timestamps_cross_midnight_and_bad_rows_do_not_break_current_weather() {
        let weather = parse(
            Provider::OpenMeteo,
            &meteo(),
            &WeatherConfig::default(),
            now(),
        )
        .unwrap();
        assert_eq!(weather.data_at, now());
        assert_eq!(weather.days[0].date, "2026-11-07");
        assert_eq!(weather.hourly.len(), 3);
        assert_eq!(
            weather.hourly[1].at,
            "2026-11-07T16:00:00Z".parse::<DateTime<Utc>>().unwrap()
        );
        assert_eq!(weather.hourly[1].precipitation_probability, Some(75.));
        assert_eq!(weather.hourly[2].precipitation_probability, None);
        assert_eq!(weather.hourly[1].description, "小雨");
        assert_eq!(
            meteo_time(&serde_json::json!(now().timestamp()), 28800),
            Some(now())
        );
    }
    #[test]
    fn cache_is_backward_compatible_and_hours_expire_without_repeating_old_forecasts() {
        let weather = parse(
            Provider::OpenMeteo,
            &meteo(),
            &WeatherConfig::default(),
            now(),
        )
        .unwrap();
        let mut encoded = serde_json::to_value(&weather).unwrap();
        encoded.as_object_mut().unwrap().remove("hourly");
        assert!(
            serde_json::from_value::<Weather>(encoded)
                .unwrap()
                .hourly
                .is_empty()
        );
        let path =
            std::env::temp_dir().join(format!("litetray-hourly-cache-{}.json", std::process::id()));
        config::atomic_write(&path, &serde_json::to_vec(&weather).unwrap()).unwrap();
        let cached =
            load_cache(&path, &WeatherConfig::default(), now() + Duration::hours(1)).unwrap();
        assert_eq!(cached.hourly.len(), 2);
        assert_eq!(cached.hourly[0].precipitation_probability, Some(75.));
        std::fs::remove_file(path).unwrap();
        assert!(
            weather
                .upcoming_hours(now() + Duration::hours(6))
                .is_empty()
        );
        let mut hours = weather.hourly.clone();
        hours.extend(weather.hourly.clone());
        sanitize(&mut hours, now() + Duration::hours(1));
        assert_eq!(hours.len(), 2);
        assert_eq!(hours[0].at.hour(), 16);
    }
    #[test]
    fn norway_uses_only_hourly_rows_and_does_not_turn_rain_amount_into_probability() {
        let data = serde_json::json!([
            {"time":"2026-11-07T16:00:00Z","data":{"instant":{"details":{"air_temperature":20}},"next_1_hours":{"summary":{"symbol_code":"rain"},"details":{"precipitation_amount":2.4}}}},
            {"time":"2026-11-07T22:00:00Z","data":{"instant":{"details":{"air_temperature":18}},"next_6_hours":{"summary":{"symbol_code":"rain"}}}}
        ]);
        let hours = met_norway(data.as_array().unwrap());
        assert_eq!(hours.len(), 1);
        assert_eq!(hours[0].precipitation_probability, None);
    }
    use chrono::Timelike;
}
