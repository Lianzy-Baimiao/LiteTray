//! Calls the real menu detour with private XAML targets and inert originals.
//! It never installs a hook or invokes the system's menu implementation.
use super::*;

thread_local! {
    static ORIGINAL_CALL: Cell<Option<(usize, usize, Option<usize>)>> = const { Cell::new(None) };
}
unsafe extern "system" fn original(this: *mut c_void, target: *mut c_void) -> i32 {
    ORIGINAL_CALL.with(|call| call.set(Some((this as usize, target as usize, None))));
    S_FALSE
}
unsafe extern "system" fn original_options(
    this: *mut c_void,
    target: *mut c_void,
    options: *mut c_void,
) -> i32 {
    ORIGINAL_CALL.with(|call| {
        call.set(Some((
            this as usize,
            target as usize,
            Some(options as usize),
        )))
    });
    S_FALSE
}
pub(crate) fn run() -> Result<Vec<&'static str>, String> {
    if runtime().is_some() {
        return Err("菜单退出自检必须没有活跃托盘会话".into());
    }
    let target = crate::clock::load(
        r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" x:Name="MainStack"/>"#,
    )?
    .query(&IID_IFrameworkElement)?;
    let menu = crate::clock::load(
        r#"<MenuFlyout xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"/>"#,
    )?
    .query(&IID_IFlyoutBase)?;
    if !shared_target(&target)? {
        return Err("菜单退出自检必须经过共享托盘分类".into());
    }
    for options in [None, Some(null_mut())] {
        ORIGINAL_CALL.with(|call| call.set(None));
        let entry = AtomicUsize::new(if options.is_some() {
            original_options as *const () as usize
        } else {
            original as *const () as usize
        });
        let result = unsafe { show(&entry, menu.raw(), target.raw(), options) };
        let expected = (
            menu.raw() as usize,
            target.raw() as usize,
            options.map(|v| v as usize),
        );
        if result != S_FALSE || ORIGINAL_CALL.with(Cell::get) != Some(expected) {
            return Err(format!(
                "停止后的菜单钩子未转发原始调用：options={}, result={result}, called={}",
                options.is_some(),
                ORIGINAL_CALL.with(|call| call.get().is_some())
            ));
        }
    }
    Ok(vec![
        "inactive_shared_menu_detour_preserves_original_call_and_hresult",
        "inactive_shared_menu_options_detour_preserves_all_arguments",
    ])
}
