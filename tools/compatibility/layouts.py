"""Reviewed, narrow x64 recipes. Unknown instruction shapes stay blocked."""
from __future__ import annotations
import struct

from capstone import Cs, CS_ARCH_X86, CS_MODE_64
from capstone.x86_const import X86_OP_IMM, X86_OP_MEM, X86_OP_REG

from evidence import EvidenceError, Image, require

HOST_SYMBOLS = (
    ("primary_site", "??_7CTaskBand@@6BITaskListWndSite@@@"),
    ("secondary_site", "??_7CSecondaryTaskBand@@6BITaskListWndSite@@@"),
    ("primary_host", "?GetTaskbarHost@CTaskBand@@UEBA?AV?$shared_ptr@VTaskbarHost@@@std@@XZ"),
    ("secondary_host", "?GetTaskbarHost@CSecondaryTaskBand@@UEBA?AV?$shared_ptr@VTaskbarHost@@@std@@XZ"),
    ("frame_height", "?FrameHeight@TaskbarHost@@QEBAHXZ"),
    ("release_host", "?_Decref@_Ref_count_base@std@@QEAAXXZ"),
)
HAS_FOCUS = "?HasFocus@TaskbarHost@@QEAA_NXZ"
CONSUME_FOCUS = "?HasFocus@?$consume_Windows_Internal_Shell_XamlExplorerHost_IXamlExplorerHostChildWindow@UIXamlExplorerHostChildWindow@XamlExplorerHost@Shell@Internal@Windows@winrt@@@impl@winrt@@QEBA@XZ"
CHILD_TABLE = "??_7?$produce@UXamlExplorerHostChildWindow@implementation@XamlExplorerHost@Shell@Internal@Windows@winrt@@UIXamlExplorerHostChildWindow@34567@@impl@winrt@@6B@"
FACTORY_TABLE = "??_7?$produce@UXamlExplorerHostChildWindow@factory_implementation@XamlExplorerHost@Shell@Internal@Windows@winrt@@UIXamlExplorerHostChildWindowFactory@34567@@impl@winrt@@6B@"
GET_CONTENT = "?get_XamlContent@?$produce@UXamlExplorerHostChildWindow@implementation@XamlExplorerHost@Shell@Internal@Windows@winrt@@UIXamlExplorerHostChildWindow@34567@@impl@winrt@@UEAAHPEAPEAX@Z"
IMPL_CONTENT = "?XamlContent@XamlExplorerHostChildWindow@implementation@XamlExplorerHost@Shell@Internal@Windows@winrt@@QEBA?AUIInspectable@Foundation@67@XZ"
FACTORY_CREATE = "?CreateInstance@?$produce@UXamlExplorerHostChildWindow@factory_implementation@XamlExplorerHost@Shell@Internal@Windows@winrt@@UIXamlExplorerHostChildWindowFactory@34567@@impl@winrt@@UEAAH_KPEAXPEAPEAX@Z"
RAW_GET_CONTENT = "winrt::impl::produce<winrt::Windows::Internal::Shell::XamlExplorerHost::implementation::XamlExplorerHostChildWindow,winrt::Windows::Internal::Shell::XamlExplorerHost::IXamlExplorerHostChildWindow>::get_XamlContent"
RAW_IMPL_CONTENT = "winrt::Windows::Internal::Shell::XamlExplorerHost::implementation::XamlExplorerHostChildWindow::XamlContent"
RAW_FACTORY_CREATE = "winrt::impl::produce<winrt::Windows::Internal::Shell::XamlExplorerHost::factory_implementation::XamlExplorerHostChildWindow,winrt::Windows::Internal::Shell::XamlExplorerHost::IXamlExplorerHostChildWindowFactory>::CreateInstance"
FACTORY_IMPLEMENTATION = "?CreateInstance@XamlExplorerHostChildWindow@factory_implementation@XamlExplorerHost@Shell@Internal@Windows@winrt@@QEAA@_KAEBUhstring@7@@Z"
CONTENT_CONSUMER = "?Content@?$consume_Windows_UI_Xaml_Hosting_IDesktopWindowXamlSource@UIDesktopWindowXamlSource@Hosting@Xaml@UI@Windows@winrt@@@impl@winrt@@QEBA@XZ"
TRAY_SYMBOLS = (
    ("invocation_point", "?GetInvocationPointRelativeToScreen@NotificationAreaIconsDataModel@implementation@SystemTray@winrt@@AEAA?AUPoint@Foundation@Windows@4@AEBU5674@@Z"),
    ("icon_bounds", "?GetIconBoundsRelativeToScreen@NotificationAreaIconsDataModel@implementation@SystemTray@winrt@@AEAA?AURect@Foundation@Windows@4@AEBU5674@@Z"),
    ("screen_rect", "?GetScreenRectFromXamlElement@@YA?AUtagRECT@@UFrameworkElement@Xaml@UI@Windows@winrt@@PEAUHWND__@@@Z"),
)
UNKNOWN_DESTRUCTOR = "??1IUnknown@Foundation@Windows@winrt@@QEAA@XZ"
UNKNOWN_RELEASE = "?unconditional_release_ref@IUnknown@Foundation@Windows@winrt@@AEAAXXZ"
GUARD_DISPATCH = "__guard_dispatch_icall_fptr"
GUARD_DISPATCH_FUNCTION = "_guard_dispatch_icall"
XFG_DISPATCH = "__guard_xfg_dispatch_icall_fptr"
RAW_UNKNOWN_EQUAL = "winrt::Windows::Foundation::operator=="
THROW_HR_DIAGNOSTIC = "?Throw_Hr@in1diag3@details@wil@@YAXPEAXIPEBDJ@Z"
TRANSFORM_TO_VISUAL = "?TransformToVisual@?$consume_Windows_UI_Xaml_IUIElement@UFrameworkElement@Xaml@UI@Windows@winrt@@@impl@winrt@@QEBA@AEBUUIElement@Xaml@UI@Windows@3@@Z"
BORROWED_SCREEN = "?ScreenRectForElement@DragDropManager@implementation@SystemTray@winrt@@AEAA?AUtagRECT@@AEBUFrameworkElement@Xaml@UI@Windows@4@PEAUHWND__@@@Z"
BORROWED_TARGETS = {
    "xaml_root": "?XamlRoot@?$consume_Windows_UI_Xaml_IUIElement10@UFrameworkElement@Xaml@UI@Windows@winrt@@@impl@winrt@@QEBA@XZ",
    "root_content": "?Content@?$consume_Windows_UI_Xaml_IXamlRoot@UIXamlRoot@Xaml@UI@Windows@winrt@@@impl@winrt@@QEBA@XZ",
    "transform": TRANSFORM_TO_VISUAL,
    "actual_width": "?ActualWidth@?$consume_Windows_UI_Xaml_IFrameworkElement@UIFrameworkElement@Xaml@UI@Windows@winrt@@@impl@winrt@@QEBA@XZ",
    "actual_height": "?ActualHeight@?$consume_Windows_UI_Xaml_IFrameworkElement@UIFrameworkElement@Xaml@UI@Windows@winrt@@@impl@winrt@@QEBA@XZ",
    "scale": "?RawPixelsPerViewPixel@?$consume_Windows_Graphics_Display_IDisplayInformation2@UDisplayInformation@Display@Graphics@Windows@winrt@@@impl@winrt@@QEBA@XZ",
    "display_factory": "??$call_factory_cast@P6A?AUDisplayInformation@Display@Graphics@Windows@winrt@@AEBUIDisplayInformationStatics@2345@@ZU12345@U62345@V<lambda_0a1437f4f9e4e85de4e92d9eedff3a11>@@@impl@winrt@@YA?A_P$$QEAV<lambda_0a1437f4f9e4e85de4e92d9eedff3a11>@@@Z",
    "throw_hresult": "?throw_hresult@winrt@@YAXUhresult@1@@Z",
    "release": UNKNOWN_RELEASE,
    "security_check": "__security_check_cookie",
    "security_cookie": "__security_cookie",
    "guard_dispatch": GUARD_DISPATCH,
}
BORROWED_CALLERS = {
    "?HitTestChevron@DragDropManager@implementation@SystemTray@winrt@@AEAA_NUtagPOINT@@@Z": 1,
    "?HitTestNotifyIcons@DragDropManager@implementation@SystemTray@winrt@@AEAAPEAUNotifyIconView@234@UtagPOINT@@@Z": 2,
    "?GetRelativeDropPosition@DragDropManager@implementation@SystemTray@winrt@@AEAA?AW4MoveTargetRelativePosition@@AEBUNotifyIconView@234@UtagPOINT@@@Z": 1,
}
OWNED_CALLER_PURPOSES = {
    "?ToggleEdgeCopilot@CopilotIcon@implementation@SystemTray@winrt@@AEAAXXZ": "copilot_button_geometry",
    "?OnPointerEntered@CopilotIcon@implementation@SystemTray@winrt@@QEAAXUPointerRoutedEventArgs@Input@Xaml@UI@Windows@4@@Z": "copilot_button_geometry",
    "?OnPointerExited@CopilotIcon@implementation@SystemTray@winrt@@QEAAXUPointerRoutedEventArgs@Input@Xaml@UI@Windows@4@@Z": "copilot_button_geometry",
    "?TryGetChevronScreenRect@NotificationAreaIconsDataModel@implementation@SystemTray@winrt@@AEAA?AV?$optional@UtagRECT@@@std@@XZ": "notification_area_bounds",
    "?TryGetIconScreenRect@NotificationAreaIconsDataModel@implementation@SystemTray@winrt@@AEAA?AV?$optional@UtagRECT@@@std@@XZ": "notification_area_bounds",
    "?OnScreenBoundsRequested@NotificationAreaIconsDataModel@implementation@SystemTray@winrt@@AEAAXAEBUIInspectable@Foundation@Windows@4@0@Z": "notification_area_bounds",
}
TRANSFORM_BOUNDS = "?TransformBounds@?$consume_Windows_UI_Xaml_Media_IGeneralTransform@UIGeneralTransform@Media@Xaml@UI@Windows@winrt@@@impl@winrt@@QEBA@AEBURect@Foundation@Windows@3@@Z"
DISPLAY_CURRENT_VIEW = "?GetForCurrentView@DisplayInformation@Display@Graphics@Windows@winrt@@SA@XZ"
AS_FRAMEWORK_ELEMENT = {
    kind: f"??$as@UFrameworkElement@Xaml@UI@Windows@winrt@@@?$com_ptr@U{kind}@implementation@SystemTray@winrt@@@winrt@@QEBA?A_PXZ"
    for kind in ("ChevronIconView", "NotifyIconView")
}
ICON_QUERY_INTERFACE = "?QueryInterface@?$root_implements@UIconView@implementation@SystemTray@winrt@@U134@UINotifyPropertyChanged@Data@Xaml@UI@Windows@4@Ucomposable@4@Ucomposing@4@UIControlOverrides@Controls@7894@UIControlOverrides6@Controls@7894@UIFrameworkElementOverrides@7894@UIFrameworkElementOverrides2@7894@UIUIElementOverrides@7894@UIUIElementOverrides7@7894@UIUIElementOverrides8@7894@UIUIElementOverrides9@7894@@impl@winrt@@QEAAHAEBUguid@3@PEAPEAX@Z"
ICON_QUERY_INNER = "?query_interface@?$root_implements@UIconView@implementation@SystemTray@winrt@@U134@UINotifyPropertyChanged@Data@Xaml@UI@Windows@4@Ucomposable@4@Ucomposing@4@UIControlOverrides@Controls@7894@UIControlOverrides6@Controls@7894@UIFrameworkElementOverrides@7894@UIFrameworkElementOverrides2@7894@UIUIElementOverrides@7894@UIUIElementOverrides7@7894@UIUIElementOverrides8@7894@UIUIElementOverrides9@7894@@impl@winrt@@AEAAHAEBUguid@3@PEAPEAX@Z"
# The complete bounded body is reviewed, not a few matching instructions amid
# unknown operations. Only exact symbol/import targets and local branch RVAs
# are relocated. Stack slots, registers, calls, cleanup and arithmetic stay fixed.
BORROWED_BODY = """
push rbp
push rbx
push rsi
push rdi
push r14
mov rbp, rsp
sub rsp, 0x80
movaps xmmword ptr [rsp + 0x70], xmm6
mov rax, [$security_cookie]
xor rax, rsp
mov qword ptr [rbp - 0x20], rax
mov r14, r9
mov rdi, r8
mov rsi, rdx
lea rdx, [rbp - 0x38]
mov rcx, r8
call $xaml_root
nop
lea rdx, [rbp - 0x40]
mov rcx, rax
call $root_content
nop
mov r8, rax
lea rdx, [rbp - 0x60]
mov rcx, rdi
call $transform
mov rbx, rax
mov rcx, rdi
call $actual_width
xorps xmm6, xmm6
cvtsd2ss xmm6, xmm0
mov rcx, rdi
call $actual_height
xorps xmm1, xmm1
cvtsd2ss xmm1, xmm0
and dword ptr [rbp - 0x50], 0
and dword ptr [rbp - 0x4c], 0
movss dword ptr [rbp - 0x48], xmm6
movss dword ptr [rbp - 0x44], xmm1
xorps xmm0, xmm0
movups xmmword ptr [rbp - 0x30], xmm0
mov rcx, qword ptr [rbx]
mov rax, qword ptr [rcx]
movups xmm0, xmmword ptr [rbp - 0x50]
movdqa xmmword ptr [rbp - 0x50], xmm0
lea r8, [rbp - 0x30]
lea rdx, [rbp - 0x50]
mov rax, qword ptr [rax + 0x48]
call [$guard_dispatch]
test eax, eax
jns @transform_ok
mov ecx, eax
call $throw_hresult
nop
@transform_ok
cmp qword ptr [rbp - 0x60], 0
je @transform_released
lea rcx, [rbp - 0x60]
call $release
nop
@transform_released
cmp qword ptr [rbp - 0x40], 0
je @content_released
lea rcx, [rbp - 0x40]
call $release
nop
@content_released
cmp qword ptr [rbp - 0x38], 0
je @root_released
lea rcx, [rbp - 0x38]
call $release
@root_released
lea rcx, [rbp - 0x60]
call $display_factory
nop
lea rcx, [rbp - 0x60]
call $scale
xorps xmm6, xmm6
cvtsd2ss xmm6, xmm0
cmp qword ptr [rbp - 0x60], 0
je @display_released
lea rcx, [rbp - 0x60]
call $release
@display_released
movss xmm1, dword ptr [rbp - 0x30]
mulss xmm1, xmm6
cvttss2si eax, xmm1
mov dword ptr [rsi], eax
movss xmm2, dword ptr [rbp - 0x2c]
mulss xmm2, xmm6
cvttss2si eax, xmm2
mov dword ptr [rsi + 4], eax
movss xmm0, dword ptr [rbp - 0x28]
mulss xmm0, xmm6
addss xmm0, xmm1
cvttss2si edx, xmm0
mov dword ptr [rsi + 8], edx
movss xmm1, dword ptr [rbp - 0x24]
mulss xmm1, xmm6
addss xmm1, xmm2
cvttss2si edx, xmm1
mov dword ptr [rsi + 0xc], edx
mov r9d, 2
mov r8, rsi
xor edx, edx
mov rcx, r14
call [$map_window_points]
mov rax, rsi
mov rcx, qword ptr [rbp - 0x20]
xor rcx, rsp
call $security_check
movaps xmm6, xmmword ptr [rsp + 0x70]
add rsp, 0x80
pop r14
pop rdi
pop rsi
pop rbx
pop rbp
ret
"""
BORROWED_DELEGATED_BODY = """
push rbp
push rbx
push rsi
push rdi
push r14
mov rbp, rsp
sub rsp, 0x70
movaps xmmword ptr [rsp + 0x60], xmm6
mov rax, [$security_cookie]
xor rax, rsp
mov qword ptr [rbp - 0x18], rax
mov r14, r9
mov rdi, r8
mov rsi, rdx
lea rdx, [rbp - 0x50]
mov rcx, r8
call $xaml_root
nop
lea rdx, [rbp - 0x30]
mov rcx, rax
call $root_content
nop
mov r8, rax
lea rdx, [rbp - 0x38]
mov rcx, rdi
call $transform
mov rbx, rax
mov rcx, rdi
call $actual_width
xorps xmm6, xmm6
cvtsd2ss xmm6, xmm0
mov rcx, rdi
call $actual_height
xorps xmm1, xmm1
cvtsd2ss xmm1, xmm0
and dword ptr [rbp - 0x48], 0
and dword ptr [rbp - 0x44], 0
movss dword ptr [rbp - 0x40], xmm6
movss dword ptr [rbp - 0x3c], xmm1
lea r8, [rbp - 0x48]
lea rdx, [rbp - 0x28]
mov rcx, rbx
call $transform_bounds
nop
lea rcx, [rbp - 0x38]
call $destructor
nop
lea rcx, [rbp - 0x30]
call $destructor
nop
lea rcx, [rbp - 0x50]
call $destructor
lea rcx, [rbp - 0x50]
call $display_factory
nop
mov rcx, rax
call $scale
xorps xmm6, xmm6
cvtsd2ss xmm6, xmm0
lea rcx, [rbp - 0x50]
call $destructor
movss xmm1, dword ptr [rbp - 0x28]
mulss xmm1, xmm6
cvttss2si eax, xmm1
mov dword ptr [rsi], eax
movss xmm2, dword ptr [rbp - 0x24]
mulss xmm2, xmm6
cvttss2si eax, xmm2
mov dword ptr [rsi + 4], eax
movss xmm0, dword ptr [rbp - 0x20]
mulss xmm0, xmm6
addss xmm0, xmm1
cvttss2si edx, xmm0
mov dword ptr [rsi + 8], edx
movss xmm1, dword ptr [rbp - 0x1c]
mulss xmm1, xmm6
addss xmm1, xmm2
cvttss2si edx, xmm1
mov dword ptr [rsi + 0xc], edx
mov r9d, 2
mov r8, rsi
xor edx, edx
mov rcx, r14
call [$map_window_points]
mov rax, rsi
mov rcx, qword ptr [rbp - 0x18]
xor rcx, rsp
call $security_check
movaps xmm6, xmmword ptr [rsp + 0x60]
add rsp, 0x70
pop r14
pop rdi
pop rsi
pop rbx
pop rbp
ret
"""
OWNED_TRANSFORM_BODY = """
mov qword ptr [rsp + 8], rbx
mov qword ptr [rsp + 0x20], rsi
push rbp
push rdi
push r12
push r14
push r15
mov rbp, rsp
sub rsp, 0x80
movaps xmmword ptr [rsp + 0x70], xmm6
mov rax, [$security_cookie]
xor rax, rsp
mov qword ptr [rbp - 0x20], rax
mov r15, r8
mov rsi, rdx
mov r14, rcx
mov qword ptr [rbp - 0x48], rdx
and qword ptr [rbp - 0x60], 0
lea r8, [rbp - 0x60]
lea rdx, [rbp - 0x50]
mov rcx, rsi
call $transform
nop
cmp qword ptr [rbp - 0x60], 0
je @branch_0
lea rcx, [rbp - 0x60]
call $release
@branch_0
mov rcx, rsi
call $actual_width
xorps xmm6, xmm6
cvtsd2ss xmm6, xmm0
mov rcx, rsi
call $actual_height
xorps xmm1, xmm1
cvtsd2ss xmm1, xmm0
and dword ptr [rbp - 0x60], 0
and dword ptr [rbp - 0x5c], 0
movss dword ptr [rbp - 0x58], xmm6
movss dword ptr [rbp - 0x54], xmm1
lea r8, [rbp - 0x60]
lea rdx, [rbp - 0x40]
lea rcx, [rbp - 0x50]
call $transform_bounds
mov rcx, r15
call [$get_dpi]
mov edi, eax
cvttss2si ecx, dword ptr [rbp - 0x40]
mov r12d, 0x60
mov r8d, r12d
mov edx, eax
call [$mul_div]
mov dword ptr [rbp - 0x30], eax
cvttss2si ecx, dword ptr [rbp - 0x3c]
mov r8d, r12d
mov edx, edi
call [$mul_div]
mov dword ptr [rbp - 0x2c], eax
cvttss2si ecx, dword ptr [rbp - 0x38]
mov r8d, r12d
mov edx, edi
call [$mul_div]
mov ebx, eax
cvttss2si ecx, dword ptr [rbp - 0x40]
mov r8d, r12d
mov edx, edi
call [$mul_div]
add ebx, eax
mov dword ptr [rbp - 0x28], ebx
cvttss2si ecx, dword ptr [rbp - 0x34]
mov r8d, r12d
mov edx, edi
call [$mul_div]
mov ebx, eax
cvttss2si ecx, dword ptr [rbp - 0x3c]
mov r8d, r12d
mov edx, edi
call [$mul_div]
add ebx, eax
mov dword ptr [rbp - 0x24], ebx
lea r9d, [r12 - 0x5e]
lea r8, [rbp - 0x30]
xor edx, edx
mov rcx, r15
call [$map_window_points]
movups xmm0, xmmword ptr [rbp - 0x30]
movdqu xmmword ptr [r14], xmm0
lea rcx, [rbp - 0x50]
call $destructor
nop
mov rcx, rsi
call $destructor
mov rax, r14
mov rcx, qword ptr [rbp - 0x20]
xor rcx, rsp
call $security_check
lea r11, [rsp + 0x80]
mov rbx, qword ptr [r11 + 0x30]
mov rsi, qword ptr [r11 + 0x48]
movaps xmm6, xmmword ptr [rsp + 0x70]
mov rsp, r11
pop r15
pop r14
pop r12
pop rdi
pop rbp
ret
"""

