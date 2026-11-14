//! Production menu restoration and preparation with synchronous native callbacks.
//! The fixture owns every COM reference and never creates or hooks shell windows.
use super::*;
use crate::tray_interaction::{prepare_menus, restore_menus, with_test_menus};
use std::{
    cell::Cell,
    mem::{offset_of, size_of},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};
use windows_sys::{
    Win32::Foundation::*,
    core::{GUID, IID_IUnknown},
};

#[repr(C)]
struct Native {
    table: *const usize,
    _slots: Box<[usize]>,
    references: Cell<u32>,
    hide_result: Cell<i32>,
    collection: Option<Object>,
    callback: Box<dyn Fn()>,
}
unsafe extern "system" fn unused() {}
unsafe extern "system" fn query(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> i32 {
    unsafe {
        if out.is_null() || iid.is_null() {
            return E_POINTER;
        }
        *out = null_mut();
        let equal = |a: GUID, b: GUID| {
            a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
        };
        if ![IID_IUnknown, IID_IMenuFlyout, IID_IFlyoutBase]
            .into_iter()
            .any(|known| equal(*iid, known))
        {
            return E_NOINTERFACE;
        }
        retain(this);
        *out = this;
        S_OK
    }
}
unsafe extern "system" fn retain(this: *mut c_void) -> u32 {
    let native = unsafe { &*this.cast::<Native>() };
    let count = native.references.get() + 1;
    native.references.set(count);
    count
}
unsafe extern "system" fn release(this: *mut c_void) -> u32 {
    let native = unsafe { &*this.cast::<Native>() };
    let count = native.references.get() - 1;
    native.references.set(count);
    if count == 0 {
        unsafe { drop(Box::from_raw(this.cast::<Native>())) };
    }
    count
}
unsafe extern "system" fn hide(this: *mut c_void) -> i32 {
    unsafe { ((*this.cast::<Native>()).callback)() };
    unsafe { (&*this.cast::<Native>()).hide_result.get() }
}
unsafe extern "system" fn collection(this: *mut c_void, out: *mut *mut c_void) -> i32 {
    unsafe {
        let native = &*this.cast::<Native>();
        let Some(value) = &native.collection else {
            return E_NOINTERFACE;
        };
        retain(value.raw());
        *out = value.raw();
        S_OK
    }
}
unsafe extern "system" fn size(_: *mut c_void, out: *mut u32) -> i32 {
    unsafe { *out = 0 };
    S_OK
}
unsafe extern "system" fn clear(_: *mut c_void) -> i32 {
    S_OK
}
fn native(mut slots: Box<[usize]>, items: Option<Object>, callback: impl Fn() + 'static) -> Object {
    slots[0] = query as *const () as usize;
    slots[1] = retain as *const () as usize;
    slots[2] = release as *const () as usize;
    let native = Box::new(Native {
        table: slots.as_ptr(),
        _slots: slots,
        references: Cell::new(1),
        hide_result: Cell::new(S_OK),
        collection: items,
        callback: Box::new(callback),
    });
    unsafe { Object::owned(Box::into_raw(native).cast()).unwrap() }
}
fn empty_collection() -> Object {
    let mut slots = vec![unused as *const () as usize; size_of::<Vector>() / size_of::<usize>()]
        .into_boxed_slice();
    slots[offset_of!(Vector, size) / size_of::<usize>()] = size as *const () as usize;
    slots[offset_of!(Vector, clear) / size_of::<usize>()] = clear as *const () as usize;
    native(slots, None, || {})
}
fn menu(callback: impl Fn() + 'static) -> Object {
    let mut slots =
        vec![unused as *const () as usize; size_of::<IFlyoutBase_Vtbl>() / size_of::<usize>()]
            .into_boxed_slice();
    slots[offset_of!(IMenuFlyout_Vtbl, Items) / size_of::<usize>()] =
        collection as *const () as usize;
    slots[offset_of!(IFlyoutBase_Vtbl, Hide) / size_of::<usize>()] = hide as *const () as usize;
    native(slots, Some(empty_collection()), callback)
}
fn restoration_fixture(callback: impl Fn() + 'static) -> Menus {
    let root = menu(|| {});
    let proxy = menu(callback);
    let home = Loan::capture(&root, &proxy).unwrap();
    Menus {
        proxies: vec![Proxy {
            root: root.clone(),
            menu: proxy,
            home: Some(home),
        }],
    }
}

#[test]
fn native_hide_reentry_is_rejected_without_borrowing_the_restoring_menus() {
    let nested = menu(|| {});
    let callback_count = Rc::new(Cell::new(0));
    let observed_count = callback_count.clone();
    let callback_panicked = Rc::new(Cell::new(false));
    let observed_panic = callback_panicked.clone();
    let callback_rejected = Rc::new(Cell::new(false));
    let observed_rejection = callback_rejected.clone();
    let fixture = restoration_fixture(move || {
        observed_count.set(observed_count.get() + 1);
        // The catch is only in this fixture. Production native callbacks have
        // no unwind boundary; the process-level regression below covers that.
        match catch_unwind(AssertUnwindSafe(|| {
            prepare_menus(|| Ok((nested.clone(), nested.clone(), true)))
        })) {
            Ok(result) => observed_rejection.set(result.is_err()),
            Err(_) => observed_panic.set(true),
        }
    });
    with_test_menus(fixture, || {
        restore_menus().unwrap();
        assert_eq!(callback_count.get(), 1);
        assert!(
            !callback_panicked.get(),
            "native Hide reentered the borrowed menu state and panicked"
        );
        assert!(
            callback_rejected.get(),
            "nested show must wait for restoration"
        );
        restore_menus().unwrap();
        assert_eq!(callback_count.get(), 1, "restoration must be idempotent");
        let menu = menu(|| {});
        let resolved = Cell::new(false);
        assert!(
            prepare_menus(|| {
                resolved.set(true);
                Ok((menu.clone(), menu, true))
            })
            .is_err()
        );
        assert!(
            resolved.get(),
            "callback gate must reopen after restoration"
        );
    });
}

#[test]
fn native_hide_reentry_does_not_abort_the_callback_process() {
    use std::{
        os::windows::process::CommandExt,
        process::Stdio,
        time::{Duration, Instant},
    };
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "tray_menu::reentry_checks::native_hide_reentry_child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("LITETRAY_MENU_REENTRY_CHILD", "1")
        .env("RUST_BACKTRACE", "0")
        .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            panic!("isolated callback process exceeded its ten-second deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "native Hide/Closing reentry terminated the isolated callback process: {:?}\n{}",
        result.status,
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn native_hide_reentry_cannot_start_a_second_restoration() {
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let fixture = restoration_fixture(move || {
        observed.set(observed.get() + 1);
        assert!(restore_menus().is_err());
    });
    with_test_menus(fixture, || {
        restore_menus().unwrap();
        assert_eq!(calls.get(), 1);
    });
}

#[test]
fn preparation_resolver_reentry_is_rejected_and_errors_reopen_the_gate() {
    let nested_ran = Cell::new(false);
    assert!(
        prepare_menus(|| {
            assert!(restore_menus().is_err());
            assert!(
                prepare_menus(|| {
                    nested_ran.set(true);
                    Err("nested resolver must not run".into())
                })
                .is_err()
            );
            Err("injected root-query error".into())
        })
        .is_err()
    );
    assert!(!nested_ran.get());
    let next_ran = Cell::new(false);
    assert!(
        prepare_menus(|| {
            next_ran.set(true);
            Err("independent resolver".into())
        })
        .is_err()
    );
    assert!(next_ran.get());
    restore_menus().unwrap();
}

#[test]
fn stopping_the_menu_session_releases_its_retired_root() {
    let root = menu(|| {});
    let proxy = menu(|| {});
    let home = Loan::capture(&menu(|| {}), &proxy).unwrap();
    let fixture = Menus {
        proxies: vec![Proxy {
            root: root.clone(),
            menu: proxy,
            home: Some(home),
        }],
    };
    with_test_menus(fixture, || {
        restore_menus().unwrap();
        let references = unsafe { (&*root.raw().cast::<Native>()).references.get() };
        assert_eq!(
            references, 1,
            "stopped menu session still holds the retired XamlRoot"
        );
    });
}

#[test]
fn failed_native_hide_retains_command_home_and_root_until_retry() {
    let root = menu(|| {});
    let home = menu(|| {});
    let proxy = menu(|| {});
    unsafe { (&*proxy.raw().cast::<Native>()).hide_result.set(E_FAIL) };
    let fixture = Menus {
        proxies: vec![Proxy {
            root: root.clone(),
            menu: proxy.clone(),
            home: Some(Loan::capture(&home, &proxy).unwrap()),
        }],
    };
    with_test_menus(fixture, || {
        assert!(restore_menus().is_err());
        for object in [&root, &home] {
            assert_eq!(
                unsafe { (&*object.raw().cast::<Native>()).references.get() },
                2
            );
        }
        unsafe { (&*proxy.raw().cast::<Native>()).hide_result.set(S_OK) };
        restore_menus().unwrap();
        for object in [&root, &home] {
            assert_eq!(
                unsafe { (&*object.raw().cast::<Native>()).references.get() },
                1
            );
        }
    });
}

#[test]
#[ignore = "only invoked by the bounded isolated callback-process regression"]
fn native_hide_reentry_child() {
    if std::env::var("LITETRAY_MENU_REENTRY_CHILD").as_deref() != Ok("1") {
        return;
    }
    let nested = menu(|| {});
    let fixture = restoration_fixture(move || {
        assert!(prepare_menus(|| Ok((nested.clone(), nested.clone(), true))).is_err());
    });
    with_test_menus(fixture, || restore_menus().unwrap());
}
