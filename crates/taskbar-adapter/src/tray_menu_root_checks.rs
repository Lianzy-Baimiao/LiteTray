//! Genuine cached MenuFlyouts and two private XAML islands. No shell window,
//! hook installation or native ShowAt invocation is needed to verify ownership.
use super::*;
use crate::{lifecycle_checks::Receiver, preview::ModelFixture};
use litetray_adapter_protocol::clock::{Frame, Owner, Style, Text};
use windows_sys::Win32::{System::Threading::GetCurrentProcessId, UI::WindowsAndMessaging::*};

fn fixture(owner: &Owner) -> Result<ModelFixture, String> {
    fixture_markup(
        owner,
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" x:Name="MenuTarget" Width="200" Height="60"/>"#,
    )
}
fn fixture_markup(owner: &Owner, markup: &str) -> Result<ModelFixture, String> {
    let frame = Frame {
        text: Text {
            top: "12:34:56".into(),
            bottom: "Menu root ownership".into(),
            tooltip: None,
        },
        style: Style {
            font_family: "Segoe UI".into(),
            font_size: 13,
            width: 260,
            compact_two_lines: false,
            top_color: None,
            bottom_color: None,
            alignment: Default::default(),
            top_style: Default::default(),
            bottom_style: Default::default(),
        },
    };
    let fixture = ModelFixture::new(owner.clone(), frame, markup)?;
    let mut message: MSG = unsafe { std::mem::zeroed() };
    unsafe {
        while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    fixture.layout()?;
    Ok(fixture)
}
fn root(menu: &Object) -> Result<Option<Object>, String> {
    let base = menu.query(&IID_IFlyoutBase6)?;
    unsafe {
        let mut root = null_mut();
        check(
            (base.table::<IFlyoutBase6_Vtbl>().XamlRoot)(base.raw(), &mut root),
            "Check cached menu.XamlRoot",
        )?;
        if root.is_null() {
            Ok(None)
        } else {
            Ok(Some(Object::owned(root)?))
        }
    }
}
fn root_is(menu: &Object, expected: Option<&Object>) -> Result<bool, String> {
    match (root(menu)?, expected) {
        (None, None) => Ok(true),
        (Some(actual), Some(expected)) => same_object(&actual, expected),
        _ => Ok(false),
    }
}
fn require(condition: bool, label: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(format!("菜单跨屏归属检查失败：{label}"))
    }
}
fn first(menu: &Object) -> Result<Object, String> {
    let items = items(menu)?;
    unsafe {
        let mut command = null_mut();
        check(
            (items.table::<Vector>().get)(items.raw(), 0, &mut command),
            "Check retained menu command",
        )?;
        Object::owned(command)
    }
}
pub(crate) fn run() -> Result<Vec<&'static str>, String> {
    let receiver = Receiver::new()?;
    let owner = Owner {
        nonce: 0x4d454e55524f4f54,
        process_id: unsafe { GetCurrentProcessId() },
        receiver: receiver.0 as u64,
    };
    let primary_fixture = fixture(&owner)?;
    let secondary_fixture = fixture(&owner)?;
    let primary = crate::xaml::root(&primary_fixture.find("MenuTarget")?)?;
    let secondary = crate::xaml::root(&secondary_fixture.find("MenuTarget")?)?;
    require(!same_object(&primary, &secondary)?, "distinct XAML islands")?;
    for initially_rooted in [false, true] {
        let menu = load(
            r#"<MenuFlyout xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"><MenuFlyoutItem Text="Retained native command"/></MenuFlyout>"#,
        )?;
        let expected = initially_rooted.then_some(&primary);
        if initially_rooted {
            let base = menu.query(&IID_IFlyoutBase6)?;
            unsafe {
                check(
                    (base.table::<IFlyoutBase6_Vtbl>().SetXamlRoot)(base.raw(), primary.raw()),
                    "Initialize private primary menu root",
                )?;
            }
        }
        require(root_is(&menu, expected)?, "fixture native menu root")?;
        let command = first(&menu)?;
        let mut menus = Menus::default();
        let prepared = menus.prepare(&menu, &secondary, true)?;
        require(
            root_is(&menu, expected)?,
            if initially_rooted {
                "secondary preparation replaced the native primary root"
            } else {
                "secondary first use permanently rooted the native cached menu"
            },
        )?;
        let proxy = prepared.ok_or("跨屏菜单未使用隔离代理")?;
        require(
            !same_object(&proxy, &menu)? && root_is(&proxy, Some(&secondary))?,
            "secondary proxy owns the requested island",
        )?;
        require(
            count(&items(&menu)?)? == 0
                && count(&items(&proxy)?)? == 1
                && same_object(&first(&proxy)?, &command)?,
            "native command borrowed by proxy",
        )?;
        require(
            menus.prepare(&proxy, &primary, false).is_err(),
            "cached secondary proxy cannot reach native ShowAt on primary root",
        )?;
        menus.retire()?;
        require(
            root_is(&menu, expected)?
                && count(&items(&menu)?)? == 1
                && same_object(&first(&menu)?, &command)?,
            "stop returns command without changing native root",
        )?;
        require(
            count(&items(&proxy)?)? == 0,
            "retired proxy releases command",
        )?;
        let primary_prepared = menus.prepare(&menu, &primary, false)?;
        require(
            primary_prepared.is_none(),
            "primary uses its native cache including first-use initialization",
        )?;
        menus.retire()?;
        require(
            root_is(&menu, expected)? && count(&items(&menu)?)? == 1,
            "primary return remains idempotent",
        )?;
    }
    let flyout = load(
        r#"<Flyout xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"><TextBlock Text="Native content"/></Flyout>"#,
    )?;
    let mut menus = Menus::default();
    require(
        menus.prepare(&flyout, &primary, false)?.is_none(),
        "unrooted native ordinary Flyout passes through",
    )?;
    require(
        root_is(&flyout, None)?,
        "preparation does not set native Flyout root",
    )?;
    require(
        menus.prepare(&flyout, &secondary, true).is_err(),
        "unrooted cross-root ordinary Flyout remains protected",
    )?;
    let base = flyout.query(&IID_IFlyoutBase6)?;
    unsafe {
        check(
            (base.table::<IFlyoutBase6_Vtbl>().SetXamlRoot)(base.raw(), primary.raw()),
            "Initialize private ordinary Flyout root",
        )?;
    }
    require(
        menus.prepare(&flyout, &primary, false)?.is_none(),
        "ordinary Flyout uses its own root",
    )?;
    require(
        menus.prepare(&flyout, &secondary, true).is_err() && root_is(&flyout, Some(&primary))?,
        "mismatched ordinary Flyout remains protected without replacing root",
    )?;
    require(
        crate::tray_interaction::copied_root(&secondary, [&secondary].into_iter())?
            && !crate::tray_interaction::copied_root(&primary, [&secondary].into_iter())?,
        "native primary outside selected copies is classified correctly",
    )?;
    let markup = format!(
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" x:Name="MainStack" Width="200" Height="60">{}<Grid x:Name="DeepTarget"/>{}</Grid>"#,
        "<Grid>".repeat(34),
        "</Grid>".repeat(34)
    );
    let deep_fixture = fixture_markup(&owner, &markup)?;
    crate::tray_interaction::check_context_limit(&deep_fixture.find("DeepTarget")?)?;
    Ok(vec![
        "secondary_first_use_preserves_unrooted_native_cached_menu",
        "secondary_menu_preserves_original_primary_xaml_root",
        "menu_stop_returns_native_commands_and_retires_secondary_proxy",
        "primary_menu_preparation_preserves_root_after_secondary_session_stop",
        "native_unrooted_ordinary_flyout_uses_original_first_show",
        "ordinary_flyout_cross_root_rejected_without_root_mutation",
        "native_primary_outside_copied_scope_is_not_a_secondary_proxy",
        "cached_proxy_rejects_a_different_native_xaml_root",
        "native_menu_ancestry_limit_cannot_authorize_original_show",
    ])
}
