//! Bounded, non-waiting diagnostic delivery. File I/O belongs to one worker,
//! never to a window procedure or the Explorer taskbar thread.
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Sender, SyncSender},
    },
    time::Duration,
};

const MAX_RECORD_BYTES: usize = 4096;

pub struct Log {
    sender: SyncSender<Entry>,
    dropped: AtomicU64,
}
enum Entry {
    Line(Vec<u8>),
    Barrier(Sender<()>),
}

impl Log {
    pub fn new(capacity: usize, mut sink: impl FnMut(&[u8]) + Send + 'static) -> io::Result<Self> {
        if capacity == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "empty log queue",
            ));
        }
        let (sender, receiver) = mpsc::sync_channel::<Entry>(capacity);
        std::thread::Builder::new()
            .name("LiteTray event writer".into())
            .spawn(move || {
                // Dropping the sender drains queued records, without making
                // its owner join a possibly stalled filesystem operation.
                while let Ok(entry) = receiver.recv() {
                    match entry {
                        Entry::Line(line) => sink(&line),
                        Entry::Barrier(acknowledged) => {
                            let _ = acknowledged.send(());
                        }
                    }
                }
            })?;
        Ok(Self {
            sender,
            dropped: AtomicU64::new(0),
        })
    }

    pub fn file(path: PathBuf, limit: u64) -> io::Result<Self> {
        Self::new(128, move |line| {
            let _ = write_file(&path, limit, line);
        })
    }

    /// Saturated or failed diagnostics are dropped. Waiting for diagnostic
    /// capacity would reintroduce the UI/taskbar stall this queue prevents.
    pub fn append(&self, line: Vec<u8>) -> bool {
        if line.len() <= MAX_RECORD_BYTES && self.sender.try_send(Entry::Line(line)).is_ok() {
            true
        } else {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            false
        }
    }

    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Only for process shutdown, after the UI message loop has ended. The
    /// deadline remains bounded when either the queue or filesystem stalls.
    pub fn flush(&self, timeout: Duration) -> bool {
        let (acknowledged, result) = mpsc::channel();
        self.sender.try_send(Entry::Barrier(acknowledged)).is_ok()
            && result.recv_timeout(timeout).is_ok()
    }
}

/// At most one current file and one previous file. An unsuccessful rotation
/// must not silently let the active file grow without a bound on Windows.
pub fn write_file(path: &Path, limit: u64, line: &[u8]) -> io::Result<()> {
    if line.len() as u64 > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "log record exceeds file limit",
        ));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let length = match std::fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
        Err(error) => return Err(error),
    };
    if length.saturating_add(line.len() as u64) > limit {
        let previous = path.with_extension("old.log");
        match std::fs::remove_file(&previous) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        std::fs::rename(path, previous)?;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn stalled_writer_has_bounded_memory_and_never_waits_for_queue_space() {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (written_tx, written_rx) = mpsc::channel();
        let mut gate = Some(release_rx);
        let log = Log::new(2, move |line| {
            if let Some(gate) = gate.take() {
                let _ = entered_tx.send(());
                let _ = gate.recv_timeout(Duration::from_secs(3));
            }
            let _ = written_tx.send(line.to_vec());
        })
        .unwrap();
        assert!(log.append(b"first".to_vec()));
        entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(log.append(b"second".to_vec()));
        assert!(log.append(b"third".to_vec()));
        assert!(!log.append(b"queue full".to_vec()));
        assert!(!log.append(vec![0; MAX_RECORD_BYTES + 1]));
        assert_eq!(log.dropped(), 2);
        // Even Drop must not wait for the writer, which is still blocked.
        drop(log);
        let _ = release_tx.send(());
        let received: Vec<_> = (0..3)
            .map(|_| written_rx.recv_timeout(Duration::from_secs(3)).unwrap())
            .collect();
        assert_eq!(
            received,
            [b"first".to_vec(), b"second".to_vec(), b"third".to_vec()]
        );
    }

    #[test]
    fn repeated_windows_rotation_replaces_the_previous_file_and_keeps_both_bounded() {
        let path =
            std::env::temp_dir().join(format!("LiteTray-log-rotation-{}.log", std::process::id()));
        let previous = path.with_extension("old.log");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&previous);
        for line in [b"one!", b"two!", b"tri!", b"four"] {
            write_file(&path, 4, line).unwrap();
            assert!(std::fs::metadata(&path).unwrap().len() <= 4);
        }
        assert_eq!(std::fs::read(&path).unwrap(), b"four");
        assert_eq!(std::fs::read(&previous).unwrap(), b"tri!");
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_file(&previous).unwrap();
    }

    #[test]
    fn failed_rotation_cannot_turn_into_unbounded_append() {
        let path =
            std::env::temp_dir().join(format!("LiteTray-log-blocked-{}.log", std::process::id()));
        let previous = path.with_extension("old.log");
        std::fs::write(&path, b"full").unwrap();
        std::fs::create_dir(&previous).unwrap();
        assert!(write_file(&path, 4, b"more").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"full");
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(previous).unwrap();
    }

    #[test]
    fn shutdown_flush_times_out_instead_of_joining_a_stalled_writer() {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut gate = Some(release_rx);
        let log = Log::new(4, move |_| {
            if let Some(gate) = gate.take() {
                let _ = entered_tx.send(());
                let _ = gate.recv_timeout(Duration::from_secs(3));
            }
        })
        .unwrap();
        assert!(log.append(b"restored".to_vec()));
        entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let flushed_while_stalled = log.flush(Duration::from_millis(20));
        let _ = release_tx.send(());
        assert!(!flushed_while_stalled);
        assert!(log.flush(Duration::from_secs(3)));
    }
}
