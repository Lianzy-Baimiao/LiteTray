//! A preflight permit must survive host exit before native injection. The
//! service owns the permit until restoration is proved; callbacks never wait
//! for the journal writer.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    fs::{File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, SyncSender},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const MAX_RECORDS: usize = 8;
const MAX_BYTES: u64 = 16 * 1024;
const QUEUE_SIZE: usize = 8;
const ACK_TIMEOUT: Duration = Duration::from_secs(5);
static PROCESS_JOURNALS: OnceLock<Mutex<std::collections::BTreeMap<PathBuf, Journal>>> =
    OnceLock::new();

pub fn for_process(path: PathBuf) -> Result<Journal, String> {
    let mut journals = PROCESS_JOURNALS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "保护日志服务不可用")?;
    if let Some(journal) = journals.get(&path) {
        return Ok(journal.clone());
    }
    if journals.len() >= 32 {
        return Err("保护日志服务数量超限".into());
    }
    let journal = Journal::new(path.clone());
    journals.insert(path, journal.clone());
    Ok(journal)
}
/// Only after the UI loop and COM teardown. Normal Stop is nonblocking, but
/// its final receipt needs a bounded chance to reach disk before process exit.
/// An I/O timeout keeps the pending marker; shutdown never joins the writer.
pub fn flush(timeout: Duration) {
    let start = Instant::now();
    let Some(journals) = PROCESS_JOURNALS.get() else {
        return;
    };
    let Ok(journals) = journals
        .lock()
        .map(|values| values.values().cloned().collect::<Vec<_>>())
    else {
        return;
    };
    for journal in journals {
        let remaining = timeout.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            break;
        }
        journal.flush(remaining);
    }
}

