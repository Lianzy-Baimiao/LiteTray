//! Exercise the selected native detours without installing hooks or creating
//! XAML. The COM fixture records which references survive the original call.
use super::*;
use std::sync::{Mutex, MutexGuard};
use windows_sys::core::{GUID, IUnknown_Vtbl};

#[derive(Default)]
struct References {
    count: Cell<u32>,
    events: RefCell<Vec<&'static str>>,
    destroyed: Cell<bool>,
}
#[repr(C)]
struct Element {
    table: *const IUnknown_Vtbl,
    references: Rc<References>,
    query_supported: bool,
}
fn element(query_supported: bool) -> (*mut c_void, Rc<References>) {
    let references = Rc::new(References {
        count: Cell::new(1),
        ..Default::default()
    });
    let raw = Box::into_raw(Box::new(Element {
        table: &ELEMENT_TABLE,
        references: references.clone(),
        query_supported,
    }));
    (raw.cast(), references)
}
unsafe extern "system" fn query_element(
    this: *mut c_void,
    iid: *const GUID,
    out: *mut *mut c_void,
) -> i32 {
    unsafe {
        if iid.is_null() || out.is_null() {
            return E_POINTER;
        }
        *out = null_mut();
        let element = &*this.cast::<Element>();
        element.references.events.borrow_mut().push("query");
        let iid = *iid;
        let expected = IID_IFrameworkElement;
        if !element.query_supported
            || iid.data1 != expected.data1
            || iid.data2 != expected.data2
            || iid.data3 != expected.data3
            || iid.data4 != expected.data4
        {
            return E_NOINTERFACE;
        }
        retain_element(this);
        *out = this;
        S_OK
    }
}
unsafe extern "system" fn retain_element(this: *mut c_void) -> u32 {
    unsafe {
        let references = &(*this.cast::<Element>()).references;
        references.events.borrow_mut().push("retain");
        let count = references.count.get() + 1;
        references.count.set(count);
        count
    }
}
unsafe extern "system" fn release_element(this: *mut c_void) -> u32 {
    unsafe {
        let references = &(*this.cast::<Element>()).references;
        references.events.borrow_mut().push("release");
        let count = references.count.get() - 1;
        references.count.set(count);
        if count == 0 {
            references.destroyed.set(true);
            drop(Box::from_raw(this.cast::<Element>()));
        }
        count
    }
}
const ELEMENT_TABLE: IUnknown_Vtbl = IUnknown_Vtbl {
    QueryInterface: query_element,
    AddRef: retain_element,
    Release: release_element,
};

#[derive(Debug, PartialEq, Eq)]
struct Call {
    opaque_this: usize,
    result: usize,
    wrapper: usize,
    element: usize,
    hwnd: usize,
    references: Option<u32>,
}
#[derive(Clone, Copy, Default)]
enum ReturnTarget {
    #[default]
    Argument,
    Other(*mut RECT),
}
#[derive(Default)]
struct Native {
    calls: Vec<Call>,
    returned: ReturnTarget,
}
thread_local! {
    static NATIVE: RefCell<Native> = RefCell::new(Native::default());
}
const NATIVE_RECT: RECT = RECT {
    left: -1200,
    top: 800,
    right: -1160,
    bottom: 840,
};
unsafe fn native_rect(
    opaque_this: *mut c_void,
    result: *mut RECT,
    wrapper: *const *mut c_void,
    hwnd: HWND,
    consume: bool,
) -> *mut RECT {
    unsafe {
        let raw = wrapper.as_ref().copied().unwrap_or(null_mut());
        let references = if raw.is_null() {
            None
        } else {
            let references = &(*raw.cast::<Element>()).references;
            references.events.borrow_mut().push("native");
            Some(references.count.get())
        };
        let returned = NATIVE.with(|state| {
            let mut state = state.borrow_mut();
            state.calls.push(Call {
                opaque_this: opaque_this as usize,
                result: result as usize,
                wrapper: wrapper as usize,
                element: raw as usize,
                hwnd: hwnd as usize,
                references,
            });
            match state.returned {
                ReturnTarget::Argument => result,
                ReturnTarget::Other(pointer) => pointer,
            }
        });
        if consume && !raw.is_null() {
            *wrapper.cast_mut() = null_mut();
            release_element(raw);
        }
        if let Some(rect) = returned.as_mut() {
            *rect = NATIVE_RECT;
        }
        returned
    }
}
unsafe extern "system" fn owned_original(
    result: *mut RECT,
    wrapper: *const *mut c_void,
    hwnd: HWND,
) -> *mut RECT {
    unsafe { native_rect(null_mut(), result, wrapper, hwnd, true) }
}
unsafe extern "system" fn borrowed_original(
    opaque_this: *mut c_void,
    result: *mut RECT,
    wrapper: *const *mut c_void,
    hwnd: HWND,
) -> *mut RECT {
    unsafe { native_rect(opaque_this, result, wrapper, hwnd, false) }
}