BORROWED_INLINE_TAIL_BODY = """
push rbp
push rbx
push rsi
push rdi
push r14
mov rbp, rsp
sub rsp, 0x70
movaps xmmword ptr [rsp + 0x60], xmm6
mov rax, [$security_cookie]
xor rax, rsp
mov qword ptr [rbp - 0x18], rax
mov r14, r9
mov rdi, r8
mov rsi, rdx
lea rdx, [rbp - 0x50]
mov rcx, r8
call $xaml_root
nop
lea rdx, [rbp - 0x30]
mov rcx, rax
call $root_content
nop
mov r8, rax
lea rdx, [rbp - 0x48]
mov rcx, rdi
call $transform
mov rbx, rax
mov rcx, rdi
call $actual_width
xorps xmm6, xmm6
cvtsd2ss xmm6, xmm0
mov rcx, rdi
call $actual_height
xorps xmm1, xmm1
cvtsd2ss xmm1, xmm0
and dword ptr [rbp - 0x40], 0
and dword ptr [rbp - 0x3c], 0
movss dword ptr [rbp - 0x38], xmm6
movss dword ptr [rbp - 0x34], xmm1
xorps xmm0, xmm0
movups xmmword ptr [rbp - 0x28], xmm0
mov rcx, qword ptr [rbx]
mov rax, qword ptr [rcx]
movups xmm0, xmmword ptr [rbp - 0x40]
movdqa xmmword ptr [rbp - 0x40], xmm0
lea r8, [rbp - 0x28]
lea rdx, [rbp - 0x40]
mov rax, qword ptr [rax + 0x48]
call [$guard_dispatch]
test eax, eax
js @branch_4
cmp qword ptr [rbp - 0x48], 0
je @branch_0
lea rcx, [rbp - 0x48]
call $release
nop
@branch_0
cmp qword ptr [rbp - 0x30], 0
je @branch_1
lea rcx, [rbp - 0x30]
call $release
nop
@branch_1
cmp qword ptr [rbp - 0x50], 0
je @branch_2
lea rcx, [rbp - 0x50]
call $release
@branch_2
lea rcx, [rbp - 0x50]
call $display_factory
nop
mov rcx, rax
call $scale
xorps xmm6, xmm6
cvtsd2ss xmm6, xmm0
cmp qword ptr [rbp - 0x50], 0
je @branch_3
lea rcx, [rbp - 0x50]
call $release
@branch_3
movss xmm1, dword ptr [rbp - 0x28]
mulss xmm1, xmm6
cvttss2si eax, xmm1
mov dword ptr [rsi], eax
movss xmm2, dword ptr [rbp - 0x24]
mulss xmm2, xmm6
cvttss2si eax, xmm2
mov dword ptr [rsi + 4], eax
movss xmm0, dword ptr [rbp - 0x20]
mulss xmm0, xmm6
addss xmm0, xmm1
cvttss2si edx, xmm0
mov dword ptr [rsi + 8], edx
movss xmm1, dword ptr [rbp - 0x1c]
mulss xmm1, xmm6
addss xmm1, xmm2
cvttss2si edx, xmm1
mov dword ptr [rsi + 0xc], edx
mov r9d, 2
mov r8, rsi
xor edx, edx
mov rcx, r14
call [$map_window_points]
mov rax, rsi
mov rcx, qword ptr [rbp - 0x18]
xor rcx, rsp
call $security_check
movaps xmm6, xmmword ptr [rsp + 0x60]
add rsp, 0x70
pop r14
pop rdi
pop rsi
pop rbx
pop rbp
ret
@branch_4
mov ecx, eax
call $throw_hresult
int3
"""
TRANSFORM_BOUNDS_BODY = """
push rbx
sub rsp, 0x30
xorps xmm0, xmm0
mov rbx, rdx
movups xmmword ptr [rdx], xmm0
mov rcx, qword ptr [rcx]
movups xmm0, xmmword ptr [r8]
mov r8, rdx
lea rdx, [rsp + 0x20]
mov rax, qword ptr [rcx]
movdqu xmmword ptr [rsp + 0x20], xmm0
mov rax, qword ptr [rax + 0x48]
call [$guard_dispatch]
test eax, eax
js @failed
mov rax, rbx
add rsp, 0x30
pop rbx
ret
@failed
mov ecx, eax
call $throw_hresult
int3
"""
RETURNED_WRAPPER_BODY = """
push rbx
sub rsp, 0x40
mov rax, [$security_cookie]
xor rax, rsp
mov qword ptr [rsp + 0x30], rax
mov qword ptr [rsp + 0x28], rdx
mov rbx, rdx
mov rcx, qword ptr [rcx]
test rcx, rcx
jne @query
and qword ptr [rdx], rcx
jmp @return_wrapper
@query
and qword ptr [rsp + 0x28], 0
lea r8, [rsp + 0x28]
lea rdx, [$framework_element_iid]
call $query_interface
test eax, eax
js @failed
mov rcx, qword ptr [rsp + 0x28]
mov qword ptr [rbx], rcx
@return_wrapper
mov rax, rbx
mov rcx, qword ptr [rsp + 0x30]
xor rcx, rsp
call $security_check
add rsp, 0x40
pop rbx
ret
@failed
mov ecx, eax
call $throw_hresult
int3
"""
ICON_QUERY_BODY = """
mov qword ptr [rsp + 8], rbx
mov qword ptr [rsp + 0x10], rsi
push rdi
sub rsp, 0x20
mov rbx, rcx
mov rdi, r8
mov rcx, qword ptr [rcx + 0x18]
mov rsi, rdx
test rcx, rcx
jne @query_slot_zero
mov rcx, rbx
call $query_interface_inner
cmp eax, 0x80004002
jne @return_result
mov rcx, qword ptr [rbx + 8]
test rcx, rcx
je @return_result
mov r8, rdi
mov rdx, rsi
@query_slot_zero
mov rax, qword ptr [rcx]
mov rax, qword ptr [rax]
call [$guard_dispatch]
@return_result
mov rbx, qword ptr [rsp + 0x30]
mov rsi, qword ptr [rsp + 0x38]
add rsp, 0x20
pop rdi
ret
"""


def instruction_record(ins):
    return {"rva": ins.address, "bytes": ins.bytes.hex(), "instruction": f"{ins.mnemonic} {ins.op_str}".strip()}


def function_evidence(image, decorated, logs):
    rva = image.exact(decorated)
    return bounded_evidence(image, rva, decorated, logs)


def bounded_evidence(image, rva, label, logs):
    code = image.function(rva)
    logs.append({"module_role": image.role, "symbol": label, "rva": rva,
                 "instructions": [instruction_record(ins) for ins in code]})
    return rva, code


def anchored_symbol(image, names, rva, anchor):
    """Finite raw-name aliases require an independent vtable/direct-call anchor.

    The old raw implementation name denotes both overloads. Never ask exact()
    to guess between them, synthesize a decorated name, or use substring search.
    """
    primary = {s["rva"] for s in image.symbols if s["decorated_name"] == names[0]}
    require(not primary or primary == {rva}, "Decorated source symbol conflicts with its independent anchor")
    matches = sorted({s["decorated_name"] for s in image.symbols
                      if s["rva"] == rva and s["decorated_name"] in names})
    require(matches, f"Source anchor has no reviewed exact raw name: {anchor}")
    return {"rva": rva, "raw_names": matches, "anchor": anchor,
            "uses_raw_alias": names[0] not in matches}


def anchored_source_functions(shell, child, factory, logs):
    getter_rva = shell.pointer(child + 8 * 8)
    factory_rva = shell.pointer(factory + 6 * 8)
    getter_binding = anchored_symbol(shell, (GET_CONTENT, RAW_GET_CONTENT), getter_rva, "child_vtable_slot_8")
    factory_binding = anchored_symbol(shell, (FACTORY_CREATE, RAW_FACTORY_CREATE), factory_rva,
                                      "factory_vtable_slot_6")
    _, getter = bounded_evidence(shell, getter_rva, getter_binding["raw_names"][0], logs)
    names = (IMPL_CONTENT, RAW_IMPL_CONTENT)
    candidates = {ins.operands[0].imm for ins in getter if ins.mnemonic == "call" and
                  len(ins.operands) == 1 and ins.operands[0].type == X86_OP_IMM and
                  any(s["rva"] == ins.operands[0].imm and s["decorated_name"] in names for s in shell.symbols)}
    require(len(candidates) == 1, "Getter must call one independently named XamlContent implementation")
    impl_rva = candidates.pop()
    immediate_call(getter, impl_rva)
    impl_binding = anchored_symbol(shell, names, impl_rva, "direct_call_from_child_vtable_getter")
    _, implementation = bounded_evidence(shell, impl_rva, impl_binding["raw_names"][0], logs)
    bindings = {"child_getter": getter_binding, "implementation_getter": impl_binding,
                "factory_create": factory_binding}
    if any(binding["uses_raw_alias"] for binding in bindings.values()):
        _, creator = bounded_evidence(shell, factory_rva, factory_binding["raw_names"][0], logs)
        captured = find_sequence(creator, (("mov", "rbx, r9"), ("mov", "qword ptr [rsp + 0x28], r8"),
                                          ("and", "qword ptr [r9], 0"), ("lea", "r9, [rsp + 0x28]"),
                                          ("mov", "r8, rdx"), ("lea", "rdx, [rsp + 0x20]")))
        require(immediate_call(creator, shell.exact(FACTORY_IMPLEMENTATION)) == captured + 6,
                "Raw factory alias does not reach the exact creator with the reviewed argument flow")
        function_evidence(shell, FACTORY_IMPLEMENTATION, logs)
        factory_binding["implementation_call_rva"] = creator[captured + 6].address
    return getter_rva, getter, impl_rva, implementation, bindings


