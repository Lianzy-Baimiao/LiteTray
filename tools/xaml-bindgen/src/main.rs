fn main() {
    let output = std::env::args().nth(1).expect("Output .rs file required");
    let metadata = std::env::var("LITETRAY_XAML_METADATA")
        .unwrap_or_else(|_| "C:/Windows/System32/WinMetadata/Windows.UI.Xaml.winmd".into());
    let warnings = windows_bindgen::bindgen([
        "--in",
        "default",
        &metadata,
        "--out",
        &output,
        "--flat",
        "--sys",
        "--filter",
        "Windows.UI.Xaml.IFrameworkElement",
        "Windows.UI.Xaml.IFrameworkElement2",
        "Windows.UI.Xaml.IFrameworkElementStatics",
        "Windows.UI.Xaml.IUIElement10",
        "Windows.UI.Xaml.IUIElement",
        "Windows.UI.Xaml.IUIElementStatics",
        "Windows.UI.Xaml.Controls.IControl",
        "Windows.UI.Xaml.Controls.IPanel",
        "Windows.UI.Xaml.Controls.IPanelStatics",
        "Windows.UI.Xaml.FocusState",
        "Windows.UI.Xaml.Visibility",
        "Windows.UI.Xaml.Input.ITappedRoutedEventArgs",
        "Windows.UI.Xaml.Input.IKeyRoutedEventArgs",
        "Windows.UI.Xaml.Input.IPointerRoutedEventArgs",
        "Windows.UI.Input.IPointerPoint",
        "Windows.UI.Input.IPointerPointProperties",
        "Windows.UI.Input.PointerUpdateKind",
        "Windows.UI.Xaml.Hosting.IDesktopWindowXamlSource",
        "Windows.UI.Xaml.Media.IGeneralTransform",
        "Windows.Foundation.Point",
        "Windows.Foundation.Rect",
        "Windows.Foundation.Size",
        "Windows.System.VirtualKey",
        "Windows.UI.Core.CorePhysicalKeyStatus",
        "Windows.UI.Xaml.IXamlRoot",
        "Windows.UI.Xaml.IDependencyObject",
        "Windows.UI.Xaml.IDependencyObject2",
        "Windows.UI.Xaml.IDependencyProperty",
        "Windows.UI.Xaml.IDependencyPropertyStatics",
        "Windows.UI.Xaml.Data.IBindingExpression",
        "Windows.UI.Xaml.Data.IBindingExpressionBase",
        "Windows.UI.Xaml.Media.IFontFamilyFactory",
        "Windows.UI.Xaml.Media.IFontFamily",
        "Windows.UI.Xaml.Hosting.IWindowsXamlManagerStatics",
        "Windows.Foundation.IPropertyValueStatics",
        "Windows.Foundation.IPropertyValue",
        "Windows.Foundation.PropertyType",
        "Windows.UI.Xaml.Markup.IXamlReaderStatics",
        "Windows.UI.Xaml.Media.IVisualTreeHelperStatics",
        "Windows.UI.Xaml.Controls.ITextBlock",
        "Windows.UI.Xaml.Controls.ITextBlockStatics",
        "Windows.UI.Xaml.Controls.IToolTipServiceStatics",
        "Windows.UI.Xaml.TextAlignment",
        "Windows.UI.Xaml.Media.ISolidColorBrush",
        "Windows.UI.Color",
        "Windows.UI.Xaml.Controls.IItemsControl",
        "Windows.UI.Xaml.Controls.IItemsControlStatics",
        "Windows.UI.Xaml.Controls.IContentControl",
        "Windows.UI.Xaml.Controls.IContentControlStatics",
        "Windows.UI.Xaml.Controls.IMenuFlyout",
        "Windows.UI.Xaml.Controls.Primitives.IFlyoutBase",
        "Windows.UI.Xaml.Controls.Primitives.IFlyoutBase5",
        "Windows.UI.Xaml.Controls.Primitives.IFlyoutBase6",
    ]);
    if std::env::args().any(|a| a == "--raw") {
        return;
    }
    // Deliberately omit unrelated class projections. bindgen preserves their
    // ABI slots as usize. Only these WinRT reference-type methods are needed;
    // expose their raw ABI with object references represented as void pointers.
    // No slot is added or removed by the substitutions below.
    let mut source = std::fs::read_to_string(&output).unwrap();
    let getter = "unsafe extern \"system\" fn(*mut core::ffi::c_void, *mut *mut core::ffi::c_void) -> windows_sys::core::HRESULT";
    for (method, signature) in [
        (
            "GetCurrentPoint",
            "*mut core::ffi::c_void, *mut core::ffi::c_void, *mut *mut core::ffi::c_void",
        ),
        (
            "AddHandler",
            "*mut core::ffi::c_void, *mut core::ffi::c_void, *mut core::ffi::c_void, bool",
        ),
        (
            "RemoveHandler",
            "*mut core::ffi::c_void, *mut core::ffi::c_void, *mut core::ffi::c_void",
        ),
        (
            "TappedEvent",
            "*mut core::ffi::c_void, *mut *mut core::ffi::c_void",
        ),
        (
            "RegisterPropertyChangedCallback",
            "*mut core::ffi::c_void, *mut core::ffi::c_void, *mut core::ffi::c_void, *mut i64",
        ),
        (
            "UnregisterPropertyChangedCallback",
            "*mut core::ffi::c_void, *mut core::ffi::c_void, i64",
        ),
    ] {
        let before = format!("    {method}: usize,");
        assert_eq!(source.matches(&before).count(), 1);
        source = source.replace(&before, &format!("    pub {method}: unsafe extern \"system\" fn({signature}) -> windows_sys::core::HRESULT,"));
    }
    let patches=[
        ("    Parent: usize,".to_owned(),format!("    pub Parent: {getter},")),
        ("    TextProperty: usize,".to_owned(),format!("    pub TextProperty: {getter},")),
        ("    WidthProperty: usize,".to_owned(),format!("    pub WidthProperty: {getter},")),
        ("    FontSizeProperty: usize,".to_owned(),format!("    pub FontSizeProperty: {getter},")),
        ("    FontFamilyProperty: usize,".to_owned(),format!("    pub FontFamilyProperty: {getter},")),
        ("    ForegroundProperty: usize,".to_owned(),format!("    pub ForegroundProperty: {getter},")),
        ("    TextAlignmentProperty: usize,".to_owned(),format!("    pub TextAlignmentProperty: {getter},")),
        ("    ToolTipProperty: usize,".to_owned(),format!("    pub ToolTipProperty: {getter},")),
        ("    ParentBinding: usize,".to_owned(),format!("    pub ParentBinding: {getter},")),
        ("    DataContextProperty: usize,".to_owned(),format!("    pub DataContextProperty: {getter},")),
        ("    ItemsSourceProperty: usize,".to_owned(),format!("    pub ItemsSourceProperty: {getter},")),
        ("    ContentProperty: usize,".to_owned(),format!("    pub ContentProperty: {getter},")),
        ("    VisibilityProperty: usize,".to_owned(),format!("    pub VisibilityProperty: {getter},")),
        ("    InitializeForCurrentThread: usize,".to_owned(),format!("    pub InitializeForCurrentThread: {getter},")),
        ("    CreateInstanceWithName: usize,".to_owned(),"    pub CreateInstanceWithName: unsafe extern \"system\" fn(*mut core::ffi::c_void, windows_sys::core::HSTRING, *mut core::ffi::c_void, *mut *mut core::ffi::c_void, *mut *mut core::ffi::c_void) -> windows_sys::core::HRESULT,".into()),
        ("    GetChild: usize,".to_owned(),"    pub GetChild: unsafe extern \"system\" fn(*mut core::ffi::c_void, *mut core::ffi::c_void, i32, *mut *mut core::ffi::c_void) -> windows_sys::core::HRESULT,".into()),
        ("    GetChildrenCount: usize,".to_owned(),"    pub GetChildrenCount: unsafe extern \"system\" fn(*mut core::ffi::c_void, *mut core::ffi::c_void, *mut i32) -> windows_sys::core::HRESULT,".into()),
        ("    GetParent: usize,".to_owned(),"    pub GetParent: unsafe extern \"system\" fn(*mut core::ffi::c_void, *mut core::ffi::c_void, *mut *mut core::ffi::c_void) -> windows_sys::core::HRESULT,".into()),
    ];
    // These getters occur on UIElement/FlyoutBase and ItemsControl/MenuFlyout.
    for method in ["XamlRoot", "Items"] {
        source = source.replace(
            &format!("    {method}: usize,"),
            &format!("    pub {method}: {getter},"),
        );
    }
    for (method, signature) in [
        ("ShowAt", "*mut core::ffi::c_void, *mut core::ffi::c_void"),
        (
            "ShowAtWithOptions",
            "*mut core::ffi::c_void, *mut core::ffi::c_void, *mut core::ffi::c_void",
        ),
        (
            "SetXamlRoot",
            "*mut core::ffi::c_void, *mut core::ffi::c_void",
        ),
    ] {
        source = source.replace(&format!("    {method}: usize,"), &format!("    pub {method}: unsafe extern \"system\" fn({signature}) -> windows_sys::core::HRESULT,"));
    }
    for method in [
        "OpacityProperty",
        "IsHitTestVisibleProperty",
        "MinWidthProperty",
        "BackgroundProperty",
    ] {
        source = source.replace(
            &format!("    {method}: usize,"),
            &format!("    pub {method}: {getter},"),
        );
    }
    let start = source.find("pub struct IFlyoutBase5_Vtbl").unwrap();
    let end = start + source[start..].find("pub const IID_IFlyoutBase6").unwrap();
    let section = source[start..end].replace(
        "fn(*mut core::ffi::c_void, *mut core::ffi::c_void) -> windows_sys::core::HRESULT",
        "fn(*mut core::ffi::c_void, *mut core::ffi::c_void, *mut core::ffi::c_void) -> windows_sys::core::HRESULT",
    );
    source.replace_range(start..end, &section);
    // Object-valued ContentControl.Content is generated directly; these two
    // opaque UIElement slots belong to XamlRoot and DesktopWindowXamlSource.
    assert_eq!(source.matches("    Content: usize,").count(), 2);
    source = source.replace(
        "    Content: usize,",
        &format!("    pub Content: {getter},"),
    );
    for (before, after) in patches {
        assert_eq!(
            source.matches(&before).count(),
            1,
            "Metadata slot changed: {before}"
        );
        source = source.replace(&before, &after);
    }
    for method in [
        "GetValue",
        "ReadLocalValue",
        "GetAnimationBaseValue",
        "GetBindingExpression",
    ] {
        let before = format!("    {method}: usize,");
        assert_eq!(source.matches(&before).count(), 1);
        source=source.replace(&before,&format!("    pub {method}: unsafe extern \"system\" fn(*mut core::ffi::c_void, *mut core::ffi::c_void, *mut *mut core::ffi::c_void) -> windows_sys::core::HRESULT,"));
    }
    for (method, signature) in [
        (
            "SetContent",
            "*mut core::ffi::c_void, *mut core::ffi::c_void",
        ),
        (
            "SetValue",
            "*mut core::ffi::c_void, *mut core::ffi::c_void, *mut core::ffi::c_void",
        ),
        (
            "SetBinding",
            "*mut core::ffi::c_void, *mut core::ffi::c_void, *mut core::ffi::c_void",
        ),
        (
            "ClearValue",
            "*mut core::ffi::c_void, *mut core::ffi::c_void",
        ),
    ] {
        let before = format!("    {method}: usize,");
        assert_eq!(source.matches(&before).count(), 1);
        source=source.replace(&before,&format!("    pub {method}: unsafe extern \"system\" fn({signature}) -> windows_sys::core::HRESULT,"));
    }
    for method in ["Tapped", "KeyDown", "PointerPressed", "PointerWheelChanged"] {
        let before = format!("    {method}: usize,");
        assert_eq!(source.matches(&before).count(), 1);
        source = source.replace(&before, &format!("    pub {method}: unsafe extern \"system\" fn(*mut core::ffi::c_void, *mut core::ffi::c_void, *mut i64) -> windows_sys::core::HRESULT,"));
    }
    for method in ["Properties"] {
        let before = format!("    {method}: usize,");
        assert_eq!(source.matches(&before).count(), 1);
        source = source.replace(&before, &format!("    pub {method}: {getter},"));
    }
    let before = "    TransformToVisual: usize,";
    assert_eq!(source.matches(before).count(), 1);
    source = source.replace(before, "    pub TransformToVisual: unsafe extern \"system\" fn(*mut core::ffi::c_void, *mut core::ffi::c_void, *mut *mut core::ffi::c_void) -> windows_sys::core::HRESULT,");
    std::fs::write(&output, source).unwrap();
    let status = std::process::Command::new("rustfmt")
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());
    println!(
        "Generated selected XAML ABI; {} unrelated methods remain opaque",
        warnings.len()
    );
}
