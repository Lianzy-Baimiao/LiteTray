//! Exact proxy ShowAt overload/arguments/HRESULT at the detour dispatch seam.
use super::*;
use std::mem::{offset_of, size_of};
use windows_sys::core::{GUID, IID_IUnknown};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Call {
    options_overload: bool,
    target: usize,
    options: usize,
}
#[repr(C)]
struct Native {
    table: *const usize,
    _slots: Box<[usize]>,
    refs: Cell<u32>,
    record: Rc<Cell<Option<Call>>>,
    supports_options: Cell<bool>,
}
unsafe extern "system" fn unused() {}
unsafe extern "system" fn query(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> i32 {
    unsafe {
        *out = null_mut();
        let supported = [IID_IUnknown, IID_IFlyoutBase, IID_IFlyoutBase5]
            .into_iter()
            .filter(|known| {
                known.data1 != IID_IFlyoutBase5.data1
                    || (&*this.cast::<Native>()).supports_options.get()
            })
            .any(|known| {
                (*iid).data1 == known.data1
                    && (*iid).data2 == known.data2
                    && (*iid).data3 == known.data3
                    && (*iid).data4 == known.data4
            });
        if !supported {
            return E_NOINTERFACE;
        }
        retain(this);
        *out = this;
        S_OK
    }
}
unsafe extern "system" fn retain(this: *mut c_void) -> u32 {
    let native = unsafe { &*this.cast::<Native>() };
    native.refs.set(native.refs.get() + 1);
    native.refs.get()
}
unsafe extern "system" fn release(this: *mut c_void) -> u32 {
    let native = unsafe { &*this.cast::<Native>() };
    let refs = native.refs.get() - 1;
    native.refs.set(refs);
    if refs == 0 {
        unsafe { drop(Box::from_raw(this.cast::<Native>())) };
    }
    refs
}
unsafe extern "system" fn regular(this: *mut c_void, target: *mut c_void) -> i32 {
    unsafe {
        (&*this.cast::<Native>()).record.set(Some(Call {
            options_overload: false,
            target: target as usize,
            options: 0,
        }));
    }
    S_FALSE
}
unsafe extern "system" fn options(
    this: *mut c_void,
    target: *mut c_void,
    options: *mut c_void,
) -> i32 {
    unsafe {
        (&*this.cast::<Native>()).record.set(Some(Call {
            options_overload: true,
            target: target as usize,
            options: options as usize,
        }));
    }
    E_ACCESSDENIED
}
fn proxy(record: Rc<Cell<Option<Call>>>) -> Object {
    let mut slots =
        vec![unused as *const () as usize; size_of::<IFlyoutBase_Vtbl>() / size_of::<usize>()]
            .into_boxed_slice();
    slots[0] = query as *const () as usize;
    slots[1] = retain as *const () as usize;
    slots[2] = release as *const () as usize;
    slots[offset_of!(IFlyoutBase_Vtbl, ShowAt) / size_of::<usize>()] =
        regular as *const () as usize;
    slots[offset_of!(IFlyoutBase5_Vtbl, ShowAt) / size_of::<usize>()] =
        options as *const () as usize;
    let native = Box::new(Native {
        table: slots.as_ptr(),
        _slots: slots,
        refs: Cell::new(1),
        record,
        supports_options: Cell::new(true),
    });
    unsafe { Object::owned(Box::into_raw(native).cast()).unwrap() }
}
pub(crate) fn run() -> Result<Vec<&'static str>, String> {
    let record = Rc::new(Cell::new(None));
    let proxy = proxy(record.clone());
    let mut target_marker = 1u8;
    let mut options_marker = 2u8;
    let target = (&mut target_marker as *mut u8).cast();
    for options in [(&mut options_marker as *mut u8).cast(), null_mut()] {
        let result = unsafe { show_proxy(&proxy, target, Some(options)) };
        if result != E_ACCESSDENIED
            || record.get()
                != Some(Call {
                    options_overload: true,
                    target: target as usize,
                    options: options as usize,
                })
        {
            return Err("代理菜单 options 重载、参数或 HRESULT 未保留".into());
        }
    }
    if unsafe { show_proxy(&proxy, target, None) } != S_FALSE
        || record.get()
            != Some(Call {
                options_overload: false,
                target: target as usize,
                options: 0,
            })
    {
        return Err("代理菜单基本重载、参数或 HRESULT 未保留".into());
    }
    record.set(None);
    unsafe {
        (&*proxy.raw().cast::<Native>()).supports_options.set(false);
    }
    if unsafe { show_proxy(&proxy, target, Some(null_mut())) } != S_OK || record.get().is_some() {
        return Err("已拒绝的代理显示没有保持原有无操作 HRESULT".into());
    }
    Ok(vec![
        "native_proxy_forwards_show_options_and_exact_abi_target",
        "native_proxy_forwards_basic_overload_and_original_hresult",
        "rejected_proxy_show_keeps_the_nonthrowing_noop_hresult",
    ])
}
#[test]
fn native_proxy_preserves_options_overload_exact_target_options_and_hresult() {
    let record = Rc::new(Cell::new(None));
    let proxy = proxy(record.clone());
    let mut target_marker = 1u8;
    let mut options_marker = 2u8;
    let target = (&mut target_marker as *mut u8).cast();
    for options in [(&mut options_marker as *mut u8).cast(), null_mut()] {
        assert_eq!(
            unsafe { show_proxy(&proxy, target, Some(options)) },
            E_ACCESSDENIED
        );
        assert_eq!(
            record.get(),
            Some(Call {
                options_overload: true,
                target: target as usize,
                options: options as usize
            })
        );
    }
}
#[test]
fn native_proxy_preserves_basic_overload_exact_target_and_hresult() {
    let record = Rc::new(Cell::new(None));
    let proxy = proxy(record.clone());
    let mut marker = 1u8;
    let target = (&mut marker as *mut u8).cast();
    assert_eq!(unsafe { show_proxy(&proxy, target, None) }, S_FALSE);
    assert_eq!(
        record.get(),
        Some(Call {
            options_overload: false,
            target: target as usize,
            options: 0
        })
    );
}
#[test]
fn rejected_proxy_interface_suppresses_show_without_adding_a_shell_hresult_exception() {
    let record = Rc::new(Cell::new(None));
    let proxy = proxy(record.clone());
    unsafe {
        (&*proxy.raw().cast::<Native>()).supports_options.set(false);
    }
    assert_eq!(
        unsafe { show_proxy(&proxy, null_mut(), Some(null_mut())) },
        S_OK
    );
    assert_eq!(record.get(), None);
}
