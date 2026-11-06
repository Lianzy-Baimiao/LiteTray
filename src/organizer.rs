//! Local agenda and durable timers. Window rendering and notification delivery
//! consume this model; neither owns the clock or an extra copy of the data.
use chrono::{DateTime, Datelike, Days, Duration, Local, NaiveDate, NaiveTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Todo,
    Event,
    Birthday,
    Anniversary,
    Countdown,
    Alarm,
}
impl Kind {
    pub const ALL: [Self; 6] = [
        Self::Todo,
        Self::Event,
        Self::Birthday,
        Self::Anniversary,
        Self::Countdown,
        Self::Alarm,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Todo => "待办",
            Self::Event => "日程",
            Self::Birthday => "生日",
            Self::Anniversary => "纪念日",
            Self::Countdown => "倒数日",
            Self::Alarm => "闹钟",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Repeat {
    Once,
    Daily,
    Weekdays,
    Weekly,
    Yearly,
}
impl Repeat {
    pub const ALL: [Self; 5] = [
        Self::Once,
        Self::Daily,
        Self::Weekdays,
        Self::Weekly,
        Self::Yearly,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Once => "不重复",
            Self::Daily => "每天",
            Self::Weekdays => "工作日（周一至周五）",
            Self::Weekly => "每周",
            Self::Yearly => "每年",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    pub kind: Kind,
    pub title: String,
    pub date: NaiveDate,
    pub time: Option<NaiveTime>,
    pub repeat: Repeat,
    pub remind_minutes: Option<u32>,
    pub notes: String,
    pub done: bool,
    pub enabled: bool,
}
impl Entry {
    pub fn new(kind: Kind, date: NaiveDate) -> Self {
        Self {
            id: 0,
            kind,
            title: String::new(),
            date,
            time: None,
            repeat: if matches!(kind, Kind::Birthday | Kind::Anniversary) {
                Repeat::Yearly
            } else {
                Repeat::Once
            },
            remind_minutes: Some(0),
            notes: String::new(),
            done: false,
            enabled: true,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty() || self.title.chars().count() > 120 {
            return Err("标题须为 1–120 个字".into());
        }
        if self.notes.chars().count() > 2000 {
            return Err("备注最多 2000 个字".into());
        }
        if !(1901..=2099).contains(&self.date.year()) {
            return Err("日期须在 1901–2099 年之间".into());
        }
        if self.remind_minutes.is_some_and(|m| m > 10080) {
            return Err("提醒最多提前 7 天".into());
        }
        if self.kind == Kind::Alarm && (self.time.is_none() || self.remind_minutes.is_none()) {
            return Err("闹钟需要设置时间并开启提醒".into());
        }
        Ok(())
    }
    pub fn occurs_on(&self, date: NaiveDate) -> bool {
        if !self.enabled || self.done || date < self.date {
            return false;
        }
        match self.repeat {
            Repeat::Once => date == self.date,
            Repeat::Daily => true,
            Repeat::Weekdays => date.weekday().num_days_from_monday() < 5,
            Repeat::Weekly => date.weekday() == self.date.weekday(),
            // A Feb 29 annual item falls on Feb 28 in a non-leap year.
            Repeat::Yearly => {
                NaiveDate::from_ymd_opt(date.year(), self.date.month(), self.date.day())
                    .or_else(|| NaiveDate::from_ymd_opt(date.year(), 2, 28))
                    .is_some_and(|d| d == date)
            }
        }
    }
    pub fn next_date(&self, from: NaiveDate) -> Option<NaiveDate> {
        (0..=366)
            .filter_map(|n| from.checked_add_days(Days::new(n)))
            .find(|d| self.occurs_on(*d))
    }
    pub fn describe(&self, today: NaiveDate) -> String {
        let date = self.next_date(today).unwrap_or(self.date);
        let distance = date.signed_duration_since(today).num_days();
        let day = match distance {
            0 => "今天".into(),
            1 => "明天".into(),
            n if n > 0 => format!("还有 {n} 天"),
            n => format!("已过 {} 天", -n),
        };
        format!(
            "{} · {}{} · {}",
            self.kind.label(),
            date.format("%m月%d日"),
            self.time
                .map(|t| format!(" {}", t.format("%H:%M")))
                .unwrap_or_default(),
            if self.done {
                "已完成"
            } else if !self.enabled {
                "已关闭"
            } else {
                &day
            }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Countdown,
    Focus,
    ShortBreak,
    LongBreak,
}
impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Countdown => "定时提醒",
            Self::Focus => "专注",
            Self::ShortBreak => "短休息",
            Self::LongBreak => "长休息",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timer {
    pub id: u64,
    pub title: String,
    pub phase: Phase,
    pub due: Option<DateTime<Utc>>,
    pub remaining: u32,
    pub finished: bool,
    pub rounds: u32,
}
impl Timer {
    pub fn seconds_left(&self, now: DateTime<Utc>) -> u32 {
        self.due
            .map(|due| (due - now).num_seconds().max(0) as u32)
            .unwrap_or(self.remaining)
    }
    pub fn display(&self, now: DateTime<Utc>) -> String {
        let seconds = self.seconds_left(now);
        format!(
            "{:02}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusSettings {
    pub work: u32,
    pub short_break: u32,
    pub long_break: u32,
    pub rounds: u32,
}
impl Default for FocusSettings {
    fn default() -> Self {
        Self {
            work: 25,
            short_break: 5,
            long_break: 15,
            rounds: 4,
        }
    }
}
impl FocusSettings {
    pub fn validate(&self) -> Result<(), String> {
        if [self.work, self.short_break, self.long_break]
            .into_iter()
            .any(|v| !(1..=180).contains(&v))
            || !(1..=12).contains(&self.rounds)
        {
            return Err("专注和休息须为 1–180 分钟，长休息间隔须为 1–12 轮".into());
        }
        Ok(())
    }
    pub fn seconds(&self, phase: Phase) -> u32 {
        60 * match phase {
            Phase::Focus | Phase::Countdown => self.work,
            Phase::ShortBreak => self.short_break,
            Phase::LongBreak => self.long_break,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notice {
    pub key: String,
    pub title: String,
    pub body: String,
    pub at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundSettings {
    /// 0 = silent, 1..=10 = Windows alarm sounds, 11 = a local WAV file.
    pub tone: u8,
    pub custom_file: String,
    pub seconds: u32,
}
impl Default for SoundSettings {
    fn default() -> Self {
        Self {
            tone: 1,
            custom_file: String::new(),
            seconds: 30,
        }
    }
}
impl SoundSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.tone > 11 || !(5..=120).contains(&self.seconds) {
            return Err("铃声持续时间须为 5–120 秒".into());
        }
        if self.tone == 11
            && (!Path::new(&self.custom_file).is_absolute()
                || !Path::new(&self.custom_file)
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("wav")))
        {
            return Err("请选择本机 WAV 音频文件".into());
        }
        Ok(())
    }
    pub fn path(&self) -> Option<PathBuf> {
        match self.tone {
            0 => None,
            1..=10 => std::env::var_os("WINDIR").map(|root| {
                PathBuf::from(root)
                    .join("Media")
                    .join(format!("Alarm{:02}.wav", self.tone))
            }),
            11 => Some(PathBuf::from(&self.custom_file)),
            _ => None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Organizer {
    pub schema: u32,
    pub next_id: u64,
    pub entries: Vec<Entry>,
    pub timers: Vec<Timer>,
    pub focus: FocusSettings,
    pub sound: SoundSettings,
    pub notices: Vec<Notice>,
    pub fired: Vec<String>,
    pub last_poll: DateTime<Utc>,
}
impl Default for Organizer {
    fn default() -> Self {
        Self {
            schema: 1,
            next_id: 1,
            entries: vec![],
            timers: vec![],
            focus: FocusSettings::default(),
            sound: SoundSettings::default(),
            notices: vec![],
            fired: vec![],
            last_poll: Utc::now(),
        }
    }
}
impl Organizer {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1 {
            return Err("日程数据版本暂不支持，原文件已保留".into());
        }
        if self.entries.len() > 5000 || self.timers.len() > 64 {
            return Err("日程或计时器数量超出上限".into());
        }
        self.focus.validate()?;
        self.sound.validate()?;
        let mut ids = std::collections::BTreeSet::new();
        for entry in &self.entries {
            entry.validate()?;
            if entry.id == 0 || !ids.insert(entry.id) {
                return Err("日程编号损坏".into());
            }
        }
        let mut focus_timers = 0;
        for timer in &self.timers {
            if timer.id == 0
                || !ids.insert(timer.id)
                || timer.remaining > 86400
                || timer.title.chars().count() > 120
            {
                return Err("计时器数据无效".into());
            }
            if timer.phase != Phase::Countdown {
                focus_timers += 1;
            }
        }
        if focus_timers > 1 || ids.last().is_some_and(|id| *id >= self.next_id) {
            return Err("计时器或日程编号冲突".into());
        }
        Ok(())
    }
    pub fn put(&mut self, mut entry: Entry) -> Result<u64, String> {
        entry.title = entry.title.trim().to_owned();
        entry.validate()?;
        if entry.id == 0 {
            entry.id = self.next_id;
            self.next_id = self.next_id.checked_add(1).ok_or("日程编号已用尽")?;
            self.entries.push(entry.clone());
        } else {
            *self
                .entries
                .iter_mut()
                .find(|e| e.id == entry.id)
                .ok_or("此日程已不存在")? = entry.clone();
        }
        Ok(entry.id)
    }
    pub fn start_timer(
        &mut self,
        title: &str,
        minutes: u32,
        focus: bool,
        now: DateTime<Utc>,
    ) -> Result<u64, String> {
        if !(1..=1440).contains(&minutes) || title.chars().count() > 120 {
            return Err("定时须为 1–1440 分钟，标题最多 120 个字".into());
        }
        if focus && self.timers.iter().any(|t| t.phase != Phase::Countdown) {
            return Err("番茄钟已存在，可继续或重置当前番茄钟".into());
        }
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).ok_or("计时器编号已用尽")?;
        self.timers.push(Timer {
            id,
            title: if title.trim().is_empty() {
                "定时提醒".into()
            } else {
                title.trim().into()
            },
            phase: if focus {
                Phase::Focus
            } else {
                Phase::Countdown
            },
            due: Some(now + Duration::minutes(minutes as i64)),
            remaining: minutes * 60,
            finished: false,
            rounds: 0,
        });
        Ok(id)
    }
    pub fn toggle_timer(&mut self, id: u64, now: DateTime<Utc>) -> Result<(), String> {
        let timer = self
            .timers
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or("计时器已不存在")?;
        if timer.finished {
            return Err("计时已结束，请删除后重新创建".into());
        }
        if timer.due.is_some() {
            timer.remaining = timer.seconds_left(now);
            if timer.remaining == 0 {
                return Err("计时已到期，正在处理提醒".into());
            }
            timer.due = None;
        } else {
            timer.due = Some(now + Duration::seconds(timer.remaining as i64));
        }
        Ok(())
    }
    /// Clock jumps backwards never clear delivery identities. Catch up at
    /// most 24 hours and choose the latest occurrence of each repeated item.
    pub fn poll(&mut self, now: DateTime<Utc>) -> Vec<Notice> {
        let from = self.last_poll.max(now - Duration::hours(24));
        let today = now.with_timezone(&Local).date_naive();
        let mut due = vec![];
        for entry in &self.entries {
            let Some(lead) = entry.remind_minutes else {
                continue;
            };
            let mut latest = None;
            for offset in -1..=7 {
                let Some(date) = today.checked_add_signed(Duration::days(offset)) else {
                    continue;
                };
                if !entry.occurs_on(date) {
                    continue;
                }
                let local = date.and_time(
                    entry
                        .time
                        .unwrap_or_else(|| NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
                );
                // Choose the first instance of an ambiguous local time. A
                // nonexistent DST time is skipped, not shifted silently.
                let Some(at) = Local.from_local_datetime(&local).earliest() else {
                    continue;
                };
                let at = at.with_timezone(&Utc) - Duration::minutes(lead as i64);
                if at > from && at <= now {
                    latest = Some((at, date));
                }
            }
            if let Some((at, date)) = latest {
                let key = format!("entry:{}:{}", entry.id, at.timestamp());
                if !self.fired.contains(&key) {
                    due.push(Notice {
                        key,
                        title: entry.title.clone(),
                        body: format!(
                            "{} · {}{}{}",
                            entry.kind.label(),
                            date.format("%m月%d日"),
                            entry
                                .time
                                .map(|t| format!(" {}", t.format("%H:%M")))
                                .unwrap_or_else(|| " 全天 · 09:00 提醒".into()),
                            if entry.notes.is_empty() {
                                String::new()
                            } else {
                                format!("\n{}", entry.notes)
                            }
                        ),
                        at: now,
                    });
                }
            }
        }
        for timer in &mut self.timers {
            let Some(at) = timer.due.filter(|at| *at <= now) else {
                continue;
            };
            let key = format!("timer:{}:{}:{}", timer.id, timer.rounds, at.timestamp());
            if !self.fired.contains(&key) {
                due.push(Notice {
                    key,
                    title: timer.title.clone(),
                    body: format!("{}时间到", timer.phase.label()),
                    at: now,
                });
            }
            timer.due = None;
            if timer.phase == Phase::Countdown {
                timer.finished = true;
                timer.remaining = 0;
            } else {
                timer.phase = if timer.phase == Phase::Focus {
                    timer.rounds += 1;
                    if timer.rounds.is_multiple_of(self.focus.rounds) {
                        Phase::LongBreak
                    } else {
                        Phase::ShortBreak
                    }
                } else {
                    Phase::Focus
                };
                timer.remaining = self.focus.seconds(timer.phase);
            }
        }
        for notice in &due {
            self.fired.push(notice.key.clone());
            self.notices.push(notice.clone());
        }
        if self.fired.len() > 4096 {
            self.fired.drain(..self.fired.len() - 4096);
        }
        if self.notices.len() > 50 {
            self.notices.drain(..self.notices.len() - 50);
        }
        self.last_poll = now;
        due
    }
}

pub struct Store {
    pub data: Organizer,
    path: PathBuf,
    pub error: Option<String>,
    pub revision: u64,
}
impl Store {
    pub fn open(path: &Path) -> Self {
        let result = if path.exists() {
            std::fs::read(path)
                .map_err(|e| e.to_string())
                .and_then(|bytes| {
                    if bytes.len() > 8 * 1024 * 1024 {
                        return Err("日程文件过大".into());
                    }
                    serde_json::from_slice::<Organizer>(&bytes)
                        .map_err(|e| format!("日程文件无法读取，已保留原文件：{e}"))
                })
                .and_then(|data| {
                    data.validate()?;
                    Ok(data)
                })
        } else {
            Ok(Organizer::default())
        };
        match result {
            Ok(data) => Self {
                data,
                path: path.into(),
                error: None,
                revision: 0,
            },
            Err(error) => Self {
                data: Organizer::default(),
                path: path.into(),
                error: Some(error),
                revision: 0,
            },
        }
    }
    pub fn change<T>(
        &mut self,
        apply: impl FnOnce(&mut Organizer) -> Result<T, String>,
    ) -> Result<T, String> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        let mut next = self.data.clone();
        let result = apply(&mut next)?;
        next.validate()?;
        crate::config::atomic_write(
            &self.path,
            &serde_json::to_vec_pretty(&next).map_err(|e| e.to_string())?,
        )?;
        self.data = next;
        self.revision = self.revision.wrapping_add(1);
        Ok(result)
    }
    pub fn tick(&mut self, now: DateTime<Utc>) -> Result<Vec<Notice>, String> {
        if self.error.is_some() {
            return Ok(vec![]);
        }
        let mut next = self.data.clone();
        let notices = next.poll(now);
        if notices.is_empty() {
            self.data.last_poll = now;
            return Ok(notices);
        }
        // Mark delivered durably before asking Windows to display the notice.
        crate::config::atomic_write(
            &self.path,
            &serde_json::to_vec_pretty(&next).map_err(|e| e.to_string())?,
        )?;
        self.data = next;
        self.revision = self.revision.wrapping_add(1);
        Ok(notices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn now() -> DateTime<Utc> {
        Local
            .with_ymd_and_hms(2026, 11, 6, 10, 0, 0)
            .single()
            .unwrap()
            .with_timezone(&Utc)
    }
    #[test]
    fn due_alarm_survives_roundtrip_and_clock_rollback_without_redelivery() {
        let now = now();
        let mut model = Organizer {
            last_poll: now,
            ..Default::default()
        };
        let mut entry = Entry::new(Kind::Alarm, now.with_timezone(&Local).date_naive());
        entry.title = "会议".into();
        entry.time = NaiveTime::from_hms_opt(10, 1, 0);
        model.put(entry).unwrap();
        assert!(model.poll(now + Duration::seconds(59)).is_empty());
        assert_eq!(model.poll(now + Duration::seconds(61)).len(), 1);
        let mut restored: Organizer =
            serde_json::from_slice(&serde_json::to_vec(&model).unwrap()).unwrap();
        assert!(restored.poll(now).is_empty());
        assert!(restored.poll(now + Duration::minutes(2)).is_empty());
    }
    #[test]
    fn paused_timer_does_not_expire_during_restart_and_focus_waits_for_next_phase() {
        let now = now();
        let mut model = Organizer::default();
        let id = model.start_timer("专注", 25, true, now).unwrap();
        model.toggle_timer(id, now + Duration::minutes(5)).unwrap();
        let mut model: Organizer =
            serde_json::from_slice(&serde_json::to_vec(&model).unwrap()).unwrap();
        assert!(model.poll(now + Duration::hours(1)).is_empty());
        assert_eq!(model.timers[0].remaining, 1200);
        model.toggle_timer(id, now + Duration::hours(1)).unwrap();
        assert_eq!(model.poll(now + Duration::minutes(80)).len(), 1);
        assert_eq!(model.timers[0].phase, Phase::ShortBreak);
        assert_eq!(model.timers[0].rounds, 1);
        assert!(model.timers[0].due.is_none());
        assert!(model.poll(now + Duration::hours(2)).is_empty());
    }
    #[test]
    fn annual_leap_dates_weekdays_and_completed_todos_are_resolved() {
        let mut birthday = Entry::new(
            Kind::Birthday,
            NaiveDate::from_ymd_opt(2024, 2, 29).unwrap(),
        );
        assert!(birthday.occurs_on(NaiveDate::from_ymd_opt(2025, 2, 28).unwrap()));
        assert!(!birthday.occurs_on(NaiveDate::from_ymd_opt(2024, 2, 28).unwrap()));
        birthday.repeat = Repeat::Weekdays;
        assert!(!birthday.occurs_on(NaiveDate::from_ymd_opt(2026, 11, 7).unwrap()));
        birthday.done = true;
        assert!(!birthday.occurs_on(NaiveDate::from_ymd_opt(2026, 11, 9).unwrap()));
    }
    #[test]
    fn early_reminder_crosses_midnight_and_disabled_alarm_stays_silent() {
        let now = now();
        let mut model = Organizer {
            last_poll: now,
            ..Default::default()
        };
        let mut entry = Entry::new(
            Kind::Event,
            now.with_timezone(&Local).date_naive() + Days::new(1),
        );
        entry.title = "提前一天".into();
        entry.time = NaiveTime::from_hms_opt(10, 1, 0);
        entry.remind_minutes = Some(1440);
        model.put(entry.clone()).unwrap();
        entry.enabled = false;
        model.put(entry).unwrap();
        assert_eq!(model.poll(now + Duration::minutes(2)).len(), 1);
    }
    #[test]
    fn corrupt_store_is_not_overwritten_and_failed_save_does_not_consume_due_timer() {
        let path = std::env::temp_dir().join(format!("litetray-organizer-{}", std::process::id()));
        std::fs::write(&path, b"not json").unwrap();
        let mut store = Store::open(&path);
        assert!(store.change(|_| Ok(())).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"not json");
        std::fs::remove_file(&path).unwrap();
        let mut store = Store::open(&path.join("unavailable").join("agenda.json"));
        store.data.start_timer("计时", 1, false, now()).unwrap();
        // A file in place of the parent makes atomic replacement fail.
        std::fs::write(&path, b"parent is a file").unwrap();
        assert!(store.tick(now() + Duration::minutes(2)).is_err());
        assert!(store.data.timers[0].due.is_some());
        std::fs::remove_file(path).unwrap();
    }
}
