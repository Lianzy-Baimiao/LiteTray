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
fn move_items(source: &Object, target: &Object) -> Result<(), String> {
    unsafe {
        if count(target)? != 0 {
            return Err("菜单目标仍持有项目".into());
        }
        let mut values = Vec::new();
        for index in 0..count(source)? {
            let mut value = null_mut();
            check(
                (source.table::<Vector>().get)(source.raw(), index, &mut value),
                "Menu items.GetAt",
            )?;
            values.push(Object::owned(value)?);
        }
        check(
            (source.table::<Vector>().clear)(source.raw()),
            "Menu items.Clear",
        )?;
        for value in &values {
            if let Err(error) = check(
                (target.table::<Vector>().append)(target.raw(), value.raw()),
                "Menu items.Append",
            ) {
                // Undo a partial transfer before releasing retained commands.
                (target.table::<Vector>().clear)(target.raw());
                for value in &values {
                    (source.table::<Vector>().append)(source.raw(), value.raw());
                }
                return Err(error);
            }
        }
        Ok(())
    }
}
struct Proxy {
    root: Object,
    menu: Object,
    home: Option<Object>,
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
    pub fn is_proxy(&self, menu: &Object) -> bool {
        self.proxies
            .iter()
            .any(|p| same_object(menu, &p.menu).unwrap_or(false))
    }
    pub fn restore(&mut self) -> Result<(), String> {
        for proxy in &mut self.proxies {
            if let Some(home) = &proxy.home {
                let flyout = proxy.menu.query(&IID_IFlyoutBase)?;
                unsafe {
                    check(
                        (flyout.table::<IFlyoutBase_Vtbl>().Hide)(flyout.raw()),
                        "Hide proxy",
                    )?;
                }
                let source = items(&proxy.menu)?;
                let target = items(home)?;
                if count(&target)? == 0 {
                    move_items(&source, &target)?;
                } else {
                    unsafe {
                        check(
                            (source.table::<Vector>().clear)(source.raw()),
                            "Discard rebuilt menu items",
                        )?;
                    }
                }
                proxy.home = None;
            }
        }
        Ok(())
    }
    /// None means the native show is safe. Some(proxy) suppresses the original
    /// and shows the proxy after releasing all RefCell borrows.
    pub fn prepare(&mut self, menu: &Object, root: &Object) -> Result<Option<Object>, String> {
        if self.is_proxy(menu) {
            return Ok(None);
        }
        self.restore()?;
        let base = menu.query(&IID_IFlyoutBase6)?;
        unsafe {
            let mut existing = null_mut();
            check(
                (base.table::<IFlyoutBase6_Vtbl>().XamlRoot)(base.raw(), &mut existing),
                "Flyout.XamlRoot",
            )?;
            if !existing.is_null() && same_object(&Object::owned(existing)?, root)? {
                return Ok(None);
            }
        }
        // A fresh cached shell menu can accept SetXamlRoot, but that would
        // permanently attach it to the first copied island. Preserve its
        // native root even before first show so stopping can safely pass the
        // original call through on the primary taskbar.
        let index = match self
            .proxies
            .iter()
            .position(|p| same_object(&p.root, root).unwrap_or(false))
        {
            Some(index) => index,
            None => {
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
        move_items(&items(menu)?, &items(&proxy.menu)?)?;
        proxy.home = Some(menu.clone());
        Ok(Some(proxy.menu.clone()))
    }
}
#[cfg(test)]
#[path = "tray_menu_reentry_checks.rs"]
mod reentry_checks;
#[path = "tray_menu_root_checks.rs"]
mod root_checks;
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
    move_items(&a, &b)?;
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
    if move_items(&a, &b).is_ok() || count(&b)? != 3 {
        return Err("菜单目标冲突未被拒绝".into());
    }
    move_items(&b, &a)?;
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
