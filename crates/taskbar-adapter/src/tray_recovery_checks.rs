//! Native recovery checks run in the existing isolated clock self-test STA.
//! Empty hook registration exercises Runtime without patching any function.
use super::*;
use crate::{
    discovery::View,
    lifecycle_checks::Receiver,
    preview::ModelFixture,
    routing::{self, BoundView},
    session::State,
    watchdog::Watchdog,
};
use litetray_adapter_protocol::{
    VERSION,
    clock::{Change, Command, Frame, Owner, Style, Text},
    routing::{Options, Plan, Route},
};
use windows_sys::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};

const MARKUP: &str = r#"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
<ItemsControl x:Name="Collection" Visibility="Collapsed"><x:String>Original item</x:String></ItemsControl>
<StackPanel x:Name="SystemTrayFrameGrid" Width="520" Height="130">
  <ContentControl x:Name="NotificationAreaIcons" DataContext="Icons model" Content="Icons" Visibility="Collapsed"/>
  <ContentControl x:Name="NotifyIconStack" DataContext="Overflow model" Content="Overflow" Visibility="Collapsed"/>
  <ItemsControl x:Name="MainStack" ItemsSource="{Binding}" Height="24"/>
  <ContentControl x:Name="NonActivatableStack" DataContext="Privacy model" Content="Privacy" Visibility="Collapsed"/>
  <ContentControl x:Name="ControlCenterButton" DataContext="Control model" Content="Control" Visibility="Collapsed"/>
  <ContentControl x:Name="NotificationCenterButton" MinHeight="80"><Grid>
    <StackPanel x:Name="ClockText" Width="200" Height="60">
      <TextBlock x:Name="TimeInnerTextBlock" Text="Original time" Width="200" Height="30"/>
      <TextBlock x:Name="DateInnerTextBlock" Text="Original date" Width="200" Height="30"/>
    </StackPanel><Button x:Name="Bell" Content="Notifications" HorizontalAlignment="Right"/>
  </Grid></ContentControl>
</StackPanel></Grid>"#;

