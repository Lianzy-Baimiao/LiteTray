//! Real DLL/host recovery checks. Uses only this process's hidden windows;
//! explicit host times exercise backoff without sleeping or expiring XAML.
use super::*;
use crate::recovery::Phase;
use litetray_adapter_protocol::activation::{Input, Rect};
use windows::Win32::System::WinRT::{RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize};

pub fn self_test() -> Result<Vec<&'static str>, String> {
    unsafe {
        RoInitialize(RO_INIT_SINGLETHREADED).map_err(|e| e.to_string())?;
    }
    struct Runtime;
    impl Drop for Runtime {
        fn drop(&mut self) {
            unsafe {
                RoUninitialize();
            }
        }
    }
    let _runtime = Runtime;
    struct Receiver(HWND);
    impl Drop for Receiver {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }
    let receiver = Receiver(unsafe {
        CreateWindowExW(
            0,
            super::super::wide("STATIC").as_ptr(),
            null(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        )
    });
    require(!receiver.0.is_null(), "create receiver")?;
    let frame = frame(&ClockConfig::default(), "Recovery test");
    let environment = Environment::open()?;
    // A failed first island must retain its runtime so a later retry can work.
    // Only this fixture replaces the invalid receiver with its own real HWND.
    let mut initially_failed = Preview::open(null_mut(), frame.clone())?;
    require(
        initially_failed.connection().is_none(),
        "initial create rejected invalid receiver",
    )?;
    require(
        matches!(
            initially_failed
                .state
                .borrow()
                .as_ref()
                .unwrap()
                .recovery
                .phase(),
            Phase::Waiting { .. }
        ),
        "initial failure is retryable",
    )?;
    initially_failed.receiver = receiver.0;
    initially_failed
        .state
        .borrow_mut()
        .as_mut()
        .unwrap()
        .visible = false;
    initially_failed.retry();
    require(
        initially_failed
            .connection()
            .ok_or("initial creation retry missing")?
            .status()?
            == SessionStatus::Active,
        "initial failure retry owns active native session",
    )?;
    drop(initially_failed);
    let service = Preview {
        receiver: receiver.0,
        environment: environment.clone(),
        started: std::time::Instant::now(),
        state: RefCell::new(Some(PreviewState {
            connection: Some(Rc::new(Connection::open(
                environment,
                receiver.0,
                frame.clone(),
            )?)),
            recovery: crate::recovery::Recovery::running(0),
            desired: frame.clone(),
            visible: false,
            next_restore: 0,
            applied_error: None,
        })),
    };
    let connection = service.connection().ok_or("missing initial connection")?;
    require(
        connection.status()? == SessionStatus::Active && !service.visible(),
        "hidden active session",
    )?;
    let old_event = event(&connection);
    let bytes = serde_json::to_vec(&old_event).map_err(|e| e.to_string())?;
    service.accept(connection.island, &bytes)?;
    require(service.owns_activation(&old_event), "active deferred event")?;
    let weak = Rc::downgrade(&connection);
    let mut checks = vec![
        "host_preview_failed_initial_creation_remains_retryable",
        "host_preview_native_state_and_hidden_creation",
    ];

    {
        let _call = connection.enter().ok_or("enter native call")?;
        require(
            connection.status()? == SessionStatus::Busy,
            "reentrant status is Busy",
        )?;
        step(&service, 0);
        require(
            Rc::ptr_eq(
                &connection,
                &service.connection().ok_or("busy lost connection")?,
            ),
            "busy connection retained",
        )?;
    }
    checks.push("host_preview_busy_call_retains_connection");
    let mut invalid = frame.clone();
    invalid.style.width = 1;
    require(service.update(&invalid).is_err(), "invalid draft rejected")?;
    service.heartbeat()?;
    require(
        connection.status()? == SessionStatus::Active,
        "invalid draft did not expire lease",
    )?;
    require(
        service
            .state
            .borrow()
            .as_ref()
            .unwrap()
            .applied_error
            .is_some(),
        "invalid draft diagnostic",
    )?;
    service.update(&frame)?;
    checks.push("host_preview_invalid_draft_preserves_active_session");

    require(connection.stop()?, "native stop accepted")?;
    require(
        connection.status()? == SessionStatus::Closed,
        "native Closed restoration proof",
    )?;
    require(connection.update(&frame).is_err(), "closed update rejected")?;
    require(
        !service.owns_activation(&old_event),
        "closed deferred input rejected before host poll",
    )?;
    drop(connection);
    step(&service, 0);
    require(
        weak.upgrade().is_none() && service.connection().is_none(),
        "old connection released after Closed",
    )?;
    step(&service, 999);
    require(service.connection().is_none(), "backoff enforced")?;
    checks.push("host_preview_closed_proof_before_release_and_backoff");
    step(&service, 1000);
    let new = service
        .connection()
        .ok_or_else(|| format!("host recovery did not reconnect: {}", service.status_text()))?;
    require(
        new.status()? == SessionStatus::Active && !service.visible(),
        "hidden state preserved after recreation",
    )?;
    require(
        new.owner.nonce != old_event.owner.nonce && !service.owns_activation(&old_event),
        "old generation rejected",
    )?;
    require(
        service.accept(new.island, &bytes).is_err(),
        "old event cannot enter new gate",
    )?;
    let next_event = event(&new);
    let bytes = serde_json::to_vec(&next_event).map_err(|e| e.to_string())?;
    service.accept(new.island, &bytes)?;
    require(
        service.owns_activation(&next_event),
        "new generation sequence starts fresh",
    )?;
    service.update(&frame)?;
    drop(new);
    checks.push("host_preview_recreation_keeps_hidden_and_rejects_old_generation");

    // Repeated short sessions share a failure budget; a recreation cannot
    // reset it and spin forever when its native target repeatedly disappears.
    stop(&service)?;
    step(&service, 1100);
    step(&service, 6099);
    require(service.connection().is_none(), "second backoff enforced")?;
    step(&service, 6100);
    require(service.connection().is_some(), "second recreation")?;
    stop(&service)?;
    step(&service, 6200);
    require(
        service.state.borrow().as_ref().unwrap().recovery.phase() == Phase::Suspended,
        "third failure suspended",
    )?;
    step(&service, 1_000_000);
    require(
        service.connection().is_none(),
        "suspended does not recreate",
    )?;
    checks.push("host_preview_repeated_native_failures_suspend_recovery");
    service.retry();
    let recovered = service
        .connection()
        .ok_or("manual retry failed to reconnect")?;
    require(
        recovered.status()? == SessionStatus::Active && !service.visible(),
        "manual retry active and hidden",
    )?;
    service.update(&frame)?;
    checks.push("host_preview_manual_retry_recreates_native_session");
    Ok(checks)
}
fn require(condition: bool, name: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(format!("Host preview recovery check failed: {name}"))
    }
}
fn step(service: &Preview, now: u64) {
    let mut state = service.state.borrow_mut().take().unwrap();
    service.advance(&mut state, now, false);
    *service.state.borrow_mut() = Some(state);
}
fn stop(service: &Preview) -> Result<(), String> {
    require(
        service
            .connection()
            .ok_or("missing connection to stop")?
            .stop()?,
        "stop for recovery",
    )
}
fn event(connection: &Connection) -> Activation {
    Activation {
        version: litetray_adapter_protocol::VERSION,
        owner: connection.owner.clone(),
        sequence: 1,
        clock_id: 1,
        source_window: connection.island as u64,
        anchor: Rect {
            left: 0,
            top: 0,
            right: 100,
            bottom: 40,
        },
        input: Input::Pointer,
    }
}
