//! In-process native clock preview for exercising the same properties and
//! routed input as the adapter. Never discovers or attaches to Explorer.
use crate::{
    clock::{ClockLease, load},
    island::{IID_NATIVE, Native},
    session::{InputTarget, Session, Target},
    watchdog::Watchdog,
    wide,
    xaml::{Object, StringHandle, check, factory},
    xaml_abi::*,
};
use litetray_adapter_protocol::{
    activation::{EVENT_TAG, MAX_EVENT},
    clock::{Change, Command, Frame, Owner},
};
use std::{
    cell::Cell,
    ffi::c_void,
    ptr::{null, null_mut},
};
use windows_sys::{
    Win32::{
        Foundation::*,
        System::{DataExchange::COPYDATASTRUCT, LibraryLoader::*, Threading::*},
        UI::{Input::KeyboardAndMouse::SetFocus, WindowsAndMessaging::*},
    },
    core::{GUID, IInspectable_Vtbl},
};
windows_link::link!("combase.dll" "system" fn RoActivateInstance(name:windows_sys::core::HSTRING,out:*mut *mut c_void)->i32);
// ABI from Microsoft's windows.ui.xaml.hosting.desktopwindowxamlsource.h.
const IID_NATIVE2: GUID = GUID::from_u128(0xe3dcd8c7_3057_4692_99c3_7b7720afda31);
const IID_CLOSABLE: GUID = GUID::from_u128(0x30d5a829_7fa4_4026_83bb_d75bae4ea99e);
const WM_FOCUS_CLOCK: u32 = WM_APP + 9;
const FOCUS_PROPERTY: &str = "LiteTray.PreviewFocus";
#[repr(C)]
struct Native2 {
    base: Native,
    pretranslate: unsafe extern "system" fn(*mut c_void, *const MSG, *mut i32) -> i32,
}
#[repr(C)]
struct Closable {
    base: IInspectable_Vtbl,
    close: unsafe extern "system" fn(*mut c_void) -> i32,
}
pub(crate) fn activate(name: &str) -> Result<Object, String> {
    unsafe {
        let name = StringHandle::new(name)?;
        let mut object = null_mut();
        check(
            RoActivateInstance(name.0, &mut object),
            "RoActivateInstance",
        )?;
        Object::owned(object)
    }
}
fn close(object: &Object) {
    unsafe {
        if let Ok(c) = object.query(&IID_CLOSABLE) {
            let _ = (c.table::<Closable>().close)(c.raw());
        }
    }
}
fn runtime() -> Result<Object, String> {
    unsafe {
        let init = factory(
            "Windows.UI.Xaml.Hosting.WindowsXamlManager",
            &IID_IWindowsXamlManagerStatics,
        )?;
        let mut manager = null_mut();
        check(
            (init
                .table::<IWindowsXamlManagerStatics_Vtbl>()
                .InitializeForCurrentThread)(init.raw(), &mut manager),
            "Initialize XAML preview",
        )?;
        Object::owned(manager)
    }
}
pub struct Preview {
    window: HWND,
    pub island: HWND,
    manager: Option<Object>,
    source: Option<Object>,
    native: Option<Object>,
    focus: Option<Object>,
    watchdog: Option<Watchdog>,
    owner: Owner,
    sequence: Cell<u64>,
    thread: u32,
    class: Vec<u16>,
}
impl Preview {
    pub fn create(owner: Owner, frame: Frame, visible: bool) -> Result<Self, String> {
        unsafe {
            owner.validate()?;
            frame.validate()?;
            let mut pid = 0;
            if GetWindowThreadProcessId(owner.receiver as HWND, &mut pid) != GetCurrentThreadId()
                || pid != GetCurrentProcessId()
                || pid != owner.process_id
            {
                return Err("原生预览仅可由本进程 UI 线程创建".into());
            }
            let mut preview = Self {
                window: null_mut(),
                island: null_mut(),
                manager: None,
                source: None,
                native: None,
                focus: None,
                watchdog: None,
                owner: owner.clone(),
                sequence: Cell::new(0),
                thread: GetCurrentThreadId(),
                class: wide(&format!("LiteTray.ClockPreview.{}", GetCurrentThreadId())),
            };
            preview.manager = Some(runtime()?);
            let wc = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: GetModuleHandleW(null()),
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                lpszClassName: preview.class.as_ptr(),
                ..std::mem::zeroed()
            };
            if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
                return Err("创建原生预览窗口类失败".into());
            }
            let mut parent: RECT = std::mem::zeroed();
            GetWindowRect(owner.receiver as HWND, &mut parent);
            preview.window = CreateWindowExW(
                WS_EX_APPWINDOW,
                preview.class.as_ptr(),
                wide("LiteTray 原生时钟预览 · 点击或按回车打开日历").as_ptr(),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN,
                parent.left + 80,
                parent.top + 80,
                520,
                180,
                null_mut(),
                null_mut(),
                wc.hInstance,
                null(),
            );
            if preview.window.is_null() {
                return Err("创建原生预览窗口失败".into());
            }
            let source = activate("Windows.UI.Xaml.Hosting.DesktopWindowXamlSource")?
                .query(&IID_IDesktopWindowXamlSource)?;
            let native = source.query(&IID_NATIVE)?;
            check(
                (native.table::<Native>().attach)(native.raw(), preview.window),
                "Attach XAML preview",
            )?;
            check(
                (native.table::<Native>().window)(native.raw(), &mut preview.island),
                "XAML preview HWND",
            )?;
            preview.native = Some(source.query(&IID_NATIVE2)?);
            preview.source = Some(source);
            let root = load(
                r##"<Grid xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" Background="#202020"><ContentControl x:Name="Clock" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml" Background="#303030" Padding="12" HorizontalAlignment="Center" VerticalAlignment="Center" IsTabStop="True" UseSystemFocusVisuals="True" AutomationProperties.Name="时钟预览，按回车或空格打开日历"><StackPanel><TextBlock x:Name="Time" Foreground="White" Text="Original time"/><TextBlock x:Name="Date" Foreground="White" Text="Original date"/></StackPanel></ContentControl></Grid>"##,
            )?;
            let find = |name: &str| -> Result<Object, String> {
                let fe = root.query(&IID_IFrameworkElement)?;
                let name = StringHandle::new(name)?;
                let mut object = null_mut();
                check(
                    (fe.table::<IFrameworkElement_Vtbl>().FindName)(fe.raw(), name.0, &mut object),
                    "Find preview element",
                )?;
                Object::owned(object)
            };
            let clock = find("Clock")?;
            preview.focus = Some(clock.query(&IID_IControl)?);
            // The window owns a COM reference until WM_NCDESTROY. This allows
            // a posted focus request after XAML has finished its first layout.
            let focus = clock.query(&IID_IControl)?;
            if SetPropW(preview.window, wide(FOCUS_PROPERTY).as_ptr(), focus.raw()) == 0 {
                return Err("无法保存预览焦点对象".into());
            }
            std::mem::forget(focus);
            let time = find("Time")?;
            let date = find("Date")?;
            let target = Target {
                id: 1,
                clock: ClockLease::capture(&time, &date, &clock)?,
            };
            let source = preview.source.as_ref().unwrap();
            let root_ui = root.query(&IID_IUIElement)?;
            check(
                (source.table::<IDesktopWindowXamlSource_Vtbl>().SetContent)(
                    source.raw(),
                    root_ui.raw(),
                ),
                "Set XAML preview content",
            )?;
            let receiver = owner.receiver as HWND;
            let island = preview.island;
            preview.watchdog = Some(Watchdog::attach(Session::new(
                owner,
                vec![target],
                windows_sys::Win32::System::SystemInformation::GetTickCount64(),
            )?)?);
            let watchdog = preview.watchdog.as_ref().unwrap();
            watchdog.prepare_inputs(
                vec![InputTarget {
                    id: 1,
                    element: clock,
                    island,
                }],
                move |event| {
                    let Ok(bytes) = serde_json::to_vec(&event) else {
                        return false;
                    };
                    if bytes.len() > MAX_EVENT {
                        return false;
                    }
                    let copy = COPYDATASTRUCT {
                        dwData: EVENT_TAG,
                        cbData: bytes.len() as u32,
                        lpData: bytes.as_ptr() as *mut _,
                    };
                    let mut accepted = 0;
                    SendMessageTimeoutW(
                        receiver,
                        WM_COPYDATA,
                        island as usize,
                        &copy as *const _ as isize,
                        SMTO_ABORTIFHUNG,
                        100,
                        &mut accepted,
                    ) != 0
                        && accepted == 1
                },
            )?;
            watchdog.start(frame)?;
            let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForWindow(preview.window).max(96) as i32;
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: 520 * dpi / 96,
                bottom: 130 * dpi / 96,
            };
            AdjustWindowRectEx(
                &mut rect,
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
                0,
                WS_EX_APPWINDOW,
            );
            SetWindowPos(
                preview.window,
                null_mut(),
                0,
                0,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            preview.resize();
            if visible {
                preview.show();
            }
            Ok(preview)
        }
    }
    pub fn update(&self, frame: Frame) -> Result<(), String> {
        frame.validate()?;
        self.command(Change::Update { frame })
    }
    pub fn heartbeat(&self) -> Result<(), String> {
        unsafe {
            if GetWindowThreadProcessId(self.window, null_mut()) != self.thread
                || GetWindowThreadProcessId(self.island, null_mut()) != self.thread
                || IsChild(self.window, self.island) == 0
            {
                return Err("原生预览窗口已失效".into());
            }
        }
        self.command(Change::Heartbeat)
    }
    fn status(&self) -> litetray_adapter_protocol::clock::SessionStatus {
        self.watchdog.as_ref().map_or(
            litetray_adapter_protocol::clock::SessionStatus::Busy,
            Watchdog::status,
        )
    }
    fn stop(&self) -> Result<(), String> {
        self.watchdog.as_ref().ok_or("预览会话缺失")?.stop()
    }
    fn command(&self, change: Change) -> Result<(), String> {
        if unsafe { GetCurrentThreadId() } != self.thread {
            return Err("原生预览线程不匹配".into());
        }
        let watchdog = self.watchdog.as_ref().ok_or("原生预览会话尚未创建")?;
        let sequence = self
            .sequence
            .get()
            .checked_add(1)
            .ok_or("预览命令序号已用尽")?;
        self.sequence.set(sequence);
        let bytes = serde_json::to_vec(&Command {
            version: litetray_adapter_protocol::VERSION,
            owner: self.owner.clone(),
            sequence,
            change,
        })
        .map_err(|e| e.to_string())?;
        watchdog.receive(&bytes)
    }
    pub fn visible(&self) -> bool {
        unsafe { IsWindowVisible(self.window) != 0 }
    }
    pub fn show(&self) {
        self.present(true);
    }
    fn present(&self, activate: bool) {
        unsafe {
            ShowWindow(
                self.window,
                if activate { SW_SHOW } else { SW_SHOWNOACTIVATE },
            );
            if activate {
                SetForegroundWindow(self.window);
                PostMessageW(self.window, WM_FOCUS_CLOCK, 0, 0);
            }
        }
    }
    pub fn pretranslate(&self, message: &MSG) -> bool {
        unsafe {
            if message.hwnd != self.window && IsChild(self.window, message.hwnd) == 0 {
                return false;
            }
            if message.message == WM_KEYDOWN && message.wParam == 0x1b {
                ShowWindow(self.window, SW_HIDE);
                return true;
            }
            let Some(native) = &self.native else {
                return false;
            };
            let mut handled = 0;
            (native.table::<Native2>().pretranslate)(native.raw(), message, &mut handled) >= 0
                && handled != 0
        }
    }
    fn resize(&self) {
        unsafe {
            let mut rect: RECT = std::mem::zeroed();
            GetClientRect(self.window, &mut rect);
            SetWindowPos(
                self.island,
                null_mut(),
                0,
                0,
                rect.right,
                rect.bottom,
                SWP_NOZORDER | SWP_SHOWWINDOW,
            );
        }
    }
}
impl Drop for Preview {
    fn drop(&mut self) {
        self.watchdog.take();
        unsafe {
            let focus = RemovePropW(self.window, wide(FOCUS_PROPERTY).as_ptr());
            if !focus.is_null() {
                let _ = Object::owned(focus);
            }
        }
        self.focus.take();
        self.native.take();
        if let Some(source) = self.source.take() {
            close(&source);
        }
        unsafe {
            if !self.window.is_null() {
                DestroyWindow(self.window);
            }
            UnregisterClassW(self.class.as_ptr(), GetModuleHandleW(null()));
        }
        if let Some(manager) = self.manager.take() {
            close(&manager);
        }
    }
}
/// Fresh controls for lifecycle verification, detached from the preview's
/// own session before another session captures their original properties.
pub(crate) struct InputFixture {
    time: Object,
    date: Object,
    control: Object,
    preview: Preview,
}
/// A separate hidden XAML island used to validate native model sharing. Its
/// controls and models are released before the hosting source and manager.
pub(crate) struct ModelFixture {
    root: Object,
    preview: Preview,
}
impl ModelFixture {
    pub(crate) fn source(&self) -> Result<&Object, String> {
        self.preview
            .source
            .as_ref()
            .ok_or("夹具 source 不可用".into())
    }
    pub(crate) fn replace_content(&self, root: &Object) -> Result<(), String> {
        let source = self.source()?;
        let root = root.query(&IID_IUIElement)?;
        unsafe {
            check(
                (source.table::<IDesktopWindowXamlSource_Vtbl>().SetContent)(
                    source.raw(),
                    root.raw(),
                ),
                "Replace fixture source content",
            )
        }
    }
    pub(crate) fn close_source(&self) -> Result<(), String> {
        let source = self.source()?.query(&IID_CLOSABLE)?;
        unsafe {
            check(
                (source.table::<Closable>().close)(source.raw()),
                "Close fixture source",
            )
        }
    }
    pub(crate) fn input(&self, name: &str, id: u64) -> Result<InputTarget, String> {
        Ok(InputTarget {
            id,
            element: self.find(name)?,
            island: self.preview.island,
        })
    }
    pub(crate) fn window(&self) -> HWND {
        self.preview.window
    }
    pub(crate) fn new(owner: Owner, frame: Frame, markup: &str) -> Result<Self, String> {
        let mut preview = Preview::create(owner, frame, false)?;
        if let Some(watchdog) = preview.watchdog.take() {
            watchdog.stop()?;
        }
        preview.focus = None;
        unsafe {
            let focus = RemovePropW(preview.window, wide(FOCUS_PROPERTY).as_ptr());
            if !focus.is_null() {
                drop(Object::owned(focus)?);
            }
        }
        let root = load(markup)?.query(&IID_IUIElement)?;
        let source = preview.source.as_ref().ok_or("模型检查缺少 XAML source")?;
        unsafe {
            check(
                (source.table::<IDesktopWindowXamlSource_Vtbl>().SetContent)(
                    source.raw(),
                    root.raw(),
                ),
                "Set model fixture content",
            )?;
        }
        let fixture = Self { root, preview };
        fixture.layout()?;
        Ok(fixture)
    }
    pub(crate) fn detach(&self) -> Result<(), String> {
        let source = self
            .preview
            .source
            .as_ref()
            .ok_or("模型检查缺少 XAML source")?;
        unsafe {
            check(
                (source.table::<IDesktopWindowXamlSource_Vtbl>().SetContent)(
                    source.raw(),
                    null_mut(),
                ),
                "Detach isolated model root",
            )
        }
    }
    pub(crate) fn find(&self, name: &str) -> Result<Object, String> {
        let root = self.root.query(&IID_IFrameworkElement)?;
        let name = StringHandle::new(name)?;
        unsafe {
            let mut value = null_mut();
            check(
                (root.table::<IFrameworkElement_Vtbl>().FindName)(root.raw(), name.0, &mut value),
                "Find model fixture",
            )?;
            Object::owned(value)
        }
    }
    pub(crate) fn layout(&self) -> Result<(), String> {
        unsafe {
            let table = self.root.table::<IUIElement_Vtbl>();
            check(
                (table.Measure)(
                    self.root.raw(),
                    Size {
                        Width: 520.,
                        Height: 130.,
                    },
                ),
                "Measure model fixture",
            )?;
            check(
                (table.Arrange)(
                    self.root.raw(),
                    Rect {
                        X: 0.,
                        Y: 0.,
                        Width: 520.,
                        Height: 130.,
                    },
                ),
                "Arrange model fixture",
            )?;
            check(
                (table.UpdateLayout)(self.root.raw()),
                "Layout model fixture",
            )
        }
    }
}
impl InputFixture {
    pub(crate) fn new(owner: Owner, frame: Frame) -> Result<Self, String> {
        let mut preview = Preview::create(owner, frame, false)?;
        if let Some(watchdog) = preview.watchdog.take() {
            watchdog.stop()?;
        }
        let control = preview
            .focus
            .as_ref()
            .ok_or("预览控件缺失")?
            .query(&IID_IUIElement)?;
        // A hidden island can defer its default ContentControl template and
        // report zero desired height. Give the isolated input fixture a real
        // explicit hit area; production preview layout remains content-sized.
        unsafe {
            let framework = control.query(&IID_IFrameworkElement)?;
            check(
                (framework.table::<IFrameworkElement_Vtbl>().SetMinHeight)(framework.raw(), 64.),
                "Hidden input fixture minimum height",
            )?;
        }
        let root = crate::xaml::root_content(&control)?.query(&IID_IFrameworkElement)?;
        let find = |name: &str| -> Result<Object, String> {
            unsafe {
                let name = StringHandle::new(name)?;
                let mut object = null_mut();
                check(
                    (root.table::<IFrameworkElement_Vtbl>().FindName)(
                        root.raw(),
                        name.0,
                        &mut object,
                    ),
                    "Find isolated input fixture",
                )?;
                Object::owned(object)
            }
        };
        let time = find("Time")?;
        let date = find("Date")?;
        let root = root.query(&IID_IUIElement)?;
        unsafe {
            check(
                (root.table::<IUIElement_Vtbl>().Measure)(
                    root.raw(),
                    Size {
                        Width: 520.,
                        Height: 130.,
                    },
                ),
                "Measure input fixture",
            )?;
            check(
                (root.table::<IUIElement_Vtbl>().Arrange)(
                    root.raw(),
                    Rect {
                        X: 0.,
                        Y: 0.,
                        Width: 520.,
                        Height: 130.,
                    },
                ),
                "Arrange input fixture",
            )?;
            check(
                (root.table::<IUIElement_Vtbl>().UpdateLayout)(root.raw()),
                "Layout input fixture",
            )?;
        }
        Ok(Self {
            preview,
            time,
            date,
            control,
        })
    }
    pub(crate) fn capture(&self, id: u64) -> Result<Target, String> {
        Ok(Target {
            id,
            clock: ClockLease::capture(&self.time, &self.date, &self.control)?,
        })
    }
    pub(crate) fn input(&self, id: u64) -> Result<InputTarget, String> {
        Ok(InputTarget {
            id,
            element: self.control.query(&IID_IUIElement)?,
            island: self.preview.island,
        })
    }
    /// Hidden islands do not receive the normal visible-window layout pass
    /// after a clock width/text update. Arrange explicitly before input QA.
    pub(crate) fn layout(&self) -> Result<(), String> {
        let root = crate::xaml::root_content(&self.control)?.query(&IID_IUIElement)?;
        unsafe {
            let control = self.control.query(&IID_IControl)?;
            let mut applied = false;
            check(
                (control.table::<IControl_Vtbl>().ApplyTemplate)(control.raw(), &mut applied),
                "Apply hidden clock template",
            )?;
            let table = root.table::<IUIElement_Vtbl>();
            check(
                (table.Measure)(
                    root.raw(),
                    Size {
                        Width: 520.,
                        Height: 130.,
                    },
                ),
                "Measure clock fixture",
            )?;
            check(
                (table.Arrange)(
                    root.raw(),
                    Rect {
                        X: 0.,
                        Y: 0.,
                        Width: 520.,
                        Height: 130.,
                    },
                ),
                "Arrange clock fixture",
            )?;
            check((table.UpdateLayout)(root.raw()), "Layout clock fixture")
        }
    }
    pub(crate) fn restored(&self) -> Result<bool, String> {
        unsafe {
            let time = self.time.query(&IID_ITextBlock)?;
            let date = self.date.query(&IID_ITextBlock)?;
            Ok(
                time.string(time.table::<ITextBlock_Vtbl>().Text, 4096)? == "Original time"
                    && date.string(date.table::<ITextBlock_Vtbl>().Text, 4096)? == "Original date",
            )
        }
    }
    pub(crate) fn set_date(&self, text: &str) -> Result<(), String> {
        unsafe {
            let date = self.date.query(&IID_ITextBlock)?;
            let text = StringHandle::new(text)?;
            check(
                (date.table::<ITextBlock_Vtbl>().SetText)(date.raw(), text.0),
                "External fixture date",
            )
        }
    }
    pub(crate) fn date(&self) -> Result<String, String> {
        unsafe {
            let date = self.date.query(&IID_ITextBlock)?;
            date.string(date.table::<ITextBlock_Vtbl>().Text, 4096)
        }
    }
}
pub(crate) fn self_test(owner: Owner, frame: Frame) -> Result<Vec<&'static str>, String> {
    let preview = Preview::create(owner.clone(), frame, false)?;
    let control = preview.focus.as_ref().ok_or("预览焦点对象缺失")?;
    let root = crate::xaml::root_content(control)?.query(&IID_IUIElement)?;
    unsafe {
        let table = root.table::<IUIElement_Vtbl>();
        check(
            (table.Measure)(
                root.raw(),
                Size {
                    Width: 520.,
                    Height: 130.,
                },
            ),
            "Measure isolated island",
        )?;
        check(
            (table.Arrange)(
                root.raw(),
                Rect {
                    X: 0.,
                    Y: 0.,
                    Width: 520.,
                    Height: 130.,
                },
            ),
            "Arrange isolated island",
        )?;
        check(
            (table.UpdateLayout)(root.raw()),
            "UpdateLayout isolated island",
        )?;
    }
    let checks = crate::input::self_test(control, preview.island, owner)?;
    drop(preview);
    Ok(checks)
}
unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_SETFOCUS || msg == WM_ACTIVATE && (w & 0xffff) != WA_INACTIVE as usize {
            PostMessageW(hwnd, WM_FOCUS_CLOCK, 0, 0);
        }
        if msg == WM_FOCUS_CLOCK {
            let pointer = GetPropW(hwnd, wide(FOCUS_PROPERTY).as_ptr());
            if !pointer.is_null()
                && GetForegroundWindow() == hwnd
                && let Ok(control) = Object::query_borrowed(pointer, &IID_IControl)
            {
                SetFocus(GetWindow(hwnd, GW_CHILD));
                if let Ok(ui) = control.query(&IID_IUIElement) {
                    let _ = (ui.table::<IUIElement_Vtbl>().UpdateLayout)(ui.raw());
                }
                let mut focused = false;
                let _ = (control.table::<IControl_Vtbl>().Focus)(
                    control.raw(),
                    FocusState::Keyboard,
                    &mut focused,
                );
            }
            return 0;
        }
        if msg == WM_NCDESTROY {
            let pointer = RemovePropW(hwnd, wide(FOCUS_PROPERTY).as_ptr());
            if !pointer.is_null() {
                let _ = Object::owned(pointer);
            }
        }
        if msg == WM_CLOSE {
            ShowWindow(hwnd, SW_HIDE);
            return 0;
        }
        if msg == WM_SIZE {
            let child = GetWindow(hwnd, GW_CHILD);
            if !child.is_null() {
                let mut rect: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut rect);
                SetWindowPos(
                    child,
                    null_mut(),
                    0,
                    0,
                    rect.right,
                    rect.bottom,
                    SWP_NOZORDER,
                );
            }
            return 0;
        }
        if msg == WM_DPICHANGED {
            let rect = &*(l as *const RECT);
            SetWindowPos(
                hwnd,
                null_mut(),
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOZORDER,
            );
            return 0;
        }
        DefWindowProcW(hwnd, msg, w, l)
    }
}

