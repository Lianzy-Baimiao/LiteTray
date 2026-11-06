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
        "Windows.UI.Xaml.IXamlRoot",
        "Windows.UI.Xaml.IDependencyObject",
        "Windows.UI.Xaml.IDependencyPropertyStatics",
        "Windows.UI.Xaml.Data.IBindingExpression",
        "Windows.UI.Xaml.Data.IBindingExpressionBase",
        "Windows.UI.Xaml.Media.IFontFamilyFactory",
        "Windows.UI.Xaml.Hosting.IWindowsXamlManagerStatics",
        "Windows.Foundation.IPropertyValueStatics",
        "Windows.Foundation.IPropertyValue",
        "Windows.Foundation.PropertyType",
        "Windows.UI.Xaml.Markup.IXamlReaderStatics",
        "Windows.UI.Xaml.Media.IVisualTreeHelperStatics",
        "Windows.UI.Xaml.Controls.ITextBlock",
        "Windows.UI.Xaml.Controls.ITextBlockStatics",
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
    let patches=[
        ("    XamlRoot: usize,".to_owned(),format!("    pub XamlRoot: {getter},")),
        ("    Content: usize,".to_owned(),format!("    pub Content: {getter},")),
        ("    Parent: usize,".to_owned(),format!("    pub Parent: {getter},")),
        ("    TextProperty: usize,".to_owned(),format!("    pub TextProperty: {getter},")),
        ("    WidthProperty: usize,".to_owned(),format!("    pub WidthProperty: {getter},")),
        ("    FontSizeProperty: usize,".to_owned(),format!("    pub FontSizeProperty: {getter},")),
        ("    FontFamilyProperty: usize,".to_owned(),format!("    pub FontFamilyProperty: {getter},")),
        ("    ParentBinding: usize,".to_owned(),format!("    pub ParentBinding: {getter},")),
        ("    InitializeForCurrentThread: usize,".to_owned(),format!("    pub InitializeForCurrentThread: {getter},")),
        ("    CreateInstanceWithName: usize,".to_owned(),"    pub CreateInstanceWithName: unsafe extern \"system\" fn(*mut core::ffi::c_void, windows_sys::core::HSTRING, *mut core::ffi::c_void, *mut *mut core::ffi::c_void, *mut *mut core::ffi::c_void) -> windows_sys::core::HRESULT,".into()),
        ("    GetChild: usize,".to_owned(),"    pub GetChild: unsafe extern \"system\" fn(*mut core::ffi::c_void, *mut core::ffi::c_void, i32, *mut *mut core::ffi::c_void) -> windows_sys::core::HRESULT,".into()),
        ("    GetChildrenCount: usize,".to_owned(),"    pub GetChildrenCount: unsafe extern \"system\" fn(*mut core::ffi::c_void, *mut core::ffi::c_void, *mut i32) -> windows_sys::core::HRESULT,".into()),
        ("    GetParent: usize,".to_owned(),"    pub GetParent: unsafe extern \"system\" fn(*mut core::ffi::c_void, *mut core::ffi::c_void, *mut *mut core::ffi::c_void) -> windows_sys::core::HRESULT,".into()),
    ];
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