fn require(value: bool, label: &str) -> Result<(), String> {
    if value {
        Ok(())
    } else {
        Err(format!("热插拔恢复隔离检查失败：{label}"))
    }
}
fn frame() -> Frame {
    Frame {
        text: Text {
            top: "12:34:56".into(),
            bottom: "Hotplug recovery".into(),
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
    }
}
fn text(element: &Object) -> Result<String, String> {
    let text = element.query(&IID_ITextBlock)?;
    unsafe { text.string(text.table::<ITextBlock_Vtbl>().Text, 4096) }
}
fn find(root: &Object, name: &str) -> Result<Object, String> {
    let root = root.query(&IID_IFrameworkElement)?;
    let name = crate::xaml::StringHandle::new(name)?;
    let mut value = null_mut();
    unsafe {
        check(
            (root.table::<IFrameworkElement_Vtbl>().FindName)(root.raw(), name.0, &mut value),
            "Find recovery fixture",
        )?;
        Object::owned(value)
    }
}
fn make_fixture(owner: &Owner) -> Result<ModelFixture, String> {
    let fixture = ModelFixture::new(owner.clone(), frame(), MARKUP)?;
    // Complete genuine XAML island attachment before collecting its tree.
    let mut message: MSG = unsafe { std::mem::zeroed() };
    unsafe {
        while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        let collection = fixture.find("Collection")?.query(&IID_IItemsControl)?;
        let mut model = null_mut();
        check(
            (collection.table::<IItemsControl_Vtbl>().Items)(collection.raw(), &mut model),
            "Recovery Items",
        )?;
        let model = Object::owned(model)?;
        let items = fixture.find("MainStack")?.query(&IID_IFrameworkElement)?;
        check(
            (items.table::<IFrameworkElement_Vtbl>().SetDataContext)(items.raw(), model.raw()),
            "Recovery ItemsSource binding",
        )?;
    }
    fixture.layout()?;
    Ok(fixture)
}
fn attach(fixture: &ModelFixture, owner: &Owner) -> Result<(Watchdog, Rc<Runtime>), String> {
    let view = View::from_panel(&fixture.find("SystemTrayFrameGrid")?)?;
    let route = Route {
        window: fixture.window() as u64,
        process_id: unsafe { GetCurrentProcessId() },
        thread_id: unsafe { GetCurrentThreadId() },
        secondary: false,
        device: "ISOLATED".into(),
    };
    let plan = Plan::build(
        std::slice::from_ref(&route),
        std::slice::from_ref(&route.device),
        Options {
            clocks: true,
            tray: true,
            icons: true,
            control_center: true,
            copied_taskbars_only: false,
        },
    )?;
    let bound = BoundView::capture(route, &view, fixture.source()?)?;
    let runtime = Runtime::new(vec![bound.interaction_host()?]);
    let session = routing::prepare(&plan, &[bound], owner.clone(), unsafe { GetTickCount64() })?;
    let watchdog = Watchdog::attach(session)?;
    watchdog.prepare_interaction(runtime.clone())?;
    watchdog.start(frame())?;
    Ok((watchdog, runtime))
}
fn heartbeat(watchdog: &Watchdog, owner: &Owner) -> Result<(), String> {
    watchdog.receive(
        &serde_json::to_vec(&Command {
            version: VERSION,
            owner: owner.clone(),
            sequence: 1,
            change: Change::Heartbeat,
        })
        .map_err(|error| error.to_string())?,
    )
}
struct Position(HWND, RECT);
impl Drop for Position {
    fn drop(&mut self) {
        unsafe {
            SetWindowPos(
                self.0,
                null_mut(),
                self.1.left,
                self.1.top,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
}
fn offscreen(window: HWND) -> Result<Position, String> {
    let mut original: RECT = unsafe { std::mem::zeroed() };
    unsafe {
        require(
            GetWindowRect(window, &mut original) != 0,
            "capture fixture bounds",
        )?;
        require(
            SetWindowPos(
                window,
                null_mut(),
                GetSystemMetrics(SM_XVIRTUALSCREEN) - 10_000,
                GetSystemMetrics(SM_YVIRTUALSCREEN) - 10_000,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            ) != 0,
            "move fixture outside every monitor",
        )?;
    }
    Ok(Position(window, original))
}
type Marker = (Vec<u16>, HANDLE);
unsafe extern "system" fn markers(
    _: HWND,
    name: windows_sys::core::PCWSTR,
    value: HANDLE,
    context: usize,
) -> windows_sys::core::BOOL {
    unsafe {
        if name as usize <= u16::MAX as usize {
            return 1;
        }
        let mut length = 0;
        while length < 128 && *name.add(length) != 0 {
            length += 1;
        }
        let name = std::slice::from_raw_parts(name, length);
        if String::from_utf16_lossy(name).starts_with("LiteTray.WindowLease.") {
            (&mut *(context as *mut Vec<Marker>))
                .push((name.iter().copied().chain(Some(0)).collect(), value));
        }
        1
    }
}
struct MarkerConflict(HWND, Vec<Marker>);
impl Drop for MarkerConflict {
    fn drop(&mut self) {
        for (name, original) in &self.1 {
            unsafe {
                SetPropW(self.0, name.as_ptr(), *original);
            }
        }
    }
}
fn conflict(window: HWND) -> Result<MarkerConflict, String> {
    let mut found: Vec<Marker> = vec![];
    unsafe {
        EnumPropsExW(
            window,
            Some(markers),
            &mut found as *mut Vec<Marker> as isize,
        );
    }
    require(!found.is_empty(), "captured window marker exists")?;
    let restore = MarkerConflict(window, found);
    for (name, _) in &restore.1 {
        require(
            unsafe { SetPropW(window, name.as_ptr(), std::ptr::dangling_mut()) } != 0,
            "replace marker",
        )?;
    }
    Ok(restore)
}

pub(crate) fn run() -> Result<Vec<&'static str>, String> {
    // The CLI owns this process and already initialized its XAML STA. It has
    // no installed taskbar hooks; registering zero entries patches no code.
    unsafe { hooks::install(&[], 0)? };
    let receiver = Receiver::new()?;
    let owner = Owner {
        nonce: 0x484f54504c5547,
        process_id: unsafe { GetCurrentProcessId() },
        receiver: receiver.0 as u64,
    };
    let mut checks = vec![];
    {
        let fixture = make_fixture(&owner)?;
        let (watchdog, runtime) = attach(&fixture, &owner)?;
        let items = fixture.find("MainStack")?;
        let model =
            read(&items, &Field::ItemsSource.property()?)?.ok_or("Recovery model missing")?;
        let position = offscreen(fixture.window())?;
        let error = runtime.hosts[0]
            .binding
            .validate()
            .err()
            .ok_or("Active geometry unexpectedly accepted")?;
        require(
            error.contains("显示器或 DPI 不可用"),
            "active monitor guard still fails",
        )?;
        watchdog.stop()?;
        require(
            watchdog.state() == State::Closed
                && text(&fixture.find("TimeInnerTextBlock")?)? == "Original time",
            "geometry change closes and restores clock",
        )?;
        require(
            same_object(
                &read(&items, &Field::ItemsSource.property()?)?.ok_or("Restored model missing")?,
                &model,
            )?,
            "reattach preserves exact ItemsSource",
        )?;
        require(
            matches!(
                Property::capture(&items, Field::ItemsSource.property()?)?.original,
                crate::property::Original::Binding(_)
            ),
            "reattach preserves native binding",
        )?;
        watchdog.stop()?;
        drop(position);
        let (next, _) = attach(&fixture, &owner)?;
        require(
            next.state() == State::Active,
            "fresh session starts after geometry restoration",
        )?;
        next.stop()?;
        checks.push(
            "tray_recovery_geometry_change_restores_originals_repeated_stop_and_fresh_session",
        );
    }
    {
        let fixture = make_fixture(&owner)?;
        let (watchdog, runtime) = attach(&fixture, &owner)?;
        let replacement = crate::clock::load(&MARKUP.replace("Original time", "Replacement time"))?;
        fixture.replace_content(&replacement)?;
        require(
            runtime.hosts[0].binding.restore_target()? == RestoreTarget::Retired,
            "replaced source content retires old owner",
        )?;
        require(
            heartbeat(&watchdog, &owner).is_err() && watchdog.state() == State::Closed,
            "replaced content stops old session",
        )?;
        require(
            text(&fixture.find("TimeInnerTextBlock")?)? == "Original time"
                && text(&find(&replacement, "TimeInnerTextBlock")?)? == "Replacement time",
            "old restore leaves replacement untouched",
        )?;
        watchdog.stop()?;
        // A new source has its own originals and no stale Runtime ownership.
        let fresh = make_fixture(&owner)?;
        let (next, _) = attach(&fresh, &owner)?;
        next.stop()?;
        checks.push("tray_recovery_replaced_content_retires_old_root_without_touching_replacement");
    }
    {
        let fixture = make_fixture(&owner)?;
        let (watchdog, runtime) = attach(&fixture, &owner)?;
        fixture.close_source()?;
        require(
            runtime.hosts[0].binding.restore_target()? == RestoreTarget::Retired,
            "destroyed source window retires old owner",
        )?;
        watchdog.stop()?;
        watchdog.stop()?;
        require(
            watchdog.state() == State::Closed,
            "closed source does not strand interaction",
        )?;
        let fresh = make_fixture(&owner)?;
        let (next, _) = attach(&fresh, &owner)?;
        next.stop()?;
        checks.push("tray_recovery_closed_source_closes_and_new_source_can_start");
    }
    {
        let fixture = make_fixture(&owner)?;
        let (watchdog, runtime) = attach(&fixture, &owner)?;
        let marker = conflict(fixture.window())?;
        require(
            runtime.hosts[0].binding.restore_target().is_err(),
            "live marker conflict is not retirement",
        )?;
        require(
            watchdog.stop().is_err() && watchdog.state() == State::Restoring,
            "ambiguous live window still blocks Closed",
        )?;
        drop(marker);
        watchdog.stop()?;
        watchdog.stop()?;
        require(
            watchdog.state() == State::Closed,
            "resolved identity conflict permits retry",
        )?;
        checks.push("tray_recovery_live_marker_conflict_blocks_then_verified_retry_closes");
    }
    Ok(checks)
}
