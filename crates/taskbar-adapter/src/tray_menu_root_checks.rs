//! Genuine cached MenuFlyouts and two private XAML islands. No shell window,
//! hook installation or native ShowAt invocation is needed to verify ownership.
use super::*;
use crate::{lifecycle_checks::Receiver, preview::ModelFixture};
use litetray_adapter_protocol::clock::{Frame, Owner, Style, Text};
use windows_sys::Win32::{System::Threading::GetCurrentProcessId, UI::WindowsAndMessaging::*};

fn fixture(owner: &Owner) -> Result<ModelFixture, String> {
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
    let fixture = ModelFixture::new(
        owner.clone(),
        frame,
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" x:Name="MenuTarget" Width="200" Height="60"/>"#,
    )?;
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
        let prepared = menus.prepare(&menu, &secondary)?;
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
        let primary_prepared = menus.prepare(&menu, &primary)?;
        require(
            primary_prepared.is_none() == initially_rooted,
            "primary uses its native menu or a fresh root-specific proxy",
        )?;
        menus.retire()?;
        require(
            root_is(&menu, expected)? && count(&items(&menu)?)? == 1,
            "primary return remains idempotent",
        )?;
    }
    Ok(vec![
        "secondary_first_use_preserves_unrooted_native_cached_menu",
        "secondary_menu_preserves_original_primary_xaml_root",
        "menu_stop_returns_native_commands_and_retires_secondary_proxy",
        "primary_menu_preparation_preserves_root_after_secondary_session_stop",
    ])
}