def find_sequence(code, sequence):
    """Match decoded instructions, never unaligned bytes inside operands."""
    items = [(ins.mnemonic, ins.op_str) for ins in code]
    starts = [index for index in range(len(items) - len(sequence) + 1)
              if items[index:index + len(sequence)] == list(sequence)]
    require(len(starts) == 1, f"Expected one reviewed instruction sequence, got {len(starts)}: {sequence}")
    return starts[0]


def immediate_call(code, target):
    matches = [index for index, ins in enumerate(code) if ins.mnemonic == "call" and
               len(ins.operands) == 1 and ins.operands[0].type == X86_OP_IMM and ins.operands[0].imm == target]
    require(len(matches) == 1, f"Expected exactly one direct call to {target:#x}")
    return matches[0]


def member_adjustment(ins, destination, source):
    require(len(ins.operands) == 2, "Unknown member adjustment operand count")
    left, right = ins.operands
    require(left.type == X86_OP_REG and ins.reg_name(left.reg) == destination, "Unexpected adjustment destination")
    if ins.mnemonic == "add" and destination == source and right.type == X86_OP_IMM:
        return right.imm
    if ins.mnemonic == "lea" and right.type == X86_OP_MEM:
        require(ins.reg_name(right.mem.base) == source and right.mem.index == 0, "Indexed/unknown object field")
        return right.mem.disp
    raise EvidenceError("Unknown member adjustment instruction")


def aligned_offset(offset):
    require(8 <= offset <= 256 and offset % 8 == 0, f"Field offset outside reviewed bounds: {offset}")
    return offset


def release_leaf(image, logs):
    """Exact leaf used by both owned-input and borrowed-input recipes."""
    release_rva = image.exact(UNKNOWN_RELEASE)
    # This tail-call leaf has no .pdata entry. Only these exact six decoded
    # instructions are accepted; this is not a generic missing-boundary fallback.
    decoder = Cs(CS_ARCH_X86, CS_MODE_64)
    decoder.detail = True
    release = list(decoder.disasm(image.bytes(release_rva, 27), release_rva))[:6]
    require(len(release) == 6, "Incomplete IUnknown release leaf")
    head = [(ins.mnemonic, ins.op_str) for ins in release[:5]]
    require(head[0] == ("mov", "rdx, qword ptr [rcx]") and
            head[1] in (("mov", "qword ptr [rcx], 0"), ("and", "qword ptr [rcx], 0")) and
            head[2:] == [("mov", "rcx, rdx"), ("mov", "rax, qword ptr [rdx]"),
                         ("mov", "rax, qword ptr [rax + 0x10]")],
            "Unknown IUnknown wrapper release data flow")
    tail = release[5]
    require(tail.mnemonic == "jmp" and len(tail.operands) == 1 and
            tail.operands[0].type == X86_OP_MEM, "Unknown IUnknown release tail call")
    memory = tail.operands[0].mem
    require(tail.reg_name(memory.base) == "rip" and memory.index == 0 and
            tail.address + tail.size + memory.disp == image.exact(GUARD_DISPATCH),
            "IUnknown Release does not use the exact CFG dispatch pointer")
    image.section(image.exact(GUARD_DISPATCH), 8, False)
    logs.append({"module_role": image.role, "symbol": UNKNOWN_RELEASE, "rva": release_rva,
                 "boundary_recipe": "exact_six_instruction_cfg_release_leaf_v1",
                 "instructions": [instruction_record(ins) for ins in release]})
    return release_rva


def owned_cleanup(image, code, logs):
    """Reviewed call graphs that consume the same incoming wrapper exactly once."""
    release_rva = release_leaf(image, logs)
    written = find_sequence(code, (("movdqu", "xmmword ptr [r14], xmm0"),))
    returned = find_sequence(code, (("mov", "rax, r14"),))

    checks = [i for i, ins in enumerate(code) if (ins.mnemonic, ins.op_str) == ("cmp", "qword ptr [rsi], 0")]
    if checks:
        require(len(checks) == 1, "Ambiguous owned FrameworkElement cleanup check")
        i = checks[0]
        require(i + 4 < len(code) and code[i + 1].mnemonic == "je" and
                code[i + 1].operands[0].type == X86_OP_IMM and
                code[i + 1].operands[0].imm == code[i + 4].address and
                (code[i + 2].mnemonic, code[i + 2].op_str) == ("mov", "rcx, rsi") and
                immediate_call(code[i + 3:i + 4], release_rva) == 0,
                "Unknown inline FrameworkElement cleanup flow")
        cleanup = i + 3
        recipe = "inline_null_check_release_v1"
    else:
        destructor_rva, destructor = function_evidence(image, UNKNOWN_DESTRUCTOR, logs)
        require(len(destructor) == 6 and
                [(ins.mnemonic, ins.op_str) for ins in (destructor[0], destructor[1], destructor[4], destructor[5])] ==
                [("sub", "rsp, 0x28"), ("cmp", "qword ptr [rcx], 0"), ("add", "rsp, 0x28"), ("ret", "")] and
                destructor[2].mnemonic == "je" and destructor[2].operands[0].type == X86_OP_IMM and
                destructor[2].operands[0].imm == destructor[4].address and
                immediate_call(destructor, release_rva) == 3,
                "Unknown delegated FrameworkElement cleanup flow")
        calls = [i for i, ins in enumerate(code) if ins.mnemonic == "call" and
                 len(ins.operands) == 1 and ins.operands[0].type == X86_OP_IMM and
                 ins.operands[0].imm == destructor_rva]
        owned = [i for i in calls if i > 0 and
                 (code[i - 1].mnemonic, code[i - 1].op_str) == ("mov", "rcx, rsi")]
        require(len(owned) == 1, "Owned FrameworkElement must flow to exactly one destructor call")
        cleanup = owned[0]
        if len(calls) == 1:
            recipe = "delegated_null_check_release_v1"
        else:
            # 26100.1 also destroys a local transform wrapper. Its distinct
            # stack address, initialization and adjacent cleanup are checked.
            # Its one direct release must clean the null-guarded input local;
            # extra calls cannot be silently treated as harmless temporaries.
            require(cleanup >= 4 and len(calls) == 2 and calls == [cleanup - 3, cleanup] and
                    (code[cleanup - 4].mnemonic, code[cleanup - 4].op_str) == ("lea", "rcx, [rbp - 0x50]") and
                    (code[cleanup - 2].mnemonic, code[cleanup - 2].op_str) == ("nop", ""),
                    "Unknown additional destructor call or stack temporary cleanup")
            initialized = find_sequence(code, (("lea", "rdx, [rbp - 0x50]"), ("mov", "rcx, rsi")))
            transform_rva, transform = function_evidence(image, TRANSFORM_TO_VISUAL, logs)
            require(initialized >= 2 and initialized + 2 < cleanup - 4 and
                    (code[initialized - 2].mnemonic, code[initialized - 2].op_str) ==
                    ("and", "qword ptr [rbp - 0x60], 0") and
                    (code[initialized - 1].mnemonic, code[initialized - 1].op_str) == ("lea", "r8, [rbp - 0x60]") and
                    immediate_call(code, transform_rva) == initialized + 2,
                    "Stack temporary has no exact TransformToVisual initialization call")
            find_sequence(transform, (("mov", "rsi, r8"), ("mov", "rbx, rdx")))
            find_sequence(transform, (("mov", "rcx, qword ptr [rsp + 0x30]"),
                                      ("mov", "qword ptr [rbx], rcx"), ("mov", "rax, rbx")))
            temporary_cleanup = immediate_call(code, release_rva)
            require(temporary_cleanup == initialized + 7 and temporary_cleanup < written and
                    [(code[i].mnemonic, code[i].op_str) for i in
                     (temporary_cleanup - 4, temporary_cleanup - 3, temporary_cleanup - 1)] ==
                    [("nop", ""), ("cmp", "qword ptr [rbp - 0x60], 0"),
                     ("lea", "rcx, [rbp - 0x60]")],
                    "Unknown direct release of TransformToVisual input temporary")
            branch = code[temporary_cleanup - 2]
            require(branch.mnemonic == "je" and len(branch.operands) == 1 and
                    branch.operands[0].type == X86_OP_IMM and
                    branch.operands[0].imm == code[temporary_cleanup + 1].address,
                    "Transform input null branch must skip only its direct release")
            recipe = "delegated_with_transform_temporary_v1"
    require(written < cleanup < returned, "Owned FrameworkElement cleanup is not between result write and return")
    return {"recipe": recipe, "release_rva": release_rva, "cleanup_call_rva": code[cleanup].address}


def xfg_implementation_source_field(image, implementation, logs):
    """Complete early XFG Content getter; no vtable/path authorization here.

    Its equality helper receives a zero wrapper, so the helper's nonnull-pair
    branch is unreachable. The only reachable outcomes are source-null (cold
    throw) and source-nonnull (ABI Content slot 6). Every instruction in both
    hot bodies is constrained; external calls retain exact symbolic anchors.
    """
    require(len(implementation) == 35, "Unknown XFG Content getter instruction count")
    equality = implementation[11]
    null_branch = implementation[13]
    require(equality.mnemonic == "call" and len(equality.operands) == 1 and
            equality.operands[0].type == X86_OP_IMM and
            null_branch.mnemonic == "jne" and len(null_branch.operands) == 1 and
            null_branch.operands[0].type == X86_OP_IMM,
            "Unknown XFG Content null-check targets")
    equal_rva, null_rva = equality.operands[0].imm, null_branch.operands[0].imm
    # This public name is undecorated and shared by overloads. Bind it at the
    # independently decoded call target, then prove that entire helper body.
    require(any(s["rva"] == equal_rva and s["decorated_name"] == RAW_UNKNOWN_EQUAL
                for s in image.symbols), "Source null comparison lacks its exact raw-name anchor")
    _, equal = bounded_evidence(image, equal_rva, RAW_UNKNOWN_EQUAL, logs)
    require(len(equal) == 19, "Unknown source equality helper instruction count")
    nonnull_branch = equal[10]
    require(nonnull_branch.mnemonic == "jne" and len(nonnull_branch.operands) == 1 and
            nonnull_branch.operands[0].type == X86_OP_IMM,
            "Unknown equality helper nonnull branch")
    match_instruction_template(equal, """
mov qword ptr [rsp + 0x20], rbx
push rdi
sub rsp, 0x20
mov rcx, qword ptr [rcx]
mov rdi, rdx
cmp rcx, qword ptr [rdx]
je @equal
test rcx, rcx
je @different
cmp qword ptr [rdx], 0
jne $nonnull_pair
@different
xor al, al
@return
mov rbx, qword ptr [rsp + 0x48]
add rsp, 0x20
pop rdi
ret
int3
@equal
mov al, 1
jmp @return
""", {"nonnull_pair": nonnull_branch.operands[0].imm})
    _, cold = bounded_evidence(image, null_rva, "XFG Content null-source throw", logs)
    require(len(cold) == 6 and cold[1].mnemonic == "lea" and len(cold[1].operands) == 2 and
            cold[1].operands[1].type == X86_OP_MEM,
            "Unknown XFG Content null-source failure body")
    diagnostic = cold[1].address + cold[1].size + cold[1].operands[1].mem.disp
    image.section(diagnostic, 1, False)
    match_instruction_template(cold, """
mov r9d, 0x8007139f
lea r8, [$diagnostic]
mov edx, 0x45
mov rcx, rdi
call $throw
int3
""", {"diagnostic": diagnostic, "throw": image.exact(THROW_HR_DIAGNOSTIC)})
    guard = image.exact(XFG_DISPATCH)
    image.section(guard, 8, False)
    match_instruction_template(implementation, """
mov rax, rsp
mov qword ptr [rax + 0x10], rbx
mov qword ptr [rax + 0x18], rsi
push rdi
sub rsp, 0x20
mov rbx, rdx
mov rdi, qword ptr [rsp + 0x28]
and qword ptr [rax + 8], 0
lea rsi, [rcx + 0x20]
lea rdx, [rax + 8]
mov rcx, rsi
call $equal
test al, al
jne $null
and qword ptr [rsp + 0x30], 0
mov rcx, qword ptr [rsi]
mov rax, qword ptr [rcx]
movabs r10, 0x8b15ae295055f370
lea rdx, [rsp + 0x30]
mov rax, qword ptr [rax + 0x30]
call [$guard]
test eax, eax
js @failed
mov rax, qword ptr [rsp + 0x30]
mov qword ptr [rbx], rax
mov rax, rbx
mov rbx, qword ptr [rsp + 0x38]
mov rsi, qword ptr [rsp + 0x40]
add rsp, 0x20
pop rdi
ret
int3
@failed
mov ecx, eax
call $throw_hresult
int3
""", {"equal": equal_rva, "null": null_rva, "guard": guard,
        "throw_hresult": image.exact("?throw_hresult@winrt@@YAXUhresult@1@@Z")})
    return 0x20, "rsi_xfg_inline_content_v1"


