//! State Council holiday schedules from holiday-cn. A listed off-day is part
//! of the announced break, not necessarily a statutory holiday in its own right.
//! Network/cache work is run by the host's worker; painting only reads Catalog.
use crate::{config, http::JsonHttp};
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
};

pub const SOURCE: &str = "https://github.com/NateScarlet/holiday-cn";
const MAX_BYTES: u64 = 256 * 1024;
const FIRST_YEAR: i32 = 2007;
const LAST_YEAR: i32 = 2100;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Day {
    pub name: String,
    pub date: NaiveDate,
    #[serde(rename = "isOffDay")]
    pub off: bool,
}
impl Day {
    pub fn badge(&self) -> &'static str {
        if self.off { "休" } else { "班" }
    }
    pub fn description(&self) -> String {
        format!(
            "{} · {}",
            self.name,
            if self.off { "放假" } else { "调休补班" }
        )
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Year {
    pub year: i32,
    pub papers: Vec<String>,
    pub days: Vec<Day>,
}
impl Year {
    pub fn parse(bytes: &[u8], expected: i32) -> Result<Self, String> {
        if bytes.len() as u64 > MAX_BYTES {
            return Err("节假日数据过大".into());
        }
        let data: Self = serde_json::from_slice(bytes).map_err(|_| "节假日数据格式无效")?;
        data.validate(expected)?;
        Ok(data)
    }
    fn validate(&self, expected: i32) -> Result<(), String> {
        if !(FIRST_YEAR..=LAST_YEAR).contains(&expected)
            || self.year != expected
            || self.papers.len() > 20
            || self.days.len() > 400
        {
            return Err("节假日数据年份或数量不匹配".into());
        }
        if self.papers.is_empty() || self.days.is_empty() {
            return Err("该年份暂未发布放假安排".into());
        }
        for paper in &self.papers {
            official_url(paper).ok_or("节假日数据缺少有效的国务院原文链接")?;
        }
        let first = NaiveDate::from_ymd_opt(expected - 1, 12, 1).unwrap();
        let last = NaiveDate::from_ymd_opt(expected + 1, 1, 31).unwrap();
        let mut dates = BTreeSet::new();
        for day in &self.days {
            if day.date < first
                || day.date > last
                || !dates.insert(day.date)
                || day.name.trim().is_empty()
                || day.name.chars().count() > 32
                || day.name.chars().any(char::is_control)
            {
                return Err("节假日数据包含异常日期、重复日期或名称".into());
            }
        }
        Ok(())
    }
}
fn official_url(value: &str) -> Option<String> {
    let mut url = url::Url::parse(value).ok()?;
    if value.len() > 1024
        || !matches!(url.scheme(), "http" | "https")
        || !matches!(url.host_str(), Some("www.gov.cn" | "gov.cn"))
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    url.set_scheme("https").ok()?;
    Some(url.into())
}

#[derive(Clone)]
struct Entry {
    data: Option<Year>,
    next_check: DateTime<Utc>,
    note: String,
}
pub struct Catalog {
    years: BTreeMap<i32, Entry>,
    pub revision: u64,
}
impl Default for Catalog {
    fn default() -> Self {
        let mut catalog = Self {
            years: BTreeMap::new(),
            revision: 0,
        };
        for (year, bytes) in [
            (
                2025,
                include_bytes!("../data/holidays/2025.json").as_slice(),
            ),
            (
                2026,
                include_bytes!("../data/holidays/2026.json").as_slice(),
            ),
            (
                2027,
                include_bytes!("../data/holidays/2027.json").as_slice(),
            ),
        ] {
            if let Ok(data) = Year::parse(bytes, year) {
                catalog.years.insert(
                    year,
                    Entry {
                        data: Some(data),
                        next_check: DateTime::<Utc>::MIN_UTC,
                        note: "内置数据".into(),
                    },
                );
            }
        }
        catalog
    }
}
impl Catalog {
    pub fn day(&self, date: NaiveDate) -> Option<&Day> {
        // Next year's notification can override December of this year.
        self.years
            .range(date.year() - 1..=date.year() + 1)
            .rev()
            .filter_map(|(_, entry)| entry.data.as_ref())
            .find_map(|year| year.days.iter().find(|d| d.date == date))
    }
    pub fn source(&self, date: NaiveDate) -> String {
        self.years
            .range(date.year() - 1..=date.year() + 1)
            .rev()
            .filter_map(|(_, entry)| entry.data.as_ref())
            .find(|year| year.days.iter().any(|d| d.date == date))
            .or_else(|| self.years.get(&date.year()).and_then(|e| e.data.as_ref()))
            .and_then(|year| year.papers.first())
            .and_then(|url| official_url(url))
            .unwrap_or_else(|| SOURCE.into())
    }
    pub fn status(&self, year: i32) -> String {
        match self.years.get(&year) {
            Some(entry) if entry.data.is_some() => format!("{year} 国务院安排 · {}", entry.note),
            Some(entry) => format!("{year} {}", entry.note),
            None if !(FIRST_YEAR..=LAST_YEAR).contains(&year) => "此年份无节假日数据".into(),
            None => format!("{year} 放假安排暂无数据"),
        }
    }
    pub fn needs_refresh(&self, year: i32, now: DateTime<Utc>) -> bool {
        (FIRST_YEAR..=LAST_YEAR).contains(&year)
            && self
                .years
                .get(&year)
                .is_none_or(|entry| now >= entry.next_check)
    }
    pub fn seed(&self, year: i32) -> Option<Year> {
        self.years.get(&year).and_then(|entry| entry.data.clone())
    }
    pub fn apply(&mut self, year: i32, outcome: Outcome) {
        let entry = self.years.entry(year).or_insert(Entry {
            data: None,
            next_check: outcome.next_check,
            note: String::new(),
        });
        if let Some(data) = outcome.data {
            entry.data = Some(data);
        }
        entry.note = outcome.note;
        entry.next_check = outcome.next_check;
        self.revision = self.revision.wrapping_add(1);
    }
}
pub fn years_for_month(month: NaiveDate) -> Vec<i32> {
    let mut years = vec![month.year()];
    if month.month() <= 2 {
        years.push(month.year() - 1);
    }
    if month.month() >= 11 {
        years.push(month.year() + 1);
    }
    years.retain(|y| (FIRST_YEAR..=LAST_YEAR).contains(y));
    years
}
#[derive(Serialize, Deserialize)]
struct Cache {
    data: Year,
    checked_at: DateTime<Utc>,
    etag: Option<String>,
}
pub struct Outcome {
    pub data: Option<Year>,
    pub next_check: DateTime<Utc>,
    pub note: String,
}
fn cache(path: &Path, year: i32) -> Option<Cache> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_BYTES {
        return None;
    }
    let cached: Cache = serde_json::from_slice(&bytes).ok()?;
    cached.data.validate(year).ok()?;
    Some(cached)
}
fn safe_etag(etag: Option<&str>) -> Option<&str> {
    etag.filter(|v| v.len() <= 256 && !v.chars().any(char::is_control))
}
pub fn refresh(
    http: &dyn JsonHttp,
    directory: &Path,
    year: i32,
    seed: Option<Year>,
    now: DateTime<Utc>,
) -> Outcome {
    if !(FIRST_YEAR..=LAST_YEAR).contains(&year) {
        return Outcome {
            data: None,
            next_check: now + Duration::hours(24),
            note: "此年份无节假日数据".into(),
        };
    }
    let path = directory.join(format!("{year}.json"));
    let old = cache(&path, year);
    let mut data = old
        .as_ref()
        .map(|c| c.data.clone())
        .or_else(|| seed.filter(|data| data.validate(year).is_ok()));
    if let Some(old) = &old
        && old.checked_at <= now
        && now - old.checked_at < Duration::hours(24)
    {
        return Outcome {
            data,
            next_check: old.checked_at + Duration::hours(24),
            note: "本地缓存".into(),
        };
    }
    let mut reason = "更新失败".to_owned();
    for (index, url) in [
        format!("https://raw.githubusercontent.com/NateScarlet/holiday-cn/master/{year}.json"),
        format!("https://cdn.jsdelivr.net/gh/NateScarlet/holiday-cn@master/{year}.json"),
    ]
    .iter()
    .enumerate()
    {
        let etag = if index == 0 {
            old.as_ref().and_then(|c| safe_etag(c.etag.as_deref()))
        } else {
            None
        };
        let headers = etag.map(|v| vec![("If-None-Match", v)]).unwrap_or_default();
        let response = match http.get(url, &headers) {
            Ok(response) => response,
            Err(_) => {
                continue;
            }
        };
        let result = match response.status {
            200 => serde_json::to_vec(&response.value)
                .map_err(|_| "数据格式无效".to_owned())
                .and_then(|b| Year::parse(&b, year)),
            304 if etag.is_some() => old
                .as_ref()
                .map(|c| c.data.clone())
                .ok_or_else(|| "无本地数据".into()),
            404 => {
                reason = "暂无公开数据".into();
                continue;
            }
            _ => continue,
        };
        let document = match result {
            Ok(value) => value,
            Err(error) => {
                reason = error;
                continue;
            }
        };
        let new_etag = if index == 0 {
            safe_etag(response.etag.as_deref())
                .or(if response.status == 304 { etag } else { None })
                .map(str::to_owned)
        } else {
            None
        };
        let saved = Cache {
            data: document.clone(),
            checked_at: now,
            etag: new_etag,
        };
        let stored = serde_json::to_vec(&saved)
            .map_err(|e| e.to_string())
            .and_then(|bytes| config::atomic_write(&path, &bytes))
            .is_ok();
        data = Some(document);
        return Outcome {
            data,
            next_check: now + Duration::hours(24),
            note: if stored {
                "已更新"
            } else {
                "已更新，缓存未保存"
            }
            .into(),
        };
    }
    let note = if data.is_some() {
        "离线数据 · 更新未成功".into()
    } else {
        reason
    };
    Outcome {
        data,
        next_check: now + Duration::hours(1),
        note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::JsonResponse;
    use serde_json::{Value, json};
    use std::{
        cell::RefCell,
        collections::VecDeque,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    fn date(value: &str) -> NaiveDate {
        value.parse().unwrap()
    }
    fn now() -> DateTime<Utc> {
        "2026-11-07T00:00:00Z".parse().unwrap()
    }
    fn document(year: i32, date: &str, off: bool) -> Value {
        json!({"year":year,"papers":["http://www.gov.cn/zhengce/fixture.htm"],"days":[{"date":date,"name":"元旦","isOffDay":off}]})
    }
    fn year() -> Year {
        Year::parse(
            &serde_json::to_vec(&document(2026, "2026-01-01", true)).unwrap(),
            2026,
        )
        .unwrap()
    }
    fn response(status: u16, value: Value) -> Result<JsonResponse, String> {
        Ok(JsonResponse {
            status,
            value,
            etag: Some("\"fixture\"".into()),
        })
    }
    type Request = (String, Vec<(String, String)>);
    #[derive(Default)]
    struct Http {
        responses: RefCell<VecDeque<Result<JsonResponse, String>>>,
        calls: RefCell<Vec<Request>>,
    }
    impl Http {
        fn new(responses: Vec<Result<JsonResponse, String>>) -> Self {
            Self {
                responses: RefCell::new(responses.into()),
                ..Self::default()
            }
        }
    }
    impl JsonHttp for Http {
        fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<JsonResponse, String> {
            self.calls.borrow_mut().push((
                url.into(),
                headers
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            ));
            self.responses
                .borrow_mut()
                .pop_front()
                .expect("unexpected network request")
        }
    }
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            Self(std::env::temp_dir().join(format!(
                "litetray-holiday-{}-{}-{}",
                std::process::id(),
                Utc::now().timestamp_nanos_opt().unwrap(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )))
        }
        fn store(&self, checked_at: DateTime<Utc>) {
            config::atomic_write(
                &self.0.join("2026.json"),
                &serde_json::to_vec(&Cache {
                    data: year(),
                    checked_at,
                    etag: Some("\"cached\"".into()),
                })
                .unwrap(),
            )
            .unwrap();
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn embedded_schedules_distinguish_breaks_and_adjusted_work() {
        let catalog = Catalog::default();
        for day in [
            "2026-01-01",
            "2026-02-15",
            "2026-02-23",
            "2026-10-07",
            "2027-02-05",
            "2027-02-13",
        ] {
            assert!(catalog.day(date(day)).unwrap().off, "{day}");
        }
        for day in [
            "2026-01-04",
            "2026-02-14",
            "2026-02-28",
            "2026-10-10",
            "2027-01-31",
            "2027-02-14",
        ] {
            let day = catalog.day(date(day)).unwrap();
            assert_eq!(day.badge(), "班");
            assert!(day.description().contains("调休补班"));
        }
        assert!(
            catalog.day(date("2026-11-07")).is_none(),
            "ordinary weekends must not be invented as announced holidays"
        );
        assert!(
            catalog.day(date("2030-10-01")).is_none(),
            "unknown schedule must not be guessed"
        );
        assert!(catalog.status(2030).contains("暂无数据"));
        assert!(
            catalog
                .source(date("2027-02-14"))
                .starts_with("https://www.gov.cn/")
        );
    }
    #[test]
    fn next_year_notification_can_override_previous_december() {
        let mut catalog = Catalog::default();
        let data = Year::parse(
            &serde_json::to_vec(&document(2027, "2026-12-31", true)).unwrap(),
            2027,
        )
        .unwrap();
        catalog.apply(
            2027,
            Outcome {
                data: Some(data),
                next_check: now(),
                note: "测试".into(),
            },
        );
        assert_eq!(catalog.day(date("2026-12-31")).unwrap().badge(), "休");
        assert_eq!(
            catalog.source(date("2026-12-31")),
            "https://www.gov.cn/zhengce/fixture.htm"
        );
        assert!(years_for_month(date("2026-12-01")).contains(&2027));
        assert!(years_for_month(date("2027-01-01")).contains(&2026));
    }
    #[test]
    fn malformed_wrong_year_and_unpublished_documents_are_rejected() {
        let good = document(2026, "2026-01-01", true);
        let mut duplicate = good.clone();
        duplicate["days"]
            .as_array_mut()
            .unwrap()
            .push(good["days"][0].clone());
        let mut wrong_year = good.clone();
        wrong_year["year"] = json!(2027);
        let mut distant = good.clone();
        distant["days"][0]["date"] = json!("2027-10-01");
        let mut empty = good.clone();
        empty["days"] = json!([]);
        let mut no_source = good.clone();
        no_source["papers"] = json!([]);
        let mut bad_name = good.clone();
        bad_name["days"][0]["name"] = json!("元旦\n");
        for bad in [duplicate, wrong_year, distant, empty, no_source, bad_name] {
            assert!(
                Year::parse(&serde_json::to_vec(&bad).unwrap(), 2026).is_err(),
                "{bad}"
            );
        }
        assert!(Year::parse(&vec![b' '; MAX_BYTES as usize + 1], 2026).is_err());
        assert!(Year::parse(b"broken", 2026).is_err());
    }
    #[test]
    fn source_links_only_open_the_state_council_host() {
        for url in [
            "https://www.gov.cn.evil.example/a",
            "file:///a",
            "https://evil.example/a",
            "https://user@www.gov.cn/a",
            "https://www.gov.cn:444/a",
        ] {
            let mut bad = document(2026, "2026-01-01", true);
            bad["papers"] = json!([url]);
            assert!(
                Year::parse(&serde_json::to_vec(&bad).unwrap(), 2026).is_err(),
                "{url}"
            );
        }
        assert_eq!(
            official_url("http://www.gov.cn/a").as_deref(),
            Some("https://www.gov.cn/a")
        );
    }
    #[test]
    fn fresh_cache_avoids_network_and_keeps_the_original_deadline() {
        let dir = Directory::new();
        dir.store(now() - Duration::hours(3));
        let http = Http::default();
        let result = refresh(&http, &dir.0, 2026, None, now());
        assert_eq!(result.data, Some(year()));
        assert_eq!(result.next_check, now() + Duration::hours(21));
        assert!(http.calls.borrow().is_empty());
    }
    #[test]
    fn not_modified_reuses_verified_cache_and_refreshes_timestamp() {
        let dir = Directory::new();
        dir.store(now() - Duration::days(2));
        let http = Http::new(vec![response(304, Value::Null)]);
        let result = refresh(&http, &dir.0, 2026, None, now());
        assert_eq!(result.data, Some(year()));
        assert_eq!(
            http.calls.borrow()[0].1,
            vec![("If-None-Match".into(), "\"cached\"".into())]
        );
        assert_eq!(
            cache(&dir.0.join("2026.json"), 2026).unwrap().checked_at,
            now()
        );
    }
    #[test]
    fn fallback_fetches_anonymous_data_and_caches_for_restart() {
        let dir = Directory::new();
        let http = Http::new(vec![
            Err("offline".into()),
            response(200, document(2026, "2026-01-01", true)),
        ]);
        let result = refresh(&http, &dir.0, 2026, None, now());
        assert_eq!(result.data, Some(year()));
        let calls = http.calls.borrow();
        assert!(calls[0].0.starts_with("https://raw.githubusercontent.com/"));
        assert!(calls[1].0.starts_with("https://cdn.jsdelivr.net/"));
        assert!(calls.iter().all(|(_, headers)| headers.is_empty()));
        assert!(
            cache(&dir.0.join("2026.json"), 2026)
                .unwrap()
                .etag
                .is_none(),
            "CDN etag must not be sent to GitHub"
        );
        let restarted = refresh(
            &Http::default(),
            &dir.0,
            2026,
            None,
            now() + Duration::hours(1),
        );
        assert_eq!(restarted.data, Some(year()));
    }
    #[test]
    fn broken_refresh_keeps_the_last_good_file_and_uses_retry_backoff() {
        let dir = Directory::new();
        dir.store(now() - Duration::days(2));
        let path = dir.0.join("2026.json");
        let before = std::fs::read(&path).unwrap();
        let http = Http::new(vec![
            response(200, json!({"year":2026,"papers":[],"days":[]})),
            response(200, json!({"broken":true})),
        ]);
        let result = refresh(&http, &dir.0, 2026, None, now());
        assert_eq!(result.data, Some(year()));
        assert!(result.note.contains("离线"));
        assert_eq!(result.next_check, now() + Duration::hours(1));
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
    #[test]
    fn unpublished_year_is_unknown_and_does_not_hammer_the_server() {
        let dir = Directory::new();
        let http = Http::new(vec![response(404, Value::Null), response(404, Value::Null)]);
        let outcome = refresh(&http, &dir.0, 2030, None, now());
        assert!(outcome.data.is_none());
        let mut catalog = Catalog::default();
        catalog.apply(2030, outcome);
        assert!(!catalog.needs_refresh(2030, now() + Duration::minutes(30)));
        assert!(catalog.needs_refresh(2030, now() + Duration::hours(1)));
        assert!(catalog.status(2030).contains("暂无公开数据"));
    }
    #[test]
    fn unusable_cache_cannot_make_304_a_success() {
        let dir = Directory::new();
        config::atomic_write(&dir.0.join("2026.json"), b"broken").unwrap();
        let http = Http::new(vec![response(304, Value::Null), response(404, Value::Null)]);
        let result = refresh(&http, &dir.0, 2026, None, now());
        assert!(result.data.is_none());
        assert!(
            http.calls
                .borrow()
                .iter()
                .all(|(_, headers)| headers.is_empty())
        );
    }
    #[test]
    fn offline_first_launch_still_has_embedded_holidays() {
        let dir = Directory::new();
        let mut catalog = Catalog::default();
        let http = Http::new(vec![Err("offline".into()), Err("offline".into())]);
        let result = refresh(&http, &dir.0, 2027, catalog.seed(2027), now());
        catalog.apply(2027, result);
        assert_eq!(catalog.day(date("2027-02-14")).unwrap().badge(), "班");
        assert!(catalog.status(2027).contains("离线数据"));
    }

    #[test]
    #[ignore = "Explicit online smoke check; writes only to a temporary holiday cache"]
    fn live_public_holiday_data() {
        let dir = Directory::new();
        let result = refresh(&crate::http::Client::new(8), &dir.0, 2027, None, Utc::now());
        let data = result.data.expect("No verified schedule downloaded");
        assert!(
            data.days
                .iter()
                .any(|d| d.date == date("2027-02-14") && !d.off)
        );
        assert!(cache(&dir.0.join("2027.json"), 2027).is_some());
        println!(
            "Fetched {} announced days for {}: {}",
            data.days.len(),
            data.year,
            result.note
        );
    }
}
