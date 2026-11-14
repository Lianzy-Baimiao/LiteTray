//! Real dependency-property restoration synchronously enters a native callback.
//! The check runs only in the private self-test process, without shell hooks.
use super::*;
use crate::{lifecycle_checks::Receiver, session::Session, tray::Pair, watchdog::Watchdog};
use std::sync::atomic::AtomicU32;
use windows_sys::core::{GUID, IID_IUnknown, IUnknown_Vtbl};

const IID_CALLBACK: GUID = GUID::from_u128(0x45883d16_27bf_4bc1_ac26_94c1601f3a49);
type NativeAction = Box<dyn FnOnce() -> Result<(), String>>;
#[derive(Default)]
struct Probe {
    popup: HWND,
    callbacks: Cell<u32>,
    active_callbacks: Cell<u32>,
    redirected_positions: Cell<u32>,
    on_change: Cell<Option<NativeAction>>,
    only_value: Option<Object>,
    action_calls: Cell<u32>,
    error: Cell<Option<String>>,
}
thread_local! {
    static POSITION_CALL: Cell<Option<(i32, i32, u32)>> = const { Cell::new(None) };
}
unsafe extern "system" fn record_position(
    _: HWND,
    _: HWND,
    x: i32,
    y: i32,
    _: i32,
    _: i32,
    flags: u32,
) -> i32 {
    POSITION_CALL.with(|call| call.set(Some((x, y, flags))));
    1
}
struct PositionOriginal(usize);
impl PositionOriginal {
    fn install() -> Result<Self, String> {
        if POSITION_ORIGINAL.load(Ordering::Acquire) != 0 {
            return Err("停止回调自检不允许已有定位钩子".into());
        }
        Ok(Self(POSITION_ORIGINAL.swap(
            record_position as *const () as usize,
            Ordering::AcqRel,
        )))
    }
}
impl Drop for PositionOriginal {
    fn drop(&mut self) {
        POSITION_ORIGINAL.store(self.0, Ordering::Release);
    }
}
struct Popup(HWND, Vec<u16>);
impl Popup {
    fn new() -> Result<Self, String> {
        let class = crate::wide("TopLevelWindowForOverflowXamlIsland");
        unsafe {
            let instance = GetModuleHandleW(null_mut());
            let definition = WNDCLASSW {
                lpfnWndProc: Some(DefWindowProcW),
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                ..std::mem::zeroed()
            };
            if RegisterClassW(&definition) == 0 {
                return Err("无法注册私有定位回调窗口".into());
            }
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                crate::wide("Own stop callback probe").as_ptr(),
                WS_POPUP,
                10,
                20,
                80,
                40,
                null_mut(),
                null_mut(),
                instance,
                null_mut(),
            );
            if hwnd.is_null() {
                UnregisterClassW(class.as_ptr(), instance);
                return Err("无法创建私有定位回调窗口".into());
            }
            Ok(Self(hwnd, class))
        }
    }
}
impl Drop for Popup {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.0);
            UnregisterClassW(self.1.as_ptr(), GetModuleHandleW(null_mut()));
        }
    }
}
#[repr(C)]
struct CallbackTable {
    base: IUnknown_Vtbl,
    invoke: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void) -> i32,
}
#[repr(C)]
struct Callback {
    table: &'static CallbackTable,
    references: AtomicU32,
    probe: Rc<Probe>,
}
unsafe extern "system" fn query_callback(
    this: *mut c_void,
    iid: *const GUID,
    output: *mut *mut c_void,
) -> i32 {
    unsafe {
        if iid.is_null() || output.is_null() {
            return E_POINTER;
        }
        *output = null_mut();
        let equal = |a: GUID, b: GUID| {
            a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
        };
        if !equal(*iid, IID_IUnknown) && !equal(*iid, IID_CALLBACK) {
            return E_NOINTERFACE;
        }
        retain_callback(this);
        *output = this;
        S_OK
    }
}
unsafe extern "system" fn retain_callback(this: *mut c_void) -> u32 {
    unsafe {
        (&*this.cast::<Callback>())
            .references
            .fetch_add(1, Ordering::Relaxed)
            + 1
    }
}
unsafe extern "system" fn release_callback(this: *mut c_void) -> u32 {
    let remaining = unsafe {
        (&*this.cast::<Callback>())
            .references
            .fetch_sub(1, Ordering::AcqRel)
            - 1
    };
    if remaining == 0 {
        unsafe { drop(Box::from_raw(this.cast::<Callback>())) };
    }
    remaining
}
unsafe extern "system" fn observe_callback(
    this: *mut c_void,
    sender: *mut c_void,
    property: *mut c_void,
) -> i32 {
    let probe = unsafe { &(&*this.cast::<Callback>()).probe };
    let result: Result<(), String> = (|| {
        // Taking the action before invoking COM keeps a nested callback from
        // repeating the external write or borrowing the fixture twice.
        if let Some(action) = probe.on_change.take() {
            probe.action_calls.set(probe.action_calls.get() + 1);
            action()?;
        }
        if let Some(expected) = &probe.only_value {
            let sender = unsafe { Object::query_borrowed(sender, &IID_IDependencyObject) }?;
            let property = unsafe { Object::query_borrowed(property, &IID_IDependencyProperty) }?;
            if !crate::property::equal_optional(read(&sender, &property)?.as_ref(), Some(expected))?
            {
                return Ok(());
            }
        }
        observe_restore(probe);
        Ok(())
    })();
    if let Err(error) = result {
        probe.error.set(Some(error));
    }
    S_OK
}
fn observe_restore(probe: &Probe) {
    probe.callbacks.set(probe.callbacks.get() + 1);
    if runtime().is_some() {
        probe.active_callbacks.set(probe.active_callbacks.get() + 1);
    }
    POSITION_CALL.with(|call| call.set(None));
    // Call the actual detour, whose saved original only records arguments.
    // No hook is installed and no real system or user popup is moved.
    unsafe {
        position(probe.popup, null_mut(), 10, 20, 80, 40, 0);
    }
    if POSITION_CALL.with(Cell::get) != Some((10, 20, 0)) {
        probe
            .redirected_positions
            .set(probe.redirected_positions.get() + 1);
    }
}
static CALLBACK_TABLE: CallbackTable = CallbackTable {
    base: IUnknown_Vtbl {
        QueryInterface: query_callback,
        AddRef: retain_callback,
        Release: release_callback,
    },
    invoke: observe_callback,
};
struct Observer {
    target: Object,
    property: Object,
    _callback: Object,
    token: i64,
}
impl Observer {
    fn attach(target: &Object, property: &Object, probe: Rc<Probe>) -> Result<Self, String> {
        let target = target.query(&IID_IDependencyObject2)?;
        let callback = unsafe {
            Object::owned(
                Box::into_raw(Box::new(Callback {
                    table: &CALLBACK_TABLE,
                    references: AtomicU32::new(1),
                    probe,
                }))
                .cast(),
            )?
        };
        let mut token = 0;
        unsafe {
            check(
                (target
                    .table::<IDependencyObject2_Vtbl>()
                    .RegisterPropertyChangedCallback)(
                    target.raw(),
                    property.raw(),
                    callback.raw(),
                    &mut token,
                ),
                "Register stop callback probe",
            )?;
        }
        Ok(Self {
            target,
            property: property.clone(),
            _callback: callback,
            token,
        })
    }
}
impl Drop for Observer {
    fn drop(&mut self) {
        unsafe {
            let _ = (self
                .target
                .table::<IDependencyObject2_Vtbl>()
                .UnregisterPropertyChangedCallback)(
                self.target.raw(),
                self.property.raw(),
                self.token,
            );
        }
    }
}
fn scenario(stop_interaction_first: bool) -> Result<(), String> {
    let receiver = Receiver::new()?;
    let popup = Popup::new()?;
    let _position_original = PositionOriginal::install()?;
    let source = crate::clock::load(
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Opacity="1"/>"#,
    )?;
    let target = crate::clock::load(
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Opacity="0.25"/>"#,
    )?;
    let property = Field::Opacity.property()?;
    let original = read(&target, &property)?.ok_or("Stop probe original missing")?;
    let owner = litetray_adapter_protocol::clock::Owner {
        nonce: 0x53544f5043414c4c,
        process_id: unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() },
        receiver: receiver.0 as u64,
    };
    let mut session = Session::new(owner, vec![], unsafe { GetTickCount64() })?;
    session.prepare_tray(vec![Pair {
        source,
        target: target.clone(),
        fields: vec![Field::Opacity],
    }])?;
    let interaction = Runtime::new(vec![]);
    session.prepare_interaction(interaction.clone())?;
    let watchdog = Watchdog::attach(session)?;
    watchdog.start_tray()?;
    if runtime().is_none() {
        return Err("停止回调夹具未启用托盘交互".into());
    }
    let probe = Rc::new(Probe {
        popup: popup.0,
        ..Default::default()
    });
    let _observer = Observer::attach(&target, &property, probe.clone())?;
    if probe.callbacks.get() != 0 {
        return Err("停止夹具混入了启动回调".into());
    }
    arm_anchor()?;
    if stop_interaction_first {
        interaction.stop()?;
    }
    watchdog.stop()?;
    if probe.callbacks.get() == 0 {
        return Err("停止回调夹具未触发真实 XAML 属性恢复".into());
    }
    if !crate::property::equal_optional(read(&target, &property)?.as_ref(), Some(&original))?
        || watchdog.state() != crate::session::State::Closed
    {
        return Err("停止回调夹具未恢复原值或未关闭".into());
    }
    if probe.active_callbacks.get() != 0 || probe.redirected_positions.get() != 0 {
        return Err(format!(
            "停止恢复的原生属性回调仍允许托盘交互：callbacks={}, active_callbacks={}, redirected_position_callbacks={}",
            probe.callbacks.get(),
            probe.active_callbacks.get(),
            probe.redirected_positions.get()
        ));
    }
    Ok(())
}
fn refresh_failure_scenario(deactivate_before_refresh: bool) -> Result<(), String> {
    let receiver = Receiver::new()?;
    let popup = Popup::new()?;
    let _position_original = PositionOriginal::install()?;
    let source = crate::clock::load(
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Opacity="1"/>"#,
    )?;
    let target = crate::clock::load(
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Opacity="0.25"/>"#,
    )?;
    let conflicting = crate::clock::load(
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Opacity="0.5"/>"#,
    )?;
    let property = Field::Opacity.property()?;
    let original = read(&target, &property)?.ok_or("Refresh rollback original missing")?;
    let owner = litetray_adapter_protocol::clock::Owner {
        nonce: 0x5245465245534846,
        process_id: unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() },
        receiver: receiver.0 as u64,
    };
    let mut session = Session::new(owner, vec![], unsafe { GetTickCount64() })?;
    session.prepare_tray(vec![
        Pair {
            source: source.clone(),
            target: target.clone(),
            fields: vec![Field::Opacity],
        },
        Pair {
            source,
            target: conflicting.clone(),
            fields: vec![Field::Opacity],
        },
    ])?;
    let interaction = Runtime::new(vec![]);
    session.prepare_interaction(interaction.clone())?;
    let watchdog = Watchdog::attach(session)?;
    watchdog.start_tray()?;
    if runtime().is_none() || watchdog.state() != crate::session::State::Active {
        return Err("刷新回滚夹具未启用托盘会话".into());
    }
    let probe = Rc::new(Probe {
        popup: popup.0,
        ..Default::default()
    });
    let _observer = Observer::attach(&target, &property, probe.clone())?;
    arm_anchor()?;
    let endpoint = watchdog
        .endpoint()
        .ok_or("Refresh rollback endpoint missing")?;
    unsafe { SendMessageW(endpoint, WM_TIMER, 1, 0) };
    if runtime().is_none()
        || watchdog.state() != crate::session::State::Active
        || watchdog.error().is_some()
        || probe.callbacks.get() != 0
        || OVERFLOW
            .lock()
            .map_err(|_| "正常刷新定位锁已损坏")?
            .is_none()
    {
        return Err("正常刷新不应停用交互、恢复属性或丢失定位锚点".into());
    }
    // A separate native writer changes one target. The unchanged target must
    // be rolled back by the real timer's ModelLease failure path.
    let conflicting_ui = conflicting.query(&IID_IUIElement)?;
    unsafe {
        check(
            (conflicting_ui.table::<IUIElement_Vtbl>().SetOpacity)(conflicting_ui.raw(), 0.75),
            "Set external refresh conflict",
        )?;
    }
    let external = read(&conflicting, &property)?.ok_or("External conflict value missing")?;
    if deactivate_before_refresh {
        interaction.deactivate();
    }
    unsafe { SendMessageW(endpoint, WM_TIMER, 1, 0) };
    if probe.callbacks.get() == 0
        || !watchdog
            .error()
            .is_some_and(|error| error.contains("托盘属性已被其他组件修改"))
        || watchdog.state() != crate::session::State::Closed
        || !crate::property::equal_optional(read(&target, &property)?.as_ref(), Some(&original))?
        || !crate::property::equal_optional(
            read(&conflicting, &property)?.as_ref(),
            Some(&external),
        )?
    {
        return Err("刷新失败夹具未触发真实恢复、保留外部值并关闭会话".into());
    }
    if probe.active_callbacks.get() != 0 || probe.redirected_positions.get() != 0 {
        return Err(format!(
            "刷新失败回滚的原生属性回调仍允许托盘交互：callbacks={}, active_callbacks={}, redirected_position_callbacks={}",
            probe.callbacks.get(),
            probe.active_callbacks.get(),
            probe.redirected_positions.get()
        ));
    }
    Ok(())
}
fn arm_anchor() -> Result<(), String> {
    *OVERFLOW.lock().map_err(|_| "回滚夹具定位锁已损坏")? = Some(PopupAnchor {
        bounds: RECT {
            left: 950,
            top: 1000,
            right: 990,
            bottom: 1040,
        },
        work: RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        },
        tick: unsafe { GetTickCount64() },
        popup: 0,
    });
    Ok(())
}
fn check_probe(probe: &Probe, name: &str) -> Result<(), String> {
    if let Some(error) = probe.error.take() {
        return Err(format!("{name}夹具回调失败：{error}"));
    }
    if probe.callbacks.get() == 0 || probe.action_calls.get() != 1 {
        return Err(format!("{name}夹具未触发外部写入及原值恢复"));
    }
    if probe.active_callbacks.get() != 0 || probe.redirected_positions.get() != 0 {
        return Err(format!(
            "{name}的原生属性回调仍允许托盘交互：callbacks={}, active_callbacks={}, redirected_position_callbacks={}",
            probe.callbacks.get(),
            probe.active_callbacks.get(),
            probe.redirected_positions.get()
        ));
    }
    Ok(())
}
fn start_failure_scenario() -> Result<(), String> {
    let receiver = Receiver::new()?;
    let popup = Popup::new()?;
    let _position_original = PositionOriginal::install()?;
    let source = crate::clock::load(
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Opacity="1"/>"#,
    )?;
    let target = crate::clock::load(
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Opacity="0.25"/>"#,
    )?;
    let conflicting = crate::clock::load(
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Opacity="0.5"/>"#,
    )?;
    let property = Field::Opacity.property()?;
    let original = read(&target, &property)?.ok_or("Start rollback original missing")?;
    let owner = litetray_adapter_protocol::clock::Owner {
        nonce: 0x5354415254464149,
        process_id: unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() },
        receiver: receiver.0 as u64,
    };
    let mut session = Session::new(owner, vec![], unsafe { GetTickCount64() })?;
    session.prepare_tray(vec![
        Pair {
            source: source.clone(),
            target: target.clone(),
            fields: vec![Field::Opacity],
        },
        Pair {
            source,
            target: conflicting.clone(),
            fields: vec![Field::Opacity],
        },
    ])?;
    let interaction = Runtime::new(vec![]);
    session.prepare_interaction(interaction)?;
    let watchdog = Watchdog::attach(session)?;
    let conflicting_ui = conflicting.query(&IID_IUIElement)?;
    let probe = Rc::new(Probe {
        popup: popup.0,
        only_value: Some(original.clone()),
        on_change: Cell::new(Some(Box::new(move || unsafe {
            check(
                (conflicting_ui.table::<IUIElement_Vtbl>().SetOpacity)(conflicting_ui.raw(), 0.75),
                "Set external start conflict",
            )
        }))),
        ..Default::default()
    });
    let _observer = Observer::attach(&target, &property, probe.clone())?;
    arm_anchor()?;
    let error = watchdog
        .start_tray()
        .err()
        .ok_or("Start conflict was accepted")?;
    let mut opacity = 0.;
    let conflicting_ui = conflicting.query(&IID_IUIElement)?;
    unsafe {
        check(
            (conflicting_ui.table::<IUIElement_Vtbl>().Opacity)(conflicting_ui.raw(), &mut opacity),
            "Read external start conflict",
        )?;
    }
    if !error.contains("XAML 属性已被其他组件修改")
        || watchdog.state() != crate::session::State::Closed
        || opacity != 0.75
        || !crate::property::equal_optional(read(&target, &property)?.as_ref(), Some(&original))?
    {
        return Err("启动失败夹具未恢复已写入值、保留外部值并关闭会话".into());
    }
    check_probe(&probe, "启动失败回滚")
}
fn rollback_frame(top: &str, font_size: u32) -> litetray_adapter_protocol::clock::Frame {
    use litetray_adapter_protocol::clock::{Frame, Style, Text};
    Frame {
        text: Text {
            top: top.into(),
            bottom: "Owned date".into(),
            tooltip: None,
        },
        style: Style {
            font_family: "Segoe UI".into(),
            font_size,
            width: 260,
            compact_two_lines: false,
            top_color: None,
            bottom_color: None,
            alignment: Default::default(),
            top_style: Default::default(),
            bottom_style: Default::default(),
        },
    }
}
fn clock_failure_scenario(deactivate_before_update: bool) -> Result<(), String> {
    let receiver = Receiver::new()?;
    let popup = Popup::new()?;
    let _position_original = PositionOriginal::install()?;
    let time = crate::clock::load(
        r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Text="Original time" FontSize="15"/>"#,
    )?;
    let date = crate::clock::load(
        r#"<TextBlock xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Text="Original date" FontSize="15"/>"#,
    )?;
    let container = crate::clock::load(
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Width="160"/>"#,
    )?;
    let statics = factory("Windows.UI.Xaml.Controls.TextBlock", &IID_ITextBlockStatics)?;
    let property = unsafe {
        crate::property::property(
            &statics,
            statics.table::<ITextBlockStatics_Vtbl>().TextProperty,
        )?
    };
    let font_property = unsafe {
        crate::property::property(
            &statics,
            statics.table::<ITextBlockStatics_Vtbl>().FontSizeProperty,
        )?
    };
    let original = read(&time, &property)?.ok_or("Clock rollback original missing")?;
    let date_original = read(&date, &property)?.ok_or("Clock date original missing")?;
    let font_original = read(&time, &font_property)?.ok_or("Clock font original missing")?;
    let owner = litetray_adapter_protocol::clock::Owner {
        nonce: 0x434c4f434b464149,
        process_id: unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() },
        receiver: receiver.0 as u64,
    };
    let mut session = Session::new(
        owner.clone(),
        vec![crate::session::Target {
            id: 1,
            clock: crate::clock::ClockLease::capture_native(&time, &date, &container)?,
        }],
        unsafe { GetTickCount64() },
    )?;
    let interaction = Runtime::new(vec![]);
    session.prepare_interaction(interaction.clone())?;
    let watchdog = Watchdog::attach(session)?;
    watchdog.start(rollback_frame("Owned initial time", 13))?;
    if runtime().is_none() || watchdog.state() != crate::session::State::Active {
        return Err("时钟回滚夹具未启用交互".into());
    }
    let date_block = date.query(&IID_ITextBlock)?;
    let probe = Rc::new(Probe {
        popup: popup.0,
        only_value: Some(font_original),
        on_change: Cell::new(Some(Box::new(move || unsafe {
            check(
                (date_block.table::<ITextBlock_Vtbl>().SetFontSize)(date_block.raw(), 19.),
                "Set external clock conflict",
            )
        }))),
        ..Default::default()
    });
    let _observer = Observer::attach(&time, &font_property, probe.clone())?;
    arm_anchor()?;
    if deactivate_before_update {
        interaction.deactivate();
    }
    let command = serde_json::to_vec(&litetray_adapter_protocol::clock::Command {
        version: litetray_adapter_protocol::VERSION,
        owner,
        sequence: 1,
        change: litetray_adapter_protocol::clock::Change::Update {
            frame: rollback_frame("Owned updated time", 14),
        },
    })
    .map_err(|error| error.to_string())?;
    let error = watchdog
        .receive(&command)
        .err()
        .ok_or("Clock conflict was accepted")?;
    let date_block = date.query(&IID_ITextBlock)?;
    let mut font_size = 0.;
    let layout = container.query(&IID_IFrameworkElement)?;
    let mut width = 0.;
    unsafe {
        check(
            (date_block.table::<ITextBlock_Vtbl>().FontSize)(date_block.raw(), &mut font_size),
            "Read external clock conflict",
        )?;
        check(
            (layout.table::<IFrameworkElement_Vtbl>().Width)(layout.raw(), &mut width),
            "Read restored clock width",
        )?;
    }
    if !error.contains("XAML 属性已被其他组件修改")
        || watchdog.state() != crate::session::State::Closed
        || font_size != 19.
        || width != 160.
        || !crate::property::equal_optional(read(&time, &property)?.as_ref(), Some(&original))?
        || !crate::property::equal_optional(read(&date, &property)?.as_ref(), Some(&date_original))?
    {
        return Err("时钟失败夹具未恢复已写入值、保留外部值并关闭会话".into());
    }
    check_probe(&probe, "时钟写入失败回滚")
}
pub(crate) fn run() -> Result<Vec<&'static str>, String> {
    // The explicit-stop control excludes startup callbacks and hook failure.
    scenario(true)?;
    scenario(false)?;
    refresh_failure_scenario(true)?;
    refresh_failure_scenario(false)?;
    start_failure_scenario()?;
    clock_failure_scenario(true)?;
    clock_failure_scenario(false)?;
    Ok(vec![
        "tray_stop_disables_runtime_before_native_property_restore_callbacks",
        "tray_stop_restore_callback_preserves_original_overflow_position_arguments",
        "tray_refresh_failure_disables_runtime_before_native_property_restore_callbacks",
        "tray_refresh_failure_restore_callback_preserves_original_overflow_position_arguments",
        "tray_successful_refresh_keeps_runtime_and_overflow_anchor_active",
        "tray_start_partial_failure_disables_runtime_before_native_restore_callbacks",
        "tray_start_partial_failure_preserves_original_position_and_external_writer",
        "clock_write_failure_disables_runtime_before_native_property_restore_callbacks",
        "clock_write_failure_preserves_original_position_and_external_writer",
    ])
}
