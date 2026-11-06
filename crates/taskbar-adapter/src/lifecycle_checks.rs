//! Integration checks using controls created in this process. Never locates
//! Explorer or calls a taskbar hook. Invoked under clock::self_test's XAML STA.
use crate::{
    clock::{ClockLease, load},
    session::{Session, State, Target},
    watchdog::Watchdog,
    wide,
    xaml::{Object, check},
    xaml_abi::*,
};
use litetray_adapter_protocol::{
    VERSION,
    clock::{Change, Command, Frame, LEASE_MILLIS, Owner, Style, Text},
};
use std::ptr::{null, null_mut};
use windows_sys::Win32::{
    Foundation::*,
    System::{LibraryLoader::*, SystemInformation::GetTickCount64, Threading::*},
    UI::WindowsAndMessaging::*,
};

struct Fixture {
    time: Object,
    date: Object,
    container: Object,
}
impl Fixture {
    fn new() -> Result<Self, String> {
        Ok(Self {
            time: load(
                r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Text="{Binding}" DataContext="System time"/>"#,
            )?,
            date: load(
                r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Text="System date"/>"#,
            )?,
            container: load(
                r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"/>"#,
            )?,
        })
    }
    fn capture(&self, id: u64) -> Result<Target, String> {
        Ok(Target {
            id,
            clock: ClockLease::capture(&self.time, &self.date, &self.container)?,
        })
    }
    fn text(&self) -> Result<(String, String), String> {
        unsafe {
            let text = |node: &Object| -> Result<String, String> {
                let tb = node.query(&IID_ITextBlock)?;
                tb.string(tb.table::<ITextBlock_Vtbl>().Text, 4096)
            };
            Ok((text(&self.time)?, text(&self.date)?))
        }
    }
    fn width(&self) -> Result<f64, String> {
        unsafe {
            let fe = self.container.query(&IID_IFrameworkElement)?;
            let mut value = 0.;
            check(
                (fe.table::<IFrameworkElement_Vtbl>().Width)(fe.raw(), &mut value),
                "Width",
            )?;
            Ok(value)
        }
    }
    fn restored(&self) -> Result<bool, String> {
        Ok(self.text()? == ("System time".into(), "System date".into()) && self.width()?.is_nan())
    }
}
fn require(condition: bool, name: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(format!("会话隔离检查失败：{name}"))
    }
}
fn frame() -> Frame {
    Frame {
        text: Text {
            top: "12:30:00".into(),
            bottom: "多云 24°C".into(),
        },
        style: Style {
            font_family: "Segoe UI".into(),
            font_size: 13,
            width: 260,
        },
    }
}
fn command(owner: &Owner, sequence: u64, change: Change) -> Vec<u8> {
    serde_json::to_vec(&Command {
        version: VERSION,
        owner: owner.clone(),
        sequence,
        change,
    })
    .expect("serializable protocol")
}
fn pump_until_closed(watchdog: &Watchdog, milliseconds: u64) -> Result<(), String> {
    let limit = unsafe { GetTickCount64() } + milliseconds;
    while watchdog.state() != State::Closed && unsafe { GetTickCount64() } < limit {
        unsafe {
            MsgWaitForMultipleObjectsEx(0, null(), 100, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    require(
        watchdog.state() == State::Closed && watchdog.error().is_none(),
        "watchdog native timer did not finish",
    )
}
struct Receiver(HWND);
impl Receiver {
    fn new() -> Result<Self, String> {
        unsafe {
            let window = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("LiteTray isolated session receiver").as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            if window.is_null() {
                Err("创建隔离检查接收窗口失败".into())
            } else {
                Ok(Self(window))
            }
        }
    }
}
impl Drop for Receiver {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.0);
        }
    }
}

