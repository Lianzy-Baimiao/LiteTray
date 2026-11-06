use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone};
#[path = "lunar_leap_months.rs"]
mod leap_months;
#[derive(Clone)]
pub struct LunarDay {
    pub short: String,
    pub full: String,
}
thread_local! { static CACHE: std::cell::RefCell<std::collections::BTreeMap<NaiveDate, Option<LunarDay>>> = const { std::cell::RefCell::new(std::collections::BTreeMap::new()) }; }
pub fn for_date(date: NaiveDate) -> Option<LunarDay> {
    CACHE.with(|cache| {
        if let Some(value) = cache.borrow().get(&date) {
            return value.clone();
        }
        let value = Local
            .from_local_datetime(&date.and_hms_opt(12, 0, 0)?)
            .single()
            .and_then(convert);
        let mut cache = cache.borrow_mut();
        if cache.len() >= 4096 {
            cache.clear();
        }
        cache.insert(date, value.clone());
        value
    })
}
pub fn describe(date: DateTime<Local>) -> Option<String> {
    for_date(date.date_naive()).map(|day| day.full)
}
fn convert(date: DateTime<Local>) -> Option<LunarDay> {
    use windows::{
        Foundation::DateTime as WinDateTime,
        Globalization::{Calendar, CalendarIdentifiers},
    };
    let calendar = Calendar::new().ok()?;
    calendar
        .ChangeCalendarSystem(&CalendarIdentifiers::ChineseLunar().ok()?)
        .ok()?;
    let ticks = (date.timestamp() + 11_644_473_600).checked_mul(10_000_000)?;
    calendar
        .SetDateTime(WinDateTime {
            UniversalTime: ticks,
        })
        .ok()?;
    // WinRT numbers the inserted leap month as the next ordinal month and
    // formats that ordinal literally (2025 leap June becomes "7月").
    let ordinal = calendar.Month().ok()?;
    // Chinese Calendar.Year is a year in the 60-year cycle. Resolve the
    // absolute lunar year from the Gregorian date around Lunar New Year.
    let year = date.year() - i32::from(date.month() <= 2 && ordinal >= 11);
    let leap = *leap_months::LEAP_MONTHS.get(year.checked_sub(1901)? as usize)? as i32;
    if calendar.NumberOfMonthsInThisYear().ok()? != if leap == 0 { 12 } else { 13 } {
        return None;
    }
    let number = ordinal - i32::from(leap != 0 && ordinal > leap);
    let names = [
        "正月", "二月", "三月", "四月", "五月", "六月", "七月", "八月", "九月", "十月", "冬月",
        "腊月",
    ];
    let month = format!(
        "{}{}",
        if leap != 0 && ordinal == leap + 1 {
            "闰"
        } else {
            ""
        },
        names.get((number - 1) as usize)?
    );
    let day = calendar.Day().ok()?;
    let days = [
        "初一", "初二", "初三", "初四", "初五", "初六", "初七", "初八", "初九", "初十", "十一",
        "十二", "十三", "十四", "十五", "十六", "十七", "十八", "十九", "二十", "廿一", "廿二",
        "廿三", "廿四", "廿五", "廿六", "廿七", "廿八", "廿九", "三十",
    ];
    let day_name = days.get((day - 1) as usize)?;
    Some(LunarDay {
        short: if day == 1 {
            month.clone()
        } else {
            (*day_name).into()
        },
        full: format!("农历 {month}{day_name}"),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_lunar_holidays_and_day_labels() {
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(
                windows::Win32::System::WinRT::RO_INIT_MULTITHREADED,
            );
        }
        let new_year = for_date(NaiveDate::from_ymd_opt(2026, 2, 17).unwrap()).unwrap();
        assert!(new_year.full.ends_with("初一"), "{}", new_year.full);
        let mid_autumn = for_date(NaiveDate::from_ymd_opt(2026, 9, 25).unwrap()).unwrap();
        assert_eq!(mid_autumn.short, "十五");
        let leap_month = for_date(NaiveDate::from_ymd_opt(2025, 7, 25).unwrap()).unwrap();
        assert_eq!(leap_month.full, "农历 闰六月初一");
        assert_eq!(
            for_date(NaiveDate::from_ymd_opt(2025, 6, 25).unwrap())
                .unwrap()
                .full,
            "农历 六月初一"
        );
        assert_eq!(
            for_date(NaiveDate::from_ymd_opt(2025, 8, 23).unwrap())
                .unwrap()
                .full,
            "农历 七月初一"
        );
        assert!(for_date(NaiveDate::from_ymd_opt(1601, 1, 1).unwrap()).is_none());
        unsafe {
            windows::Win32::System::WinRT::RoUninitialize();
        }
    }
}
