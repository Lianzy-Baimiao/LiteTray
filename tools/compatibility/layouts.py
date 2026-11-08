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
TRAY_SYMBOLS = (
    ("invocation_point", "?GetInvocationPointRelativeToScreen@NotificationAreaIconsDataModel@implementation@SystemTray@winrt@@AEAA?AUPoint@Foundation@Windows@4@AEBU5674@@Z"),
    ("icon_bounds", "?GetIconBoundsRelativeToScreen@NotificationAreaIconsDataModel@implementation@SystemTray@winrt@@AEAA?AURect@Foundation@Windows@4@AEBU5674@@Z"),
    ("screen_rect", "?GetScreenRectFromXamlElement@@YA?AUtagRECT@@UFrameworkElement@Xaml@UI@Windows@winrt@@PEAUHWND__@@@Z"),
)
UNKNOWN_DESTRUCTOR = "??1IUnknown@Foundation@Windows@winrt@@QEAA@XZ"
UNKNOWN_RELEASE = "?unconditional_release_ref@IUnknown@Foundation@Windows@winrt@@AEAAXXZ"
GUARD_DISPATCH = "__guard_dispatch_icall_fptr"


def instruction_record(ins):
    return {"rva": ins.address, "bytes": ins.bytes.hex(), "instruction": f"{ins.mnemonic} {ins.op_str}".strip()}


def function_evidence(image, decorated, logs):
    rva = image.exact(decorated)
    code = image.function(rva)
    logs.append({"module_role": image.role, "symbol": decorated, "rva": rva,
                 "instructions": [instruction_record(ins) for ins in code]})
    return rva, code


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


def owned_cleanup(image, code, logs):
    """Two reviewed call graphs, including the same consumed wrapper in both."""
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
        cleanup = immediate_call(code, destructor_rva)
        require(cleanup > 0 and (code[cleanup - 1].mnemonic, code[cleanup - 1].op_str) == ("mov", "rcx, rsi"),
                "Owned FrameworkElement does not flow directly to its destructor")
        recipe = "delegated_null_check_release_v1"
    written = find_sequence(code, (("movdqu", "xmmword ptr [r14], xmm0"),))
    returned = find_sequence(code, (("mov", "rax, r14"),))
    require(written < cleanup < returned, "Owned FrameworkElement cleanup is not between result write and return")
    return {"recipe": recipe, "release_rva": release_rva, "cleanup_call_rva": code[cleanup].address}


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
    getter_rva, getter = function_evidence(shell, GET_CONTENT, logs)
    impl_rva, implementation = function_evidence(shell, IMPL_CONTENT, logs)
    require(shell.pointer(child + 8 * 8) == getter_rva, "Child table slot 8 is not exact get_XamlContent")
    require(shell.pointer(factory + 6 * 8) == shell.exact(FACTORY_CREATE), "Factory slot 6 is not exact CreateInstance")
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
    fields = [(i, ins) for i, ins in enumerate(implementation)
              if ins.mnemonic == "lea" and ins.op_str.startswith("rdi, [rcx")]
    require(len(fields) == 1, "Unknown/ambiguous implementation source field")
    impl_index, impl_field = fields[0]
    offset = member_adjustment(impl_field, "rdi", "rcx")
    child_field = aligned_offset(delta + offset)
    require(impl_index + 3 < len(implementation) and
            implementation[impl_index + 1].mnemonic == "lea" and implementation[impl_index + 1].op_str.startswith("rdx,") and
            (implementation[impl_index + 2].mnemonic, implementation[impl_index + 2].op_str) == ("mov", "rcx, rdi") and
            implementation[impl_index + 3].mnemonic == "call", "Unknown use of source field")
    table_evidence = {}
    for label, table, count in (("child", child, 21), ("factory", factory, 7)):
        table_evidence[label] = {"rva": table, "slots": [shell.pointer(table + 8 * slot) for slot in range(count)]}
    return {
        "recipe_candidate": "xaml_explorer_host_child_v1", "host_child_offset": host_field,
        "child_source_offset": child_field, "child_vtable_rva": child, "factory_vtable_rva": factory,
        "evidence": {"has_focus_rva": focus_rva, "child_getter_rva": getter_rva,
                     "implementation_getter_rva": impl_rva, "abi_this_adjustment": delta,
                     "implementation_source_field": offset, "vtables": table_evidence},
    }


def tray_layout(image: Image, logs):
    entries = []
    cleanup = None
    for role, decorated in TRAY_SYMBOLS:
        rva, code = function_evidence(image, decorated, logs)
        if role in ("invocation_point", "icon_bounds"):
            find_sequence(code, (("mov", "rsi, r8"), ("mov", "rbx, rdx"), ("mov", "rdi, rcx")))
            find_sequence(code, (("mov", "rax, rbx"),))
            require(any(ins.mnemonic == "movss" and ins.op_str == "dword ptr [rbx], xmm0" for ins in code),
                    "Missing member result write through hidden return pointer")
        else:
            find_sequence(code, (("mov", "r15, r8"), ("mov", "rsi, rdx"), ("mov", "r14, rcx")))
            find_sequence(code, (("movdqu", "xmmword ptr [r14], xmm0"),))
            find_sequence(code, (("mov", "rax, r14"),))
            cleanup = owned_cleanup(image, code, logs)
        entries.append({"role": role, "rva": rva, **image.section(rva, 16, True),
                        "prologue": list(image.bytes(rva, 16)), "decorated_symbol": decorated})
    return {"abi_candidate": "system_tray_coordinates_x64_v1", "module_name": image.path.name,
            "entries": entries, "owned_value_cleanup": cleanup,
            "production_module_role_supported": image.path.name.casefold() == "systemtray.dll"}


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
