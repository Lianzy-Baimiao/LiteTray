//! AddHandler takes a boxed WinRT delegate (IReference<TappedEventHandler>),
//! rather than the raw delegate accepted by UIElement.Tapped.
use crate::xaml::Object;
use std::{
    ffi::c_void,
    ptr::null_mut,
    sync::atomic::{AtomicU32, Ordering},
};
use windows_sys::core::{
    GUID, HSTRING, IID_IInspectable, IID_IUnknown, IInspectable_Vtbl, IUnknown_Vtbl,
};
windows_link::link!("ole32.dll" "system" fn CoTaskMemAlloc(size: usize) -> *mut c_void);
const IID_REFERENCE: GUID = GUID::from_u128(windows_core::GUID::from_signature(
    windows_core::imp::ConstBuffer::from_slice(b"pinterface({61c17706-2d65-11e0-9ae8-d48564015472};delegate({68d940cc-9ff0-49ce-b141-3f07ec477b97}))")
).to_u128());
#[repr(C)]
struct Table {
    base: IInspectable_Vtbl,
    value: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32,
}
#[repr(C)]
struct Boxed {
    table: &'static Table,
    refs: AtomicU32,
    handler: Object,
}
unsafe extern "system" fn query(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> i32 {
    unsafe {
        if out.is_null() || iid.is_null() {
            return 0x80004003u32 as i32;
        }
        *out = null_mut();
        if ![IID_REFERENCE, IID_IInspectable, IID_IUnknown]
            .iter()
            .any(|v| {
                v.data1 == (*iid).data1
                    && v.data2 == (*iid).data2
                    && v.data3 == (*iid).data3
                    && v.data4 == (*iid).data4
            })
        {
            return 0x80004002u32 as i32;
        }
        *out = this;
        add_ref(this);
        0
    }
}
unsafe extern "system" fn add_ref(this: *mut c_void) -> u32 {
    unsafe {
        (*(this as *const Boxed))
            .refs
            .fetch_add(1, Ordering::Relaxed)
            + 1
    }
}
unsafe extern "system" fn release(this: *mut c_void) -> u32 {
    unsafe {
        let count = (*(this as *const Boxed))
            .refs
            .fetch_sub(1, Ordering::AcqRel)
            - 1;
        if count == 0 {
            drop(Box::from_raw(this as *mut Boxed));
        }
        count
    }
}
unsafe extern "system" fn iids(_: *mut c_void, count: *mut u32, values: *mut *mut GUID) -> i32 {
    unsafe {
        if count.is_null() || values.is_null() {
            return 0x80004003u32 as i32;
        }
        *count = 0;
        *values = null_mut();
        let memory = CoTaskMemAlloc(std::mem::size_of::<GUID>()).cast::<GUID>();
        if memory.is_null() {
            return 0x8007000eu32 as i32;
        }
        *memory = IID_REFERENCE;
        *values = memory;
        *count = 1;
        0
    }
}
unsafe extern "system" fn name(_: *mut c_void, value: *mut HSTRING) -> i32 {
    unsafe {
        if value.is_null() {
            return 0x80004003u32 as i32;
        }
        *value = null_mut();
        0
    }
}
unsafe extern "system" fn trust(_: *mut c_void, value: *mut i32) -> i32 {
    unsafe {
        if value.is_null() {
            return 0x80004003u32 as i32;
        }
        *value = 0;
        0
    }
}
unsafe extern "system" fn value(this: *mut c_void, out: *mut *mut c_void) -> i32 {
    unsafe {
        if out.is_null() {
            return 0x80004003u32 as i32;
        }
        let handler = &(*(this as *const Boxed)).handler;
        (handler.table::<IUnknown_Vtbl>().AddRef)(handler.raw());
        *out = handler.raw();
        0
    }
}
static TABLE: Table = Table {
    base: IInspectable_Vtbl {
        base: IUnknown_Vtbl {
            QueryInterface: query,
            AddRef: add_ref,
            Release: release,
        },
        GetIids: iids,
        GetRuntimeClassName: name,
        GetTrustLevel: trust,
    },
    value,
};
pub(crate) fn boxed(handler: Object) -> Result<Object, String> {
    unsafe {
        Object::owned(
            Box::into_raw(Box::new(Boxed {
                table: &TABLE,
                refs: AtomicU32::new(1),
                handler,
            }))
            .cast(),
        )
    }
}
