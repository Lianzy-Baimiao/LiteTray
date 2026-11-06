//! Native clocks are also written by Shell's refresh. Observe that exact Text
//! property synchronously, retain its latest restorable value, and apply the
//! custom value before returning. Other properties retain conflict detection.
use crate::{
    clock::boxed_text,
    property::{Property, property, same_object},
    xaml::{Object, check, factory},
    xaml_abi::*,
};
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr::null_mut,
    rc::{Rc, Weak},
    sync::atomic::{AtomicU32, Ordering},
};
use windows_sys::{
    Win32::{Foundation::*, System::Threading::GetCurrentThreadId},
    core::{GUID, IID_IUnknown, IUnknown_Vtbl},
};
// Windows.UI.Xaml.DependencyPropertyChangedCallback, from system WinMetadata.
const IID_CALLBACK: GUID = GUID::from_u128(0x45883d16_27bf_4bc1_ac26_94c1601f3a49);
#[repr(C)]
struct Table {
    base: IUnknown_Vtbl,
    invoke: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void) -> i32,
}
#[repr(C)]
struct Handler {
    table: &'static Table,
    refs: AtomicU32,
    context: Weak<Context>,
}
static LIVE_HANDLERS: AtomicU32 = AtomicU32::new(0);
struct Context {
    thread: u32,
    target: Object,
    property: Object,
    snapshot: RefCell<Option<Property>>,
    desired: RefCell<Option<Object>>,
    active: Cell<bool>,
    busy: Cell<bool>,
    error: RefCell<Option<String>>,
}
struct Busy<'a>(&'a Cell<bool>);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
impl Context {
    fn changed(&self, sender: *mut c_void, dp: *mut c_void) -> Result<(), String> {
        if !self.active.get() || self.busy.replace(true) {
            return Ok(());
        }
        let _busy = Busy(&self.busy);
        let sender = unsafe { Object::query_borrowed(sender, &IID_IDependencyObject)? };
        let dp = unsafe { Object::query_borrowed(dp, &IID_IDependencyProperty)? };
        if !same_object(&sender, &self.target)? || !same_object(&dp, &self.property)? {
            return Err("时钟刷新来源不匹配".into());
        }
        let desired = self.desired.borrow().clone().ok_or("时钟显示值缺失")?;
        // Native setter has written its new value. Capture before reapplying.
        let mut latest = Property::capture(&sender, self.property.clone())?;
        let result = latest.set(&desired);
        *self.snapshot.borrow_mut() = Some(latest);
        result
    }
}
unsafe extern "system" fn query(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> i32 {
    unsafe {
        if iid.is_null() || out.is_null() {
            return E_POINTER;
        }
        *out = null_mut();
        let equal = |a: GUID, b: GUID| {
            a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
        };
        if !equal(*iid, IID_IUnknown) && !equal(*iid, IID_CALLBACK) {
            return E_NOINTERFACE;
        }
        *out = this;
        add_ref(this);
        S_OK
    }
}
unsafe extern "system" fn add_ref(this: *mut c_void) -> u32 {
    unsafe {
        (&*(this as *const Handler))
            .refs
            .fetch_add(1, Ordering::Relaxed)
            + 1
    }
}
unsafe extern "system" fn release(this: *mut c_void) -> u32 {
    unsafe {
        let n = (&*(this as *const Handler))
            .refs
            .fetch_sub(1, Ordering::AcqRel)
            - 1;
        if n == 0 {
            LIVE_HANDLERS.fetch_sub(1, Ordering::Relaxed);
            drop(Box::from_raw(this as *mut Handler));
        }
        n
    }
}
unsafe extern "system" fn invoke(this: *mut c_void, sender: *mut c_void, dp: *mut c_void) -> i32 {
    unsafe {
        let Some(context) = (&*(this as *const Handler)).context.upgrade() else {
            return S_OK;
        };
        if GetCurrentThreadId() != context.thread {
            return RPC_E_WRONG_THREAD;
        }
        if sender.is_null() || dp.is_null() {
            return E_POINTER;
        }
        // Never propagate our failure into Shell's setter or recurse between
        // competing writers. Session checks will restore owned changes.
        if let Err(error) = context.changed(sender, dp) {
            context.active.set(false);
            *context.error.borrow_mut() = Some(error);
        }
        S_OK
    }
}
static TABLE: Table = Table {
    base: IUnknown_Vtbl {
        QueryInterface: query,
        AddRef: add_ref,
        Release: release,
    },
    invoke,
};
pub(crate) struct TextLayer {
    context: Rc<Context>,
    subscriber: Object,
    callback: Object,
    token: Option<i64>,
}
impl TextLayer {
    pub(crate) fn prepare(target: &Object) -> Result<Self, String> {
        let statics = factory("Windows.UI.Xaml.Controls.TextBlock", &IID_ITextBlockStatics)?;
        let property = unsafe {
            property(
                &statics,
                statics.table::<ITextBlockStatics_Vtbl>().TextProperty,
            )?
        };
        let snapshot = Property::capture(target, property.clone())?;
        let context = Rc::new(Context {
            thread: unsafe { GetCurrentThreadId() },
            target: target.query(&IID_IDependencyObject)?,
            property: property.query(&IID_IDependencyProperty)?,
            snapshot: RefCell::new(Some(snapshot)),
            desired: RefCell::new(None),
            active: Cell::new(false),
            busy: Cell::new(false),
            error: RefCell::new(None),
        });
        let subscriber = target.query(&IID_IDependencyObject2)?;
        let callback = unsafe {
            Object::owned(
                Box::into_raw(Box::new(Handler {
                    table: &TABLE,
                    refs: AtomicU32::new(1),
                    context: Rc::downgrade(&context),
                }))
                .cast(),
            )?
        };
        LIVE_HANDLERS.fetch_add(1, Ordering::Relaxed);
        Ok(Self {
            context,
            subscriber,
            callback,
            token: None,
        })
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        if let Some(error) = self.context.error.borrow().as_ref() {
            return Err(error.clone());
        }
        if self.context.busy.replace(true) {
            return Err("时钟文本正在更新".into());
        }
        let _busy = Busy(&self.context.busy);
        let snapshot = self
            .context
            .snapshot
            .borrow_mut()
            .take()
            .ok_or("时钟文本正在更新")?;
        let valid = snapshot.owns_current();
        *self.context.snapshot.borrow_mut() = Some(snapshot);
        if !valid? {
            return Err("时钟文本已被其他组件接管".into());
        }
        Ok(())
    }
    pub(crate) fn set(&mut self, text: &str) -> Result<(), String> {
        self.validate()?;
        let value = boxed_text(text)?;
        if self.token.is_none() {
            let mut token = 0;
            unsafe {
                check(
                    (self
                        .subscriber
                        .table::<IDependencyObject2_Vtbl>()
                        .RegisterPropertyChangedCallback)(
                        self.subscriber.raw(),
                        self.context.property.raw(),
                        self.callback.raw(),
                        &mut token,
                    ),
                    "Register clock refresh",
                )?;
            }
            self.token = Some(token);
        }
        if self.context.busy.replace(true) {
            return Err("时钟文本正在更新".into());
        }
        let _busy = Busy(&self.context.busy);
        *self.context.desired.borrow_mut() = Some(value.clone());
        self.context.active.set(true);
        let mut snapshot = self
            .context
            .snapshot
            .borrow_mut()
            .take()
            .ok_or("时钟文本正在更新")?;
        let result = snapshot.set(&value);
        *self.context.snapshot.borrow_mut() = Some(snapshot);
        result
    }
    pub(crate) fn stop(&mut self) -> Result<(), String> {
        self.context.active.set(false);
        if let Some(token) = self.token {
            unsafe {
                check(
                    (self
                        .subscriber
                        .table::<IDependencyObject2_Vtbl>()
                        .UnregisterPropertyChangedCallback)(
                        self.subscriber.raw(),
                        self.context.property.raw(),
                        token,
                    ),
                    "Unregister clock refresh",
                )?;
            }
            self.token = None;
        }
        let mut snapshot = self
            .context
            .snapshot
            .borrow_mut()
            .take()
            .ok_or("时钟文本正在更新")?;
        let result = snapshot.restore();
        *self.context.snapshot.borrow_mut() = Some(snapshot);
        result?;
        self.context.desired.borrow_mut().take();
        self.context.error.borrow_mut().take();
        Ok(())
    }
    pub(crate) fn clean(&self) -> bool {
        self.token.is_none()
            && self
                .context
                .snapshot
                .borrow()
                .as_ref()
                .is_some_and(|p| !p.dirty)
    }
    pub(crate) fn handler_count() -> u32 {
        LIVE_HANDLERS.load(Ordering::Relaxed)
    }
}
impl Drop for TextLayer {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