pub fn identity(
    adapter_sha256: &str,
    layout: &litetray_adapter_protocol::HostLayout,
    clocks: bool,
    mirror_icons: bool,
) -> Result<String, String> {
    if !valid_key(adapter_sha256) {
        return Err("适配器内容身份无效".into());
    }
    layout.validate()?;
    let bytes = serde_json::to_vec(&(
        env!("CARGO_PKG_VERSION"),
        adapter_sha256,
        layout,
        clocks,
        mirror_icons,
    ))
    .map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[derive(Clone)]
pub struct Journal {
    sender: SyncSender<Message>,
    uncertain: Arc<AtomicBool>,
}
pub struct Permit {
    journal: Journal,
    key: String,
    token: String,
    used: Cell<bool>,
    faulted: Cell<bool>,
    healthy: Cell<bool>,
    settled: Cell<bool>,
}
enum Message {
    Arm {
        key: String,
        retry: bool,
        reply: mpsc::Sender<Result<String, String>>,
    },
    Update {
        key: String,
        token: String,
        event: Event,
    },
    Flush(mpsc::Sender<()>),
}
#[derive(Clone, Copy)]
enum Event {
    Fault,
    Healthy,
    Restored,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    records: Vec<Record>,
}
impl Default for Document {
    fn default() -> Self {
        Self {
            version: 1,
            records: Vec::new(),
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    key: String,
    failures: u8,
    pending: Option<String>,
    last_seen: u64,
}
fn valid_key(key: &str) -> bool {
    key.len() == 64 && key.bytes().all(|c| c.is_ascii_hexdigit())
}
fn lock(path: &Path) -> Result<File, String> {
    std::fs::create_dir_all(path.parent().ok_or("保护日志目录无效")?).map_err(|e| e.to_string())?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options
            .share_mode(0)
            .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    // A deny-share handle is released even if the process dies. A leftover
    // lock file is harmless; there is no stale create_new lock to strand.
    options
        .open(path.with_extension("lock"))
        .map_err(|e| format!("保护日志正忙或无法锁定：{e}"))
}
fn read(path: &Path) -> Result<Document, String> {
    let mut bytes = Vec::new();
    match File::open(path) {
        Ok(file) => {
            file.take(MAX_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Document::default());
        }
        Err(error) => return Err(format!("无法读取保护日志：{error}")),
    }
    if bytes.len() as u64 > MAX_BYTES {
        return Err("保护日志大小超限".into());
    }
    let document: Document =
        serde_json::from_slice(&bytes).map_err(|e| format!("保护日志损坏：{e}"))?;
    if document.version != 1 || document.records.len() > MAX_RECORDS {
        return Err("保护日志格式或数量无效".into());
    }
    let mut keys = std::collections::BTreeSet::new();
    for record in &document.records {
        if !valid_key(&record.key)
            || !keys.insert(&record.key)
            || record.failures > crate::recovery::MAX_FAILURES
            || record.pending.as_ref().is_some_and(|token| {
                token.is_empty()
                    || token.len() > 64
                    || !token
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            })
        {
            return Err("保护日志记录无效".into());
        }
    }
    Ok(document)
}
fn save(
    path: &Path,
    document: &Document,
    write: &mut impl FnMut(&Path, &[u8]) -> Result<(), String>,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(document).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("保护日志大小超限".into());
    }
    write(path, &bytes).map_err(|e| format!("保护日志落盘失败：{e}"))
}
fn arm(
    path: &Path,
    key: &str,
    retry: bool,
    write: &mut impl FnMut(&Path, &[u8]) -> Result<(), String>,
) -> Result<String, String> {
    if !valid_key(key) {
        return Err("保护日志身份无效".into());
    }
    let _lock = lock(path)?;
    let mut document = match read(path) {
        Ok(document) => document,
        Err(_) if retry => Document::default(),
        Err(error) => return Err(error),
    };
    let index = match document.records.iter().position(|record| record.key == key) {
        Some(index) => index,
        None => {
            if document.records.len() == MAX_RECORDS {
                let oldest = document
                    .records
                    .iter()
                    .enumerate()
                    .filter(|(_, record)| retry || record.pending.is_none() && record.failures == 0)
                    .min_by_key(|(_, record)| record.last_seen)
                    .map(|(index, _)| index)
                    .ok_or("保护日志已满，需显式重试后再接入")?;
                document.records.remove(oldest);
            }
            document.records.push(Record {
                key: key.into(),
                failures: 0,
                pending: None,
                last_seen: 0,
            });
            document.records.len() - 1
        }
    };
    let record = &mut document.records[index];
    if retry {
        record.failures = 0;
        record.pending = None;
    }
    if record.pending.take().is_some() {
        record.failures = record
            .failures
            .saturating_add(1)
            .min(crate::recovery::MAX_FAILURES);
    }
    if record.failures >= crate::recovery::MAX_FAILURES {
        save(path, &document, write)?;
        return Err(
            "相同适配器和系统布局连续三次连接失败或结束未确认，已跨重启暂停；可点击重试".into(),
        );
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    let token = format!(
        "{}-{}-{}",
        std::process::id(),
        now.as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    record.pending = Some(token.clone());
    record.last_seen = now.as_millis().try_into().map_err(|_| "保护日志时间超限")?;
    save(path, &document, write)?;
    Ok(token)
}
fn update(
    path: &Path,
    key: &str,
    token: &str,
    event: Event,
    write: &mut impl FnMut(&Path, &[u8]) -> Result<(), String>,
) -> Result<(), String> {
    let _lock = lock(path)?;
    let mut document = read(path)?;
    let Some(record) = document
        .records
        .iter_mut()
        .find(|record| record.key == key && record.pending.as_deref() == Some(token))
    else {
        return Ok(());
    };
    match event {
        Event::Fault => {
            record.failures = record
                .failures
                .saturating_add(1)
                .min(crate::recovery::MAX_FAILURES);
            record.pending = None;
        }
        Event::Healthy => {
            record.failures = 0;
        }
        Event::Restored => {
            record.pending = None;
        }
    }
    save(path, &document, write)
}
impl Journal {
    pub fn new(path: PathBuf) -> Self {
        Self::with_writer(path, crate::config::atomic_write)
    }
    fn with_writer(
        path: PathBuf,
        mut write: impl FnMut(&Path, &[u8]) -> Result<(), String> + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::sync_channel(QUEUE_SIZE);
        let uncertain = Arc::new(AtomicBool::new(false));
        let failure = uncertain.clone();
        std::thread::spawn(move || {
            while let Ok(message) = receiver.recv() {
                match message {
                    Message::Arm { key, retry, reply } => {
                        let result = if failure.load(Ordering::Acquire) && !retry {
                            Err("保护日志更新未确认，已暂停；可点击重试".into())
                        } else {
                            arm(&path, &key, retry, &mut write)
                        };
                        if result.is_ok() && retry {
                            failure.store(false, Ordering::Release);
                        }
                        // A timed-out/cancelled worker never injects. Undo its
                        // durable arm when the acknowledgement has no reader.
                        if let Err(mpsc::SendError(Ok(token))) = reply.send(result)
                            && update(&path, &key, &token, Event::Restored, &mut write).is_err()
                        {
                            failure.store(true, Ordering::Release);
                        }
                    }
                    Message::Update { key, token, event } => {
                        if let Err(error) = update(&path, &key, &token, event, &mut write) {
                            failure.store(true, Ordering::Release);
                            crate::event_log::record("taskbar safety journal", &error);
                        }
                    }
                    Message::Flush(reply) => {
                        let _ = reply.send(());
                    }
                }
            }
        });
        Self { sender, uncertain }
    }
    /// Call only in preflight workers. No permit exists until write + sync +
    /// atomic replacement succeeds. UI threads only move the resulting permit.
    pub fn arm(&self, key: &str, retry: bool) -> Result<Permit, String> {
        if !valid_key(key) {
            return Err("保护日志身份无效".into());
        }
        let (reply, receiver) = mpsc::channel();
        self.sender
            .try_send(Message::Arm {
                key: key.into(),
                retry,
                reply,
            })
            .map_err(|_| "保护日志队列正忙，已暂停接入")?;
        let token = receiver
            .recv_timeout(ACK_TIMEOUT)
            .map_err(|_| "保护日志落盘未确认，已暂停接入")??;
        Ok(Permit {
            journal: self.clone(),
            key: key.into(),
            token,
            used: Cell::new(false),
            faulted: Cell::new(false),
            healthy: Cell::new(false),
            settled: Cell::new(false),
        })
    }
    fn notify(&self, key: &str, token: &str, event: Event) -> bool {
        if self
            .sender
            .try_send(Message::Update {
                key: key.into(),
                token: token.into(),
                event,
            })
            .is_err()
        {
            self.uncertain.store(true, Ordering::Release);
            false
        } else {
            true
        }
    }
    fn flush(&self, timeout: Duration) -> bool {
        let (reply, receiver) = mpsc::channel();
        self.sender.try_send(Message::Flush(reply)).is_ok()
            && receiver.recv_timeout(timeout).is_ok()
    }
    #[cfg(test)]
    fn barrier(&self) {
        let (reply, receiver) = mpsc::channel();
        self.sender.send(Message::Flush(reply)).unwrap();
        receiver.recv_timeout(ACK_TIMEOUT).unwrap();
    }
}
impl Permit {
    pub fn mark_used(&self) {
        self.used.set(true);
    }
    pub fn fault(&self) {
        if self.used.get() && !self.settled.get() && !self.faulted.replace(true) {
            self.journal.notify(&self.key, &self.token, Event::Fault);
        }
    }
    pub fn healthy(&self) {
        if self.used.get()
            && !self.faulted.get()
            && !self.settled.get()
            && !self.healthy.get()
            && self.journal.notify(&self.key, &self.token, Event::Healthy)
        {
            self.healthy.set(true);
        }
    }
    pub fn restored(&self) {
        if !self.settled.replace(true) && !self.faulted.get() {
            self.journal.notify(&self.key, &self.token, Event::Restored);
        }
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        // A stale preflight never reached native code. A used permit without
        // confirmed restoration remains armed even if the host is killed.
        if !self.used.get() {
            self.restored();
        }
    }
}

#[cfg(test)]
#[path = "taskbar_safety_checks.rs"]
mod tests;
