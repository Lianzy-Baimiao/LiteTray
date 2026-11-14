use super::*;
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};
const KEY: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "litetray-safety-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        Self(directory.join("safety.json"))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        // Never recursively remove a computed directory.
        let _ = fs::remove_file(&self.0);
        let _ = fs::remove_file(self.0.with_extension("lock"));
        let _ = fs::remove_file(self.0.with_extension(format!("tmp-{}", std::process::id())));
        let _ = fs::remove_dir(&self.0);
        let _ = fs::remove_dir(self.0.parent().unwrap());
    }
}
#[cfg(windows)]
fn exited_host(path: &std::path::Path, normal: bool) {
    use std::{
        os::windows::process::CommandExt,
        process::Stdio,
        time::{Duration, Instant},
    };
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "taskbar_safety::tests::durable_arm_then_exit_child",
            "--ignored",
            "--test-threads=1",
        ])
        .env("LITETRAY_SAFETY_CHILD", path)
        .env(
            "LITETRAY_SAFETY_NORMAL_EXIT",
            if normal { "1" } else { "0" },
        )
        .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("private journal host exceeded deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "private journal host failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[cfg(windows)]
#[test]
fn three_abrupt_private_host_exits_suspend_native_injection_across_restarts() {
    let scratch = Scratch::new();
    for _ in 0..3 {
        exited_host(&scratch.0, false);
    }
    let journal = Journal::new(scratch.0.clone());
    assert!(
        journal.arm(KEY, false).is_err(),
        "three terminated hosts lost their failure budget on restart"
    );
    let permit = journal.arm(KEY, true).unwrap();
    permit.restored();
    journal.barrier();
}
#[test]
fn normal_stops_and_unused_preflights_do_not_consume_the_restart_budget() {
    let scratch = Scratch::new();
    for _ in 0..5 {
        let journal = Journal::new(scratch.0.clone());
        let permit = journal.arm(KEY, false).unwrap();
        permit.mark_used();
        permit.restored();
        drop(permit);
        journal.barrier();
        let unused = journal.arm(KEY, false).unwrap();
        drop(unused);
        journal.barrier();
    }
}
#[test]
fn faulted_restore_preserves_failures_and_a_changed_module_uses_its_own_budget() {
    let scratch = Scratch::new();
    let journal = Journal::new(scratch.0.clone());
    for _ in 0..3 {
        let permit = journal.arm(KEY, false).unwrap();
        permit.mark_used();
        permit.fault();
        permit.restored();
        drop(permit);
        journal.barrier();
    }
    assert!(
        journal.arm(KEY, false).is_err(),
        "confirmed restore erased fault history"
    );
    let changed = journal.arm(OTHER, false).unwrap();
    changed.restored();
    journal.barrier();
    assert!(
        journal.arm(KEY, false).is_err(),
        "another module erased the previous module's pause"
    );
    let retry = journal.arm(KEY, true).unwrap();
    retry.restored();
    journal.barrier();
}
#[test]
fn corrupt_journal_blocks_injection_until_explicit_retry() {
    let scratch = Scratch::new();
    fs::write(&scratch.0, b"{ corrupt").unwrap();
    let journal = Journal::new(scratch.0.clone());
    assert!(
        journal.arm(KEY, false).is_err(),
        "invalid safety state was treated as a fresh budget"
    );
    let retry = journal.arm(KEY, true).unwrap();
    retry.restored();
    journal.barrier();
}
#[test]
fn stable_session_resets_old_failures_but_keeps_the_current_attempt_armed() {
    let scratch = Scratch::new();
    let journal = Journal::new(scratch.0.clone());
    for _ in 0..2 {
        let permit = journal.arm(KEY, false).unwrap();
        permit.mark_used();
        permit.fault();
        drop(permit);
        journal.barrier();
    }
    let stable = journal.arm(KEY, false).unwrap();
    stable.mark_used();
    stable.healthy();
    journal.barrier();
    drop(stable); // An abrupt exit after health still leaves one armed strike.
    let next = journal.arm(KEY, false).unwrap();
    next.mark_used();
    next.fault();
    drop(next);
    journal.barrier();
    let third = journal.arm(KEY, false).unwrap();
    third.mark_used();
    third.fault();
    drop(third);
    journal.barrier();
    assert!(journal.arm(KEY, false).is_err());
}
#[test]
#[ignore = "bounded child process of the restart-persistence regression"]
fn durable_arm_then_exit_child() {
    let Some(path) = std::env::var_os("LITETRAY_SAFETY_CHILD") else {
        return;
    };
    let path = PathBuf::from(path);
    let journal = for_process(path.clone()).unwrap();
    let permit = journal.arm(KEY, false).unwrap();
    permit.mark_used();
    assert_eq!(
        read(&path).unwrap().records[0].pending.as_deref(),
        Some(permit.token.as_str()),
        "native-use receipt was not persisted before host exit"
    );
    if std::env::var("LITETRAY_SAFETY_NORMAL_EXIT").as_deref() == Ok("1") {
        permit.restored();
        flush(Duration::from_millis(200));
        assert!(
            read(&path).unwrap().records[0].pending.is_none(),
            "normal process exit lost its Stop receipt"
        );
    }
    // Deliberately skip destructors after the durable preflight handshake.
    std::process::exit(0);
}
#[cfg(windows)]
#[test]
fn normal_private_process_exits_flush_stop_receipts_after_the_ui_lifetime() {
    let scratch = Scratch::new();
    for _ in 0..5 {
        exited_host(&scratch.0, true);
    }
    let journal = Journal::new(scratch.0.clone());
    let permit = journal.arm(KEY, false).unwrap();
    permit.restored();
    journal.barrier();
    assert_eq!(read(&scratch.0).unwrap().records[0].failures, 0);
}
#[test]
fn delayed_cancel_or_fault_cannot_discharge_a_newer_attempt() {
    let scratch = Scratch::new();
    let journal = Journal::new(scratch.0.clone());
    let old = journal.arm(KEY, false).unwrap();
    let current = journal.arm(KEY, false).unwrap();
    current.mark_used();
    drop(old);
    journal.barrier();
    assert_eq!(
        read(&scratch.0).unwrap().records[0].pending.as_deref(),
        Some(current.token.as_str())
    );
    assert!(journal.notify(KEY, "stale-token", Event::Fault));
    journal.barrier();
    assert_eq!(
        read(&scratch.0).unwrap().records[0].pending.as_deref(),
        Some(current.token.as_str())
    );
    current.restored();
    journal.barrier();
}
#[test]
fn failed_durable_write_never_returns_a_native_permit() {
    let scratch = Scratch::new();
    let journal = Journal::with_writer(scratch.0.clone(), |_, _| {
        Err("injected disk write failure".into())
    });
    assert!(journal.arm(KEY, false).is_err());
    assert!(journal.arm(KEY, true).is_err());
    assert!(!scratch.0.exists());
}
#[test]
fn full_queue_and_stalled_writer_never_block_fault_health_or_restore_callbacks() {
    use std::time::Instant;
    let scratch = Scratch::new();
    let (entered, observation) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    struct Release(mpsc::Sender<()>);
    impl Drop for Release {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }
    let release = Release(release);
    let mut writes = 0;
    let journal = Journal::with_writer(scratch.0.clone(), move |path, bytes| {
        writes += 1;
        if writes == 2 {
            entered.send(()).unwrap();
            gate.recv_timeout(ACK_TIMEOUT).unwrap();
        }
        crate::config::atomic_write(path, bytes)
    });
    let permit = journal.arm(KEY, false).unwrap();
    permit.mark_used();
    permit.healthy();
    observation.recv_timeout(ACK_TIMEOUT).unwrap();
    for _ in 0..QUEUE_SIZE {
        assert!(journal.notify(OTHER, "stale", Event::Restored));
    }
    let start = Instant::now();
    permit.fault();
    permit.healthy();
    permit.restored();
    assert!(
        start.elapsed() < Duration::from_millis(100),
        "native lifecycle callbacks waited for a blocked journal writer"
    );
    assert!(journal.uncertain.load(Ordering::Acquire));
    drop(release);
    journal.barrier();
    assert!(
        journal.arm(KEY, false).is_err(),
        "dropped callback silently reopened injection"
    );
    let retry = journal.arm(KEY, true).unwrap();
    retry.restored();
    journal.barrier();
}
#[test]
fn journal_size_record_count_and_unsettled_budgets_are_bounded() {
    let scratch = Scratch::new();
    let journal = Journal::new(scratch.0.clone());
    for value in 0..12 {
        let key = format!("{value:064x}");
        let permit = journal.arm(&key, false).unwrap();
        permit.restored();
        journal.barrier();
    }
    assert_eq!(read(&scratch.0).unwrap().records.len(), MAX_RECORDS);
    assert!(fs::metadata(&scratch.0).unwrap().len() <= MAX_BYTES);
    fs::write(&scratch.0, vec![b' '; MAX_BYTES as usize + 1]).unwrap();
    assert!(journal.arm(KEY, false).is_err());
    let permit = journal.arm(KEY, true).unwrap();
    permit.restored();
    journal.barrier();
}
#[test]
fn a_full_unconfirmed_identity_history_requires_explicit_retry_for_a_new_build() {
    let scratch = Scratch::new();
    let journal = Journal::new(scratch.0.clone());
    for value in 0..MAX_RECORDS {
        let permit = journal.arm(&format!("{value:064x}"), false).unwrap();
        permit.mark_used();
        drop(permit);
    }
    assert!(journal.arm(KEY, false).is_err());
    let retry = journal.arm(KEY, true).unwrap();
    retry.restored();
    journal.barrier();
    assert_eq!(read(&scratch.0).unwrap().records.len(), MAX_RECORDS);
}
