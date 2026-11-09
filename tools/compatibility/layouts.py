"""Reviewed, narrow x64 recipes. Unknown instruction shapes stay blocked."""
from __future__ import annotations

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


def implementation_source_field(image, implementation, logs):
    """Two observed register allocations, with direct field use in each."""
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


def host_layout(image: Image, logs):
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
    require(len(code) in (96, 114), "Borrowed ScreenRect body has an unreviewed instruction count")
    delegated = len(code) == 96
    names = dict(BORROWED_TARGETS)
    if delegated:
        names.update(display_factory=DISPLAY_CURRENT_VIEW, transform_bounds=TRANSFORM_BOUNDS,
                     destructor=UNKNOWN_DESTRUCTOR)
    targets = {key: image.exact(name) for key, name in names.items()}
    targets["map_window_points"] = image.import_slot("user32.dll", "MapWindowPoints")
    match_instruction_template(code, BORROWED_DELEGATED_BODY if delegated else BORROWED_BODY, targets)
    return targets, names, delegated


def stack_argument(ins, destination):
    require(ins.mnemonic == "lea" and len(ins.operands) == 2 and
            ins.operands[0].type == X86_OP_REG and ins.reg_name(ins.operands[0].reg) == destination and
            ins.operands[1].type == X86_OP_MEM, "Caller argument is not an explicit stack wrapper/result")
    memory = ins.operands[1].mem
    base = ins.reg_name(memory.base)
    require(base in ("rsp", "rbp") and memory.index == 0 and memory.segment == 0 and
            -256 <= memory.disp <= 256 and memory.disp % 8 == 0,
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


def borrowed_named_cleanup_thunks(image, release_rva, logs, delegated=False):
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
    offsets = ((0, 0x20), (1, 0x40), (2, 0x38), (3, 0x20)) if delegated else (
        (0, 0x48), (1, 0x40), (2, 0x20), (3, 0x20))
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
    require(len({row["rva"] for row in results}) == 3 and results[0 if delegated else 2]["rva"] == results[3]["rva"],
            "Named borrowed-path cleanup thunk aliases changed")
    return results


def borrowed_screen(image, code, logs):
    targets, names, delegated = match_borrowed_body(image, code)
    release_rva = release_leaf(image, logs)
    if delegated:
        _, transform = function_evidence(image, TRANSFORM_BOUNDS, logs)
        match_instruction_template(transform, TRANSFORM_BOUNDS_BODY, targets)
    callers = borrowed_callers(image, code[0].address, release_rva, logs)
    thunks = borrowed_named_cleanup_thunks(image, release_rva, logs, delegated)
    return {"recipe": "four_argument_borrowed_screen_rect_v1",
            "bounded_body_recipe": "delegated_transform_and_local_cleanup_v1" if delegated else "inline_transform_and_local_cleanup_v1",
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


def tray_layout(image: Image, logs):
    entries = []
    cleanup = None
    borrowed = None
    owned_present = any(s["decorated_name"] == TRAY_SYMBOLS[2][1] for s in image.symbols)
    borrowed_present = any(s["decorated_name"] == BORROWED_SCREEN for s in image.symbols)
    require(owned_present != borrowed_present, "Missing/ambiguous reviewed screen-rectangle ABI entry")
    selected = TRAY_SYMBOLS if owned_present else (*TRAY_SYMBOLS[:2], ("screen_rect", BORROWED_SCREEN))
    for role, decorated in selected:
        rva, code = function_evidence(image, decorated, logs)
        register_recipe = None
        if role in ("invocation_point", "icon_bounds"):
            register_recipe = coordinate_member(code, role)
        elif owned_present:
            find_sequence(code, (("mov", "r15, r8"), ("mov", "rsi, rdx"), ("mov", "r14, rcx")))
            find_sequence(code, (("movdqu", "xmmword ptr [r14], xmm0"),))
            find_sequence(code, (("mov", "rax, r14"),))
            cleanup = owned_cleanup(image, code, logs)
        else:
            borrowed = borrowed_screen(image, code, logs)
        entries.append({"role": role, "rva": rva, **image.section(rva, 16, True),
                        "prologue": list(image.bytes(rva, 16)), "decorated_symbol": decorated,
                        "register_recipe": register_recipe})
    return {"abi_candidate": ("system_tray_coordinates_x64_v1" if owned_present else
                              "system_tray_drag_drop_borrowed_coordinates_x64_v1"), "module_name": image.path.name,
            "entries": entries, "owned_value_cleanup": cleanup, "borrowed_value_evidence": borrowed,
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
