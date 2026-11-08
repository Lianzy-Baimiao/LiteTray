//! Bounded host-side record of taskbar session transitions. The adapter keeps
//! its own `adapter-events.log` from inside Explorer; together they explain a
//! clock that briefly returned to the native look, or an Explorer restart.
use std::io::Write;

pub const HOST_FILE: &str = "taskbar-events.log";
pub const ADAPTER_FILE: &str = "adapter-events.log";
const LIMIT: u64 = 256 * 1024;

pub fn record(kind: &str, detail: &str) {
    if cfg!(test) {
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
    let dir = crate::config::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(HOST_FILE);
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > LIMIT) {
        let _ = std::fs::rename(&path, path.with_extension("old.log"));
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = file.write_all(line.as_bytes());
    }
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
