//! Faulting COM collections at Menus::prepare/restore, with owned command
//! references. No XAML runtime or Explorer access is required.
use super::*;
use std::{cell::Cell, cell::RefCell, mem::offset_of, mem::size_of, rc::Rc};
use windows_sys::{Win32::Foundation::*, core::GUID, core::IID_IUnknown};

#[derive(Default)]
struct Collection {
    values: RefCell<Vec<Object>>,
    append_calls: Cell<usize>,
    fail_append_from: Cell<Option<usize>>,
    append_before_error: Cell<bool>,
    fail_clear: Cell<bool>,
    clear_before_error: Cell<bool>,
}
enum Data {
    Command(Rc<Cell<usize>>),
    Collection(Rc<Collection>),
    Menu(Object),
    Flyout,
}
#[repr(C)]
struct Native {
    table: *const usize,
    _slots: Box<[usize]>,
    refs: Cell<u32>,
    data: Data,
}
impl Drop for Native {
    fn drop(&mut self) {
        if let Data::Command(dropped) = &self.data {
            dropped.set(dropped.get() + 1);
        }
    }
}
unsafe extern "system" fn unused() {}
unsafe extern "system" fn query(this: *mut c_void, iid: *const GUID, out: *mut *mut c_void) -> i32 {
    unsafe {
        if iid.is_null() || out.is_null() {
            return E_POINTER;
        }
        *out = null_mut();
        let equal = |known: GUID| {
            (*iid).data1 == known.data1
                && (*iid).data2 == known.data2
                && (*iid).data3 == known.data3
                && (*iid).data4 == known.data4
        };
        let menu = matches!((&*this.cast::<Native>()).data, Data::Menu(_));
        let flyout = menu || matches!((&*this.cast::<Native>()).data, Data::Flyout);
        if !equal(IID_IUnknown)
            && !(menu && equal(IID_IMenuFlyout))
            && !(flyout && [IID_IFlyoutBase, IID_IFlyoutBase6].into_iter().any(equal))
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
unsafe fn collection(this: *mut c_void) -> Rc<Collection> {
    match &unsafe { &*this.cast::<Native>() }.data {
        Data::Collection(value) => value.clone(),
        _ => unreachable!(),
    }
}
unsafe extern "system" fn get(this: *mut c_void, index: u32, out: *mut *mut c_void) -> i32 {
    unsafe {
        *out = null_mut();
        let state = collection(this);
        let values = state.values.borrow();
        let Some(value) = values.get(index as usize) else {
            return E_BOUNDS;
        };
        let owned = value.clone();
        *out = owned.raw();
        std::mem::forget(owned);
        S_OK
    }
}
unsafe extern "system" fn size(this: *mut c_void, out: *mut u32) -> i32 {
    unsafe {
        *out = collection(this).values.borrow().len() as u32;
    }
    S_OK
}
unsafe extern "system" fn append(this: *mut c_void, value: *mut c_void) -> i32 {
    unsafe {
        let state = collection(this);
        let calls = state.append_calls.get() + 1;
        state.append_calls.set(calls);
        let fails = state
            .fail_append_from
            .get()
            .is_some_and(|from| calls >= from);
        if !fails || state.append_before_error.get() {
            let owned = Object::query_borrowed(value, &IID_IUnknown).unwrap();
            state.values.borrow_mut().push(owned);
        }
        if fails { E_FAIL } else { S_OK }
    }
}
unsafe extern "system" fn clear(this: *mut c_void) -> i32 {
    let state = unsafe { collection(this) };
    if !state.fail_clear.get() || state.clear_before_error.get() {
        state.values.borrow_mut().clear();
    }
    if state.fail_clear.get() { E_FAIL } else { S_OK }
}
unsafe extern "system" fn menu_items(this: *mut c_void, out: *mut *mut c_void) -> i32 {
    unsafe {
        let Data::Menu(value) = &(&*this.cast::<Native>()).data else {
            return E_NOINTERFACE;
        };
        let owned = value.clone();
        *out = owned.raw();
        std::mem::forget(owned);
        S_OK
    }
}
unsafe extern "system" fn root(_: *mut c_void, out: *mut *mut c_void) -> i32 {
    unsafe {
        *out = null_mut();
    }
    S_OK
}
unsafe extern "system" fn hide(_: *mut c_void) -> i32 {
    S_OK
}
fn native(data: Data, methods: &[(usize, usize)]) -> Object {
    let mut slots = vec![unused as *const () as usize; 18].into_boxed_slice();
    slots[0] = query as *const () as usize;
    slots[1] = retain as *const () as usize;
    slots[2] = release as *const () as usize;
    for &(index, method) in methods {
        slots[index / size_of::<usize>()] = method;
    }
    let value = Box::new(Native {
        table: slots.as_ptr(),
        _slots: slots,
        refs: Cell::new(1),
        data,
    });
    unsafe { Object::owned(Box::into_raw(value).cast()).unwrap() }
}
fn vector(state: Rc<Collection>) -> Object {
    native(
        Data::Collection(state),
        &[
            (offset_of!(Vector, get), get as *const () as usize),
            (offset_of!(Vector, size), size as *const () as usize),
            (offset_of!(Vector, append), append as *const () as usize),
            (offset_of!(Vector, clear), clear as *const () as usize),
        ],
    )
}
fn menu(state: Rc<Collection>) -> Object {
    native(
        Data::Menu(vector(state)),
        &[
            (
                offset_of!(IMenuFlyout_Vtbl, Items),
                menu_items as *const () as usize,
            ),
            (
                offset_of!(IFlyoutBase6_Vtbl, XamlRoot),
                root as *const () as usize,
            ),
            (
                offset_of!(IFlyoutBase_Vtbl, Hide),
                hide as *const () as usize,
            ),
        ],
    )
}
struct Fixture {
    menus: Menus,
    home: Object,
    root: Object,
    source: Rc<Collection>,
    target: Rc<Collection>,
    dropped: Rc<Cell<usize>>,
    identities: Vec<usize>,
}
impl Fixture {
    fn new() -> Self {
        let dropped = Rc::new(Cell::new(0));
        let source = Rc::new(Collection::default());
        let target = Rc::new(Collection::default());
        for _ in 0..3 {
            source
                .values
                .borrow_mut()
                .push(native(Data::Command(dropped.clone()), &[]));
        }
        let identities = source
            .values
            .borrow()
            .iter()
            .map(|v| v.raw() as usize)
            .collect();
        let home = menu(source.clone());
        let root = menu(Rc::new(Collection::default()));
        let menus = Menus {
            proxies: vec![Proxy {
                root: root.clone(),
                menu: menu(target.clone()),
                home: None,
            }],
        };
        Self {
            menus,
            home,
            root,
            source,
            target,
            dropped,
            identities,
        }
    }
    fn returned(&self) {
        assert!(self.target.values.borrow().is_empty());
        assert_eq!(
            self.source
                .values
                .borrow()
                .iter()
                .map(|v| v.raw() as usize)
                .collect::<Vec<_>>(),
            self.identities
        );
        assert_eq!(self.dropped.get(), 0);
    }
}

#[test]
fn failed_transfer_and_failed_rollback_retain_all_native_commands_until_retry() {
    let mut fixture = Fixture::new();
    fixture.target.fail_append_from.set(Some(2));
    fixture.target.fail_clear.set(true);
    fixture.source.fail_append_from.set(Some(1));
    assert!(
        fixture
            .menus
            .prepare(&fixture.home, &fixture.root, true)
            .is_err()
    );
    assert_eq!(
        fixture.dropped.get(),
        0,
        "rollback dropped native commands absent from both collections"
    );
    assert!(
        fixture.menus.proxies[0].home.is_some(),
        "failed transfer lost its return obligation"
    );
    fixture.target.fail_append_from.set(None);
    fixture.target.fail_clear.set(false);
    fixture.source.fail_append_from.set(None);
    fixture.menus.restore().unwrap();
    fixture.returned();
    fixture.menus.retire().unwrap();
    fixture.returned();
}

#[test]
fn partial_return_reporting_failure_can_retry_without_losing_or_duplicating_commands() {
    let mut fixture = Fixture::new();
    fixture
        .menus
        .prepare(&fixture.home, &fixture.root, true)
        .unwrap();
    fixture.source.fail_append_from.set(Some(2));
    fixture.source.append_before_error.set(true);
    fixture.source.fail_clear.set(true);
    assert!(fixture.menus.restore().is_err());
    fixture.source.fail_append_from.set(None);
    fixture.source.fail_clear.set(false);
    fixture.menus.restore().unwrap();
    fixture.returned();
}

#[test]
fn failed_source_clear_after_mutation_still_retains_the_return_obligation() {
    let mut fixture = Fixture::new();
    fixture.source.fail_clear.set(true);
    fixture.source.clear_before_error.set(true);
    assert!(
        fixture
            .menus
            .prepare(&fixture.home, &fixture.root, true)
            .is_err()
    );
    assert_eq!(
        fixture.dropped.get(),
        0,
        "Clear error released the only remaining command references"
    );
    fixture.source.fail_clear.set(false);
    fixture.menus.restore().unwrap();
    fixture.returned();
}

#[test]
fn rebuilt_native_home_is_preserved_and_borrowed_commands_are_released() {
    let mut fixture = Fixture::new();
    fixture
        .menus
        .prepare(&fixture.home, &fixture.root, true)
        .unwrap();
    let replacement = native(Data::Command(Rc::new(Cell::new(0))), &[]);
    fixture.source.values.borrow_mut().push(replacement.clone());
    fixture.menus.restore().unwrap();
    assert!(fixture.target.values.borrow().is_empty());
    assert_eq!(fixture.source.values.borrow().len(), 1);
    assert!(same_object(&fixture.source.values.borrow()[0], &replacement).unwrap());
    assert_eq!(fixture.dropped.get(), 3);
}

#[test]
fn unrooted_native_ordinary_flyout_is_allowed_to_initialize_its_own_root() {
    let mut fixture = Fixture::new();
    let flyout = native(
        Data::Flyout,
        &[(
            offset_of!(IFlyoutBase6_Vtbl, XamlRoot),
            root as *const () as usize,
        )],
    );
    assert!(
        fixture
            .menus
            .prepare(&flyout, &fixture.root, false)
            .unwrap()
            .is_none()
    );
    assert!(fixture.menus.prepare(&flyout, &fixture.root, true).is_err());
}
#[test]
fn a_cached_proxy_cannot_pass_through_on_another_island() {
    let mut fixture = Fixture::new();
    let proxy = fixture.menus.proxies[0].menu.clone();
    let other = menu(Rc::new(Collection::default()));
    assert!(
        fixture.menus.prepare(&proxy, &other, true).is_err(),
        "a known proxy reached native ShowAt with a mismatched root"
    );
    assert!(
        fixture
            .menus
            .prepare(&proxy, &fixture.root, true)
            .unwrap()
            .is_none()
    );
}
#[test]
fn ambiguous_owned_command_reordering_keeps_escrow_until_return_can_be_confirmed() {
    let mut fixture = Fixture::new();
    fixture
        .menus
        .prepare(&fixture.home, &fixture.root, true)
        .unwrap();
    fixture.source.values.borrow_mut().extend([
        fixture.target.values.borrow()[2].clone(),
        fixture.target.values.borrow()[0].clone(),
    ]);
    assert!(fixture.menus.restore().is_err());
    assert_eq!(fixture.dropped.get(), 0);
    assert!(fixture.menus.proxies[0].home.is_some());
    fixture.source.values.borrow_mut().clear();
    fixture.menus.restore().unwrap();
    fixture.returned();
}
