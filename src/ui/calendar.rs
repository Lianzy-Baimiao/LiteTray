use chrono::{Datelike, Days, Duration, Months, NaiveDate};

#[derive(Clone, Copy)]
pub(super) struct Calendar {
    pub month: NaiveDate,
    pub selected: NaiveDate,
}
impl Calendar {
    pub fn new(today: NaiveDate) -> Self {
        Self {
            month: today.with_day(1).unwrap(),
            selected: today,
        }
    }
    pub fn grid(&self) -> Vec<NaiveDate> {
        let start = self.month - Days::new(self.month.weekday().num_days_from_monday() as u64);
        (0..42).map(|i| start + Days::new(i)).collect()
    }
    pub fn select(&mut self, date: NaiveDate) {
        if (1601..=9999).contains(&date.year()) {
            self.month = date.with_day(1).unwrap();
            self.selected = date;
        }
    }
    pub fn move_days(&mut self, delta: i64) {
        if let Some(date) = self.selected.checked_add_signed(Duration::days(delta)) {
            self.select(date);
        }
    }
    pub fn move_month(&mut self, next: bool) {
        let month = if next {
            self.month.checked_add_months(Months::new(1))
        } else {
            self.month.checked_sub_months(Months::new(1))
        };
        if let Some(month) = month {
            self.select(month);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calendar_grid_and_keyboard_cross_leap_and_year_boundaries() {
        let mut c = Calendar::new(NaiveDate::from_ymd_opt(2024, 2, 28).unwrap());
        let grid = c.grid();
        assert_eq!(grid.len(), 42);
        assert_eq!(grid[0].weekday(), chrono::Weekday::Mon);
        assert!(grid.contains(&NaiveDate::from_ymd_opt(2024, 2, 29).unwrap()));
        c.move_days(1);
        assert_eq!(c.selected.day(), 29);
        c.move_days(1);
        assert_eq!((c.month.month(), c.selected.day()), (3, 1));
        c.select(NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());
        c.move_days(1);
        assert_eq!(c.selected, NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
        c.move_month(false);
        assert_eq!(c.month, NaiveDate::from_ymd_opt(2026, 12, 1).unwrap());
        c.select(NaiveDate::from_ymd_opt(1601, 1, 1).unwrap());
        c.move_month(false);
        c.move_days(-1);
        assert_eq!(c.selected, NaiveDate::from_ymd_opt(1601, 1, 1).unwrap());
    }
}