pub fn run() -> Result<Vec<&'static str>, String> {
    let primary = Fixture::new()?;
    let secondary = Fixture::new()?;
    let owner = Owner {
        nonce: 123,
        process_id: unsafe { GetCurrentProcessId() },
        receiver: 1,
    };
    let targets =
        || -> Result<Vec<Target>, String> { Ok(vec![primary.capture(1)?, secondary.capture(2)?]) };
    let mut checks = Vec::new();
    let mut session = Session::new(owner.clone(), targets()?, 100)?;
    session.start(frame(), 100)?;
    require(
        primary.text()? == secondary.text()?
            && primary.width()? == 260.
            && secondary.width()? == 260.,
        "start all clocks",
    )?;
    checks.push("session_multiple_targets");
    let tick = Text {
        top: "12:30:01".into(),
        bottom: "晴 25°C".into(),
    };
    session.receive(
        &command(&owner, 1, Change::Tick { text: tick.clone() }),
        200,
    )?;
    require(
        primary.text()? == (tick.top.clone(), tick.bottom.clone())
            && primary.text()? == secondary.text()?,
        "tick all clocks",
    )?;
    let stale = command(&owner, 1, Change::Tick { text: frame().text });
    require(
        session.receive(&stale, 300).is_err() && primary.text()?.0 == tick.top,
        "reject replay",
    )?;
    let mut other = owner.clone();
    other.nonce += 1;
    require(
        session
            .receive(&command(&other, 2, Change::Stop), 400)
            .is_err()
            && session.state() == State::Active,
        "reject other owner",
    )?;
    checks.push("session_replay_and_owner_validation");
    let mut updated = frame();
    updated.style.width = 320;
    session.receive(
        &command(
            &owner,
            2,
            Change::Update {
                frame: updated.clone(),
            },
        ),
        500,
    )?;
    require(
        primary.width()? == 320. && secondary.width()? == 320.,
        "style update",
    )?;
    checks.push("session_live_style_update");
    session.receive(&command(&owner, 3, Change::Heartbeat), 1000)?;
    session.poll(1000 + LEASE_MILLIS - 1, true)?;
    require(
        session.state() == State::Active,
        "heartbeat extends deadline",
    )?;
    session.poll(1000 + LEASE_MILLIS, true)?;
    require(
        session.state() == State::Closed && primary.restored()? && secondary.restored()?,
        "expired restores every target",
    )?;
    require(
        session
            .receive(
                &command(&owner, 4, Change::Update { frame: frame() }),
                17000,
            )
            .is_err()
            && primary.restored()?,
        "late update cannot restart",
    )?;
    checks.push("session_heartbeat_and_expiry");
    let mut session = Session::new(owner.clone(), targets()?, 20000)?;
    session.start(frame(), 20000)?;
    session.poll(20001, false)?;
    require(
        primary.restored()? && secondary.restored()? && session.state() == State::Closed,
        "host loss",
    )?;
    checks.push("session_host_loss");
    let mut session = Session::new(owner.clone(), targets()?, 30000)?;
    session.start(frame(), 30000)?;
    session.receive(&command(&owner, 1, Change::Stop), 30001)?;
    session.stop()?;
    require(
        primary.restored()? && secondary.restored()?,
        "explicit and repeated stop",
    )?;
    checks.push("session_explicit_stop");
    {
        let mut session = Session::new(owner.clone(), targets()?, 40000)?;
        session.start(frame(), 40000)?;
    }
    require(primary.restored()? && secondary.restored()?, "session drop")?;
    checks.push("session_drop_restore");
    require(
        Session::new(
            owner.clone(),
            vec![primary.capture(1)?, primary.capture(2)?],
            50000,
        )
        .is_err(),
        "overlapping native controls",
    )?;
    require(
        Session::new(
            owner.clone(),
            vec![primary.capture(1)?, secondary.capture(1)?],
            50000,
        )
        .is_err(),
        "duplicate target ids",
    )?;
    checks.push("session_overlap_rejection");

    // Native timer test: destroy only our synthetic host receiver and pump the
    // real Windows message queue. No direct call to Session::poll is made.
    let receiver = Receiver::new()?;
    let owner = Owner {
        receiver: receiver.0 as u64,
        ..owner
    };
    let session = Session::new(owner.clone(), targets()?, unsafe { GetTickCount64() })?;
    let watchdog = Watchdog::attach(session)?;
    watchdog.start(frame())?;
    watchdog.receive(&command(&owner, 1, Change::Heartbeat))?;
    require(
        watchdog.state() == State::Active && watchdog.error().is_none(),
        "watchdog start",
    )?;
    drop(receiver);
    pump_until_closed(&watchdog, 4000)?;
    require(
        watchdog.state() == State::Closed
            && watchdog.error().is_none()
            && primary.restored()?
            && secondary.restored()?,
        "native timer host loss",
    )?;
    checks.push("native_watchdog_receiver_loss");
    drop(watchdog);

    // Repeat the actual resource lifetime and compare both kernel handles and
    // USER objects after one warmup. XAML's own first-use resources are stable.
    let receiver = Receiver::new()?;
    let owner = Owner {
        receiver: receiver.0 as u64,
        ..owner
    };
    // Keep the host process and receiver alive but stop sending heartbeats.
    // Expiry must be driven by the real monotonic Windows timer, not a fake
    // time jump or an explicit stop call from the test.
    let session = Session::new(owner.clone(), targets()?, unsafe { GetTickCount64() })?;
    let watchdog = Watchdog::attach(session)?;
    watchdog.start(frame())?;
    pump_until_closed(&watchdog, LEASE_MILLIS + 2500)?;
    require(
        primary.restored()? && secondary.restored()?,
        "heartbeat absence restores all clocks",
    )?;
    drop(watchdog);
    checks.push("native_watchdog_heartbeat_expiry");
    let resources = || -> Result<(u32, u32), String> {
        unsafe {
            let mut handles = 0;
            if GetProcessHandleCount(GetCurrentProcess(), &mut handles) == 0 {
                return Err("无法读取隔离检查句柄数".into());
            }
            Ok((
                handles,
                GetGuiResources(GetCurrentProcess(), GR_USEROBJECTS),
            ))
        }
    };
    let baseline = resources()?;
    for _ in 0..12 {
        let session = Session::new(owner.clone(), targets()?, unsafe { GetTickCount64() })?;
        let watchdog = Watchdog::attach(session)?;
        watchdog.start(frame())?;
        watchdog.stop()?;
        drop(watchdog);
        require(
            primary.restored()? && secondary.restored()?,
            "repeated lifetime restore",
        )?;
    }
    let after = resources()?;
    require(
        after.0 <= baseline.0 && after.1 <= baseline.1,
        "repeated lifetime frees native handles",
    )?;
    checks.push("native_watchdog_resource_cleanup");
    // A competing writer changes the second clock after LiteTray starts. The
    // first clock is updated before the second reports a conflict; both must
    // leave the session while the competing writer's value survives.
    let mut session = Session::new(owner.clone(), targets()?, 60000)?;
    session.start(frame(), 60000)?;
    unsafe {
        let date = secondary.date.query(&IID_ITextBlock)?;
        let external = crate::xaml::StringHandle::new("External date")?;
        check(
            (date.table::<ITextBlock_Vtbl>().SetText)(date.raw(), external.0),
            "SetText external",
        )?;
    }
    let text = Text {
        top: "12:31:00".into(),
        bottom: "更新天气".into(),
    };
    require(
        session
            .receive(&command(&owner, 1, Change::Tick { text }), 60001)
            .is_err(),
        "conflict detected",
    )?;
    require(
        session.state() == State::Closed
            && primary.restored()?
            && secondary.text()? == ("System time".into(), "External date".into())
            && secondary.width()?.is_nan(),
        "multi-target conflict rollback",
    )?;
    checks.push("session_conflict_preserves_external_value");

    // A different width is an external change even when the original width
    // was unset. It must survive a normal stop.
    let mut session = Session::new(owner.clone(), vec![primary.capture(1)?], 70000)?;
    session.start(frame(), 70000)?;
    unsafe {
        let fe = primary.container.query(&IID_IFrameworkElement)?;
        check(
            (fe.table::<IFrameworkElement_Vtbl>().SetWidth)(fe.raw(), 444.),
            "SetWidth external",
        )?;
    }
    session.stop()?;
    require(
        primary.width()? == 444. && primary.text()? == ("System time".into(), "System date".into()),
        "preserve competing style on stop",
    )?;
    checks.push("session_stop_preserves_external_style");
    // Rebinding to an identical visible value must still transfer ownership;
    // equality of the displayed text is insufficient to identify our write.
    let isolated = Fixture::new()?;
    let mut session = Session::new(owner.clone(), vec![isolated.capture(1)?], 80000)?;
    session.start(frame(), 80000)?;
    unsafe {
        let source = Fixture::new()?;
        let statics =
            crate::xaml::factory("Windows.UI.Xaml.Controls.TextBlock", &IID_ITextBlockStatics)?;
        let mut property = null_mut();
        check(
            (statics.table::<ITextBlockStatics_Vtbl>().TextProperty)(statics.raw(), &mut property),
            "TextProperty test",
        )?;
        let property = Object::owned(property)?;
        let fe2 = source.time.query(&IID_IFrameworkElement2)?;
        let mut expression = null_mut();
        check(
            (fe2.table::<IFrameworkElement2_Vtbl>().GetBindingExpression)(
                fe2.raw(),
                property.raw(),
                &mut expression,
            ),
            "GetBindingExpression test",
        )?;
        let expression = Object::owned(expression)?.query(&IID_IBindingExpression)?;
        let mut binding = null_mut();
        check(
            (expression.table::<IBindingExpression_Vtbl>().ParentBinding)(
                expression.raw(),
                &mut binding,
            ),
            "ParentBinding test",
        )?;
        let binding = Object::owned(binding)?;
        let target = isolated.date.query(&IID_IFrameworkElement)?;
        check(
            (target.table::<IFrameworkElement_Vtbl>().SetDataContext)(
                target.raw(),
                crate::clock::boxed_text("多云 24°C")?.raw(),
            ),
            "DataContext external",
        )?;
        check(
            (target.table::<IFrameworkElement_Vtbl>().SetBinding)(
                target.raw(),
                property.raw(),
                binding.raw(),
            ),
            "SetBinding external",
        )?;
        session.stop()?;
        check(
            (target.table::<IFrameworkElement_Vtbl>().SetDataContext)(
                target.raw(),
                crate::clock::boxed_text("External binding still alive")?.raw(),
            ),
            "DataContext after stop",
        )?;
        require(
            isolated.text()?.1 == "External binding still alive" && isolated.width()?.is_nan(),
            "preserve external binding",
        )?;
    }
    checks.push("session_stop_preserves_external_binding");
    Ok(checks)
}