// Only these tests replace RECT_ORIGINAL. Keep the guard until the detour and
// all ownership assertions finish, including when a test assertion unwinds.
static ORIGINAL_LOCK: Mutex<()> = Mutex::new(());
struct OriginalGuard {
    slot: &'static AtomicUsize,
    previous: usize,
    _lock: MutexGuard<'static, ()>,
}
impl OriginalGuard {
    fn new(entry: &Entry) -> Self {
        let lock = ORIGINAL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        assert!(runtime().is_none());
        NATIVE.with(|state| *state.borrow_mut() = Native::default());
        Self {
            slot: entry.original,
            previous: entry.original.swap(entry.target, Ordering::AcqRel),
            _lock: lock,
        }
    }
}
impl Drop for OriginalGuard {
    fn drop(&mut self) {
        self.slot.store(self.previous, Ordering::Release);
        NATIVE.with(|state| *state.borrow_mut() = Native::default());
    }
}
unsafe fn call_borrowed(
    entry: &Entry,
    opaque_this: *mut c_void,
    result: *mut RECT,
    wrapper: *const *mut c_void,
    hwnd: HWND,
) -> *mut RECT {
    assert_eq!(entry.arguments, 4);
    unsafe {
        let detour: unsafe extern "system" fn(
            *mut c_void,
            *mut RECT,
            *const *mut c_void,
            HWND,
        ) -> *mut RECT = std::mem::transmute(entry.detour);
        detour(opaque_this, result, wrapper, hwnd)
    }
}
fn coordinates(rect: RECT) -> (i32, i32, i32, i32) {
    (rect.left, rect.top, rect.right, rect.bottom)
}
fn assert_call(expected: Call) {
    NATIVE.with(|state| assert_eq!(state.borrow().calls.as_slice(), &[expected]));
}

#[test]
fn borrowed_entry_forwards_all_four_arguments_and_preserves_callers_reference() {
    let entry = screen_rect_entry(
        TrayAbi::SystemTrayDragDropBorrowedCoordinatesX64V1,
        borrowed_original as *const () as usize,
    );
    let _original = OriginalGuard::new(&entry);
    let (mut wrapper, references) = element(true);
    let raw = wrapper;
    let wrapper_pointer = &mut wrapper as *mut *mut c_void;
    let mut result = RECT::default();
    // An intentionally non-dereferenceable member pointer matches callers
    // whose RCX is unspecified; the detour must not inspect or reject it.
    let opaque_this = std::ptr::dangling_mut::<c_void>();
    let hwnd = 0x6543usize as HWND;
    let returned =
        unsafe { call_borrowed(&entry, opaque_this, &mut result, wrapper_pointer, hwnd) };
    assert_eq!(returned, &mut result as *mut RECT);
    assert_eq!(coordinates(result), coordinates(NATIVE_RECT));
    assert_eq!(wrapper, raw);
    assert_call(Call {
        opaque_this: opaque_this as usize,
        result: &mut result as *mut RECT as usize,
        wrapper: wrapper_pointer as usize,
        element: raw as usize,
        hwnd: hwnd as usize,
        references: Some(2),
    });
    assert_eq!(references.count.get(), 1);
    assert!(!references.destroyed.get());
    assert_eq!(
        references.events.borrow().as_slice(),
        &["query", "retain", "native", "release"]
    );
    unsafe { release_element(wrapper) };
    assert_eq!(references.count.get(), 0);
    assert!(references.destroyed.get());
}

