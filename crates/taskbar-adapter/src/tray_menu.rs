//! Cached shell menus cannot change XamlRoot after first use. Keep native
//! command items, temporarily lending them to one proxy rooted on each island.
use crate::{
    clock::load,
    property::same_object,
    xaml::{Object, check},
    xaml_abi::*,
};
use std::{ffi::c_void, ptr::null_mut};
use windows_sys::core::IInspectable_Vtbl;

#[repr(C)]
struct Vector {
    base: IInspectable_Vtbl,
    get: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> i32,
    size: unsafe extern "system" fn(*mut c_void, *mut u32) -> i32,
    view: usize,
    index: usize,
    set: usize,
    insert: usize,
    remove: usize,
    append: unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32,
    remove_last: usize,
    clear: unsafe extern "system" fn(*mut c_void) -> i32,
}
fn items(menu: &Object) -> Result<Object, String> {
    let menu = menu.query(&IID_IMenuFlyout)?;
    unsafe {
        let mut value = null_mut();
        check(
            (menu.table::<IMenuFlyout_Vtbl>().Items)(menu.raw(), &mut value),
            "MenuFlyout.Items",
        )?;
        Object::owned(value)
    }
}
fn count(items: &Object) -> Result<u32, String> {
    unsafe {
        let mut size = 0;
        check(
            (items.table::<Vector>().size)(items.raw(), &mut size),
            "Menu items.Size",
        )?;
        if size > 256 {
            return Err("系统菜单项数量超限".into());
        }
        Ok(size)
    }
}
fn snapshot(source: &Object) -> Result<Vec<Object>, String> {
    unsafe {
        let mut values = Vec::new();
        for index in 0..count(source)? {
            let mut value = null_mut();
            check(
                (source.table::<Vector>().get)(source.raw(), index, &mut value),
                "Menu items.GetAt",
            )?;
            values.push(Object::owned(value)?);
        }
        Ok(values)
    }
}
fn clear(collection: &Object) -> Result<(), String> {
    unsafe {
        check(
            (collection.table::<Vector>().clear)(collection.raw()),
            "Menu items.Clear",
        )?;
    }
    if count(collection)? != 0 {
        return Err("菜单 Clear 后仍有项目".into());
    }
    Ok(())
}
fn append(collection: &Object, value: &Object) -> Result<(), String> {
    unsafe {
        check(
            (collection.table::<Vector>().append)(collection.raw(), value.raw()),
            "Menu items.Append",
        )
    }
}
/// Own the complete command snapshot before the first mutation. Failed COM
/// calls can mutate collections; an HRESULT never releases this obligation.
struct Loan {
    home: Object,
    source: Object,
    borrowed: Object,
    values: Vec<Object>,
}
impl Loan {
    fn capture(home: &Object, proxy: &Object) -> Result<Self, String> {
        let source = items(home)?;
        let borrowed = items(proxy)?;
        if count(&borrowed)? != 0 {
            return Err("菜单目标仍持有项目".into());
        }
        let values = snapshot(&source)?;
        Ok(Self {
            home: home.clone(),
            source,
            borrowed,
            values,
        })
    }
    fn lend(&self) -> Result<(), String> {
        clear(&self.source)?;
        for value in &self.values {
            append(&self.borrowed, value)?;
        }
        let borrowed = snapshot(&self.borrowed)?;
        if borrowed.len() != self.values.len() {
            return Err("菜单借用数量未确认".into());
        }
        for (actual, expected) in borrowed.iter().zip(&self.values) {
            if !same_object(actual, expected)? {
                return Err("菜单借用顺序未确认".into());
            }
        }
        Ok(())
    }
    fn give_back(&self) -> Result<(), String> {
        // Detach every UIElement from the proxy before assigning a parent at
        // home. Even a failed Clear that mutated must be retried/confirmed.
        clear(&self.borrowed)?;
        let current = items(&self.home)?;
        if !same_object(&current, &self.source)? {
            // The shell replaced the collection. Its new collection wins.
            return Ok(());
        }
        let present = snapshot(&current)?;
        let mut prefix = present.len() <= self.values.len();
        for (actual, expected) in present.iter().zip(&self.values) {
            prefix &= same_object(actual, expected)?;
        }
        if !prefix {
            // A new native command proves a rebuilt menu; preserve it. An
            // ambiguous reordering of our own commands cannot prove rebuild.
            // Normalize each COM identity once; a 256-item failure must not
            // perform a quadratic number of QueryInterface calls in Explorer.
            let mut retained = std::collections::BTreeSet::new();
            for expected in &self.values {
                retained.insert(expected.query(&windows_sys::core::IID_IUnknown)?.raw() as usize);
            }
            for actual in &present {
                if !retained
                    .contains(&(actual.query(&windows_sys::core::IID_IUnknown)?.raw() as usize))
                {
                    return Ok(());
                }
            }
            return Err("原生菜单中的借用命令顺序不明确，保留归还责任".into());
        }
        // Append can insert and then return failure. Read the actual prefix
        // on every retry rather than clearing it or appending duplicates.
        for value in &self.values[present.len()..] {
            append(&current, value)?;
        }
        let returned = snapshot(&current)?;
        if returned.len() != self.values.len() {
            return Err("菜单归还数量未确认".into());
        }
        for (actual, expected) in returned.iter().zip(&self.values) {
            if !same_object(actual, expected)? {
                return Err("菜单归还顺序未确认".into());
            }
        }
        Ok(())
    }
}
struct Proxy {
    root: Object,
    menu: Object,
    home: Option<Loan>,
}
impl Proxy {
    fn restore(&mut self) -> Result<(), String> {
        if let Some(loan) = &self.home {
            let flyout = self.menu.query(&IID_IFlyoutBase)?;
            unsafe {
                check(
                    (flyout.table::<IFlyoutBase_Vtbl>().Hide)(flyout.raw()),
                    "Hide proxy",
                )?;
            }
            loan.give_back()?;
            self.home = None;
        }
        Ok(())
    }
}
#[derive(Default)]
pub(crate) struct Menus {
    proxies: Vec<Proxy>,
}
impl Menus {
    /// Stop owns the complete proxy cache. After command restoration succeeds,
    /// release roots from discarded islands instead of retaining their trees
    /// for the lifetime of the pinned adapter module.
    pub(crate) fn retire(&mut self) -> Result<(), String> {
        self.restore()?;
        self.proxies.clear();
        Ok(())
    }
    pub fn restore(&mut self) -> Result<(), String> {
        for proxy in &mut self.proxies {
            proxy.restore()?;
        }
        Ok(())
    }
    /// None means the native show is safe. Some(proxy) suppresses the original
    /// and shows the proxy after releasing all RefCell borrows.
    pub fn prepare(
        &mut self,
        menu: &Object,
        root: &Object,
        copied_root: bool,
    ) -> Result<Option<Object>, String> {
        for proxy in &self.proxies {
            if same_object(menu, &proxy.menu)? {
                if same_object(root, &proxy.root)? {
                    return Ok(None);
                }
                return Err("菜单代理不属于请求的 XAML 根".into());
            }
        }
        self.restore()?;
        let base = menu.query(&IID_IFlyoutBase6)?;
        unsafe {
            let mut existing = null_mut();
            check(
                (base.table::<IFlyoutBase6_Vtbl>().XamlRoot)(base.raw(), &mut existing),
                "Flyout.XamlRoot",
            )?;
            if existing.is_null() && !copied_root {
                // The native primary (or an unmanaged island) initializes its
                // own cache through ShowAt. Never SetXamlRoot on that cache.
                return Ok(None);
            }
            if !existing.is_null() && same_object(&Object::owned(existing)?, root)? {
                return Ok(None);
            }
        }
        // A fresh cached shell menu can accept SetXamlRoot, but that would
        // permanently attach it to the first copied island. Preserve its
        // native root even before first show so stopping can safely pass the
        // original call through on the primary taskbar.
        if menu.try_query(&IID_IMenuFlyout)?.is_none() {
            return Err("不同 XAML 根的普通 Flyout 尚不支持内容代理".into());
        }
        let mut matching = None;
        for (index, proxy) in self.proxies.iter().enumerate() {
            if same_object(&proxy.root, root)? {
                matching = Some(index);
                break;
            }
        }
        let index = match matching {
            Some(index) => index,
            None => {
                if self.proxies.len() >= 32 {
                    return Err("菜单代理根数量超限，需先恢复会话".into());
                }
                let proxy = load(
                    r#"<MenuFlyout xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"/>"#,
                )?;
                let base = proxy.query(&IID_IFlyoutBase6)?;
                unsafe {
                    check(
                        (base.table::<IFlyoutBase6_Vtbl>().SetXamlRoot)(base.raw(), root.raw()),
                        "Proxy.XamlRoot",
                    )?;
                }
                self.proxies.push(Proxy {
                    root: root.clone(),
                    menu: proxy,
                    home: None,
                });
                self.proxies.len() - 1
            }
        };
        let proxy = &mut self.proxies[index];
        proxy.home = Some(Loan::capture(menu, &proxy.menu)?);
        if let Err(error) = proxy.home.as_ref().expect("captured loan").lend() {
            return match proxy.restore() {
                Ok(()) => Err(error),
                Err(restore) => Err(format!("{error}；归还尚未确认：{restore}")),
            };
        }
        Ok(Some(proxy.menu.clone()))
    }
}
#[cfg(test)]
#[path = "tray_menu_reentry_checks.rs"]
mod reentry_checks;
#[path = "tray_menu_root_checks.rs"]
mod root_checks;
#[cfg(test)]
#[path = "tray_menu_transfer_checks.rs"]
mod transfer_checks;
pub(crate) fn self_test() -> Result<Vec<&'static str>, String> {
    let source = load(
        r#"<MenuFlyout xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"><MenuFlyoutItem Text="Native command"/><MenuFlyoutSeparator/><MenuFlyoutItem Text="Second command"/></MenuFlyout>"#,
    )?;
    let target =
        load(r#"<MenuFlyout xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"/>"#)?;
    let a = items(&source)?;
    let b = items(&target)?;
    let first = unsafe {
        let mut value = null_mut();
        check(
            (a.table::<Vector>().get)(a.raw(), 0, &mut value),
            "First native command",
        )?;
        Object::owned(value)?
    };
    let loan = Loan::capture(&source, &target)?;
    loan.lend()?;
    if count(&a)? != 0 || count(&b)? != 3 {
        return Err("菜单借用项目数量不匹配".into());
    }
    let copied = unsafe {
        let mut value = null_mut();
        check(
            (b.table::<Vector>().get)(b.raw(), 0, &mut value),
            "Borrowed native command",
        )?;
        Object::owned(value)?
    };
    if !same_object(&first, &copied)? {
        return Err("菜单原生命令身份未保留".into());
    }
    if Loan::capture(&source, &target).is_ok() || count(&b)? != 3 {
        return Err("菜单目标冲突未被拒绝".into());
    }
    loan.give_back()?;
    if count(&a)? != 3 || count(&b)? != 0 {
        return Err("菜单归还失败".into());
    }
    let mut checks = vec![
        "native_menu_borrow_preserves_command_identity_and_order",
        "native_menu_conflicting_target_rejected",
        "native_menu_items_returned_without_duplicates",
    ];
    checks.extend(root_checks::run()?);
    Ok(checks)
}
