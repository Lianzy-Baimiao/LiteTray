//! Thread-bound owner of the in-process XAML preview DLL and event gate.
use crate::{clock, config::ClockConfig};
use litetray_adapter_protocol::{
    activation::{Activation, Gate},
    clock::{Frame, Owner, Style, Text},
};
use std::{
    ffi::c_void,
    marker::PhantomData,
    ptr::{null, null_mut},
    rc::Rc,
};
use windows_sys::Win32::{
    Foundation::*,
    System::{LibraryLoader::*, Threading::GetCurrentProcessId},
    UI::WindowsAndMessaging::*,
};
type Create = unsafe extern "system" fn(*const u8, usize, *mut u64, *mut u8, usize) -> *mut c_void;
type Update = unsafe extern "system" fn(*mut c_void, *const u8, usize) -> i32;
type Message = unsafe extern "system" fn(*mut c_void, *const MSG) -> i32;
type Action = unsafe extern "system" fn(*mut c_void);
pub struct Preview {
    module: HMODULE,
    handle: *mut c_void,
    update: Update,
    message: Message,
    show: Action,
    destroy: Action,
    gate: Gate,
    island: HWND,
    _thread: PhantomData<Rc<()>>,
}
pub fn frame(config: &ClockConfig, weather: &str) -> Frame {
    let now = chrono::Local::now();
    Frame {
        text: Text {
            top: clock::format(&config.top_line, config, now, weather),
            bottom: clock::format(&config.bottom_line, config, now, weather),
        },
        style: Style {
            font_family: config.font_family.clone(),
            font_size: config.font_size,
            width: config.width,
        },
    }
}
impl Preview {
    pub fn open(receiver: HWND, frame: Frame) -> Result<Self, String> {
        unsafe {
            let dll = std::env::current_exe()
                .map_err(|e| e.to_string())?
                .with_file_name("litetray_taskbar_adapter.dll");
            let module = LoadLibraryExW(
                super::wide(&dll.to_string_lossy()).as_ptr(),
                null_mut(),
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            );
            if module.is_null() {
                return Err("无法加载原生时钟预览组件".into());
            }
            struct Library(HMODULE);
            impl Drop for Library {
                fn drop(&mut self) {
                    unsafe {
                        FreeLibrary(self.0);
                    }
                }
            }
            let guard = Library(module);
            let create: Create = std::mem::transmute(
                GetProcAddress(module, c"LiteTrayPreviewCreate".as_ptr().cast())
                    .ok_or("缺少原生预览入口")?,
            );
            let update: Update = std::mem::transmute(
                GetProcAddress(module, c"LiteTrayPreviewUpdate".as_ptr().cast())
                    .ok_or("缺少原生预览更新入口")?,
            );
            let message: Message = std::mem::transmute(
                GetProcAddress(module, c"LiteTrayPreviewMessage".as_ptr().cast())
                    .ok_or("缺少原生预览消息入口")?,
            );
            let show: Action = std::mem::transmute(
                GetProcAddress(module, c"LiteTrayPreviewShow".as_ptr().cast())
                    .ok_or("缺少原生预览显示入口")?,
            );
            let destroy: Action = std::mem::transmute(
                GetProcAddress(module, c"LiteTrayPreviewDestroy".as_ptr().cast())
                    .ok_or("缺少原生预览关闭入口")?,
            );
            let owner = Owner {
                nonce: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos() as u64,
                process_id: GetCurrentProcessId(),
                receiver: receiver as u64,
            };
            let bytes = serde_json::to_vec(&serde_json::json!({"owner":owner,"frame":frame}))
                .map_err(|e| e.to_string())?;
            let mut island = 0;
            let mut error = vec![0u8; 2048];
            let handle = create(
                bytes.as_ptr(),
                bytes.len(),
                &mut island,
                error.as_mut_ptr(),
                error.len(),
            );
            if handle.is_null() {
                let n = error.iter().position(|b| *b == 0).unwrap_or(error.len());
                return Err(String::from_utf8_lossy(&error[..n]).into());
            }
            let gate = match Gate::new(owner, vec![(1, island)]) {
                Ok(g) => g,
                Err(e) => {
                    destroy(handle);
                    return Err(e);
                }
            };
            std::mem::forget(guard);
            Ok(Self {
                module,
                handle,
                update,
                message,
                show,
                destroy,
                gate,
                island: island as HWND,
                _thread: PhantomData,
            })
        }
    }
    #[expect(
        clippy::not_unsafe_ptr_arg_deref,
        reason = "HWND is an opaque Windows handle, validated by GetWindowThreadProcessId"
    )]
    pub fn accept(&mut self, sender: HWND, bytes: &[u8]) -> Result<Activation, String> {
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(sender, &mut pid);
            if sender != self.island || pid != GetCurrentProcessId() {
                return Err("原生预览点击窗口不匹配".into());
            }
            self.gate.accept(bytes)
        }
    }
    pub fn update(&self, frame: &Frame) -> Result<(), String> {
        let bytes = serde_json::to_vec(frame).map_err(|e| e.to_string())?;
        if unsafe { (self.update)(self.handle, bytes.as_ptr(), bytes.len()) } == 0 {
            Err("原生预览更新失败".into())
        } else {
            Ok(())
        }
    }
    pub fn visible(&self) -> bool {
        unsafe { (self.message)(self.handle, null()) != 0 }
    }
    pub fn show(&self) {
        unsafe {
            (self.show)(self.handle);
        }
    }
    /// Copy the call target before dispatch, so reentrant input can borrow App.
    /// The caller must retain this Preview throughout the native call.
    pub fn pretranslator(&self) -> (Message, *mut c_void) {
        (self.message, self.handle)
    }
}
impl Drop for Preview {
    fn drop(&mut self) {
        unsafe {
            (self.destroy)(self.handle);
            FreeLibrary(self.module);
        }
    }
}
