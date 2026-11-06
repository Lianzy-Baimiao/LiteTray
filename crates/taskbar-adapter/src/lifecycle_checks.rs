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
    clock::{COMMAND_TAG, Change, Command, Frame, LEASE_MILLIS, Owner, Style, Text},
};
use std::ptr::{null, null_mut};
use windows_sys::Win32::{
    Foundation::*,
    System::{
        DataExchange::COPYDATASTRUCT, LibraryLoader::*, SystemInformation::GetTickCount64,
        Threading::*,
    },
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
fn native_send(endpoint: HWND, sender: HWND, tag: usize, bytes: &[u8]) -> Result<bool, String> {
    let copy = COPYDATASTRUCT {
        dwData: tag,
        cbData: bytes.len() as u32,
        lpData: bytes.as_ptr() as *mut _,
    };
    let mut accepted = 0;
    if unsafe {
        SendMessageTimeoutW(
            endpoint,
            WM_COPYDATA,
            sender as usize,
            &copy as *const _ as isize,
            SMTO_ABORTIFHUNG,
            1000,
            &mut accepted,
        )
    } == 0
    {
        return Err("隔离原生消息发送超时或失败".into());
    }
    Ok(accepted == 1)
}
fn native_transport_checks(
    primary: &Fixture,
    secondary: &Fixture,
) -> Result<Vec<&'static str>, String> {
    let receiver = Receiver::new()?;
    let other = Receiver::new()?;
    let owner = Owner {
        nonce: 456,
        process_id: unsafe { GetCurrentProcessId() },
        receiver: receiver.0 as u64,
    };
    let session = Session::new(
        owner.clone(),
        vec![primary.capture(1)?, secondary.capture(2)?],
        unsafe { GetTickCount64() },
    )?;
    let watchdog = Watchdog::attach(session)?;
    watchdog.start(frame())?;
    let endpoint = watchdog.endpoint().ok_or("缺少隔离会话消息端点")?;
    let tick = Text {
        top: "20:31:12".into(),
        bottom: "小雨 23°C".into(),
    };
    let bytes = command(&owner, 1, Change::Tick { text: tick.clone() });
    // The actual WM_COPYDATA callback, not Session::receive, rejects invalid
    // envelopes before any native XAML call or sequence update can happen.
    for (sender, tag, input) in [
        (other.0, COMMAND_TAG, bytes.clone()),
        (receiver.0, COMMAND_TAG + 1, bytes.clone()),
        (receiver.0, COMMAND_TAG, Vec::new()),
        (
            receiver.0,
            COMMAND_TAG,
            vec![b' '; litetray_adapter_protocol::MAX_REQUEST + 1],
        ),
        (receiver.0, COMMAND_TAG, b"{".to_vec()),
        (
            receiver.0,
            COMMAND_TAG,
            command(
                &Owner {
                    nonce: owner.nonce + 1,
                    ..owner.clone()
                },
                1,
                Change::Stop,
            ),
        ),
    ] {
        require(
            !native_send(endpoint, sender, tag, &input)?,
            "native reject invalid sender or envelope",
        )?;
    }
    require(
        primary.text()?.0 == frame().text.top && watchdog.state() == State::Active,
        "invalid input leaves clocks active and unchanged",
    )?;
    require(
        native_send(endpoint, receiver.0, COMMAND_TAG, &bytes)?,
        "native tick accepted",
    )?;
    require(
        primary.text()? == (tick.top, tick.bottom) && primary.text()? == secondary.text()?,
        "native tick updates all clocks",
    )?;
    require(
        !native_send(endpoint, receiver.0, COMMAND_TAG, &bytes)?,
        "native replay rejected",
    )?;

    // Send from a different thread while this XAML STA pumps its queue. Only
    // integers and owned bytes cross threads; native controls stay here.
    let sender = receiver.0 as usize;
    let destination = endpoint as usize;
    let mut changed = frame();
    changed.style.width = 340;
    let bytes = command(&owner, 2, Change::Update { frame: changed });
    let thread = std::thread::spawn(move || {
        native_send(destination as HWND, sender as HWND, COMMAND_TAG, &bytes)
    });
    let limit = unsafe { GetTickCount64() } + 3000;
    while !thread.is_finished() && unsafe { GetTickCount64() } < limit {
        unsafe {
            MsgWaitForMultipleObjectsEx(0, null(), 25, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    require(thread.is_finished(), "native sender thread finished")?;
    require(
        thread.join().map_err(|_| "隔离消息发送线程失败")??,
        "cross-thread native update acknowledged",
    )?;
    require(
        primary.width()? == 340. && secondary.width()? == 340.,
        "cross-thread native update on owning XAML thread",
    )?;
    require(
        native_send(
            endpoint,
            receiver.0,
            COMMAND_TAG,
            &command(&owner, 3, Change::Heartbeat),
        )?,
        "native heartbeat",
    )?;
    require(
        native_send(
            endpoint,
            receiver.0,
            COMMAND_TAG,
            &command(&owner, 4, Change::Stop),
        )?,
        "native stop",
    )?;
    require(
        primary.restored()? && secondary.restored()? && watchdog.state() == State::Closed,
        "native stop restores every target",
    )?;
    require(
        !native_send(
            endpoint,
            receiver.0,
            COMMAND_TAG,
            &command(&owner, 5, Change::Tick { text: frame().text }),
        )?,
        "late native update cannot restart session",
    )?;
    drop(watchdog);

    let session = Session::new(
        owner,
        vec![primary.capture(1)?, secondary.capture(2)?],
        unsafe { GetTickCount64() },
    )?;
    let watchdog = Watchdog::attach(session)?;
    watchdog.start(frame())?;
    let endpoint = watchdog.endpoint().ok_or("缺少隔离会话消息端点")?;
    require(
        unsafe { DestroyWindow(endpoint) } != 0,
        "destroy native endpoint",
    )?;
    require(
        primary.restored()?
            && secondary.restored()?
            && watchdog.state() == State::Closed
            && watchdog.endpoint().is_none(),
        "endpoint destruction restores before timer disappears",
    )?;
    drop(watchdog);
    Ok(vec![
        "native_command_sender_and_envelope_validation",
        "native_command_tick_and_replay",
        "native_command_cross_thread_style",
        "native_command_heartbeat_and_stop",
        "native_command_late_update_rejected",
        "native_endpoint_destruction_restore",
    ])
}
pub(crate) fn pump_until_closed(watchdog: &Watchdog, milliseconds: u64) -> Result<(), String> {
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
struct IsolatedChild {
    process: std::process::Child,
    input: std::process::ChildStdin,
    output: std::sync::mpsc::Receiver<Result<serde_json::Value, String>>,
    reader: Option<std::thread::JoinHandle<()>>,
}
impl IsolatedChild {
    fn start() -> Result<Self, String> {
        use std::{
            io::{BufRead, BufReader, Read},
            os::windows::process::CommandExt,
            process::Stdio,
        };
        let mut process =
            std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
                .arg("--self-test-clock-owner")
                .creation_flags(CREATE_NO_WINDOW)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("创建隔离宿主子进程失败：{e}"))?;
        let input = process.stdin.take().expect("piped stdin");
        let stdout = process.stdout.take().expect("piped stdout");
        let (tx, output) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut pipe = BufReader::new(stdout.take(4097));
            for _ in 0..2 {
                let mut line = String::new();
                let result = pipe
                    .read_line(&mut line)
                    .map_err(|e| e.to_string())
                    .and_then(|length| {
                        if length == 0 || length > 4096 {
                            return Err("隔离子进程响应缺失或过长".into());
                        }
                        serde_json::from_str(&line).map_err(|_| "隔离子进程响应无效".into())
                    });
                let failed = result.is_err();
                if tx.send(result).is_err() || failed {
                    break;
                }
            }
        });
        Ok(Self {
            process,
            input,
            output,
            reader: Some(reader),
        })
    }
    fn receive(&self) -> Result<serde_json::Value, String> {
        let limit = unsafe { GetTickCount64() } + 5000;
        loop {
            match self.output.try_recv() {
                Ok(result) => return result,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err("隔离子进程响应通道已关闭".into());
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
            if unsafe { GetTickCount64() } >= limit {
                return Err("隔离子进程响应超时".into());
            }
            unsafe {
                MsgWaitForMultipleObjectsEx(0, null(), 25, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
                let mut message: MSG = std::mem::zeroed();
                while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        }
    }
}
impl Drop for IsolatedChild {
    fn drop(&mut self) {
        if self.process.try_wait().ok().flatten().is_none() {
            let _ = self.process.kill();
        }
        let _ = self.process.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
fn child_process_checks(
    primary: &Fixture,
    secondary: &Fixture,
) -> Result<Vec<&'static str>, String> {
    use std::io::Write;
    let mut child = IsolatedChild::start()?;
    let ready = child.receive()?;
    require(
        ready["process_id"].as_u64() == Some(child.process.id() as u64),
        "child process identity",
    )?;
    let owner = Owner {
        nonce: 789,
        process_id: child.process.id(),
        receiver: ready["receiver"].as_u64().ok_or("隔离子进程接收窗口缺失")?,
    };
    let session = Session::new(
        owner.clone(),
        vec![primary.capture(1)?, secondary.capture(2)?],
        unsafe { GetTickCount64() },
    )?;
    let watchdog = Watchdog::attach(session)?;
    watchdog.start(frame())?;
    let endpoint = watchdog.endpoint().ok_or("缺少跨进程测试端点")?;
    writeln!(
        child.input,
        "{}",
        serde_json::json!({"endpoint":endpoint as u64,"nonce":owner.nonce})
    )
    .and_then(|_| child.input.flush())
    .map_err(|e| e.to_string())?;
    let reply = child.receive()?;
    require(
        reply["accepted"].as_bool() == Some(true),
        "cross-process update acknowledged",
    )?;
    require(
        primary.text()? == ("Child process tick".into(), "跨进程天气 25°C".into())
            && primary.text()? == secondary.text()?,
        "cross-process XAML update",
    )?;
    // Terminate only the child that this check just created. Its HWND disappears
    // and the retained kernel process handle becomes signaled, without Stop.
    child.process.kill().map_err(|e| e.to_string())?;
    child.process.wait().map_err(|e| e.to_string())?;
    pump_until_closed(&watchdog, 4000)?;
    require(
        primary.restored()? && secondary.restored()?,
        "actual child process exit restores clocks",
    )?;
    Ok(vec![
        "native_command_cross_process_tick",
        "native_watchdog_process_exit",
    ])
}
pub(crate) struct Receiver(pub HWND);
impl Receiver {
    pub(crate) fn new() -> Result<Self, String> {
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

    checks.extend(native_transport_checks(&primary, &secondary)?);
    checks.extend(child_process_checks(&primary, &secondary)?);

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
    require(
        native_send(
            watchdog.endpoint().ok_or("缺少隔离消息端点")?,
            receiver.0,
            COMMAND_TAG,
            &command(&owner, 1, Change::Heartbeat),
        )?,
        "native watchdog heartbeat",
    )?;
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
        require(
            native_send(
                watchdog.endpoint().ok_or("缺少隔离消息端点")?,
                receiver.0,
                COMMAND_TAG,
                &command(&owner, 1, Change::Stop),
            )?,
            "native repeated stop",
        )?;
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
    checks.extend(crate::preview::self_test(owner.clone(), frame())?);
    checks.extend(crate::input_checks::run(owner.clone(), frame())?);
    checks.extend(crate::tray_checks::run(owner.clone(), frame())?);
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
