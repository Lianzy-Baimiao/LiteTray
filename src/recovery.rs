//! Host recovery policy. A failed/unknown call is not evidence of restoration.
//! Only an observed Closed session permits dropping it and trying a new one.
pub const MAX_FAILURES: u8 = 3;
const STABLE_MILLIS: u64 = 60_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Running { since: u64 },
    Restoring,
    Waiting { until: u64 },
    Suspended,
    RestoreBlocked,
}
pub struct Recovery {
    phase: Phase,
    failures: u8,
    restore_failures: u8,
    reason: Option<String>,
}
impl Recovery {
    /// An intentional topology/configuration change still requires restoration,
    /// but is not a failed connection attempt.
    pub fn restarting(reason: &str) -> Self {
        Self {
            phase: Phase::Restoring,
            failures: 0,
            restore_failures: 0,
            reason: Some(reason.chars().take(1024).collect()),
        }
    }
    /// Initial creation has no previous session to restore.
    pub fn connecting(now: u64) -> Self {
        Self {
            phase: Phase::Waiting { until: now },
            failures: 0,
            restore_failures: 0,
            reason: None,
        }
    }
    pub fn running(now: u64) -> Self {
        Self {
            phase: Phase::Running { since: now },
            failures: 0,
            restore_failures: 0,
            reason: None,
        }
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }
    pub fn failures(&self) -> u8 {
        self.failures
    }
    pub fn fault(&mut self, reason: &str) {
        if matches!(self.phase, Phase::Running { .. }) {
            self.failures = self.failures.saturating_add(1);
            self.reason = Some(reason.chars().take(1024).collect());
            self.restore_failures = 0;
            self.phase = Phase::Restoring;
        }
    }
    pub fn healthy(&mut self, now: u64) {
        if let Phase::Running { since } = self.phase
            && now
                .checked_sub(since)
                .is_some_and(|elapsed| elapsed >= STABLE_MILLIS)
        {
            self.failures = 0;
            self.reason = None;
        }
    }
    pub fn restored(&mut self, now: u64) {
        if matches!(self.phase, Phase::Restoring | Phase::RestoreBlocked) {
            self.wait_or_suspend(now);
        }
    }
    pub fn restore_failed(&mut self, reason: &str) {
        if self.phase == Phase::Restoring {
            self.restore_failures = self.restore_failures.saturating_add(1);
            self.reason = Some(reason.chars().take(1024).collect());
            if self.restore_failures >= MAX_FAILURES {
                self.phase = Phase::RestoreBlocked;
            }
        }
    }
    pub fn ready(&self, now: u64) -> bool {
        matches!(self.phase,Phase::Waiting { until } if now >= until)
    }
    pub fn connected(&mut self, now: u64) {
        if self.ready(now) {
            self.phase = Phase::Running { since: now };
        }
    }
    pub fn connect_failed(&mut self, now: u64, reason: &str) {
        if self.ready(now) {
            self.failures = self.failures.saturating_add(1);
            self.reason = Some(reason.chars().take(1024).collect());
            self.wait_or_suspend(now);
        }
    }
    /// Environmental notifications invalidate routes, but are not an explicit
    /// user retry. Preserve failure/backoff and unresolved restore budgets.
    pub fn environment_changed(&mut self, now: u64, connected: bool, reason: &str) {
        if matches!(self.phase, Phase::Suspended | Phase::RestoreBlocked) {
            return;
        }
        if connected {
            if self.phase != Phase::Restoring {
                self.restore_failures = 0;
            }
            self.phase = Phase::Restoring;
        } else if !matches!(self.phase, Phase::Waiting { .. }) {
            self.phase = Phase::Waiting { until: now };
        }
        if self.failures == 0 {
            self.reason = Some(reason.chars().take(1024).collect());
        }
    }
    fn wait_or_suspend(&mut self, now: u64) {
        self.phase = if self.failures >= MAX_FAILURES {
            Phase::Suspended
        } else {
            Phase::Waiting {
                until: now.saturating_add(if self.failures <= 1 { 1000 } else { 5000 }),
            }
        };
    }
    pub fn retry(&mut self, now: u64) {
        self.failures = 0;
        self.restore_failures = 0;
        self.phase = match self.phase {
            Phase::RestoreBlocked | Phase::Restoring => Phase::Restoring,
            Phase::Running { .. } => self.phase,
            _ => Phase::Waiting { until: now },
        };
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_creation_failure_uses_the_same_bounded_retry_budget() {
        let mut recovery = Recovery::connecting(0);
        recovery.connect_failed(0, "unavailable");
        assert!(!recovery.ready(999));
        recovery.connect_failed(1000, "unavailable");
        recovery.connect_failed(6000, "unavailable");
        assert_eq!(recovery.phase(), Phase::Suspended);
        recovery.retry(7000);
        recovery.connected(7000);
        assert_eq!(recovery.phase(), Phase::Running { since: 7000 });
    }
    #[test]
    fn no_reconnect_before_confirmed_restore_and_manual_retry_preserves_obligation() {
        let mut r = Recovery::running(0);
        r.fault("lost");
        for _ in 0..3 {
            r.restore_failed("restore unknown");
        }
        assert_eq!(r.phase(), Phase::RestoreBlocked);
        assert!(!r.ready(u64::MAX));
        r.retry(90000);
        assert_eq!(r.phase(), Phase::Restoring);
        assert!(!r.ready(90000));
        r.restored(90000);
        assert!(!r.ready(90999));
        assert!(r.ready(91000));
    }
    #[test]
    fn consecutive_short_lived_connections_exhaust_shared_failure_budget() {
        let mut r = Recovery::running(0);
        r.fault("first");
        r.restored(10);
        assert!(!r.ready(1009));
        r.connected(1010);
        r.fault("second");
        r.restored(1020);
        assert!(!r.ready(6019));
        r.connected(6020);
        r.fault("third");
        r.restored(6030);
        assert_eq!(r.phase(), Phase::Suspended);
        r.retry(7000);
        assert!(r.ready(7000));
        r.connected(7000);
        assert_eq!(r.failures(), 0);
    }
    #[test]
    fn failed_recreation_and_health_reset_have_bounded_monotonic_behavior() {
        let mut r = Recovery::running(100);
        r.fault("failure");
        r.restored(200);
        r.connect_failed(1200, "open failed");
        assert_eq!(r.failures(), 2);
        r.connect_failed(6199, "too early");
        assert_eq!(r.failures(), 2);
        r.connected(6200);
        r.healthy(6199);
        r.healthy(66199);
        assert_eq!(r.failures(), 2);
        r.healthy(66200);
        assert_eq!(r.failures(), 0);
        assert_eq!(r.reason(), None);
    }
}
