//! User-initiated city search and approximate public-IP location. These
//! services require no API keys. Only city/region/coordinates are retained.
use crate::http::JsonHttp;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub name: String,
    pub region: String,
    pub latitude: f64,
    pub longitude: f64,
    pub source: &'static str,
}
impl Place {
    pub fn label(&self) -> String {
        if self.region.is_empty() {
            self.name.clone()
        } else {
            format!("{} · {}", self.name, self.region)
        }
    }
    fn valid(&self) -> bool {
        !self.name.is_empty()
            && self.latitude.is_finite()
            && self.longitude.is_finite()
            && (-90.0..=90.0).contains(&self.latitude)
            && (-180.0..=180.0).contains(&self.longitude)
    }
}
fn text(value: &Value, field: &str) -> String {
    value[field]
        .as_str()
        .filter(|s| s.len() <= 240 && !s.chars().any(char::is_control))
        .unwrap_or_default()
        .trim()
        .to_owned()
}
fn number(value: &Value, field: &str) -> f64 {
    value[field]
        .as_f64()
        .or_else(|| value[field].as_str()?.parse().ok())
        .unwrap_or(f64::NAN)
}
fn region(value: &Value, name: &str, fields: &[&str]) -> String {
    let mut parts = Vec::new();
    for field in fields {
        let item = text(value, field);
        if !item.is_empty() && item != name && !parts.contains(&item) {
            parts.push(item);
        }
    }
    parts.join(" · ")
}
pub fn validate_query(query: &str) -> Result<&str, String> {
    let query = query.trim();
    if !(2..=80).contains(&query.chars().count()) || query.chars().any(char::is_control) {
        return Err("请输入至少 2 个字的城市名、区县名或拼音（最多 80 字）".into());
    }
    Ok(query)
}
pub fn search(http: &dyn JsonHttp, query: &str) -> Result<Vec<Place>, String> {
    let query = validate_query(query)?;
    let mut url = url::Url::parse("https://geocoding-api.open-meteo.com/v1/search").unwrap();
    url.query_pairs_mut()
        .append_pair("name", query)
        .append_pair("count", "20")
        .append_pair("language", "zh")
        .append_pair("format", "json");
    let response = http.get(url.as_str(), &[])?;
    if response.status != 200 || response.value["error"].as_bool() == Some(true) {
        return Err(format!(
            "城市搜索暂不可用（HTTP {}），请稍后重试",
            response.status
        ));
    }
    if !response.value.is_object() {
        return Err("城市搜索返回了无效数据".into());
    }
    let Some(rows) = response.value.get("results") else {
        return Ok(vec![]);
    };
    let rows = rows.as_array().ok_or("城市搜索返回了无效列表")?;
    let mut places = Vec::new();
    for row in rows.iter().take(20) {
        let name = text(row, "name");
        let place = Place {
            region: region(row, &name, &["admin2", "admin1", "country"]),
            name,
            latitude: number(row, "latitude"),
            longitude: number(row, "longitude"),
            source: "Open-Meteo / GeoNames",
        };
        if place.valid() && !places.contains(&place) {
            places.push(place);
        }
    }
    if !rows.is_empty() && places.is_empty() {
        return Err("搜索结果缺少有效城市或坐标，请换个城市名重试".into());
    }
    Ok(places)
}
pub fn nearby(http: &dyn JsonHttp) -> Result<Place, String> {
    for (url, source, needs_success) in [
        (
            "https://ipwho.is/?lang=zh-CN&fields=success,message,city,region,country,latitude,longitude",
            "IPWHOIS",
            true,
        ),
        ("https://get.geojs.io/v1/ip/geo.json", "GeoJS", false),
    ] {
        let Ok(response) = http.get(url, &[]) else {
            continue;
        };
        if response.status != 200
            || needs_success && response.value["success"].as_bool() != Some(true)
        {
            continue;
        }
        let name = text(&response.value, "city");
        let place = Place {
            region: region(&response.value, &name, &["region", "country"]),
            name,
            latitude: number(&response.value, "latitude"),
            longitude: number(&response.value, "longitude"),
            source,
        };
        if place.valid() {
            return Ok(place);
        }
    }
    Err("暂时无法获取当地位置，可输入城市名搜索；原位置已保留".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::JsonResponse;
    use serde_json::json;
    use std::cell::RefCell;
    struct Fake {
        responses: RefCell<Vec<Result<JsonResponse, String>>>,
        urls: RefCell<Vec<String>>,
    }
    impl Fake {
        fn new(values: Vec<Value>) -> Self {
            Self {
                responses: RefCell::new(
                    values
                        .into_iter()
                        .rev()
                        .map(|value| {
                            Ok(JsonResponse {
                                status: 200,
                                value,
                                etag: None,
                            })
                        })
                        .collect(),
                ),
                urls: RefCell::new(vec![]),
            }
        }
    }
    impl JsonHttp for Fake {
        fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<JsonResponse, String> {
            assert!(headers.is_empty());
            self.urls.borrow_mut().push(url.into());
            self.responses
                .borrow_mut()
                .pop()
                .expect("unexpected request")
        }
    }
    #[test]
    fn search_encodes_chinese_and_disambiguates_names_without_guessing() {
        let row = json!({"name":"朝阳","latitude":41.57,"longitude":120.45,"admin1":"辽宁省","country":"中国"});
        let http = Fake::new(vec![
            json!({"results":[row, row, {"name":"朝阳","latitude":43.83,"longitude":125.28,"admin1":"吉林省","country":"中国"}, {"name":"invalid","latitude":91,"longitude":0}]}),
        ]);
        let result = search(&http, " 朝阳 & 中文 ").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].label(), "朝阳 · 辽宁省 · 中国");
        let url = url::Url::parse(&http.urls.borrow()[0]).unwrap();
        assert_eq!(url.host_str(), Some("geocoding-api.open-meteo.com"));
        assert_eq!(
            url.query_pairs().find(|(k, _)| k == "name").unwrap().1,
            "朝阳 & 中文"
        );
        assert_eq!(result[0].latitude, 41.57);
    }
    #[test]
    fn invalid_queries_and_service_errors_do_not_become_locations() {
        let http = Fake::new(vec![
            json!({}),
            json!({"error":true}),
            json!({"results":[{"name":"x","latitude":0,"longitude":181}]}),
        ]);
        assert!(search(&http, " ").is_err());
        assert!(search(&http, "a\nb").is_err());
        assert!(http.urls.borrow().is_empty());
        assert!(search(&http, "不存在").unwrap().is_empty());
        assert!(search(&http, "深圳").is_err());
        assert!(search(&http, "北京").is_err());
    }
    #[test]
    fn ip_lookup_falls_back_and_never_uses_missing_coordinates_as_zero() {
        let http = Fake::new(vec![
            json!({"success":true,"city":"invalid"}),
            json!({"city":"Shenzhen","region":"Guangdong","country":"China","latitude":"22.55","longitude":"114.06"}),
        ]);
        let place = nearby(&http).unwrap();
        assert_eq!(place.source, "GeoJS");
        assert_eq!((place.latitude, place.longitude), (22.55, 114.06));
        assert_eq!(http.urls.borrow().len(), 2);
        let http = Fake::new(vec![
            json!({"success":false}),
            json!({"city":"x","latitude":"NaN","longitude":"0"}),
        ]);
        assert!(nearby(&http).is_err());
    }
    #[test]
    fn primary_ip_result_keeps_only_place_and_short_circuits() {
        let http = Fake::new(vec![
            json!({"success":true,"city":"深圳","region":"广东","country":"中国","latitude":22.55,"longitude":114.06,"ip":"unused"}),
        ]);
        let place = nearby(&http).unwrap();
        assert_eq!(place.label(), "深圳 · 广东 · 中国");
        assert_eq!(http.urls.borrow().len(), 1);
        assert!(!format!("{place:?}").contains("unused"));
    }
    #[test]
    #[ignore = "User-initiated live geocoding and IP services; no profile writes"]
    fn live_public_location_services() {
        let http = crate::http::Client::new(8);
        let cities = search(&http, "深圳").unwrap();
        assert!(
            cities
                .iter()
                .any(|p| p.name == "深圳" && (p.latitude - 22.55).abs() < 0.2)
        );
        let place = nearby(&http).unwrap();
        assert!(place.valid());
        eprintln!(
            "public city search: {} results; IP source: {}",
            cities.len(),
            place.source
        );
    }
}
