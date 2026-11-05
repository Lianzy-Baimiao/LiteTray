use chrono::{DateTime, Local};
pub fn describe(date: DateTime<Local>) -> Option<String> {
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
    Some(format!(
        "农历 {}{}日",
        calendar.MonthAsFullString().ok()?,
        calendar.DayAsString().ok()?
    ))
}
