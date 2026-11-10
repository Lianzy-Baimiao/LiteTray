//! Experimental taskbar-thread entry. Prepared sessions never change visible
//! content. The DLL stays pinned for Explorer's lifetime so no late timer or
//! routed delegate can call unloaded code, even if the host disappears.
use crate::{
    discovery::View,
    routing::{self, BoundView},
    watchdog::Watchdog,
};
use litetray_adapter_protocol::{VERSION, clock::SessionStatus, taskbar::*};
use std::{cell::RefCell, ptr::null_mut, rc::Rc};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{
        DataExchange::COPYDATASTRUCT, LibraryLoader::*, SystemInformation::GetTickCount64,
        Threading::*,
    },
    UI::WindowsAndMessaging::*,
};
struct Active {
    watchdog: Watchdog,
}
thread_local! {static ACTIVE:RefCell<Option<Rc<Active>>>=const{RefCell::new(None)};static PREPARING:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
struct Preparing;
impl Drop for Preparing {
    fn drop(&mut self) {
        PREPARING.with(|v| v.set(false));
    }
}
unsafe fn prepare(request: &Prepare) -> Result<Reply, String> {
    unsafe {
        if PREPARING.with(|v| v.replace(true)) {
            return Err("任务栏准备正在进行".into());
        }
        let _busy = Preparing;
        let previous = ACTIVE.with(|v| v.borrow().clone());
        if previous
            .as_ref()
            .is_some_and(|p| p.watchdog.status() != SessionStatus::Closed)
        {
            return Err("已有任务栏会话，须先停止并确认恢复".into());
        }
        ACTIVE.with(|v| v.borrow_mut().take());
        drop(previous);
        let plan = request.plan()?;
        let mut routes = request.routes.clone();
        if let Some(source) = &plan.source
            && !routes.iter().any(|r| r.window == source.window)
        {
            routes.push(source.clone());
        }
        for route in &routes {
            let hwnd = route.window as HWND;
            let mut pid = 0;
            if GetWindowThreadProcessId(hwnd, &mut pid) != GetCurrentThreadId()
                || pid != GetCurrentProcessId()
                || pid != route.process_id
                || route.thread_id != GetCurrentThreadId()
                || !crate::is_taskbar(hwnd)
                || route.secondary != (crate::class_name(hwnd) == "Shell_SecondaryTrayWnd")
            {
                return Err("任务栏窗口身份不匹配".into());
            }
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL);
            let mut info: MONITORINFOEXW = std::mem::zeroed();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if monitor.is_null()
                || GetMonitorInfoW(monitor, (&mut info as *mut MONITORINFOEXW).cast()) == 0
            {
                return Err("无法核对任务栏屏幕".into());
            }
            let n = info
                .szDevice
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(info.szDevice.len());
            if !String::from_utf16_lossy(&info.szDevice[..n]).eq_ignore_ascii_case(&route.device) {
                return Err("任务栏屏幕已变化".into());
            }
        }
        let implementation = crate::compatibility::resolve(&request.layout, request.mirror_icons)?;
        let sources = routes
            .iter()
            .map(|r| crate::taskbar_source::resolve(r.window as HWND, &implementation))
            .collect::<Result<Vec<_>, _>>()?;
        let views = sources
            .iter()
            .map(|s| View::discover(&s.root, implementation.topology))
            .collect::<Result<Vec<_>, _>>()?;
        let bound = routes
            .iter()
            .zip(&views)
            .zip(&sources)
            .map(|((r, v), s)| BoundView::capture_bound(r.clone(), v, s.binding.clone()))
            .collect::<Result<Vec<_>, _>>()?;
        // Once registered callbacks can outlive a timed-out caller. Pin once;
        // no detached worker or remote unload is needed for this first build.
        let mut module = null_mut();
        if GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN,
            crate::LiteTrayCallWndProc as *const u16,
            &mut module,
        ) == 0
        {
            return Err("无法保持适配模块生命周期".into());
        }
        let receiver = request.owner.receiver as HWND;
        let interaction = if request.mirror_icons {
            crate::tray_interaction::install(
                implementation.tray.as_ref().ok_or("托盘实现未准备")?,
            )?;
            Some(crate::tray_interaction::Runtime::new(
                bound
                    .iter()
                    .map(BoundView::interaction_host)
                    .collect::<Result<Vec<_>, _>>()?,
            ))
        } else {
            None
        };
        let watchdog = if request.custom_calendar || request.volume_scroll || request.volume_mute {
            routing::attach_with_options(
                &plan,
                &bound,
                request.owner.clone(),
                GetTickCount64(),
                request.input_options(),
                move |event| {
                    let Ok(bytes) = serde_json::to_vec(&event) else {
                        return false;
                    };
                    let copy = COPYDATASTRUCT {
                        dwData: litetray_adapter_protocol::activation::EVENT_TAG,
                        cbData: bytes.len() as u32,
                        lpData: bytes.as_ptr() as *mut _,
                    };
                    let mut accepted = 0;
                    SendMessageTimeoutW(
                        receiver,
                        WM_COPYDATA,
                        event.source_window as usize,
                        &copy as *const _ as isize,
                        SMTO_ABORTIFHUNG,
                        500,
                        &mut accepted,
                    ) != 0
                        && accepted == 1
                },
            )?
        } else {
            Watchdog::attach(routing::prepare(
                &plan,
                &bound,
                request.owner.clone(),
                GetTickCount64(),
            )?)?
        };
        let endpoint = watchdog.endpoint().ok_or("任务栏端点未建立")?;
        let reply = Reply {
            version: VERSION,
            owner: request.owner.clone(),
            endpoint: endpoint as u64,
            targets: request
                .routes
                .iter()
                .zip(&bound)
                .filter(|(r, _)| {
                    plan.targets
                        .iter()
                        .any(|t| t.clock && t.route.window == r.window)
                })
                .map(|(r, b)| (r.window, b.input_window()))
                .collect(),
            error: None,
        };
        // Session releases private hosts only after callbacks/properties have
        // restored successfully, including watchdog expiry after host loss.
        watchdog.retain_sources(sources)?;
        if let Some(interaction) = interaction {
            watchdog.prepare_interaction(interaction)?;
        }
        ACTIVE.with(|v| *v.borrow_mut() = Some(Rc::new(Active { watchdog })));
        Ok(reply)
    }
}
pub(crate) unsafe fn request(taskbar: HWND, sender: HWND, data: isize) {
    unsafe {
        let copy = &*(data as *const COPYDATASTRUCT);
        if copy.dwData != PREPARE_TAG
            || copy.cbData == 0
            || copy.cbData as usize > litetray_adapter_protocol::MAX_REQUEST
            || copy.lpData.is_null()
        {
            return;
        }
        let bytes =
            std::slice::from_raw_parts(copy.lpData.cast::<u8>(), copy.cbData as usize).to_vec();
        let Ok(request) = Prepare::decode(&bytes) else {
            return;
        };
        let mut pid = 0;
        GetWindowThreadProcessId(sender, &mut pid);
        if sender as u64 != request.owner.receiver
            || pid != request.owner.process_id
            || crate::class_name(sender) != RECEIVER_CLASS
            || !crate::same_installation(pid)
            || !request.routes.iter().any(|r| r.window == taskbar as u64)
        {
            return;
        }
        let reply = crate::events::install_fault_beacon()
            .and_then(|()| prepare(&request))
            .unwrap_or_else(|error| Reply {
                version: VERSION,
                owner: request.owner.clone(),
                endpoint: 0,
                targets: vec![],
                error: Some(error.chars().take(1000).collect()),
            });
        let Ok(bytes) = serde_json::to_vec(&reply) else {
            return;
        };
        let copy = COPYDATASTRUCT {
            dwData: REPLY_TAG,
            cbData: bytes.len() as u32,
            lpData: bytes.as_ptr() as *mut _,
        };
        let mut accepted = 0;
        let sent = SendMessageTimeoutW(
            sender,
            WM_COPYDATA,
            taskbar as usize,
            &copy as *const _ as isize,
            SMTO_ABORTIFHUNG,
            1000,
            &mut accepted,
        );
        match &reply.error {
            Some(error) => crate::events::record("prepare refused", error),
            None => crate::events::record("prepared", "session awaiting start"),
        }
        if (sent == 0 || accepted != 1)
            && reply.error.is_none()
            && let Some(active) = ACTIVE.with(|v| v.borrow().clone())
        {
            let _ = active.watchdog.stop();
        }
    }
}