def implementation_source_field(image, implementation, logs):
    """Observed register/dispatch forms, with direct field use in each."""
    if any(ins.mnemonic == "movabs" and ins.op_str.startswith("r10,") for ins in implementation):
        return xfg_implementation_source_field(image, implementation, logs)
    fields = [(i, ins, register) for i, ins in enumerate(implementation)
              for register in ("rdi", "rsi")
              if ins.mnemonic == "lea" and ins.op_str.startswith(f"{register}, [rcx")]
    require(len(fields) == 1, "Unknown/ambiguous implementation source field")
    index, field, register = fields[0]
    offset = member_adjustment(field, register, "rcx")
    require(index + 3 < len(implementation) and
            implementation[index + 1].mnemonic == "lea" and implementation[index + 1].op_str.startswith("rdx,") and
            (implementation[index + 2].mnemonic, implementation[index + 2].op_str) == ("mov", f"rcx, {register}") and
            implementation[index + 3].mnemonic == "call" and
            len(implementation[index + 3].operands) == 1 and
            implementation[index + 3].operands[0].type == X86_OP_IMM,
            "Unknown use of source field")
    if register == "rsi":
        # The 26100.1 implementation inlines the source's Content getter. It
        # dereferences this same field and calls ABI slot 6; do not infer the
        # field merely from a different nonvolatile register in an arbitrary LEA.
        expected = (("test", "al, al"), ("jne", None),
                    ("and", "qword ptr [rsp + 0x20], 0"),
                    ("mov", "rcx, qword ptr [rsi]"), ("mov", "rax, qword ptr [rcx]"),
                    ("lea", "rdx, [rsp + 0x20]"), ("mov", "rax, qword ptr [rax + 0x30]"),
                    ("call", None))
        sequence = implementation[index + 4:index + 4 + len(expected)]
        require(len(sequence) == len(expected) and all(
            ins.mnemonic == mnemonic and (operand is None or ins.op_str == operand)
            for ins, (mnemonic, operand) in zip(sequence, expected)),
            "Unknown inline Content getter for the RSI source field")
        branch = sequence[1]
        require(len(branch.operands) == 1 and branch.operands[0].type == X86_OP_IMM and
                branch.operands[0].imm > sequence[-1].address and
                branch.operands[0].imm in {ins.address for ins in implementation},
                "Inline Content null check does not branch to a bounded failure path")
        call = sequence[-1]
        require(len(call.operands) == 1 and call.operands[0].type == X86_OP_IMM,
                "Inline Content getter does not call a direct CFG thunk")
        thunk_rva = call.operands[0].imm
        decoder = Cs(CS_ARCH_X86, CS_MODE_64)
        decoder.detail = True
        thunk = list(decoder.disasm(image.bytes(thunk_rva, 5), thunk_rva))
        dispatch = image.exact(GUARD_DISPATCH_FUNCTION)
        image.section(dispatch, 1, True)
        require(len(thunk) == 1 and thunk[0].mnemonic == "jmp" and thunk[0].size == 5 and
                len(thunk[0].operands) == 1 and thunk[0].operands[0].type == X86_OP_IMM and
                thunk[0].operands[0].imm == dispatch,
                "Inline Content call does not reach the exact CFG dispatch function")
        logs.append({"module_role": image.role, "symbol": GUARD_DISPATCH_FUNCTION + " (direct thunk)",
                     "rva": thunk_rva, "boundary_recipe": "exact_five_byte_cfg_dispatch_thunk_v1",
                     "instructions": [instruction_record(thunk[0])]})
        return offset, "rsi_inline_content_v1"
    content = immediate_call(implementation, image.exact(CONTENT_CONSUMER))
    require(content > index + 3 and
            (implementation[content - 1].mnemonic, implementation[content - 1].op_str) == ("mov", "rcx, rdi") and
            implementation[content - 2].mnemonic == "lea" and
            implementation[content - 2].op_str.startswith("rdx, [rsp"),
            "RDI source field does not flow directly to exact DesktopWindowXamlSource::Content")
    for ins in implementation[index + 1:content]:
        _, written = ins.regs_access()
        require(not any(ins.reg_name(reg) in ("rdi", "edi", "di", "dil") for reg in written),
                "RDI source field is overwritten before exact Content call")
    return offset, "rdi_content_helper_v1"


def xfg_host_layout(image, logs):
    """Five real host roles with exact shared_ptr/XFG ABI, no FrameHeight."""
    entries, bodies = [], {}
    for role, name in HOST_SYMBOLS:
        if role == "frame_height":
            continue
        rva = image.exact(name)
        executable = role not in ("primary_site", "secondary_site")
        entries.append({"role": role, "rva": rva,
                        **image.section(rva, 16 if executable else 8, executable),
                        "prologue": list(image.bytes(rva, 16)) if executable else []})
        if executable:
            _, bodies[role] = function_evidence(image, name, logs)
    guard = image.exact(XFG_DISPATCH)
    image.section(guard, 8, False)
    for role, field, slot, tag in (
        ("primary_host", "0x270", "0x3f0", "0xd769b6855ad33a70"),
        ("secondary_host", "0x38", "0x118", "0x984d0757145d2170"),
    ):
        match_instruction_template(bodies[role], f"""
push rbx
sub rsp, 0x20
mov rcx, qword ptr [rcx + {field}]
mov rbx, rdx
mov rax, qword ptr [rcx]
movabs r10, {tag}
mov rax, qword ptr [rax + {slot}]
call [$guard]
mov rax, rbx
add rsp, 0x20
pop rbx
ret
""", {"guard": guard})
    copy_name = "??0?$shared_ptr@VTaskbarHost@@@std@@QEAA@AEBV01@@Z"
    copy = image.exact(copy_name)
    decoder = Cs(CS_ARCH_X86, CS_MODE_64)
    decoder.detail = True
    leaf = list(decoder.disasm(image.bytes(copy, 40), copy))
    require(sum(i.size for i in leaf) == 40, "Incomplete shared_ptr copy leaf")
    match_instruction_template(leaf, """
and qword ptr [rcx], 0
and qword ptr [rcx + 8], 0
mov rax, qword ptr [rdx + 8]
test rax, rax
je @copy
lock inc dword ptr [rax + 8]
@copy
mov rax, qword ptr [rdx]
mov qword ptr [rcx], rax
mov rax, qword ptr [rdx + 8]
mov qword ptr [rcx + 8], rax
mov rax, rcx
ret
""", {})
    logs.append({"module_role": image.role, "symbol": copy_name, "rva": copy,
                 "boundary_recipe": "exact_40_byte_shared_ptr_copy_leaf_v1",
                 "instructions": [instruction_record(i) for i in leaf]})
    virtuals = (
        ("?GetTaskbarHost@TrayUI@@UEBA?AV?$shared_ptr@VTaskbarHost@@@std@@XZ",
         "??_7TrayUI@@6BITrayComponentHost@@@", 0x3f0, "0x2d0"),
        ("?GetTaskbarHost@CSecondaryTray@@UEBA?AV?$shared_ptr@VTaskbarHost@@@std@@XZ",
         "??_7CSecondaryTray@@6BISecondaryTray@@@", 0x118, "0x140"),
    )
    for name, table_name, slot, field in virtuals:
        rva, body = function_evidence(image, name, logs)
        require(image.pointer(image.exact(table_name) + slot) == rva,
                "Shared_ptr virtual entry is not the exact named getter")
        match_instruction_template(body, f"""
sub rsp, 0x28
mov r8, rdx
lea rdx, [rcx + {field}]
mov rcx, r8
call $copy
mov rax, r8
add rsp, 0x28
ret
""", {"copy": copy})
    match_instruction_template(bodies["release_host"], """
push rbx
sub rsp, 0x20
mov rbx, rcx
or eax, 0xffffffff
lock xadd dword ptr [rcx + 8], eax
cmp eax, 1
je @destroy
@return
add rsp, 0x20
pop rbx
ret
@destroy
mov rax, qword ptr [rcx]
movabs r10, 0xa545274f70dcb170
mov rax, qword ptr [rax]
call [$guard]
or eax, 0xffffffff
lock xadd dword ptr [rbx + 0xc], eax
cmp eax, 1
jne @return
mov rax, qword ptr [rbx]
movabs r10, 0x9e241f5a3adb5a70
mov rax, qword ptr [rax + 8]
mov rcx, rbx
add rsp, 0x20
pop rbx
jmp [$guard]
""", {"guard": guard})
    return {"schema": 1, "module": image.identity, "symbols": entries}


EARLY_CHILD_QI_SYMBOLS = {
    'child_iid': '??$guid_v@UIXamlExplorerHostChildWindow@XamlExplorerHost@Shell@Internal@Windows@winrt@@@impl@winrt@@3Uguid@2@B',
    'implementation_table': '??_7XamlExplorerHostChildWindow@implementation@XamlExplorerHost@Shell@Internal@Windows@winrt@@6B@',
    'query': '?QueryInterface@?$produce_base@UXamlExplorerHostChildWindow@implementation@XamlExplorerHost@Shell@Internal@Windows@winrt@@UIXamlExplorerHostChildWindow@34567@X@impl@winrt@@UEAAHAEBUguid@3@PEAPEAX@Z',
    'root_query': '?query_interface@?$root_implements@UXamlExplorerHostChildWindow@implementation@XamlExplorerHost@Shell@Internal@Windows@winrt@@U134567@UIClosable@Foundation@67@Unon_agile@7@@impl@winrt@@AEAAHAEBUguid@3@PEAPEAX@Z',
    'find': '?find_interface@?$implements@UXamlExplorerHostChildWindow@implementation@XamlExplorerHost@Shell@Internal@Windows@winrt@@U134567@UIClosable@Foundation@67@Unon_agile@7@@winrt@@UEBAPEAXAEBUguid@2@@Z',
    'iids': '?get_local_iids@?$implements@UXamlExplorerHostChildWindow@implementation@XamlExplorerHost@Shell@Internal@Windows@winrt@@U134567@UIClosable@Foundation@67@Unon_agile@7@@winrt@@UEBA?AU?$pair@IPEBUguid@winrt@@@std@@XZ',
    'weak': '?make_weak_ref@?$root_implements@UXamlExplorerHostChildWindow@implementation@XamlExplorerHost@Shell@Internal@Windows@winrt@@U134567@UIClosable@Foundation@67@Unon_agile@7@@impl@winrt@@AEAAPEAUIWeakReferenceSource@23@XZ',
    'constructor': '??0XamlExplorerHostChildWindow@implementation@XamlExplorerHost@Shell@Internal@Windows@winrt@@QEAA@PEAUHWND__@@AEBUhstring@6@@Z',
    'tearoff': '?query_interface_tearoff@?$root_implements@UXamlExplorerHostChildWindow@implementation@XamlExplorerHost@Shell@Internal@Windows@winrt@@U134567@UIClosable@Foundation@67@Unon_agile@7@@impl@winrt@@MEBAHAEBUguid@3@PEAPEAX@Z',
    'closable_table': '??_7?$produce@UXamlExplorerHostChildWindow@implementation@XamlExplorerHost@Shell@Internal@Windows@winrt@@UIClosable@Foundation@67@@impl@winrt@@6B@',
}

EARLY_CHILD_QI_RAW_NAMES = {
    'root_query': 'winrt::impl::root_implements<winrt::Windows::Internal::Shell::XamlExplorerHost::implementation::XamlExplorerHostChildWindow,winrt::Windows::Internal::Shell::XamlExplorerHost::XamlExplorerHostChildWindow,winrt::Windows::Foundation::IClosable,winrt::non_agile>::query_interface',
    'weak': 'winrt::impl::root_implements<winrt::Windows::Internal::Shell::XamlExplorerHost::implementation::XamlExplorerHostChildWindow,winrt::Windows::Internal::Shell::XamlExplorerHost::XamlExplorerHostChildWindow,winrt::Windows::Foundation::IClosable,winrt::non_agile>::make_weak_ref',
    'constructor': 'winrt::Windows::Internal::Shell::XamlExplorerHost::implementation::XamlExplorerHostChildWindow::XamlExplorerHostChildWindow',
}

EARLY_CHILD_FIND_BODY = """
sub rsp, 0x28
mov r10, rdx
mov r9, rcx
mov rcx, r10
lea rdx, [$child_iid]
call $equal
xor r8d, r8d
test al, al
jne @branch_0
lea rdx, [$closable_iid]
call $equal
test al, al
jne @branch_1
@branch_2
mov rax, r8
add rsp, 0x28
ret
int3
@branch_1
lea rax, [r9 + 0x18]
@branch_3
neg r9
sbb r8, r8
and r8, rax
jmp @branch_2
@branch_0
lea rax, [r9 + 0x10]
jmp @branch_3
"""

EARLY_CHILD_QUERY_BODY = """
mov qword ptr [rsp + 8], rbx
mov qword ptr [rsp + 0x10], rsi
push rdi
sub rsp, 0x20
mov rax, qword ptr [rcx]
mov rsi, r8
mov rdi, rdx
mov rbx, rcx
movabs r10, 0xc91417db5e5ee170
mov rax, qword ptr [rax + 0x30]
call qword ptr [$guard]
mov qword ptr [rsi], rax
test rax, rax
jne @branch_0
lea rdx, [$inspectable_iid]
mov rcx, rdi
call $equal
test al, al
jne $cold
lea rdx, [$unknown_iid]
call $equal
test al, al
jne $cold_unknown
lea rdx, [$weak_iid]
call $equal
test al, al
je $cold_tearoff
mov rcx, rbx
call $weak
mov qword ptr [rsi], rax
neg rax
sbb eax, eax
not eax
and eax, 0x8007000e
@branch_2
mov rbx, qword ptr [rsp + 0x30]
mov rsi, qword ptr [rsp + 0x38]
add rsp, 0x20
pop rdi
ret
int3
@branch_0
mov rdx, qword ptr [rbx + 8]
@branch_1
test rdx, rdx
js $cold_count
lea rcx, [rdx + 1]
mov rax, rdx
lock cmpxchg qword ptr [rbx + 8], rcx
mov rdx, rax
jne @branch_1
xor eax, eax
jmp @branch_2
"""

EARLY_CHILD_QUERY_COLD_BODY = """
mov rax, qword ptr [rbx]
movabs r10, 0xd52cb78974de0870
mov rax, qword ptr [rax + 0x38]
mov rcx, rbx
call qword ptr [$guard]
mov qword ptr [rsi], rax
mov rdx, qword ptr [rbx + 8]
@branch_1
test rdx, rdx
js @branch_0
lea rcx, [rdx + 1]
mov rax, rdx
lock cmpxchg qword ptr [rbx + 8], rcx
mov rdx, rax
jne @branch_1
jmp $success
mov rax, qword ptr [rbx]
movabs r10, 0xfe9c1eea34550970
mov rax, qword ptr [rax + 0x18]
mov rcx, rbx
call qword ptr [$guard]
mov qword ptr [rsi], rax
mov rdx, qword ptr [rbx + 8]
@branch_2
test rdx, rdx
js @branch_0
lea rcx, [rdx + 1]
mov rax, rdx
lock cmpxchg qword ptr [rbx + 8], rcx
mov rdx, rax
jne @branch_2
jmp $success
@branch_0
mov eax, 1
lock xadd dword ptr [rdx + rdx + 0x18], eax
jmp $success
mov rax, qword ptr [rbx]
movabs r10, 0xe6012f053a556b70
mov rax, qword ptr [rax]
mov r8, rsi
mov rdx, rdi
mov rcx, rbx
call qword ptr [$guard]
nop
jmp $return_result
"""

