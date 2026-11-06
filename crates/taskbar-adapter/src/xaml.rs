//! Thread-affine COM ownership and bounded XAML traversal. No COM reference
//! escapes its UI thread. The inspection path never sets properties.
use crate::xaml_abi::*;
use litetray_adapter_protocol::ElementInfo;
use std::{
    ffi::c_void,
    marker::PhantomData,
    ptr::{NonNull, null_mut},
    rc::Rc,
};
use windows_sys::core::{GUID, HSTRING, IID_IInspectable, IInspectable_Vtbl, IUnknown_Vtbl};
windows_link::link!("combase.dll" "system" fn WindowsCreateString(source:*const u16,length:u32,value:*mut HSTRING)->i32);
windows_link::link!("combase.dll" "system" fn WindowsDeleteString(value:HSTRING)->i32);
windows_link::link!("combase.dll" "system" fn WindowsGetStringRawBuffer(value:HSTRING,length:*mut u32)->*const u16);
windows_link::link!("combase.dll" "system" fn RoGetActivationFactory(name:HSTRING,iid:*const GUID,value:*mut *mut c_void)->i32);

pub struct Object {
    pointer: NonNull<c_void>,
    _thread: PhantomData<Rc<()>>,
}
impl Clone for Object {
    fn clone(&self) -> Self {
        // COM identity (QI(IUnknown)) need not have the same pointer/vtable as
        // this interface. Preserve the exact ABI pointer when retaining it.
        unsafe {
            (self.table::<IUnknown_Vtbl>().AddRef)(self.raw());
        }
        Self {
            pointer: self.pointer,
            _thread: PhantomData,
        }
    }
}
impl Object {
    /// The pointer must own one valid COM reference on this thread.
    pub unsafe fn owned(pointer: *mut c_void) -> Result<Self, String> {
        Ok(Self {
            pointer: NonNull::new(pointer).ok_or("XAML 接口为空")?,
            _thread: PhantomData,
        })
    }
    pub fn raw(&self) -> *mut c_void {
        self.pointer.as_ptr()
    }
    pub unsafe fn table<T>(&self) -> &T {
        unsafe { &**(self.raw() as *const *const T) }
    }
    pub fn query(&self, iid: &GUID) -> Result<Self, String> {
        self.try_query(iid)?.ok_or("XAML 接口不受支持".into())
    }
    /// Only E_NOINTERFACE means an absent capability. A disconnected or
    /// otherwise failing COM object must not be treated as a safe model.
    pub fn try_query(&self, iid: &GUID) -> Result<Option<Self>, String> {
        unsafe {
            let mut result = null_mut();
            let hr = (self.table::<IUnknown_Vtbl>().QueryInterface)(self.raw(), iid, &mut result);
            if hr == 0x80004002u32 as i32 {
                return Ok(None);
            }
            check(hr, "QueryInterface")?;
            Ok(Some(Self::owned(result)?))
        }
    }
    /// Borrow a trusted live COM object long enough to obtain our own interface.
    pub unsafe fn query_borrowed(pointer: *mut c_void, iid: &GUID) -> Result<Self, String> {
        unsafe {
            if pointer.is_null() {
                return Err("XAML 对象为空".into());
            }
            let table = &**(pointer as *const *const IUnknown_Vtbl);
            let mut result = null_mut();
            check(
                (table.QueryInterface)(pointer, iid, &mut result),
                "QueryInterface borrowed",
            )?;
            Self::owned(result)
        }
    }
    pub fn class_name(&self) -> Result<String, String> {
        unsafe {
            let inspectable = self.query(&IID_IInspectable)?;
            let mut value = null_mut();
            check(
                (inspectable.table::<IInspectable_Vtbl>().GetRuntimeClassName)(
                    inspectable.raw(),
                    &mut value,
                ),
                "GetRuntimeClassName",
            )?;
            string_owned(value, 256)
        }
    }
    pub(crate) fn string(
        &self,
        get: unsafe extern "system" fn(*mut c_void, *mut HSTRING) -> i32,
        limit: usize,
    ) -> Result<String, String> {
        unsafe {
            let mut value = null_mut();
            check(get(self.raw(), &mut value), "XAML string property")?;
            string_owned(value, limit)
        }
    }
    /// Compare full HSTRING values for ownership; display-oriented bounded
    /// extraction must not treat different long strings as the same value.
    pub(crate) fn same_string(&self, other: &Object) -> Result<bool, String> {
        unsafe {
            let get = |object: &Object| -> Result<StringHandle, String> {
                let mut value = null_mut();
                check(
                    (object.table::<IPropertyValue_Vtbl>().GetString)(object.raw(), &mut value),
                    "PropertyValue.String equality",
                )?;
                Ok(StringHandle(value))
            };
            let a = get(self)?;
            let b = get(other)?;
            let (mut alen, mut blen) = (0, 0);
            let aptr = WindowsGetStringRawBuffer(a.0, &mut alen);
            let bptr = WindowsGetStringRawBuffer(b.0, &mut blen);
            if alen != blen {
                return Ok(false);
            }
            if alen == 0 {
                return Ok(true);
            }
            if aptr.is_null() || bptr.is_null() {
                return Err("XAML 字符串无效".into());
            }
            Ok(std::slice::from_raw_parts(aptr, alen as usize)
                == std::slice::from_raw_parts(bptr, blen as usize))
        }
    }
}
impl Drop for Object {
    fn drop(&mut self) {
        unsafe {
            (self.table::<IUnknown_Vtbl>().Release)(self.raw());
        }
    }
}
pub fn check(hr: i32, context: &str) -> Result<(), String> {
    if hr < 0 {
        Err(format!("{context}: HRESULT 0x{:08X}", hr as u32))
    } else {
        Ok(())
    }
}
pub(crate) struct StringHandle(pub HSTRING);
impl StringHandle {
    pub fn new(text: &str) -> Result<Self, String> {
        let chars = text.encode_utf16().collect::<Vec<_>>();
        let mut value = null_mut();
        unsafe {
            check(
                WindowsCreateString(chars.as_ptr(), chars.len() as u32, &mut value),
                "WindowsCreateString",
            )?;
        }
        Ok(Self(value))
    }
}
impl Drop for StringHandle {
    fn drop(&mut self) {
        unsafe {
            WindowsDeleteString(self.0);
        }
    }
}
unsafe fn string_owned(value: HSTRING, limit: usize) -> Result<String, String> {
    unsafe {
        let value = StringHandle(value);
        let mut length = 0;
        let ptr = WindowsGetStringRawBuffer(value.0, &mut length);
        if length == 0 {
            return Ok(String::new());
        }
        if ptr.is_null() {
            return Err("XAML 字符串无效".into());
        }
        Ok(String::from_utf16_lossy(std::slice::from_raw_parts(
            ptr,
            (length as usize).min(limit),
        )))
    }
}
pub(crate) fn factory(name: &str, iid: &GUID) -> Result<Object, String> {
    unsafe {
        let chars = name.encode_utf16().collect::<Vec<_>>();
        let mut value = null_mut();
        check(
            WindowsCreateString(chars.as_ptr(), chars.len() as u32, &mut value),
            "WindowsCreateString",
        )?;
        let value = StringHandle(value);
        let mut factory = null_mut();
        check(
            RoGetActivationFactory(value.0, iid, &mut factory),
            "RoGetActivationFactory",
        )?;
        Object::owned(factory)
    }
}
pub(crate) fn root(element: &Object) -> Result<Object, String> {
    unsafe {
        let ui = element.query(&IID_IUIElement10)?;
        let mut root = null_mut();
        check(
            (ui.table::<IUIElement10_Vtbl>().XamlRoot)(ui.raw(), &mut root),
            "UIElement.XamlRoot",
        )?;
        Object::owned(root)?.query(&IID_IXamlRoot)
    }
}
pub fn root_content(element: &Object) -> Result<Object, String> {
    unsafe {
        let root = root(element)?;
        let mut content = null_mut();
        check(
            (root.table::<IXamlRoot_Vtbl>().Content)(root.raw(), &mut content),
            "XamlRoot.Content",
        )?;
        Object::owned(content)?.query(&IID_IDependencyObject)
    }
}
pub struct Tree {
    pub elements: Vec<ElementInfo>,
    pub truncated: bool,
}
pub(crate) fn visibility(node: &Object) -> Result<Option<bool>, String> {
    let Some(ui) = node.try_query(&IID_IUIElement)? else {
        return Ok(None);
    };
    unsafe {
        let mut value = Visibility::Collapsed;
        if (ui.table::<IUIElement_Vtbl>().Visibility)(ui.raw(), &mut value) < 0 {
            return Ok(None);
        }
        Ok(match value.0 {
            0 => Some(true),
            1 => Some(false),
            _ => None,
        })
    }
}
pub fn inspect(root: Object) -> Result<Tree, String> {
    walk(root, |_, _| Ok(()))
}
/// The visitor borrows each object on its owning UI thread. It must acquire
/// its own COM reference if retaining a target beyond this call.
pub(crate) fn walk(
    root: Object,
    mut visit: impl FnMut(&Object, &ElementInfo) -> Result<(), String>,
) -> Result<Tree, String> {
    unsafe {
        let helper = factory(
            "Windows.UI.Xaml.Media.VisualTreeHelper",
            &IID_IVisualTreeHelperStatics,
        )?;
        let table = helper.table::<IVisualTreeHelperStatics_Vtbl>();
        let mut pending = vec![(root, 0u32, false)];
        let mut elements = Vec::new();
        let mut truncated = false;
        while let Some((node, depth, in_tray)) = pending.pop() {
            if elements.len() >= 320 || depth > 40 {
                truncated = true;
                break;
            }
            let class_name = node.class_name().unwrap_or_default();
            let framework = node.query(&IID_IFrameworkElement).ok();
            let mut name = String::new();
            let mut width = None;
            let mut height = None;
            if let Some(fe) = &framework {
                let fe_table = fe.table::<IFrameworkElement_Vtbl>();
                name = fe.string(fe_table.Name, 128).unwrap_or_default();
                let mut value = 0.;
                if (fe_table.ActualWidth)(fe.raw(), &mut value) >= 0 && value.is_finite() {
                    width = Some(value);
                }
                if (fe_table.ActualHeight)(fe.raw(), &mut value) >= 0 && value.is_finite() {
                    height = Some(value);
                }
            }
            let in_tray =
                in_tray || class_name.contains("SystemTray") || name.contains("SystemTray");
            let text = if in_tray {
                node.query(&IID_ITextBlock)
                    .ok()
                    .and_then(|text| text.string(text.table::<ITextBlock_Vtbl>().Text, 256).ok())
            } else {
                None
            };
            let visible = visibility(&node)?;
            let info = ElementInfo {
                name: name.clone(),
                class_name: class_name.clone(),
                depth,
                width,
                height,
                text,
                visible,
            };
            visit(&node, &info)?;
            elements.push(info);
            // Application task buttons are unrelated to the tray. Avoid collecting
            // app titles and do not let their large trees consume the inspection budget.
            if !in_tray && (name == "TaskbarFrameRepeater" || class_name.contains("TaskList")) {
                continue;
            }
            let mut count = 0;
            check(
                (table.GetChildrenCount)(helper.raw(), node.raw(), &mut count),
                "VisualTreeHelper.GetChildrenCount",
            )?;
            if !(0..=512).contains(&count) {
                return Err("XAML 子元素数量异常".into());
            }
            for index in (0..count).rev() {
                if pending.len() + elements.len() >= 640 {
                    truncated = true;
                    break;
                }
                let mut child = null_mut();
                check(
                    (table.GetChild)(helper.raw(), node.raw(), index, &mut child),
                    "VisualTreeHelper.GetChild",
                )?;
                pending.push((
                    Object::owned(child)?.query(&IID_IDependencyObject)?,
                    depth + 1,
                    in_tray,
                ));
            }
        }
        Ok(Tree {
            elements,
            truncated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};
    #[test]
    fn projected_abi_slots_match_windows_metadata() {
        let slot = size_of::<usize>();
        assert_eq!(size_of::<IInspectable_Vtbl>(), 6 * slot);
        assert_eq!(offset_of!(IFrameworkElement_Vtbl, Name), 33 * slot);
        assert_eq!(offset_of!(IUIElement10_Vtbl, XamlRoot), 8 * slot);
        assert_eq!(offset_of!(IXamlRoot_Vtbl, Content), 6 * slot);
        assert_eq!(
            offset_of!(IVisualTreeHelperStatics_Vtbl, GetChild),
            10 * slot
        );
        assert_eq!(
            offset_of!(IVisualTreeHelperStatics_Vtbl, GetChildrenCount),
            11 * slot
        );
        assert_eq!(offset_of!(ITextBlock_Vtbl, Text), 26 * slot);
        assert_eq!(offset_of!(ITextBlockStatics_Vtbl, TextProperty), 16 * slot);
        assert_eq!(
            offset_of!(IFrameworkElementStatics_Vtbl, DataContextProperty),
            20 * slot
        );
        assert_eq!(offset_of!(IItemsControl_Vtbl, ItemsSource), 6 * slot);
        assert_eq!(offset_of!(IItemsControl_Vtbl, Items), 8 * slot);
        assert_eq!(
            offset_of!(IItemsControlStatics_Vtbl, ItemsSourceProperty),
            6 * slot
        );
        assert_eq!(offset_of!(IContentControl_Vtbl, Content), 6 * slot);
        assert_eq!(
            offset_of!(IContentControlStatics_Vtbl, ContentProperty),
            6 * slot
        );
    }
}
