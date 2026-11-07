//! Calendar-day solar terms in the Chinese calendar (UTC+8). A term labels
//! only its starting date, not every day until the following term.
use chrono::{Datelike, NaiveDate};
use std::{cell::RefCell, collections::VecDeque};
use tyme4rs::tyme::{Culture, solar::SolarTerm};

struct Year {
    number: i32,
    days: Vec<(NaiveDate, String)>,
}
thread_local! {
    static YEARS: RefCell<VecDeque<Year>> = const { RefCell::new(VecDeque::new()) };
}
pub fn on_date(date: NaiveDate) -> Option<String> {
    if !(1901..=2100).contains(&date.year()) {
        return None;
    }
    YEARS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.iter().any(|year| year.number == date.year()) {
            // Tyme index 0 is the previous December's winter solstice;
            // indices 1..=24 cover this Gregorian January through December.
            let days = (1..=24)
                .map(|index| {
                    let term = SolarTerm::from_index(date.year() as isize, index);
                    let day = term.get_solar_day();
                    (
                        NaiveDate::from_ymd_opt(
                            day.get_year() as i32,
                            day.get_month() as u32,
                            day.get_day() as u32,
                        )
                        .expect("Tyme returns a valid modern calendar date"),
                        term.get_name(),
                    )
                })
                .collect();
            if cache.len() >= 8 {
                cache.pop_front();
            }
            cache.push_back(Year {
                number: date.year(),
                days,
            });
        }
        cache
            .iter()
            .find(|year| year.number == date.year())?
            .days
            .iter()
            .find(|(day, _)| *day == date)
            .map(|(_, name)| name.clone())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terms_appear_only_on_their_date_including_winter_and_year_boundaries() {
        for (year, month, day, name) in [
            (2026, 11, 7, "立冬"),
            (2026, 11, 22, "小雪"),
            (2026, 12, 22, "冬至"),
            (2027, 1, 5, "小寒"),
            (2026, 2, 4, "立春"),
            (2026, 4, 5, "清明"),
        ] {
            let date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
            assert_eq!(on_date(date).as_deref(), Some(name), "{date}");
            assert_eq!(on_date(date.pred_opt().unwrap()), None);
            assert_eq!(on_date(date.succ_opt().unwrap()), None);
        }
    }
    #[test]
    fn full_years_have_24_terms_and_navigation_keeps_a_bounded_cache() {
        for year in [1901, 2024, 2026, 2100] {
            let first = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
            let terms: Vec<_> = (0..366)
                .map(|i| first + chrono::Days::new(i))
                .take_while(|d| d.year() == year)
                .filter_map(on_date)
                .collect();
            assert_eq!(terms.len(), 24, "{year}");
            assert_eq!(terms.first().unwrap(), "小寒");
            assert_eq!(terms.last().unwrap(), "冬至");
        }
        for year in 2000..2020 {
            on_date(NaiveDate::from_ymd_opt(year, 1, 1).unwrap());
        }
        YEARS.with(|cache| assert_eq!(cache.borrow().len(), 8));
        for year in [1601, 1900, 2101, 9999] {
            assert!(on_date(NaiveDate::from_ymd_opt(year, 1, 1).unwrap()).is_none());
        }
    }
}
