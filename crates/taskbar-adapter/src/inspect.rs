use crate::{class_name, read_memory, wide, xaml};
use litetray_adapter_protocol::{
    HostLayout, InspectResponse, SymbolRole, TaskbarInspection, VERSION,
};
use std::{ffi::c_void, mem::size_of, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::*,
    System::{LibraryLoader::*, Memory::*, Threading::*},
    UI::WindowsAndMessaging::*,
};

/// All entry addresses are validated before any undocumented function is called.
pub(crate) unsafe fn validate_loaded(layout: &HostLayout) -> Result<usize, String> {
    unsafe {
        layout.validate()?;
        let module = GetModuleHandleW(wide("taskbar.dll").as_ptr()) as usize;
        if module == 0 {
            return Err("taskbar.dll 尚未加载".into());
        }
        let loaded = crate::loaded_identity(module).ok_or("无法读取已加载模块的完整 PDB 身份")?;
        let expected = serde_json::to_value(&layout.module).map_err(|e| e.to_string())?;
        for field in ["machine", "timestamp", "image_size", "pdb_guid", "pdb_age"] {
            if loaded[field] != expected[field] {
                return Err(format!("已加载 taskbar.dll 与符号不匹配：{field}"));
            }
        }
        for symbol in &layout.symbols {
            let address = module
                .checked_add(symbol.rva as usize)
                .ok_or("模块地址溢出")?;
            check_region(
                address,
                if symbol.role.executable() { 16 } else { 8 },
                module,
                symbol.role.executable(),
            )?;
            if symbol.role.executable() {
                let bytes = read_memory(address, 16).ok_or("无法读取函数入口")?;
                if bytes != symbol.prologue {
                    return Err(format!("{:?} 入口已变化或被其他模块改写", symbol.role));
                }
            }
        }
        Ok(module)
    }
}
pub(crate) unsafe fn check_region(
    address: usize,
    length: usize,
    module: usize,
    executable: bool,
) -> Result<(), String> {
    unsafe {
        let mut info: MEMORY_BASIC_INFORMATION = std::mem::zeroed();
        if VirtualQuery(
            address as *const _,
            &mut info,
            size_of::<MEMORY_BASIC_INFORMATION>(),
        ) == 0
        {
            return Err("无法检查内存区域".into());
        }
        let end = address.checked_add(length).ok_or("地址溢出")?;
        if info.State != MEM_COMMIT
            || info.Protect & (PAGE_GUARD | PAGE_NOACCESS) != 0
            || end > (info.BaseAddress as usize).saturating_add(info.RegionSize)
        {
            return Err("目标内存不可访问".into());
        }
        if module != 0 && info.AllocationBase as usize != module {
            return Err("目标地址不属于已校验模块".into());
        }
        let exec = info.Protect
            & (PAGE_EXECUTE | PAGE_EXECUTE_READ | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY)
            != 0;
        if executable && !exec {
            return Err("函数入口不可执行".into());
        }
        Ok(())
    }
}
pub(crate) unsafe fn pointer(address: usize) -> Result<usize, String> {
    unsafe {
        let bytes = read_memory(address, size_of::<usize>()).ok_or("无法读取原生对象指针")?;
        Ok(usize::from_ne_bytes(
            bytes.try_into().map_err(|_| "对象指针尺寸无效")?,
        ))
    }
}
type GetHost = unsafe extern "system" fn(*mut c_void, *mut [usize; 2]) -> *mut c_void;
type ReleaseHost = unsafe extern "system" fn(*mut c_void);
pub(crate) struct SharedHost {
    pub(crate) value: [usize; 2],
    release: ReleaseHost,
}
impl Drop for SharedHost {
    fn drop(&mut self) {
        if self.value[1] != 0 {
            unsafe {
                (self.release)(self.value[1] as *mut _);
            }
        }
    }
}
pub(crate) unsafe fn host(
    hwnd: HWND,
    secondary: bool,
    module: usize,
    layout: &HostLayout,
) -> Result<SharedHost, String> {
    unsafe {
        let task_window = if secondary {
            FindWindowExW(hwnd, null_mut(), wide("WorkerW").as_ptr(), std::ptr::null())
        } else {
            GetPropW(hwnd, wide("TaskbandHWND").as_ptr())
        };
        if task_window.is_null() {
            return Err("未找到原生 TaskBand 窗口".into());
        }
        let object = GetWindowLongPtrW(task_window, 0) as usize;
        if object == 0 {
            return Err("TaskBand 对象未就绪".into());
        }
        let site = module
            + layout
                .symbol(if secondary {
                    SymbolRole::SecondarySite
                } else {
                    SymbolRole::PrimarySite
                })
                .rva as usize;
        let mut matched = None;
        for i in 0..=20 {
            let slot = object
                .checked_add(i * size_of::<usize>())
                .ok_or("对象地址溢出")?;
            if pointer(slot)? == site {
                matched = Some(slot);
                break;
            }
        }
        let matched = matched.ok_or("TaskBand 接口布局不匹配")?;
        let get: GetHost = std::mem::transmute(
            module
                + layout
                    .symbol(if secondary {
                        SymbolRole::SecondaryHost
                    } else {
                        SymbolRole::PrimaryHost
                    })
                    .rva as usize,
        );
        let release: ReleaseHost =
            std::mem::transmute(module + layout.symbol(SymbolRole::ReleaseHost).rva as usize);
        let mut shared = SharedHost {
            value: [0; 2],
            release,
        };
        get(matched as *mut _, &mut shared.value);
        if shared.value[0] == 0 || shared.value[1] == 0 {
            return Err("TaskbarHost 不可用".into());
        }
        Ok(shared)
    }
}
unsafe fn content(
    hwnd: HWND,
    secondary: bool,
    module: usize,
    layout: &HostLayout,
) -> Result<xaml::Object, String> {
    unsafe {
        let shared = host(hwnd, secondary, module, layout)?;
        let element = pointer(
            shared.value[0]
                .checked_add(layout.element_offset as usize)
                .ok_or("宿主字段地址溢出")?,
        )?;
        check_region(element, size_of::<usize>(), 0, false)?;
        let vtable = pointer(element)?;
        check_region(vtable, 3 * size_of::<usize>(), 0, false)?;
        let qi = pointer(vtable)?;
        check_region(qi, 1, 0, true)?;
        let element = xaml::Object::query_borrowed(
            element as *mut _,
            &crate::xaml_abi::IID_IFrameworkElement,
        )?;
        let root = xaml::root_content(&element)?;
        drop(shared);
        Ok(root)
    }
}
pub unsafe fn run(layout: &HostLayout, nonce: u64) -> InspectResponse {
    unsafe {
        let mut response = InspectResponse {
            version: VERSION,
            nonce,
            process_id: GetCurrentProcessId(),
            thread_id: GetCurrentThreadId(),
            changes_applied: false,
            taskbars: vec![],
            error: None,
            tray_input: Some(crate::tray_interaction::diagnostics()),
        };
        let module = match validate_loaded(layout) {
            Ok(module) => module,
            Err(error) => {
                response.error = Some(error);
                return response;
            }
        };
        let mut windows = Vec::<HWND>::new();
        EnumThreadWindows(
            GetCurrentThreadId(),
            Some(collect),
            &mut windows as *mut _ as isize,
        );
        for hwnd in windows {
            let secondary = class_name(hwnd) == "Shell_SecondaryTrayWnd";
            let mut report = TaskbarInspection {
                hwnd: hwnd as u64,
                secondary,
                elements: vec![],
                time_elements: vec![],
                date_elements: vec![],
                truncated: false,
                error: None,
                topology: None,
                geometry: vec![],
                geometry_error: None,
            };
            match content(hwnd, secondary, module, layout).and_then(|root| {
                report.topology = Some(match crate::discovery::View::discover(&root) {
                    Ok(view) => view.report,
                    Err(error) => litetray_adapter_protocol::topology::Report {
                        elements: vec![],
                        selection: None,
                        error: Some(error),
                    },
                });
                let binding = crate::taskbar_source::resolve(hwnd, layout).and_then(|source| {
                    crate::island::Binding::capture(hwnd, &source.source, &root)
                });
                if let Err(error) = &binding {
                    report.geometry_error = Some(error.clone());
                }
                let model_property = crate::tray::Field::DataContext.property()?;
                let mut index = 0;
                let mut tray_depth = None;
                xaml::walk(root, |node, info| {
                    if tray_depth.is_some_and(|depth| info.depth <= depth) {
                        tray_depth = None;
                    }
                    if info.class_name == "SystemTray.SystemTrayFrame" {
                        tray_depth = Some(info.depth);
                    }
                    if let Ok(binding) = &binding
                        && tray_depth.is_some()
                        && let Ok(bounds) = crate::input::bounds(node, binding.window())
                    {
                        let model_identity = crate::tray::read(node, &model_property)?
                            .map(|model| {
                                model
                                    .query(&windows_sys::core::IID_IUnknown)
                                    .map(|value| value.raw() as u64)
                            })
                            .transpose()?;
                        report
                            .geometry
                            .push(litetray_adapter_protocol::ElementGeometry {
                                index,
                                bounds,
                                model_identity,
                                hit_test: node
                                    .try_query(&crate::xaml_abi::IID_IUIElement)?
                                    .and_then(|ui| {
                                        let mut value = false;
                                        ((ui.table::<crate::xaml_abi::IUIElement_Vtbl>()
                                            .IsHitTestVisible)(
                                            ui.raw(), &mut value
                                        ) >= 0)
                                            .then_some(value)
                                    }),
                                enabled: node.try_query(&crate::xaml_abi::IID_IControl)?.and_then(
                                    |control| {
                                        let mut value = false;
                                        ((control
                                            .table::<crate::xaml_abi::IControl_Vtbl>()
                                            .IsEnabled)(
                                            control.raw(), &mut value
                                        ) >= 0)
                                            .then_some(value)
                                    },
                                ),
                                opacity: node
                                    .try_query(&crate::xaml_abi::IID_IUIElement)?
                                    .and_then(|ui| {
                                        let mut value = 0.;
                                        ((ui.table::<crate::xaml_abi::IUIElement_Vtbl>().Opacity)(
                                            ui.raw(),
                                            &mut value,
                                        ) >= 0)
                                            .then_some(value)
                                    }),
                            });
                    }
                    index += 1;
                    Ok(())
                })
            }) {
                Ok(tree) => {
                    report.truncated = tree.truncated;
                    for element in &tree.elements {
                        if element.name == "TimeInnerTextBlock" {
                            report.time_elements.push(element.name.clone());
                        }
                        if element.name == "DateInnerTextBlock" {
                            report.date_elements.push(element.name.clone());
                        }
                    }
                    report.elements = tree.elements;
                }
                Err(error) => report.error = Some(error),
            }
            response.taskbars.push(report);
        }
        response
    }
}
unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> i32 {
    unsafe {
        if crate::is_taskbar(hwnd) {
            let windows = &mut *(data as *mut Vec<HWND>);
            if windows.len() >= 16 {
                return 0;
            }
            windows.push(hwnd);
        }
        1
    }
}
