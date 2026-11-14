//! Bounded host-side record of taskbar session transitions. The adapter keeps
//! its own `adapter-events.log` from inside Explorer; together they explain a
//! clock that briefly returned to the native look, or an Explorer restart.
#[cfg(not(test))]
use std::sync::OnceLock;

pub const HOST_FILE: &str = "taskbar-events.log";
pub const ADAPTER_FILE: &str = "adapter-events.log";
#[cfg(not(test))]
const LIMIT: u64 = 256 * 1024;
#[cfg(not(test))]
static LOG: OnceLock<Option<litetray_event_log::Log>> = OnceLock::new();

pub fn record(kind: &str, detail: &str) {
    #[cfg(test)]
    if !testing::enabled() {
        return;
    }
    let detail: String = detail
        .chars()
        .take(600)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let line = format!(
        "{} pid={} {kind}: {detail}\r\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
        std::process::id(),
    );
    #[cfg(not(test))]
    {
        if let Some(log) = LOG.get_or_init(|| {
            litetray_event_log::Log::file(crate::config::data_dir().join(HOST_FILE), LIMIT).ok()
        }) {
            log.append(line.into_bytes());
        }
    }
    #[cfg(test)]
    testing::append(line.into_bytes());
}

/// Best-effort drain after the process UI loop ends, with no unbounded join.
pub fn flush(timeout: std::time::Duration) {
    #[cfg(not(test))]
    if let Some(Some(log)) = LOG.get() {
        log.flush(timeout);
    }
    #[cfg(test)]
    let _ = timeout;
}

/// Last lines of both logs, for the diagnostics export.
pub fn tail(lines: usize) -> serde_json::Value {
    let read = |name: &str| -> Vec<String> {
        let text = std::fs::read(crate::config::data_dir().join(name))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        let all: Vec<&str> = text.lines().collect();
        all[all.len().saturating_sub(lines)..]
            .iter()
            .map(|l| l.to_string())
            .collect()
    };
    serde_json::json!({ "host": read(HOST_FILE), "adapter": read(ADAPTER_FILE) })
}

#[cfg(test)]
pub(crate) mod testing {
    use std::{cell::RefCell, sync::Arc};

    thread_local! {
        static LOG: RefCell<Option<Arc<litetray_event_log::Log>>> = const { RefCell::new(None) };
    }

    pub(super) fn enabled() -> bool {
        LOG.with(|log| log.borrow().is_some())
    }

    pub(super) fn append(line: Vec<u8>) {
        LOG.with(|log| {
            if let Some(log) = log.borrow().as_ref() {
                log.append(line);
            }
        });
    }

    pub(crate) fn with_log<T>(log: Arc<litetray_event_log::Log>, action: impl FnOnce() -> T) -> T {
        struct Restore(Option<Arc<litetray_event_log::Log>>);
        impl Drop for Restore {
            fn drop(&mut self) {
                LOG.with(|log| *log.borrow_mut() = self.0.take());
            }
        }
        let _restore = Restore(LOG.with(|slot| slot.replace(Some(log))));
        action()
    }
}