def xfg_child_query_interface(shell, child, slots, logs):
    """Bind either reviewed private IID to its concrete QI and owned result.

    Public type names are shared with later versions; their GUIDs are not.
    All bodies below are complete fixed recipes except the explicitly bounded
    constructor prefix that binds this ABI subobject to the implementation.
    """
    names = dict(EARLY_CHILD_QI_SYMBOLS)
    # DbgHelp exposes these private methods as exact raw names on 22000.282.
    # Accept only the finite aliases and one shared RVA; the QI jump/call and
    # constructor table stores below independently validate their roles.
    for key, raw_name in EARLY_CHILD_QI_RAW_NAMES.items():
        aliases = (names[key], raw_name)
        matches = [s for s in shell.symbols if s["decorated_name"] in aliases]
        require(len({s["rva"] for s in matches}) == 1,
                "Missing/ambiguous exact early QI symbol alias: " + key)
        names[key] = names[key] if any(s["decorated_name"] == names[key] for s in matches) else raw_name
    targets = {key: shell.exact(name) for key, name in names.items()}
    targets["guard"] = shell.exact(XFG_DISPATCH)
    iid_contracts = {
        bytes.fromhex("32f6f8ee0d6ccb5fb6a1354819e51f3b"):
            ("early_private_iid_owned_query_v1", "eef8f632-6c0d-5fcb-b6a1-354819e51f3b"),
        bytes.fromhex("d0af97ec0d5c1b589508074773c6f1e0"):
            ("ec97_private_iid_owned_query_v1", "ec97afd0-5c0d-581b-9508-074773c6f1e0"),
    }
    child_iid = shell.bytes(targets["child_iid"], 16, False)
    require(child_iid in iid_contracts, "Unknown exact named XFG child IID")
    query_recipe, iid = iid_contracts[child_iid]
    require(slots[0]["rva"] == targets["query"], "Early child slot zero is not its exact named QI")

    def leaf(rva, size, label):
        decoder = Cs(CS_ARCH_X86, CS_MODE_64)
        decoder.detail = True
        code = list(decoder.disasm(shell.bytes(rva, size), rva))
        require(sum(ins.size for ins in code) == size, "Incomplete early QI leaf")
        logs.append({"module_role": shell.role, "symbol": label, "rva": rva,
                     "boundary_recipe": f"exact_{size}_byte_early_qi_leaf_v1",
                     "instructions": [instruction_record(ins) for ins in code]})
        return code

    def rip(ins):
        memory = [op.mem for op in ins.operands if op.type == X86_OP_MEM and
                  ins.reg_name(op.mem.base) == "rip" and op.mem.index == 0 and op.mem.segment == 0]
        require(len(memory) == 1, "Early QI operand has no unique RIP-relative target")
        return ins.address + ins.size + memory[0].disp

    def direct(ins):
        require(len(ins.operands) == 1 and ins.operands[0].type == X86_OP_IMM,
                "Early QI branch has no direct target")
        return ins.operands[0].imm

    match_instruction_template(leaf(targets["query"], 18, names["query"]), """
lea rax, [rcx - 0x10]
neg rcx
sbb rcx, rcx
and rcx, rax
jmp $root_query
""", targets)
    table = targets["implementation_table"]
    sections = [s for s in shell.pe.sections if s.VirtualAddress <= table and
                table + 64 <= s.VirtualAddress + min(s.Misc_VirtualSize, s.SizeOfRawData)]
    require(table % 8 == 0 and len(sections) == 1 and sections[0].Name.rstrip(b"\0") == b".data" and
            sections[0].Characteristics == 0xc0000040, "Unknown early implementation table section")
    relocations = [(e.rva, e.type) for b in getattr(shell.pe, "DIRECTORY_ENTRY_BASERELOC", []) for e in b.entries]
    implementation_slots = [(0, "tearoff"), (0x20, "iids"), (0x30, "find")]
    if query_recipe == "ec97_private_iid_owned_query_v1":
        targets["destructor"] = shell.exact("??_GXamlExplorerHostChildWindow@implementation@XamlExplorerHost@Shell@Internal@Windows@winrt@@UEAAPEAXI@Z")
        implementation_slots.append((8, "destructor"))
    for offset, key in implementation_slots:
        location = table + offset
        require([kind for address, kind in relocations if address == location] == [10],
                "Early implementation QI slot needs exactly one DIR64 relocation")
        raw = shell.pe.get_data(location, 8)
        require(len(raw) == 8 and struct.unpack("<Q", raw)[0] - shell.pe.OPTIONAL_HEADER.ImageBase == targets[key],
                "Early implementation slot is not its exact named QI target")

    _, constructor = function_evidence(shell, names["constructor"], logs)
    match_instruction_template(constructor[:20], """
mov qword ptr [rsp + 8], rcx
push rbp
push rbx
push rsi
push rdi
push r12
push r13
push r14
push r15
mov rbp, rsp
sub rsp, 0x38
mov rdi, r8
mov rsi, rdx
mov r15, rcx
lea rax, [$child]
mov qword ptr [rcx + 0x10], rax
lea rax, [$closable_table]
mov qword ptr [rcx + 0x18], rax
mov ecx, 1
mov eax, ecx
""", {**targets, "child": child})
    require(len(constructor) > 24 and constructor[20].mnemonic == "lock xadd" and
            len(constructor[20].operands) == 2 and constructor[20].operands[0].type == X86_OP_MEM and
            constructor[20].operands[0].size == 4 and constructor[20].operands[1].type == X86_OP_REG and
            constructor[20].reg_name(constructor[20].operands[1].reg) == "eax",
            "Early constructor has changed its intervening module-count update")
    module_counter = rip(constructor[20])
    match_instruction_template(constructor[21:25], """
mov qword ptr [r15 + 8], rcx
lea rax, [$implementation_table]
mov qword ptr [r15], rax
lea rcx, [r15 + 0x20]
""", targets)

    _, find = function_evidence(shell, names["find"], logs)
    require(len(find) == 24, "Unknown early find_interface body")
    targets.update(equal=direct(find[5]), closable_iid=rip(find[9]))
    closable_iid = bytes.fromhex("29a8d530a47f264083bbd75bae4ea99e")
    require(shell.bytes(targets["closable_iid"], 16, False) == closable_iid,
            "Early find_interface has changed its IClosable IID")
    match_instruction_template(find, EARLY_CHILD_FIND_BODY, targets)
    match_instruction_template(leaf(targets["equal"], 28, "early QI direct GUID equality"), """
mov rax, qword ptr [rcx]
sub rax, qword ptr [rdx]
jne @different_half
mov rax, qword ptr [rcx + 8]
sub rax, qword ptr [rdx + 8]
@different_half
test rax, rax
je @equal
xor al, al
ret
int3
@equal
mov al, 1
ret
""", {})
    iids = leaf(targets["iids"], 21, names["iids"])
    targets["iid_array"] = rip(iids[0])
    match_instruction_template(iids, """
lea rax, [$iid_array]
mov dword ptr [rdx], 2
mov qword ptr [rdx + 8], rax
mov rax, rdx
ret
""", targets)
    require(shell.bytes(targets["iid_array"], 32, False) == child_iid + closable_iid,
            "Early GetIids disagrees with the complete supported private IID pair")
    _, query = function_evidence(shell, names["root_query"], logs)
    require(len(query) == 50, "Unknown early root query_interface body")
    for key, index, value in (
        ("inspectable_iid", 14, "e0e286af2db16a4c9c5ad7aa65101e90"),
        ("unknown_iid", 19, "0000000000000000c000000000000046"),
        ("weak_iid", 23, "3800000000000000c000000000000046"),
    ):
        targets[key] = rip(query[index])
        require(shell.bytes(targets[key], 16, False) == bytes.fromhex(value),
                "Early root QI has changed a standard interface IID")
    for key, index in (("cold", 18), ("cold_unknown", 22), ("cold_count", 42), ("cold_tearoff", 26)):
        targets[key] = direct(query[index])
    targets.update(success=query[48].address, return_result=query[34].address)
    match_instruction_template(query, EARLY_CHILD_QUERY_BODY, targets)
    _, cold = bounded_evidence(shell, targets["cold"], "early root QI cold paths", logs)
    require(len(cold) == 42 and cold[15].address == targets["cold_unknown"] and
            cold[30].address == targets["cold_count"] and cold[33].address == targets["cold_tearoff"],
            "Early root QI cold branches do not enter their reviewed blocks")
    match_instruction_template(cold, EARLY_CHILD_QUERY_COLD_BODY, targets)
    match_instruction_template(leaf(targets["tearoff"], 6, names["tearoff"]), """
mov eax, 0x80004002
ret
""", {})
    return {"recipe": query_recipe, "iid": iid,
            **({"module_counter_rva": module_counter} if query_recipe == "ec97_private_iid_owned_query_v1" else {}),
            "iid_rva": targets["child_iid"], "query_interface_rva": targets["query"],
            "root_query_interface_rva": targets["root_query"], "implementation_vtable_rva": table,
            "find_interface_rva": targets["find"], "get_local_iids_rva": targets["iids"],
            "get_local_iids_array_rva": targets["iid_array"], "abi_this_adjustment": -0x10,
            "returned_implementation_offset": 0x10, "owned_reference": True}


def xfg_child_reference_contract(shell, slots, module_counter, logs):
    """The ec97 13-slot variant must prove the owned QI result's release path."""
    root_type = "?$root_implements@UXamlExplorerHostChildWindow@implementation@XamlExplorerHost@Shell@Internal@Windows@winrt@@U134567@UIClosable@Foundation@67@Unon_agile@7@@impl@winrt@@"
    root_release = shell.exact("?NonDelegatingRelease@" + root_type + "IEAAIXZ")
    subtract = shell.exact("?subtract_reference@" + root_type + "IEAAIXZ")
    addref = slots[1]["rva"]
    release = slots[2]["rva"]
    base = "winrt::impl::produce_base<winrt::Windows::Internal::Shell::XamlExplorerHost::implementation::XamlExplorerHostChildWindow,winrt::Windows::Internal::Shell::XamlExplorerHost::IXamlExplorerHostChildWindow,void>"
    anchored_symbol(shell, ("?AddRef@?$produce_base@UXamlExplorerHostChildWindow@implementation@XamlExplorerHost@Shell@Internal@Windows@winrt@@UIXamlExplorerHostChildWindow@34567@X@impl@winrt@@UEAAIXZ", base + "::AddRef"), addref, "child_slot_1_addref")
    anchored_symbol(shell, ("?Release@?$produce_base@UXamlExplorerHostChildWindow@implementation@XamlExplorerHost@Shell@Internal@Windows@winrt@@UIXamlExplorerHostChildWindow@34567@X@impl@winrt@@UEAAIXZ", base + "::Release"), release, "child_slot_2_release")

    def leaf(rva, size, label):
        decoder = Cs(CS_ARCH_X86, CS_MODE_64)
        decoder.detail = True
        code = list(decoder.disasm(shell.bytes(rva, size), rva))
        require(sum(ins.size for ins in code) == size, "Incomplete child reference leaf")
        logs.append({"module_role": shell.role, "symbol": label, "rva": rva,
                     "boundary_recipe": f"exact_{size}_byte_child_reference_leaf_v1",
                     "instructions": [instruction_record(ins) for ins in code]})
        return code

    add = leaf(addref, 48, "child AddRef")
    require(len(add) == 14 and add[6].mnemonic == "js" and add[6].operands[0].type == X86_OP_IMM,
            "Unknown child AddRef weak-reference branch")
    weak_add = add[6].operands[0].imm
    match_instruction_template(add, """
test rcx, rcx
lea r8, [rcx - 8]
mov eax, 8
cmove r8, rax
mov rcx, qword ptr [r8]
@retry
test rcx, rcx
js $weak
lea rdx, [rcx + 1]
mov rax, rcx
lock cmpxchg qword ptr [r8], rdx
mov rcx, rax
jne @retry
mov eax, edx
ret
""", {"weak": weak_add})
    match_instruction_template(leaf(weak_add, 18, "child weak AddRef"), """
mov edx, 1
lock xadd dword ptr [rcx + rcx + 0x18], edx
inc edx
jmp $result
""", {"result": add[-2].address})
    match_instruction_template(leaf(release, 18, "child Release thunk"), """
lea rax, [rcx - 0x10]
neg rcx
sbb rcx, rcx
and rcx, rax
jmp $release
""", {"release": root_release})
    _, release_body = bounded_evidence(shell, root_release, "child NonDelegatingRelease", logs)
    match_instruction_template(release_body, """
mov qword ptr [rsp + 8], rbx
push rdi
sub rsp, 0x20
mov rbx, rcx
call $subtract
mov edi, eax
test eax, eax
je @destroy
@return
mov rbx, qword ptr [rsp + 0x30]
mov eax, edi
add rsp, 0x20
pop rdi
ret
int3
@destroy
mov edx, 1
mov r8d, edx
xchg qword ptr [rbx + 8], r8
test rbx, rbx
je @return
mov rcx, qword ptr [rbx]
mov rax, qword ptr [rcx + 8]
movabs r10, 0xa3b1ae513e58f270
mov rcx, rbx
call qword ptr [$guard]
jmp @return
""", {"subtract": subtract, "guard": shell.exact(XFG_DISPATCH)})
    _, decrement = bounded_evidence(shell, subtract, "child subtract_reference", logs)
    require(len(decrement) == 22 and decrement[-2].mnemonic == "call" and
            decrement[-2].operands[0].type == X86_OP_IMM, "Unknown child weak control release")
    weak_release = decrement[-2].operands[0].imm
    match_instruction_template(decrement, """
push rbx
sub rsp, 0x20
mov rax, qword ptr [rcx + 8]
@retry
test rax, rax
js @weak
lea rdx, [rax - 1]
lock cmpxchg qword ptr [rcx + 8], rdx
jne @retry
mov eax, edx
@return
add rsp, 0x20
pop rbx
ret
int3
@weak
lea rcx, [rax + rax]
or ebx, 0xffffffff
lock xadd dword ptr [rcx + 0x18], ebx
sub ebx, 1
je @last
@result
mov eax, ebx
jmp @return
@last
call $weak_release
jmp @result
""", {"weak_release": weak_release})
    # The final strong reference releases its weak control block. Bind the
    # complete direct callee, including its independent weak-count decrement;
    # the object destructor remains the concrete checked implementation vtable.
    _, weak = bounded_evidence(shell, weak_release, "child weak control release", logs)
    require(len(weak) == 21 and weak[11].mnemonic == "call" and weak[14].mnemonic == "call" and
            weak[11].operands[0].type == X86_OP_IMM and weak[14].operands[0].type == X86_OP_IMM,
            "Unknown child weak control destructor")
    decrement_module = weak[11].operands[0].imm
    delete = weak[14].operands[0].imm
    require(weak[10].operands[1].type == X86_OP_MEM, "Missing child module counter")
    counter = weak[10].address + weak[10].size + weak[10].operands[1].mem.disp
    require(counter == module_counter, "Weak control release uses a different module counter")
    match_instruction_template(weak, """
mov qword ptr [rsp + 8], rbx
push rdi
sub rsp, 0x20
mov rbx, rcx
or edi, 0xffffffff
lock xadd dword ptr [rcx + 0x1c], edi
sub edi, 1
jne @return
test rcx, rcx
je @return
lea rcx, [$counter]
call $decrement_module
lea edx, [rdi + 0x20]
mov rcx, rbx
call $delete
@return
mov rbx, qword ptr [rsp + 0x30]
mov eax, edi
add rsp, 0x20
pop rdi
ret
int3
""", {"counter": counter, "decrement_module": decrement_module, "delete": delete})
    _, module_release = bounded_evidence(shell, decrement_module, "child module counter release", logs)
    require(len(module_release) == 11 and module_release[6].mnemonic == "js" and
            module_release[6].operands[0].type == X86_OP_IMM, "Unknown module counter failure path")
    failure = module_release[6].operands[0].imm
    match_instruction_template(module_release, """
sub rsp, 0x28
or eax, 0xffffffff
lock xadd dword ptr [rcx], eax
sub eax, 1
je @return
test eax, eax
js $failure
@return
add rsp, 0x28
ret
int3
jmp @return
""", {"failure": failure})
    _, fail_body = bounded_evidence(shell, failure, "child module counter underflow", logs)
    match_instruction_template(fail_body, """
call qword ptr [$terminate]
nop dword ptr [rax + rax]
int3
""", {"terminate": shell.import_slot("api-ms-win-crt-private-l1-1-0.dll", "_o_terminate")})
    # The sized delete thunk reaches the CRT free import through two exact
    # direct jumps. Never accept a call target merely because it is executable.
    destination = delete
    for index in range(2):
        jump = leaf(destination, 5, f"child delete thunk {index}")
        require(len(jump) == 1 and jump[0].mnemonic == "jmp" and
                jump[0].operands[0].type == X86_OP_IMM, "Unknown child delete thunk")
        destination = jump[0].operands[0].imm
    match_instruction_template(leaf(destination, 6, "child CRT free import"),
                               "jmp qword ptr [$free]",
                               {"free": shell.import_slot("api-ms-win-crt-private-l1-1-0.dll", "_o_free")})
    return {"recipe": "xfg_child_owned_reference_v1", "add_ref_rva": addref,
            "release_rva": release, "root_release_rva": root_release,
            "subtract_reference_rva": subtract, "weak_release_rva": weak_release,
            "implementation_reference_offset": 8, "weak_strong_offset": 0x18,
            "weak_count_offset": 0x1c}


