//! Real dependency-property restoration synchronously enters a native callback.
//! The check runs only in the private self-test process, without shell hooks.
use super::*;
use crate::{lifecycle_checks::Receiver, session::Session, tray::Pair, watchdog::Watchdog};
use std::sync::atomic::AtomicU32;
use windows_sys::core::{GUID, IID_IUnknown, IUnknown_Vtbl};

const IID_CALLBACK: GUID = GUID::from_u128(0x45883d16_27bf_4bc1_ac26_94c1601f3a49);
#[derive(Default)]
struct Probe {
    popup: HWND,
    callbacks: Cell<u32>,
    active_callbacks: Cell<u32>,
    redirected_positions: Cell<u32>,
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
    _: *mut c_void,
    _: *mut c_void,
) -> i32 {
    let probe = unsafe { &(&*this.cast::<Callback>()).probe };
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
    S_OK
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
    *OVERFLOW.lock().map_err(|_| "停止夹具定位锁已损坏")? = Some(PopupAnchor {
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
pub(crate) fn run() -> Result<Vec<&'static str>, String> {
    // The explicit-stop control excludes startup callbacks and hook failure.
    scenario(true)?;
    scenario(false)?;
    Ok(vec![
        "tray_stop_disables_runtime_before_native_property_restore_callbacks",
        "tray_stop_restore_callback_preserves_original_overflow_position_arguments",
    ])
}
