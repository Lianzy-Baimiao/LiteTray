//! Session/input lifecycle checks over two separate hidden native XAML islands.
use crate::{
    input::Probe,
    preview::InputFixture,
    session::{Session, State},
    watchdog::Watchdog,
};
use litetray_adapter_protocol::{
    VERSION,
    clock::{Change, Command, Frame, LEASE_MILLIS, Owner},
};
use std::{
    cell::{Cell, RefCell},
    ptr::null_mut,
    rc::Rc,
};
use windows_sys::Win32::{System::SystemInformation::GetTickCount64, UI::WindowsAndMessaging::*};
fn require(condition: bool, name: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(format!("输入会话检查失败：{name}"))
    }
}
fn stop(owner: &Owner) -> Vec<u8> {
    serde_json::to_vec(&Command {
        version: VERSION,
        owner: owner.clone(),
        sequence: 1,
        change: Change::Stop,
    })
    .expect("serializable command")
}
pub fn run(owner: Owner, frame: Frame) -> Result<Vec<&'static str>, String> {
    let first = InputFixture::new(owner.clone(), frame.clone())?;
    let second = InputFixture::new(owner.clone(), frame.clone())?;
    let new_session = |owner: Owner| {
        Session::new(owner, vec![first.capture(1)?, second.capture(2)?], unsafe {
            GetTickCount64()
        })
    };
    let targets = || Ok::<_, String>(vec![first.input(1)?, second.input(2)?]);
    let events = Rc::new(RefCell::new(Vec::new()));
    let seen = events.clone();
    let nested = Rc::new(RefCell::new(None::<Probe>));
    let nested_callback = nested.clone();
    let nested_consumed = Rc::new(Cell::new(false));
    let nested_result = nested_consumed.clone();
    let watchdog = Watchdog::attach(new_session(owner.clone())?)?;
    watchdog.prepare_inputs(targets()?, move |event| {
        if let Some(probe) = nested_callback.borrow().as_ref() {
            nested_result.set(probe.invoke().unwrap_or(true));
        }
        seen.borrow_mut().push(event);
        true
    })?;
    let a = watchdog.input_probe(1)?;
    let b = watchdog.input_probe(2)?;
    require(
        !a.invoke()? && !b.invoke()? && events.borrow().is_empty(),
        "prepared delegates preserve default input",
    )?;
    watchdog.start(frame.clone())?;
    *nested.borrow_mut() = Some(watchdog.input_probe(2)?);
    require(
        a.invoke()? && !nested_consumed.get(),
        "nested second-clock event is not consumed",
    )?;
    nested.borrow_mut().take();
    require(b.invoke()?, "second clock activation")?;
    require(
        events
            .borrow()
            .iter()
            .map(|event| (event.clock_id, event.sequence))
            .collect::<Vec<_>>()
            == vec![(1, 1), (2, 2)],
        "one monotonic sequence across both clocks",
    )?;
    require(
        a.invoke_with_handled(true)? && events.borrow().len() == 3,
        "clock still publishes a tap already handled by its native IconView",
    )?;
    watchdog.receive(&stop(&owner))?;
    require(
        watchdog.state() == State::Closed
            && !a.invoke()?
            && !b.invoke()?
            && first.restored()?
            && second.restored()?,
        "stop detaches all input and restores both clocks",
    )?;
    drop(watchdog);
    drop(a);
    drop(b);

    let watchdog = Watchdog::attach(new_session(owner.clone())?)?;
    watchdog.prepare_inputs(targets()?, |_| true)?;
    let a = watchdog.input_probe(1)?;
    let b = watchdog.input_probe(2)?;
    watchdog.start(frame.clone())?;
    second.set_date("External date")?;
    let bytes = serde_json::to_vec(&Command {
        version: VERSION,
        owner: owner.clone(),
        sequence: 1,
        change: Change::Tick {
            text: frame.text.clone(),
        },
    })
    .map_err(|e| e.to_string())?;
    require(
        watchdog.receive(&bytes).is_err()
            && watchdog.state() == State::Closed
            && !a.invoke()?
            && !b.invoke()?
            && first.restored()?
            && second.date()? == "External date",
        "external write stops and detaches both clocks without overwriting writer",
    )?;
    second.set_date("Original date")?;
    drop(watchdog);
    drop(a);
    drop(b);

    let watchdog = Watchdog::attach(new_session(owner.clone())?)?;
    watchdog.prepare_inputs(targets()?, |_| true)?;
    let a = watchdog.input_probe(1)?;
    let b = watchdog.input_probe(2)?;
    watchdog.start(frame.clone())?;
    drop(watchdog);
    require(
        !a.invoke()? && !b.invoke()? && first.restored()? && second.restored()?,
        "owner drop cannot retain active input",
    )?;
    drop(a);
    drop(b);

    // Fail the second native registration after the first was attached. None
    // of the prepared delegates may ever become active or leak into a retry.
    let watchdog = Watchdog::attach(new_session(owner.clone())?)?;
    let mut bad = targets()?;
    bad[1].island = null_mut();
    require(
        watchdog.prepare_inputs(bad, |_| true).is_err()
            && watchdog.state() == State::Closed
            && first.restored()?
            && second.restored()?,
        "partial input setup rollback",
    )?;
    drop(watchdog);

    let mut session = Session::new(
        owner.clone(),
        vec![first.capture(1)?, second.capture(2)?],
        100,
    )?;
    session.prepare_inputs(targets()?, |_| true)?;
    let a = session.input_probe(1)?;
    let b = session.input_probe(2)?;
    session.start(frame.clone(), 100)?;
    require(a.invoke()? && b.invoke()?, "synthetic-time session active")?;
    session.poll(100 + LEASE_MILLIS, true)?;
    require(
        !a.invoke()? && !b.invoke()? && first.restored()? && second.restored()?,
        "lease expiry detaches input",
    )?;
    drop(session);
    drop(a);
    drop(b);

    // The publisher synchronously stops this session while a native delegate
    // is on the stack. No RefCell borrow or mutable Session spans that call.
    let watchdog = Rc::new(Watchdog::attach(new_session(owner.clone())?)?);
    let weak = Rc::downgrade(&watchdog);
    watchdog.prepare_inputs(targets()?, move |_| {
        weak.upgrade().is_some_and(|w| w.stop().is_ok())
    })?;
    let a = watchdog.input_probe(1)?;
    let b = watchdog.input_probe(2)?;
    watchdog.start(frame.clone())?;
    require(
        !a.invoke()?
            && !b.invoke()?
            && watchdog.state() == State::Closed
            && first.restored()?
            && second.restored()?,
        "reentrant stop preserves default input",
    )?;
    drop(watchdog);
    drop(a);
    drop(b);

    // Destroy only a receiver created for this test. The native timer removes
    // the subscriptions; even before that timer, publishing is refused.
    let receiver = crate::lifecycle_checks::Receiver::new()?;
    let lost = Owner {
        receiver: receiver.0 as u64,
        ..owner
    };
    let watchdog = Watchdog::attach(new_session(lost)?)?;
    watchdog.prepare_inputs(targets()?, |_| true)?;
    let a = watchdog.input_probe(1)?;
    let b = watchdog.input_probe(2)?;
    watchdog.start(frame.clone())?;
    drop(receiver);
    require(
        !a.invoke()? && !b.invoke()?,
        "receiver loss rejects input before next timer",
    )?;
    crate::lifecycle_checks::pump_until_closed(&watchdog, 4000)?;
    require(
        first.restored()? && second.restored()? && !a.invoke()? && !b.invoke()?,
        "receiver loss restores and detaches",
    )?;
    drop(watchdog);
    drop(a);
    drop(b);
    // Destroy the adapter's command endpoint while both inputs are active.
    let watchdog = Watchdog::attach(new_session(owner)?)?;
    watchdog.prepare_inputs(targets()?, |_| true)?;
    let a = watchdog.input_probe(1)?;
    let b = watchdog.input_probe(2)?;
    watchdog.start(frame)?;
    require(
        unsafe { DestroyWindow(watchdog.endpoint().ok_or("输入测试端点缺失")?) } != 0,
        "destroy input session endpoint",
    )?;
    require(
        watchdog.state() == State::Closed
            && !a.invoke()?
            && !b.invoke()?
            && first.restored()?
            && second.restored()?,
        "endpoint destruction detaches both clocks",
    )?;
    // Keep the public checks compact; each group asserts its full lifecycle.
    Ok(vec![
        "input_session_prepared_is_inactive",
        "input_session_shared_sequence_and_reentrancy",
        "input_session_native_handled_tap",
        "input_session_stop_restores_and_detaches",
        "input_session_partial_prepare_rollback",
        "input_session_expiry_detaches",
        "input_session_reentrant_stop",
        "input_session_native_receiver_loss",
        "input_session_native_endpoint_loss",
        "input_session_external_conflict_detaches",
        "input_session_owner_drop_detaches",
    ])
}
