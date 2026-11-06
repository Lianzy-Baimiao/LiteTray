//! One background download at a time shared by settings and every flyout.
use super::*;
pub(super) const REQUEST: u32 = WM_APP + 91;
impl App {
    pub(super) unsafe fn request_holidays(&mut self, month: chrono::NaiveDate) {
        unsafe {
            for year in holidays::years_for_month(month) {
                if self.holiday_active != Some(year)
                    && self.holidays.borrow().needs_refresh(year, Utc::now())
                    && self.holiday_queue.len() < 8
                {
                    self.holiday_queue.insert(year);
                }
            }
            self.start_next_holiday();
        }
    }
    pub(super) unsafe fn start_next_holiday(&mut self) {
        if self.holiday_active.is_some() {
            return;
        }
        let Some(year) = self.holiday_queue.pop_first() else {
            return;
        };
        self.holiday_active = Some(year);
        let seed = self.holidays.borrow().seed(year);
        let sender = self.tx.clone();
        let window = self.hwnd as usize;
        let path = config::data_dir().join("holidays");
        thread::spawn(move || {
            let result = holidays::refresh(&http::Client::new(8), &path, year, seed, Utc::now());
            let _ = sender.send(Event::Holidays(year, result));
            unsafe {
                PostMessageW(window as HWND, WM_RESULT, 0, 0);
            }
        });
    }
    pub(super) unsafe fn refresh_visible_holidays(&mut self) {
        unsafe {
            let mut months = vec![Local::now().date_naive()];
            if self.page == 0
                && let Some(calendar) = dashboard::selection(self.field(dashboard::ID))
            {
                months.push(calendar.month);
            }
            for &panel in &self.panel_cache {
                if IsWindowVisible(panel) != 0
                    && let Some(calendar) =
                        dashboard::selection(GetDlgItem(panel, dashboard::ID as i32))
                {
                    months.push(calendar.month);
                }
            }
            for month in months {
                self.request_holidays(month);
            }
        }
    }
}