/// In-process ABI: all functions must be called on the creating STA, with the
/// module retained until Destroy. Opaque handles never cross a process.
/// Keep one runtime alive across all preview generations on this STA. Closing
/// its last XAML manager is terminal for the thread's XAML hosting lifetime.
/// # Safety
/// The error buffer is writable for capacity bytes. Call on an initialized STA.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn LiteTrayPreviewRuntimeCreate(
    error: *mut u8,
    capacity: usize,
) -> *mut c_void {
    match runtime() {
        Ok(manager) => Box::into_raw(Box::new(manager)).cast(),
        Err(message) => {
            if !error.is_null() && capacity > 0 {
                let n = message.len().min(capacity - 1);
                unsafe {
                    std::ptr::copy_nonoverlapping(message.as_ptr(), error, n);
                    *error.add(n) = 0;
                }
            }
            null_mut()
        }
    }
}
/// # Safety
/// Destroy once on its creating STA, after every preview handle and native call
/// is finished and before unloading the module. Do not restart XAML afterward.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn LiteTrayPreviewRuntimeDestroy(handle: *mut c_void) {
    if !handle.is_null() {
        let manager = unsafe { Box::from_raw(handle.cast::<Object>()) };
        close(&manager);
    }
}
/// # Safety
/// All input/output buffers are valid for their supplied lengths. Call on an STA.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn LiteTrayPreviewCreate(
    bytes: *const u8,
    length: usize,
    island: *mut u64,
    error: *mut u8,
    capacity: usize,
) -> *mut c_void {
    unsafe {
        if bytes.is_null() || island.is_null() || length > litetray_adapter_protocol::MAX_REQUEST {
            return null_mut();
        }
        let result = (|| {
            let value: serde_json::Value =
                serde_json::from_slice(std::slice::from_raw_parts(bytes, length))
                    .map_err(|e| e.to_string())?;
            let owner =
                serde_json::from_value(value["owner"].clone()).map_err(|e| e.to_string())?;
            let frame =
                serde_json::from_value(value["frame"].clone()).map_err(|e| e.to_string())?;
            Preview::create(owner, frame, value["visible"].as_bool().unwrap_or(true))
        })();
        match result {
            Ok(preview) => {
                *island = preview.island as u64;
                Box::into_raw(Box::new(preview)).cast()
            }
            Err(message) => {
                if !error.is_null() && capacity > 0 {
                    let n = message.len().min(capacity - 1);
                    std::ptr::copy_nonoverlapping(message.as_ptr(), error, n);
                    *error.add(n) = 0;
                }
                null_mut()
            }
        }
    }
}
/// # Safety
/// `handle` is the live handle returned by Create; bytes reference `length` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn LiteTrayPreviewUpdate(
    handle: *mut c_void,
    bytes: *const u8,
    length: usize,
) -> i32 {
    unsafe {
        if handle.is_null() || bytes.is_null() || length > litetray_adapter_protocol::MAX_REQUEST {
            return 0;
        }
        let result = serde_json::from_slice(std::slice::from_raw_parts(bytes, length))
            .map_err(|e| e.to_string())
            .and_then(|f| (&*handle.cast::<Preview>()).update(f));
        result.is_ok() as i32
    }
}
/// # Safety
/// `handle` is the live Create handle, used on its owning STA.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn LiteTrayPreviewHeartbeat(handle: *mut c_void) -> i32 {
    unsafe {
        if handle.is_null() {
            return 0;
        }
        (&*handle.cast::<Preview>()).heartbeat().is_ok() as i32
    }
}
/// # Safety
/// `handle` is the live Create handle, used on its owning STA.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn LiteTrayPreviewState(handle: *mut c_void) -> i32 {
    if handle.is_null() {
        return -1;
    }
    unsafe { (&*handle.cast::<Preview>()).status() as i32 }
}
/// # Safety
/// `handle` is the live Create handle, used on its owning STA. Only Closed
/// from State proves restoration; a rejected stop does not authorize unload.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn LiteTrayPreviewStop(handle: *mut c_void) -> i32 {
    if handle.is_null() {
        return 0;
    }
    unsafe { (&*handle.cast::<Preview>()).stop().is_ok() as i32 }
}
/// # Safety
/// `handle` is the live Create handle, used on its owning STA.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn LiteTrayPreviewPresent(handle: *mut c_void, activate: i32) {
    if !handle.is_null() {
        unsafe {
            (&*handle.cast::<Preview>()).present(activate != 0);
        }
    }
}
/// # Safety
/// `handle` is the live Create handle, used on the owning UI thread, and `message` is a valid MSG.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn LiteTrayPreviewMessage(
    handle: *mut c_void,
    message: *const MSG,
) -> i32 {
    unsafe {
        if handle.is_null() {
            return 0;
        }
        let p = &*handle.cast::<Preview>();
        if message.is_null() {
            p.visible() as i32
        } else {
            p.pretranslate(&*message) as i32
        }
    }
}
/// # Safety
/// `handle` is the live Create handle, used on the owning UI thread.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn LiteTrayPreviewShow(handle: *mut c_void) {
    unsafe {
        if !handle.is_null() {
            (&*handle.cast::<Preview>()).show();
        }
    }
}
/// # Safety
/// Destroy the Create handle exactly once on its owning STA, before unloading this module.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn LiteTrayPreviewDestroy(handle: *mut c_void) {
    unsafe {
        if !handle.is_null() {
            drop(Box::from_raw(handle.cast::<Preview>()));
        }
    }
}