def xfg_source_layout(taskbar, shell, logs):
    """Early child getter and writable table with exact relocations/targets."""
    focus_name = CONSUME_FOCUS.replace("QEBA@XZ", "QEBA_NXZ")
    focus_rva, focus = function_evidence(taskbar, HAS_FOCUS, logs)
    consume, body = function_evidence(taskbar, focus_name, logs)
    match_instruction_template(focus, """
sub rsp, 0x28
add rcx, 0x20
xor eax, eax
cmp qword ptr [rcx], rax
je @return
call $focus
@return
add rsp, 0x28
ret
""", {"focus": consume})
    match_instruction_template(body, """
sub rsp, 0x38
mov rax, [$cookie]
xor rax, rsp
mov qword ptr [rsp + 0x28], rax
mov rcx, qword ptr [rcx]
mov byte ptr [rsp + 0x20], 0
mov rax, qword ptr [rcx]
movabs r10, 0xd0cca7f81cd14170
mov rax, qword ptr [rax + 0x60]
lea rdx, [rsp + 0x20]
call [$guard]
test eax, eax
jns @success
mov ecx, eax
call $throw
int3
@success
mov al, byte ptr [rsp + 0x20]
mov rcx, qword ptr [rsp + 0x28]
xor rcx, rsp
call $check
add rsp, 0x38
ret
""", {"cookie": taskbar.exact("__security_cookie"), "guard": taskbar.exact(XFG_DISPATCH),
        "throw": taskbar.exact("?throw_hresult@winrt@@YAXUhresult@1@@Z"),
        "check": taskbar.exact("__security_check_cookie")})
    child = shell.exact(CHILD_TABLE)
    # This named early table is writable. Require the exact finite table range
    # and every DIR64 relocation; targets still use the normal immutable code
    # checks. This does not alter Image.section for any other role or data.
    sections = [s for s in shell.pe.sections if s.VirtualAddress <= child and
                child + 13 * 8 <= s.VirtualAddress + min(s.Misc_VirtualSize, s.SizeOfRawData)]
    require(child % 8 == 0 and len(sections) == 1 and sections[0].Name.rstrip(b"\0") == b".data" and
            sections[0].Characteristics == 0xc0000040, "Unknown early writable child table section")
    relocations = [(e.rva, e.type) for b in getattr(shell.pe, "DIRECTORY_ENTRY_BASERELOC", []) for e in b.entries]
    slots = []
    for slot in range(13):
        location = child + slot * 8
        require([kind for address, kind in relocations if address == location] == [10],
                "Each early child table slot needs exactly one DIR64 relocation")
        raw = shell.pe.get_data(location, 8)
        require(len(raw) == 8, "Truncated early child table slot")
        rva = struct.unpack("<Q", raw)[0] - shell.pe.OPTIONAL_HEADER.ImageBase
        slots.append({"rva": rva, "prologue": list(shell.bytes(rva, 16))})
    query_evidence = xfg_child_query_interface(shell, child, slots, logs)
    reference_evidence = None
    if query_evidence["iid"] == "ec97afd0-5c0d-581b-9508-074773c6f1e0":
        reference_evidence = xfg_child_reference_contract(shell, slots, query_evidence["module_counter_rva"], logs)
    getter_rva = slots[8]["rva"]
    getter_binding = anchored_symbol(shell, (GET_CONTENT, RAW_GET_CONTENT), getter_rva,
                                     "exact_relocated_child_vtable_slot_8")
    _, getter = bounded_evidence(shell, getter_rva, getter_binding["raw_names"][0], logs)
    require(len(getter) == 22, "Unknown early child ABI getter body")
    implementation = getter[9]
    require(implementation.mnemonic == "call" and implementation.operands[0].type == X86_OP_IMM,
            "Missing direct early child implementation call")
    impl_rva = implementation.operands[0].imm
    impl_binding = anchored_symbol(shell, (IMPL_CONTENT, RAW_IMPL_CONTENT), impl_rva,
                                   "early_child_vtable_getter_direct_call")
    _, impl = bounded_evidence(shell, impl_rva, impl_binding["raw_names"][0], logs)
    offset, field_recipe = xfg_implementation_source_field(shell, impl, logs)
    cleanup = getter[14]
    require(cleanup.mnemonic == "jne" and cleanup.operands[0].type == X86_OP_IMM,
            "Missing early getter cleanup branch")
    cleanup_rva = cleanup.operands[0].imm
    match_instruction_template(getter, """
push rbx
sub rsp, 0x20
mov rbx, rdx
and qword ptr [rdx], 0
lea rax, [rcx - 0x10]
neg rcx
sbb rcx, rcx
and rcx, rax
lea rdx, [rsp + 0x30]
call $implementation
mov rcx, qword ptr [rax]
and qword ptr [rax], 0
mov qword ptr [rbx], rcx
cmp qword ptr [rsp + 0x30], 0
jne $cleanup
@success
xor eax, eax
@return
add rsp, 0x20
pop rbx
ret
int3
mov eax, dword ptr [rsp + 0x30]
jmp @return
""", {"implementation": impl_rva, "cleanup": cleanup_rva})
    _, cold = bounded_evidence(shell, cleanup_rva, "early child result cleanup", logs)
    match_instruction_template(cold, """
lea rcx, [rsp + 0x30]
call $release
nop
jmp $success
int3
int3
""", {"release": shell.exact(UNKNOWN_RELEASE), "success": getter[15].address})
    contract = {}
    if query_evidence["iid"] == "ec97afd0-5c0d-581b-9508-074773c6f1e0":
        contract["child_interface"] = "xaml_explorer_host_child_ec97_v1"
    return {"recipe_candidate": "xaml_explorer_host_child_xfg_v1", **contract, "host_child_offset": 0x20,
            "child_source_offset": offset - 0x10, "child_vtable_rva": child,
            "child_vtable_entries": slots,
            "evidence": {"has_focus_rva": focus_rva, "child_getter_rva": getter_rva,
                         "implementation_getter_rva": impl_rva, "abi_this_adjustment": -0x10,
                         "implementation_source_field": offset, "implementation_field_recipe": field_recipe,
                         "symbol_bindings": {"child_getter": getter_binding, "implementation_getter": impl_binding},
                         "child_query_interface": query_evidence,
                         **({"child_reference_contract": reference_evidence} if reference_evidence else {}),
                         "factory_absent": True, "child_table_dir64_relocations": [child + i * 8 for i in range(13)]}}


def host_layout(image: Image, logs):
    if not any(s["decorated_name"] == HOST_SYMBOLS[4][1] for s in image.symbols):
        return xfg_host_layout(image, logs)
    entries, code_by_role = [], {}
    for role, decorated in HOST_SYMBOLS:
        rva = image.exact(decorated)
        executable = role not in ("primary_site", "secondary_site")
        entries.append({"role": role, "rva": rva, **image.section(rva, 16 if executable else 8, executable),
                        "prologue": list(image.bytes(rva, 16)) if executable else []})
        if executable:
            _, code_by_role[role] = function_evidence(image, decorated, logs)
    frame = code_by_role["frame_height"]
    require(len(frame) >= 4 and (frame[0].mnemonic, frame[0].op_str) == ("sub", "rsp, 0x28"),
            "Unknown FrameHeight stack/field recipe")
    field = aligned_offset(member_adjustment(frame[1], "rcx", "rcx"))
    require((frame[2].mnemonic, frame[2].op_str) == ("cmp", "qword ptr [rcx], 0"),
            "FrameHeight does not test the adjusted field")
    for role in ("primary_host", "secondary_host"):
        code = code_by_role[role]
        find_sequence(code, (("mov", "rbx, rdx"), ("mov", "rax, qword ptr [rcx]")))
        find_sequence(code, (("mov", "rax, rbx"), ("add", "rsp, 0x20"), ("pop", "rbx"), ("ret", "")))
        require(any(ins.mnemonic == "call" for ins in code), "Missing shared_ptr host call")
    require(any(ins.mnemonic == "lock xadd" and ins.op_str == "dword ptr [rcx + 8], eax"
                for ins in code_by_role["release_host"]), "Unknown shared_ptr release ABI")
    return {"schema": 1, "module": image.identity, "symbols": entries, "element_offset": field}


def source_layout(taskbar: Image, shell: Image, logs):
    if not any(s["decorated_name"] == FACTORY_TABLE for s in shell.symbols):
        return xfg_source_layout(taskbar, shell, logs)
    focus_rva, focus = function_evidence(taskbar, HAS_FOCUS, logs)
    require(len(focus) >= 6 and (focus[0].mnemonic, focus[0].op_str) == ("sub", "rsp, 0x28"),
            "Unknown TaskbarHost::HasFocus recipe")
    host_field = aligned_offset(member_adjustment(focus[1], "rcx", "rcx"))
    consume = taskbar.exact(CONSUME_FOCUS)
    call_index = immediate_call(focus, consume)
    require(call_index > 1, "HasFocus call before field adjustment")
    allowed = {"xor", "cmp", "je", "test"}
    require(all(ins.mnemonic in allowed for ins in focus[2:call_index]), "Unknown mutation before child HasFocus call")
    require(any(ins.mnemonic == "cmp" and ins.op_str.startswith("qword ptr [rcx],")
                for ins in focus[2:call_index]), "Missing adjusted-child null check")
    function_evidence(taskbar, CONSUME_FOCUS, logs)

    child, factory = shell.exact(CHILD_TABLE), shell.exact(FACTORY_TABLE)
    shell.section(child, 21 * 8, False)
    shell.section(factory, 7 * 8, False)
    getter_rva, getter, impl_rva, implementation, bindings = anchored_source_functions(shell, child, factory, logs)
    adjustment_candidates = [(index, ins) for index, ins in enumerate(getter)
                             if ins.mnemonic == "lea" and ins.op_str.startswith("rax, [rcx")]
    require(len(adjustment_candidates) == 1, "Unknown/ambiguous ABI this adjustment")
    index, adjustment = adjustment_candidates[0]
    delta = member_adjustment(adjustment, "rax", "rcx")
    require(delta < 0, "Unknown ABI subobject relationship")
    require([(ins.mnemonic, ins.op_str) for ins in getter[index + 1:index + 4]] ==
            [("neg", "rcx"), ("sbb", "rcx, rcx"), ("and", "rcx, rax")],
            "Unknown null-preserving ABI this adjustment")
    impl_call = immediate_call(getter, impl_rva)
    require(impl_call == index + 5 and getter[index + 4].mnemonic == "lea" and
            getter[index + 4].op_str.startswith("rdx, [rsp"), "Adjusted object does not flow directly to implementation")
    offset, field_recipe = implementation_source_field(shell, implementation, logs)
    child_field = aligned_offset(delta + offset)
    table_evidence = {}
    for label, table, count in (("child", child, 21), ("factory", factory, 7)):
        table_evidence[label] = {"rva": table, "slots": [shell.pointer(table + 8 * slot) for slot in range(count)]}
    return {
        "recipe_candidate": "xaml_explorer_host_child_v1", "host_child_offset": host_field,
        "child_source_offset": child_field, "child_vtable_rva": child, "factory_vtable_rva": factory,
        "evidence": {"has_focus_rva": focus_rva, "child_getter_rva": getter_rva,
                     "implementation_getter_rva": impl_rva, "abi_this_adjustment": delta,
                     "implementation_source_field": offset, "implementation_field_recipe": field_recipe,
                     "symbol_bindings": bindings, "vtables": table_evidence},
    }


def match_instruction_template(code, template, targets):
    """Match all bounded instructions; address operands require exact bindings."""
    labels, expected = {}, []
    for line in template.strip().splitlines():
        if line.startswith("@"):
            labels[line] = len(expected)
        elif line.startswith("lock "):
            prefix, mnemonic, operand = line.split(" ", 2)
            expected.append((prefix + " " + mnemonic, operand))
        else:
            expected.append(tuple(line.split(" ", 1)) if " " in line else (line, ""))
    require(len(code) == len(expected), "Borrowed ScreenRect body has an unreviewed instruction count")
    for index, (ins, (mnemonic, operand)) in enumerate(zip(code, expected)):
        require(ins.mnemonic == mnemonic, f"Borrowed ScreenRect changed instruction {index}")
        if operand.startswith("$") or operand.startswith("@"):
            target = targets[operand[1:]] if operand.startswith("$") else code[labels[operand]].address
            require(len(ins.operands) == 1 and ins.operands[0].type == X86_OP_IMM and
                    ins.operands[0].imm == target,
                    f"Borrowed ScreenRect changed exact call/branch target at instruction {index}")
        elif "[$" in operand:
            key = operand.split("[$", 1)[1].removesuffix("]")
            memory_index = 1 if ", " in operand else 0
            require(len(ins.operands) == memory_index + 1 and
                    (not memory_index or ins.operands[0].type == X86_OP_REG and
                     ins.reg_name(ins.operands[0].reg) == operand.split(", ", 1)[0]) and
                    ins.operands[memory_index].type == X86_OP_MEM,
                    f"Borrowed ScreenRect changed indirect operand at instruction {index}")
            memory = ins.operands[memory_index].mem
            require(ins.reg_name(memory.base) == "rip" and memory.index == 0 and memory.segment == 0 and
                    ins.address + ins.size + memory.disp == targets[key],
                    f"Borrowed ScreenRect changed exact import/global target at instruction {index}")
        else:
            require(ins.op_str == operand,
                    f"Borrowed ScreenRect changed register, stack local or data flow at instruction {index}")


def match_borrowed_body(image, code):
    matches, failures = [], []
    for shape, template, overrides in (
        ("inline", BORROWED_BODY, {}),
        ("delegated", BORROWED_DELEGATED_BODY,
         {"display_factory": DISPLAY_CURRENT_VIEW, "transform_bounds": TRANSFORM_BOUNDS,
          "destructor": UNKNOWN_DESTRUCTOR}),
        ("inline_tail", BORROWED_INLINE_TAIL_BODY, {"display_factory": DISPLAY_CURRENT_VIEW}),
    ):
        count = sum(1 for line in template.strip().splitlines() if not line.startswith("@"))
        if len(code) != count:
            continue
        names = {**BORROWED_TARGETS, **overrides}
        try:
            targets = {key: image.exact(name) for key, name in names.items()}
            targets["map_window_points"] = image.import_slot("user32.dll", "MapWindowPoints")
            match_instruction_template(code, template, targets)
            matches.append((targets, names, shape))
        except EvidenceError as error:
            failures.append(f"{shape}: {error}")
    require(len(matches) == 1, "Missing/ambiguous complete borrowed ScreenRect recipe: " +
            ("; ".join(failures) if failures else f"{len(matches)} matches, {len(code)} instructions"))
    return matches[0]


