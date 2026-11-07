//! Familiar calendar observances, independent of official days off. Dates
//! are calculated offline; a festival never implies a rest/work badge.
use chrono::{Datelike, NaiveDate, Weekday};
use std::{cell::RefCell, collections::VecDeque};
use tyme4rs::tyme::solar::SolarDay;

const FIXED: &[(u32, u32, &str)] = &[
    (1, 1, "元旦"),
    (2, 14, "情人节"),
    (3, 8, "妇女节"),
    (3, 12, "植树节"),
    (4, 1, "愚人节"),
    (4, 22, "地球日"),
    (5, 1, "劳动节"),
    (5, 4, "青年节"),
    (5, 12, "护士节"),
    (6, 1, "儿童节"),
    (6, 5, "环境日"),
    (7, 1, "建党节"),
    (8, 1, "建军节"),
    (9, 10, "教师节"),
    (10, 1, "国庆节"),
    // Use the familiar Chinese calendar label for Halloween on October 31.
    (10, 31, "万圣节"),
    (12, 24, "平安夜"),
    (12, 25, "圣诞节"),
];
type Names = Vec<&'static str>;
thread_local! {
    static DAYS: RefCell<VecDeque<(NaiveDate, Names)>> = const { RefCell::new(VecDeque::new()) };
}

pub fn on_date(date: NaiveDate) -> Vec<&'static str> {
    if !(1901..=2100).contains(&date.year()) {
        return vec![];
    }
    DAYS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, names)) = cache.iter().find(|(day, _)| *day == date) {
            return names.clone();
        }
        let names = calculate(date);
        if cache.len() >= 512 {
            cache.pop_front();
        }
        cache.push_back((date, names.clone()));
        names
    })
}

fn calculate(date: NaiveDate) -> Names {
    let mut names = vec![];
    let lunar = SolarDay::from_ymd(
        date.year() as isize,
        date.month() as usize,
        date.day() as usize,
    )
    .get_lunar_day();
    let month = lunar.get_lunar_month();
    // Leap months repeat the lunar date, not its annual festival.
    if !month.is_leap() {
        let name = match (month.get_month(), lunar.get_day()) {
            (1, 1) => Some("春节"),
            (1, 15) => Some("元宵节"),
            (2, 2) => Some("龙抬头"),
            (5, 5) => Some("端午节"),
            (7, 7) => Some("七夕节"),
            (7, 15) => Some("中元节"),
            (8, 15) => Some("中秋节"),
            (9, 9) => Some("重阳节"),
            (12, 8) => Some("腊八节"),
            (12, day) if day == month.get_day_count() => Some("除夕"),
            _ => None,
        };
        if let Some(name) = name {
            names.push(name);
        }
    }
    names.extend(
        FIXED
            .iter()
            .filter(|(month, day, _)| (*month, *day) == (date.month(), date.day()))
            .map(|(_, _, name)| *name),
    );
    let occurrence = (date.day() - 1) / 7 + 1;
    let weekday = match (date.month(), date.weekday(), occurrence) {
        (5, Weekday::Sun, 2) => Some("母亲节"),
        (6, Weekday::Sun, 3) => Some("父亲节"),
        (11, Weekday::Thu, 4) => Some("感恩节"),
        _ => None,
    };
    if let Some(name) = weekday {
        names.push(name);
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    fn day(year: i32, month: u32, date: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, date).unwrap()
    }
    #[test]
    fn familiar_fixed_dates_and_weekday_rules_follow_each_year() {
        for (date, name) in [
            (day(2026, 10, 31), "万圣节"),
            (day(2025, 12, 24), "平安夜"),
            (day(2026, 12, 25), "圣诞节"),
            (day(2026, 2, 14), "情人节"),
            (day(2026, 5, 10), "母亲节"),
            (day(2027, 5, 9), "母亲节"),
            (day(2026, 6, 21), "父亲节"),
            (day(2027, 6, 20), "父亲节"),
            (day(2026, 11, 26), "感恩节"),
            (day(2027, 11, 25), "感恩节"),
        ] {
            assert!(on_date(date).contains(&name), "{date}: {name}");
            assert!(!on_date(date.pred_opt().unwrap()).contains(&name));
            assert!(!on_date(date.succ_opt().unwrap()).contains(&name));
        }
        for &(month, date, name) in FIXED {
            assert!(on_date(day(2026, month, date)).contains(&name));
        }
        assert!(!on_date(day(2026, 11, 1)).contains(&"万圣节"));
    }
    #[test]
    fn lunar_festivals_move_and_new_years_eve_can_have_29_or_30_days() {
        for (date, name) in [
            (day(2026, 2, 17), "春节"),
            (day(2026, 3, 3), "元宵节"),
            (day(2026, 6, 19), "端午节"),
            (day(2026, 9, 25), "中秋节"),
            (day(2025, 10, 6), "中秋节"),
            (day(2022, 1, 31), "除夕"),
            (day(2024, 2, 9), "除夕"),
            (day(2026, 2, 16), "除夕"),
        ] {
            assert!(on_date(date).contains(&name), "{date}: {name}");
        }
        assert!(on_date(day(2009, 5, 28)).contains(&"端午节"));
        assert!(
            !on_date(day(2009, 6, 27)).contains(&"端午节"),
            "leap fifth month"
        );
        assert_eq!(on_date(day(2020, 10, 1)), ["中秋节", "国庆节"]);
        assert_eq!(
            on_date(day(2026, 10, 2)),
            Vec::<&str>::new(),
            "a break is not a festival date"
        );
    }
    #[test]
    fn calendar_navigation_has_bounded_cache_and_supported_range() {
        for i in 0..550 {
            let date = day(2025, 1, 1) + chrono::Days::new(i);
            assert_eq!(on_date(date), on_date(date));
        }
        DAYS.with(|cache| assert_eq!(cache.borrow().len(), 512));
        for year in [1900, 2101] {
            assert!(on_date(day(year, 10, 31)).is_empty());
        }
        for year in [1901, 2100] {
            assert!(on_date(day(year, 10, 31)).contains(&"万圣节"));
        }
    }
}
