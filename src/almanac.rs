//! Offline traditional almanac. Keep the supported range explicit and cache
//! only display data, never an unbounded collection of calendar objects.
use chrono::{Datelike, NaiveDate};
use std::{cell::RefCell, collections::VecDeque};
use tyme4rs::tyme::{Culture, solar::SolarDay};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Day {
    pub lunar: String,
    pub cycles: String,
    pub suitable: String,
    pub avoid: String,
    pub detail: String,
}
thread_local! {
    static CACHE: RefCell<VecDeque<(NaiveDate, Day)>> = const { RefCell::new(VecDeque::new()) };
}
pub fn for_date(date: NaiveDate) -> Option<Day> {
    if !(1901..=2100).contains(&date.year()) {
        return None;
    }
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, day)) = cache.iter().find(|(d, _)| *d == date) {
            return Some(day.clone());
        }
        let solar = SolarDay::from_ymd(
            date.year() as isize,
            date.month() as usize,
            date.day() as usize,
        );
        let lunar = solar.get_lunar_day();
        let cycle = solar.get_sixty_cycle_day();
        let branch = cycle.get_sixty_cycle().get_earth_branch();
        let items = |values: Vec<tyme4rs::tyme::culture::Taboo>| {
            let names: Vec<_> = values.iter().map(Culture::get_name).collect();
            if names.is_empty() {
                "无".into()
            } else {
                names.join("　")
            }
        };
        let day = Day {
            lunar: lunar.to_string(),
            cycles: format!(
                "{}年  {}月  {}日",
                cycle.get_year(),
                cycle.get_month(),
                cycle.get_sixty_cycle()
            ),
            suitable: items(cycle.get_recommends()),
            avoid: items(cycle.get_avoids()),
            detail: format!(
                "冲{} · 煞{}  ·  {}日",
                branch.get_opposite().get_zodiac(),
                branch.get_ominous(),
                cycle.get_duty()
            ),
        };
        if cache.len() >= 128 {
            cache.pop_front();
        }
        cache.push_back((date, day.clone()));
        Some(day)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lunar_new_year_leap_month_and_day_cycles() {
        let new_year = for_date(NaiveDate::from_ymd_opt(2026, 2, 17).unwrap()).unwrap();
        assert_eq!(new_year.lunar, "农历丙午年正月初一");
        let leap = for_date(NaiveDate::from_ymd_opt(2025, 7, 25).unwrap()).unwrap();
        assert_eq!(leap.lunar, "农历乙巳年闰六月初一");
        let epoch = for_date(NaiveDate::from_ymd_opt(2000, 1, 7).unwrap()).unwrap();
        assert!(epoch.cycles.ends_with("甲子日"));
        assert!(epoch.detail.starts_with("冲马 · 煞南"));
        assert!(!new_year.suitable.is_empty() && !new_year.avoid.is_empty());
    }
    #[test]
    fn bounds_and_cache_do_not_grow_with_navigation() {
        for year in [1601, 1900, 2101, 9999] {
            assert!(for_date(NaiveDate::from_ymd_opt(year, 1, 1).unwrap()).is_none());
        }
        for year in [1901, 2100] {
            assert!(for_date(NaiveDate::from_ymd_opt(year, 12, 31).unwrap()).is_some());
        }
        let first = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        for index in 0..140 {
            let date = first + chrono::Days::new(index);
            assert_eq!(for_date(date), for_date(date));
        }
        CACHE.with(|cache| assert_eq!(cache.borrow().len(), 128));
    }
}