def stack_argument(ins, destination, alignment=8):
    require(ins.mnemonic == "lea" and len(ins.operands) == 2 and
            ins.operands[0].type == X86_OP_REG and ins.reg_name(ins.operands[0].reg) == destination and
            ins.operands[1].type == X86_OP_MEM, "Caller argument is not an explicit stack wrapper/result")
    memory = ins.operands[1].mem
    base = ins.reg_name(memory.base)
    require(base in ("rsp", "rbp") and memory.index == 0 and memory.segment == 0 and
            -256 <= memory.disp <= 256 and memory.disp % alignment == 0,
            "Caller argument has an unknown stack location")
    return base, memory.disp, ins.op_str.split(", ", 1)[1]


def borrowed_caller_window(code, index, release_rva):
    require(3 <= index and index + 6 < len(code), "Truncated borrowed ScreenRect caller window")
    wrapper_base, wrapper_offset, wrapper = stack_argument(code[index - 2], "r8")
    result_base, result_offset, result = stack_argument(code[index - 1], "rdx")
    require(wrapper_base == result_base and
            (wrapper_offset + 8 <= result_offset or result_offset + 16 <= wrapper_offset),
            "Borrowed caller result overlaps its owned wrapper")
    hwnd = code[index - 3]
    require(hwnd.mnemonic == "mov" and len(hwnd.operands) == 2 and hwnd.operands[0].type == X86_OP_REG and
            hwnd.reg_name(hwnd.operands[0].reg) == "r9", "Borrowed caller does not pass HWND in R9")
    guard = code[index + 2]
    zero_origin = None
    if (guard.mnemonic, guard.op_str) == ("cmp", f"qword ptr {wrapper}, rdi"):
        zero = find_sequence(code[:index], (("xor", "edi, edi"),))
        require(not any(ins.mnemonic.startswith("j") or ins.mnemonic in ("call", "ret") for ins in code[:zero]),
                "Caller can bypass zero-register initialization")
        for ins in code[zero + 1:index + 2]:
            _, written = ins.regs_access()
            require(not any(ins.reg_name(reg) in ("rdi", "edi", "di", "dil") for reg in written),
                    "Caller null-check register changed after zero initialization")
            if ins.mnemonic.startswith("j"):
                require(len(ins.operands) == 1 and ins.operands[0].type == X86_OP_IMM and
                        ins.operands[0].imm in {item.address for item in code[zero + 1:]},
                        "Caller null-check register has an unbounded incoming branch")
        zero_origin = code[zero].address
    else:
        require((guard.mnemonic, guard.op_str) == ("cmp", f"qword ptr {wrapper}, 0"),
                "Caller does not null-check its same borrowed wrapper")
    require((code[index + 1].mnemonic, code[index + 1].op_str) == ("nop", "") and
            (code[index + 4].mnemonic, code[index + 4].op_str) == ("lea", f"rcx, {wrapper}") and
            immediate_call(code[index + 5:index + 6], release_rva) == 0,
            "Caller does not retain and release its same borrowed wrapper")
    after = index + 6
    if (code[after].mnemonic, code[after].op_str) == ("nop", ""):
        after += 1
    require(after < len(code), "Caller cleanup has no bounded continuation")
    branch = code[index + 3]
    require(branch.mnemonic == "je" and len(branch.operands) == 1 and
            branch.operands[0].type == X86_OP_IMM and branch.operands[0].imm == code[after].address,
            "Caller null branch must skip only its own wrapper release")
    return {"borrowed_wrapper_stack_location": wrapper, "result_stack_location": result,
            "caller_release_rva": code[index + 5].address,
            "null_guard_zero_register_origin_rva": zero_origin,
            "instructions": [instruction_record(ins) for ins in code[index - 3:after]]}


def returned_wrapper_helper(image, symbol, logs):
    """The named as<FrameworkElement> returns its original hidden result buffer."""
    rva, code = function_evidence(image, symbol, logs)
    require(len(code) == 30, "Unreviewed FrameworkElement conversion helper length")
    query, iid_load = code[15], code[14]
    require(query.mnemonic == "call" and len(query.operands) == 1 and query.operands[0].type == X86_OP_IMM and
            iid_load.mnemonic == "lea" and len(iid_load.operands) == 2 and
            iid_load.operands[1].type == X86_OP_MEM and iid_load.reg_name(iid_load.operands[1].mem.base) == "rip",
            "Conversion has no bounded direct QI call and IID load")
    iid_rva = iid_load.address + iid_load.size + iid_load.operands[1].mem.disp
    require(image.bytes(iid_rva, 16, False) == bytes.fromhex("9bd091a3994a7c4b9d8d6fa5d01f6fbf"),
            "Conversion does not query the exact IFrameworkElement IID")
    targets = {key: image.exact(BORROWED_TARGETS[key]) for key in
               ("security_cookie", "security_check", "throw_hresult", "guard_dispatch")}
    targets.update(framework_element_iid=iid_rva, query_interface=image.exact(ICON_QUERY_INTERFACE),
                   query_interface_inner=image.exact(ICON_QUERY_INNER))
    match_instruction_template(code, RETURNED_WRAPPER_BODY, targets)
    _, query_body = function_evidence(image, ICON_QUERY_INTERFACE, logs)
    match_instruction_template(query_body, ICON_QUERY_BODY, targets)
    function_evidence(image, ICON_QUERY_INNER, logs)
    return {"symbol": symbol, "rva": rva, "result_capture_rva": code[6].address,
            "result_return_rva": code[20].address, "framework_element_iid_rva": iid_rva,
            "query_interface_symbol": ICON_QUERY_INTERFACE, "query_interface_target_rva": targets["query_interface"],
            "query_interface_recipe": "icon_root_exact_inner_or_slot_zero_cfg_v1",
            "query_interface_inner_symbol": ICON_QUERY_INNER,
            "query_interface_inner_rva": targets["query_interface_inner"]}


def borrowed_returned_caller_window(image, code, index, caller_name, logs):
    require(index >= 7 and index + 4 < len(code), "Truncated returned-wrapper caller window")
    wrapper_base, wrapper_offset, wrapper = stack_argument(code[index - 7], "rdx")
    stack_argument(code[index - 6], "rcx")
    result_base, result_offset, result = stack_argument(code[index - 1], "rdx")
    require(wrapper_base == result_base and
            (wrapper_offset + 8 <= result_offset or result_offset + 16 <= wrapper_offset),
            "Returned-wrapper caller result overlaps the wrapper")
    kind = "ChevronIconView" if caller_name == next(iter(BORROWED_CALLERS)) else "NotifyIconView"
    conversion = returned_wrapper_helper(image, AS_FRAMEWORK_ELEMENT[kind], logs)
    require(immediate_call(code[index - 5:index - 4], conversion["rva"]) == 0 and
            (code[index - 4].mnemonic, code[index - 4].op_str) == ("nop", "") and
            code[index - 3].mnemonic == "mov" and code[index - 3].op_str.startswith("r9, ") and
            (code[index - 2].mnemonic, code[index - 2].op_str) == ("mov", "r8, rax") and
            (code[index + 1].mnemonic, code[index + 1].op_str) == ("nop", "") and
            (code[index + 2].mnemonic, code[index + 2].op_str) == ("lea", f"rcx, {wrapper}") and
            immediate_call(code[index + 3:index + 4], image.exact(UNKNOWN_DESTRUCTOR)) == 0,
            "Caller must borrow and then destroy its same returned FrameworkElement wrapper")
    return {"borrowed_wrapper_stack_location": wrapper, "result_stack_location": result,
            "caller_destructor_rva": code[index + 3].address, "wrapper_return_proof": conversion,
            "instructions": [instruction_record(ins) for ins in code[index - 7:index + 4]]}


def borrowed_callers(image, screen_rva, release_rva, logs):
    sites, rejected = image.direct_callers(screen_rva)
    require(not rejected, "Unresolved direct-call byte hits prevent a complete borrowed caller audit")
    expected = {image.exact(name): (name, count) for name, count in BORROWED_CALLERS.items()}
    require(len(expected) == len(BORROWED_CALLERS) and len(sites) == sum(BORROWED_CALLERS.values()),
            "Unknown or incomplete borrowed ScreenRect caller set")
    counts = {rva: 0 for rva in expected}
    result = []
    for site in sites:
        start = site["function_rva"]
        require(start in expected, "Borrowed ScreenRect is called by an unreviewed exact function")
        name, _ = expected[start]
        counts[start] += 1
        code, index = site["code"], site["index"]
        if index >= 2 and (code[index - 2].mnemonic, code[index - 2].op_str) == ("mov", "r8, rax"):
            proof = borrowed_returned_caller_window(image, code, index, name, logs)
        else:
            proof = borrowed_caller_window(code, index, release_rva)
        result.append({"caller_rva": start, "caller_symbol": name, "call_rva": site["call_rva"],
                       "caller_end_rva": site["code"][-1].address + site["code"][-1].size, **proof})
    require(all(counts[rva] == value[1] for rva, value in expected.items()),
            "Borrowed caller multiplicities changed")
    for rva, (name, _) in expected.items():
        bounded_evidence(image, rva, name, logs)
    return result


def borrowed_named_cleanup_thunks(image, release_rva, logs, delegated=False, inline_tail=False):
    """Corroborate named local cleanup leaves; do not claim C++ EH table proof."""
    destructor_rva, destructor = function_evidence(image, UNKNOWN_DESTRUCTOR, logs)
    require(len(destructor) == 6 and
            [(ins.mnemonic, ins.op_str) for ins in (destructor[0], destructor[1], destructor[4], destructor[5])] ==
            [("sub", "rsp, 0x28"), ("cmp", "qword ptr [rcx], 0"), ("add", "rsp, 0x28"), ("ret", "")] and
            destructor[2].mnemonic == "je" and len(destructor[2].operands) == 1 and
            destructor[2].operands[0].type == X86_OP_IMM and
            destructor[2].operands[0].imm == destructor[4].address and
            immediate_call(destructor, release_rva) == 3,
            "Unknown borrowed-path local-wrapper destructor")
    decoder = Cs(CS_ARCH_X86, CS_MODE_64)
    decoder.detail = True
    results = []
    offsets = ((0, 0x20), (1, 0x40), (2, 0x28), (3, 0x20)) if inline_tail else (
        ((0, 0x20), (1, 0x40), (2, 0x38), (3, 0x20)) if delegated else
        ((0, 0x48), (1, 0x40), (2, 0x20), (3, 0x20)))
    for number, offset in offsets:
        name = f"`winrt::SystemTray::implementation::DragDropManager::ScreenRectForElement'::`1'::dtor${number}"
        rva = image.exact(name)
        code = list(decoder.disasm(image.bytes(rva, 12), rva))
        require(len(code) == 2 and sum(ins.size for ins in code) == 12 and
                (code[0].mnemonic, code[0].op_str) == ("lea", f"rcx, [rdx + {offset:#x}]") and
                code[1].mnemonic == "jmp" and len(code[1].operands) == 1 and
                code[1].operands[0].type == X86_OP_IMM and code[1].operands[0].imm == destructor_rva,
                "Named exception cleanup is not the reviewed parent-stack-local destructor thunk")
        proof = {"symbol": name, "rva": rva, "parent_stack_offset": offset,
                 "boundary_recipe": "exact_twelve_byte_local_cleanup_leaf_v1",
                 "instructions": [instruction_record(ins) for ins in code]}
        logs.append({"module_role": image.role, **proof})
        results.append(proof)
    require(len({row["rva"] for row in results}) == 3 and
            results[0 if delegated or inline_tail else 2]["rva"] == results[3]["rva"],
            "Named borrowed-path cleanup thunk aliases changed")
    return results


def borrowed_screen(image, code, logs):
    targets, names, shape = match_borrowed_body(image, code)
    release_rva = release_leaf(image, logs)
    if shape == "delegated":
        _, transform = function_evidence(image, TRANSFORM_BOUNDS, logs)
        match_instruction_template(transform, TRANSFORM_BOUNDS_BODY, targets)
    callers = borrowed_callers(image, code[0].address, release_rva, logs)
    thunks = borrowed_named_cleanup_thunks(image, release_rva, logs, shape == "delegated", shape == "inline_tail")
    return {"recipe": "four_argument_borrowed_screen_rect_v1",
            "bounded_body_recipe": {"delegated": "delegated_transform_and_local_cleanup_v1",
                                    "inline": "inline_transform_and_local_cleanup_v1",
                                    "inline_tail": "inline_transform_tail_failure_and_local_cleanup_v1"}[shape],
            "function_rva": code[0].address, "function_end_rva": code[-1].address + code[-1].size,
            "parameters": {"rcx": "opaque manager this, unused before overwrite", "rdx": "hidden RECT result",
                           "r8": "borrowed FrameworkElement wrapper", "r9": "HWND"},
            "return": "RAX is the original result pointer", "borrowed_input_consumed": False,
            "exact_targets": {key: {"symbol": name, "rva": targets[key]} for key, name in names.items()},
            "map_window_points_iat_rva": targets["map_window_points"], "callers": callers,
            "named_local_cleanup_thunks": thunks, "cpp_exception_metadata_parsed": False}


def coordinate_member(code, role):
    legacy = (("mov", "rsi, r8"), ("mov", "rbx, rdx"), ("mov", "rdi, rcx"))
    delegated = (("mov", "rbx, r8"), ("mov", "rdi, rdx"), ("mov", "rsi, rcx"))
    items = [(ins.mnemonic, ins.op_str) for ins in code]
    old = any(items[i:i + 3] == list(legacy) for i in range(len(items) - 2))
    captured = find_sequence(code, legacy if old else delegated)
    result = "rbx" if old else "rdi"
    returned = find_sequence(code, (("mov", f"rax, {result}"),))
    require(captured + 3 < returned, "Coordinate method returns before using its captured result buffer")
    aliases = ("rbx", "ebx", "bx", "bl", "bh") if old else ("rdi", "edi", "di", "dil")
    for ins in code[captured + 3:returned]:
        _, written = ins.regs_access()
        require(not any(ins.reg_name(reg) in aliases for reg in written),
                "Coordinate method overwrites its hidden result pointer")
    offsets = (0, 4) if role == "invocation_point" else (0, 4, 8, 12)
    writes = []
    for offset in offsets:
        address = f"[{result}]" if not offset else f"[{result} + {offset:#x}]" if offset >= 10 else f"[{result} + {offset}]"
        matches = [i for i, ins in enumerate(code) if ins.mnemonic == "movss" and
                   ins.op_str.startswith(f"dword ptr {address}, xmm")]
        require(len(matches) == 1 and captured < matches[0] < returned,
                "Coordinate method is missing an exact result field write")
        writes.append(matches[0])
    require(writes == sorted(writes), "Coordinate method result fields are written out of order")
    return "rsi_input_rbx_result_v1" if old else "rbx_input_rdi_result_v1"