#[test]
fn owned_entry_retains_before_original_consumes_and_clears_the_temporary() {
    let entry = screen_rect_entry(
        TrayAbi::SystemTrayCoordinatesX64V1,
        owned_original as *const () as usize,
    );
    let _original = OriginalGuard::new(&entry);
    assert_eq!(entry.arguments, 3);
    let (mut wrapper, references) = element(true);
    let raw = wrapper;
    let wrapper_pointer = &mut wrapper as *mut *mut c_void;
    let mut result = RECT::default();
    let hwnd = 0x3456usize as HWND;
    let returned = unsafe {
        let detour: unsafe extern "system" fn(*mut RECT, *const *mut c_void, HWND) -> *mut RECT =
            std::mem::transmute(entry.detour);
        detour(&mut result, wrapper_pointer, hwnd)
    };
    assert_eq!(returned, &mut result as *mut RECT);
    assert_eq!(coordinates(result), coordinates(NATIVE_RECT));
    assert!(wrapper.is_null());
    assert_call(Call {
        opaque_this: 0,
        result: &mut result as *mut RECT as usize,
        wrapper: wrapper_pointer as usize,
        element: raw as usize,
        hwnd: hwnd as usize,
        references: Some(2),
    });
    assert_eq!(references.count.get(), 0);
    assert!(references.destroyed.get());
    assert_eq!(
        references.events.borrow().as_slice(),
        &["query", "retain", "native", "release", "release"]
    );
}

#[test]
fn borrowed_entry_passes_through_when_framework_element_query_fails() {
    let entry = screen_rect_entry(
        TrayAbi::SystemTrayDragDropBorrowedCoordinatesX64V1,
        borrowed_original as *const () as usize,
    );
    let _original = OriginalGuard::new(&entry);
    let (mut wrapper, references) = element(false);
    let raw = wrapper;
    let wrapper_pointer = &mut wrapper as *mut *mut c_void;
    let mut result = RECT::default();
    let returned =
        unsafe { call_borrowed(&entry, null_mut(), &mut result, wrapper_pointer, null_mut()) };
    assert_eq!(returned, &mut result as *mut RECT);
    assert_eq!(coordinates(result), coordinates(NATIVE_RECT));
    assert_eq!(wrapper, raw);
    assert_call(Call {
        opaque_this: 0,
        result: &mut result as *mut RECT as usize,
        wrapper: wrapper_pointer as usize,
        element: raw as usize,
        hwnd: 0,
        references: Some(1),
    });
    assert_eq!(references.count.get(), 1);
    assert!(!references.destroyed.get());
    assert_eq!(references.events.borrow().as_slice(), &["query", "native"]);
    unsafe { release_element(wrapper) };
    assert!(references.destroyed.get());
}

#[test]
fn borrowed_entry_preserves_original_return_pointer_and_balances_retention() {
    let entry = screen_rect_entry(
        TrayAbi::SystemTrayDragDropBorrowedCoordinatesX64V1,
        borrowed_original as *const () as usize,
    );
    let _original = OriginalGuard::new(&entry);
    let mut other = RECT::default();
    for returned in [&mut other as *mut RECT, null_mut()] {
        NATIVE.with(|state| {
            *state.borrow_mut() = Native {
                returned: ReturnTarget::Other(returned),
                ..Default::default()
            };
        });
        let (wrapper, references) = element(true);
        let raw = wrapper;
        let mut result = RECT::default();
        let actual =
            unsafe { call_borrowed(&entry, null_mut(), &mut result, &wrapper, null_mut()) };
        assert_eq!(actual, returned);
        assert_eq!(wrapper, raw);
        assert_eq!(coordinates(result), (0, 0, 0, 0));
        if !returned.is_null() {
            assert_eq!(coordinates(other), coordinates(NATIVE_RECT));
        }
        assert_eq!(references.count.get(), 1);
        assert!(!references.destroyed.get());
        assert_eq!(
            references.events.borrow().as_slice(),
            &["query", "retain", "native", "release"]
        );
        unsafe { release_element(wrapper) };
        assert!(references.destroyed.get());
    }
}

#[test]
fn borrowed_entry_forwards_null_wrappers_without_querying_them() {
    let entry = screen_rect_entry(
        TrayAbi::SystemTrayDragDropBorrowedCoordinatesX64V1,
        borrowed_original as *const () as usize,
    );
    let _original = OriginalGuard::new(&entry);
    let empty = null_mut();
    for wrapper in [&empty as *const *mut c_void, std::ptr::null()] {
        NATIVE.with(|state| *state.borrow_mut() = Native::default());
        let mut result = RECT::default();
        let returned =
            unsafe { call_borrowed(&entry, null_mut(), &mut result, wrapper, null_mut()) };
        assert_eq!(returned, &mut result as *mut RECT);
        assert_eq!(coordinates(result), coordinates(NATIVE_RECT));
        assert_call(Call {
            opaque_this: 0,
            result: &mut result as *mut RECT as usize,
            wrapper: wrapper as usize,
            element: 0,
            hwnd: 0,
            references: None,
        });
    }
}