def owned_screen(image, code, logs):
    find_sequence(code, (("mov", "r15, r8"), ("mov", "rsi, rdx"), ("mov", "r14, rcx")))
    find_sequence(code, (("movdqu", "xmmword ptr [r14], xmm0"),))
    find_sequence(code, (("mov", "rax, r14"),))
    cleanup = owned_cleanup(image, code, logs)
    complete = cleanup["recipe"] == "delegated_with_transform_temporary_v1"
    if complete:
        names = {key: BORROWED_TARGETS[key] for key in
                 ("transform", "actual_width", "actual_height", "release", "security_check",
                  "security_cookie", "guard_dispatch", "throw_hresult")}
        names.update(destructor=UNKNOWN_DESTRUCTOR, transform_bounds=TRANSFORM_BOUNDS)
        targets = {key: image.exact(name) for key, name in names.items()}
        targets.update(get_dpi=image.import_slot("user32.dll", "GetDpiForWindow"),
                       map_window_points=image.import_slot("user32.dll", "MapWindowPoints"),
                       mul_div=image.import_slot("api-ms-win-core-largeinteger-l1-1-0.dll", "MulDiv"))
        match_instruction_template(code, OWNED_TRANSFORM_BODY, targets)
        _, transform = function_evidence(image, TRANSFORM_BOUNDS, logs)
        match_instruction_template(transform, TRANSFORM_BOUNDS_BODY, targets)
    return {"recipe": "three_argument_owned_screen_rect_v1", "owned_value_cleanup": cleanup,
            "bounded_body_recipe": ("owned_transform_and_window_dpi_v1" if complete else
                                    "owned_arguments_result_and_cleanup_v1"),
            "complete_instruction_template": complete,
            "parameters": {"rcx": "hidden RECT result", "rdx": "owned FrameworkElement wrapper", "r8": "HWND"},
            "return": "RAX is the original result pointer", "owned_input_consumed": True}


def owned_caller_window(image, code, index):
    """A QI-created local is transferred to the consuming helper, then RECT is read."""
    require(index >= 12 and index + 1 < len(code), "Truncated owned ScreenRect caller window")
    wrapper_base, wrapper_offset, wrapper = stack_argument(code[index - 2], "rdx", 1)
    result_base, result_offset, result = stack_argument(code[index - 1], "rcx", 1)
    if wrapper_base == result_base:
        require(wrapper_offset + 8 <= result_offset or result_offset + 16 <= wrapper_offset,
                "Owned caller result overlaps the consumed wrapper")
    else:
        # This reviewed frame establishes RSP=entry-0x108 and RBP=entry-0x5f.
        # Thus RSP+0x20 and RBP+7 are disjoint; different base names alone do not prove that.
        require((wrapper, result) == ("[rsp + 0x20]", "[rbp + 7]") and
                [(ins.mnemonic, ins.op_str) for ins in code[:7]] == [
                    ("mov", "rax, rsp"), ("mov", "qword ptr [rax + 0x10], rbx"),
                    ("mov", "qword ptr [rax + 0x18], rsi"), ("mov", "qword ptr [rax + 0x20], rdi"),
                    ("push", "rbp"), ("lea", "rbp, [rax - 0x5f]"), ("sub", "rsp, 0x100")],
                "Owned caller has an unreviewed relation between stack bases")
    via_rbx = (code[index - 8].mnemonic, code[index - 8].op_str) == ("mov", "rcx, rbx")
    query_index = index - (10 if via_rbx else 9)
    _, _, query_wrapper = stack_argument(code[query_index], "r8", 1)
    iid_load = code[query_index + 1]
    require(iid_load.mnemonic == "lea" and len(iid_load.operands) == 2 and
            iid_load.operands[0].type == X86_OP_REG and iid_load.reg_name(iid_load.operands[0].reg) == "rdx" and
            iid_load.operands[1].type == X86_OP_MEM and
            iid_load.reg_name(iid_load.operands[1].mem.base) == "rip" and
            iid_load.operands[1].mem.index == 0,
            "Owned caller does not load the exact FrameworkElement IID")
    iid_rva = iid_load.address + iid_load.size + iid_load.operands[1].mem.disp
    require(image.bytes(iid_rva, 16, False) == bytes.fromhex("9bd091a3994a7c4b9d8d6fa5d01f6fbf"),
            "Owned caller queries a different interface")
    query = code[query_index - 2:index - 5]
    template = f"""
and qword ptr {query_wrapper}, 0
mov rax, qword ptr [{'rbx' if via_rbx else 'rcx'}]
lea r8, {query_wrapper}
lea rdx, [$framework_element_iid]
""" + ("mov rcx, rbx\n" if via_rbx else "") + "mov rax, qword ptr [rax]\ncall [$guard_dispatch]"
    match_instruction_template(query, template,
                               {"framework_element_iid": iid_rva, "guard_dispatch": image.exact(GUARD_DISPATCH)})
    require([(ins.mnemonic, ins.op_str) for ins in code[index - 5:index - 3]] ==
            [("mov", f"rax, qword ptr {query_wrapper}"), ("mov", f"qword ptr {wrapper}, rax")],
            "Owned caller does not transfer its queried wrapper")
    hwnd, returned = code[index - 3], code[index + 1]
    require((hwnd.mnemonic, hwnd.op_str) in (("mov", "r8, rdi"), ("mov", "r8, rsi")) and
            returned.mnemonic == "movups" and len(returned.operands) == 2 and
            returned.operands[1].type == X86_OP_MEM and
            returned.reg_name(returned.operands[1].mem.base) == "rax" and
            returned.operands[1].mem.disp == 0 and returned.operands[1].mem.index == 0,
            "Owned caller changed its HWND argument or returned RECT use")
    # The RCX variant has a complete local null-or-QI diamond. Every branch is
    # tied to the actual decoded instruction, not just its mnemonic.
    if not via_rbx:
        require(index >= 15 and [(ins.mnemonic, ins.op_str) for ins in
                (code[index - 15], code[index - 13])] ==
                [("test", "rcx, rcx"), ("and", f"qword ptr {wrapper}, rcx")],
                "Owned caller lost its null wrapper initialization")
        for ins, mnemonic, target in ((code[index - 14], "jne", code[index - 11].address),
                                      (code[index - 12], "jmp", code[index - 3].address)):
            require(ins.mnemonic == mnemonic and len(ins.operands) == 1 and
                    ins.operands[0].type == X86_OP_IMM and ins.operands[0].imm == target,
                    "Owned caller null branch changed its wrapper initialization")
    null_jump = code[index - 12].address if not via_rbx else None
    if via_rbx:
        diamonds = []
        for i in range(query_index - 4):
            test, branch, clear, jump = code[i:i + 4]
            if ((test.mnemonic, test.op_str), (clear.mnemonic, clear.op_str)) != (
                    ("test", "rbx, rbx"), ("and", f"qword ptr {wrapper}, rbx")):
                continue
            if all(ins.mnemonic == mnemonic and len(ins.operands) == 1 and
                   ins.operands[0].type == X86_OP_IMM and ins.operands[0].imm == target
                   for ins, mnemonic, target in ((branch, "jne", query[0].address),
                                                 (jump, "jmp", code[index - 3].address))):
                diamonds.append((test.address, jump.address))
        require(len(diamonds) == 1, "Owned caller lost its exact RBX null-or-QI diamond")
        null_start, null_jump = diamonds[0]
        require(not any(ins.mnemonic.startswith("j") and len(ins.operands) == 1 and
                        ins.operands[0].type == X86_OP_IMM and null_start < ins.operands[0].imm <= null_jump
                        for ins in code), "Owned caller branch bypasses its RBX null test")
    start = query[0].address
    for ins in code:
        if ins.mnemonic.startswith("j") and len(ins.operands) == 1 and ins.operands[0].type == X86_OP_IMM:
            target = ins.operands[0].imm
            if start < target <= code[index].address:
                require(ins.address == null_jump and target == code[index - 3].address,
                        "Owned caller branch can bypass queried-wrapper transfer")
    return {"recipe": "qi_owned_wrapper_to_rect_v1", "owned_wrapper_stack_location": wrapper,
            "result_stack_location": result, "query_wrapper_stack_location": query_wrapper,
            "framework_element_iid_rva": iid_rva, "query_interface_slot": 0,
            "instructions": [instruction_record(ins) for ins in code[query_index - 2:index + 2]]}


def owned_callers(image, screen_rva, logs):
    sites, rejected = image.direct_callers(screen_rva)
    require(not rejected, "Unresolved direct-call byte hits prevent a complete owned caller audit")
    require(0 < len(sites) <= len(OWNED_CALLER_PURPOSES), "Missing/unknown owned ScreenRect caller set")
    bindings = {image.exact(name): (name, purpose) for name, purpose in OWNED_CALLER_PURPOSES.items()
                if any(s["decorated_name"] == name for s in image.symbols)}
    require(len(bindings) == len({name for name in OWNED_CALLER_PURPOSES
                                if any(s["decorated_name"] == name for s in image.symbols)}),
            "Ambiguous owned caller function aliases")
    result, purposes, seen = [], set(), set()
    for site in sites:
        start = site["function_rva"]
        require(start in bindings and start not in seen, "Unknown/repeated owned ScreenRect consumer")
        seen.add(start)
        name, purpose = bindings[start]
        proof = owned_caller_window(image, site["code"], site["index"])
        purposes.add(purpose)
        result.append({"caller_rva": start, "caller_symbol": name, "call_rva": site["call_rva"], **proof})
        bounded_evidence(image, start, name, logs)
    require(len(purposes) == 1, "Owned ScreenRect has mixed unreviewed consumer purposes")
    return {"purpose": purposes.pop(), "callers": result, "direct_call_audit_complete": True,
            "indirect_callers_proven_absent": False}


def geometry_trait(image, decorated, logs, owned):
    rva, code = function_evidence(image, decorated, logs)
    entry = {"role": "screen_rect", "rva": rva, **image.section(rva, 16, True),
             "prologue": list(image.bytes(rva, 16)), "decorated_symbol": decorated, "register_recipe": None}
    abi = "system_tray_coordinates_x64_v1" if owned else "system_tray_drag_drop_borrowed_coordinates_x64_v1"
    result = {"trait_id": "owned_screen_rect" if owned else "drag_drop_borrowed", "status": "unreviewed",
              "abi_candidate": abi, "entry": entry, "abi_status": "unreviewed", "evidence": None,
              "usage": {"purpose": "unreviewed" if owned else "drag_drop_hit_testing"}, "analysis_blockers": []}
    try:
        result["evidence"] = owned_screen(image, code, logs) if owned else borrowed_screen(image, code, logs)
        result["abi_status"] = "reviewed"
    except EvidenceError as error:
        result["analysis_blockers"].append({"stage": "abi", "error": str(error)})
    try:
        if owned:
            result["usage"] = owned_callers(image, rva, logs)
            result["trait_id"] = {"copilot_button_geometry": "copilot_button_owned",
                                  "notification_area_bounds": "notification_area_owned"}[result["usage"]["purpose"]]
        elif result["evidence"]:
            result["usage"] = {"purpose": "drag_drop_hit_testing", "callers": result["evidence"]["callers"],
                               "direct_call_audit_complete": True, "indirect_callers_proven_absent": False}
    except EvidenceError as error:
        result["analysis_blockers"].append({"stage": "usage", "error": str(error)})
    if not result["analysis_blockers"]:
        result["status"] = "reviewed"
    return result


def tray_layout(image: Image, logs):
    entries = []
    owned_present = any(s["decorated_name"] == TRAY_SYMBOLS[2][1] for s in image.symbols)
    borrowed_present = any(s["decorated_name"] == BORROWED_SCREEN for s in image.symbols)
    require(owned_present or borrowed_present, "Missing reviewed screen-rectangle signatures")
    present = [(TRAY_SYMBOLS[2][1], True)] if owned_present else []
    if borrowed_present:
        present.append((BORROWED_SCREEN, False))
    # Ambiguous symbol identity is a module-level rejection, not a choice among traits.
    rvas = [image.exact(name) for name, _ in present]
    require(len(set(rvas)) == len(rvas), "Distinct geometry signatures alias the same function")
    ranges = []
    for rva in rvas:
        tail = image.function(rva)[-1]
        ranges.append((rva, tail.address + tail.size))
    require(all(end <= other or other_end <= start for i, (start, end) in enumerate(ranges)
                for other, other_end in ranges[i + 1:]), "Geometry function boundaries overlap")
    for role, decorated in TRAY_SYMBOLS[:2]:
        rva, code = function_evidence(image, decorated, logs)
        entries.append({"role": role, "rva": rva, **image.section(rva, 16, True),
                        "prologue": list(image.bytes(rva, 16)), "decorated_symbol": decorated,
                        "register_recipe": coordinate_member(code, role)})
    traits = [geometry_trait(image, name, logs, owned) for name, owned in present]
    capabilities = {}
    for purpose in ("notification_area_bounds", "drag_drop_hit_testing", "copilot_button_geometry"):
        matches = [trait for trait in traits if trait["usage"]["purpose"] == purpose]
        require(len(matches) <= 1, "Ambiguous geometry implementations for one consumer")
        capabilities[purpose] = ({"status": matches[0]["status"], "trait_id": matches[0]["trait_id"]} if matches else
                                 {"status": "unavailable", "trait_id": None})
    single = traits[0] if len(traits) == 1 and traits[0]["status"] == "reviewed" else None
    if single:
        entries.append(single["entry"])
    owned_proof = next((trait["evidence"] for trait in traits if trait["abi_candidate"] ==
                        "system_tray_coordinates_x64_v1"), None)
    borrowed_proof = next((trait["evidence"] for trait in traits if trait["abi_candidate"] ==
                           "system_tray_drag_drop_borrowed_coordinates_x64_v1"), None)
    return {"abi_candidate": single["abi_candidate"] if single else None, "module_name": image.path.name,
            "entries": entries, "screen_rect_traits": traits, "geometry_capabilities": capabilities,
            "implementation_selection": "single_reviewed_implementation" if single else "requires_consumer_choice",
            "owned_value_cleanup": owned_proof["owned_value_cleanup"] if owned_proof else None,
            "borrowed_value_evidence": borrowed_proof,
            "production_profile_match_checked": False}


def topology_clues(image: Image):
    """Names are clues only: they cannot prove a live tree, root or ownership."""
    names = ("SystemTray.SystemTrayFrame", "SystemTrayFrameGrid", "TimeInnerTextBlock",
             "DateInnerTextBlock", "NotificationAreaIcons", "SecondaryClockStack")
    literals = []
    for name in names:
        encodings = [encoding for encoding in ("utf-8", "utf-16-le") if name.encode(encoding) in image.data]
        if encodings:
            literals.append({"name": name, "encodings": encodings})
    symbols = [s for s in image.symbols if "SystemTrayFrame" in s["name"]]
    return {"status": "unverified_live_tree", "recipe_to_test": "system_tray_frame_v1",
            "literals": literals, "symbol_count": len(symbols),
            "sample_symbols": symbols[:12], "runtime_topology_checked": False}
