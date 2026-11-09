"""Offline rejection tests: no system DLLs, downloads, COM or Explorer access."""
from pathlib import Path
import struct
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest

from capstone import Cs, CS_ARCH_X86, CS_MODE_64

from evidence import EvidenceError, Image, acquire_pdb, pdb_identity, within
from inspect_modules import choices
from layouts import (GUARD_DISPATCH, GUARD_DISPATCH_FUNCTION, TRANSFORM_TO_VISUAL,
                     UNKNOWN_DESTRUCTOR, UNKNOWN_RELEASE, CONTENT_CONSUMER,
                     RAW_GET_CONTENT, RAW_IMPL_CONTENT, RAW_FACTORY_CREATE, GET_CONTENT, IMPL_CONTENT,
                     FACTORY_IMPLEMENTATION, BORROWED_TARGETS, BORROWED_CALLERS, BORROWED_SCREEN,
                     AS_FRAMEWORK_ELEMENT, DISPLAY_CURRENT_VIEW, TRANSFORM_BOUNDS,
                     ICON_QUERY_INTERFACE, ICON_QUERY_INNER,
                     aligned_offset, anchored_symbol, anchored_source_functions, borrowed_caller_window,
                     borrowed_returned_caller_window, borrowed_named_cleanup_thunks, coordinate_member,
                     find_sequence, implementation_source_field, match_borrowed_body, owned_cleanup,
                     returned_wrapper_helper)


def pdb_fixture(info_age=3, dbi_age=1):
    """Minimal MSF with the two identity streams and independent public age."""
    block_size = 512
    image = bytearray(block_size * 6)
    image[:32] = b"Microsoft C/C++ MSF 7.00\r\n\x1aDS\0\0\0"
    # Directory: 4 stream lengths, then blocks for streams 1 and 3.
    directory = struct.pack("<7I", 4, 0, 28, 0, 12, 4, 5)
    struct.pack_into("<6I", image, 32, block_size, 1, 6, len(directory), 0, 2)
    struct.pack_into("<I", image, block_size * 2, 3)
    image[block_size * 3:block_size * 3 + len(directory)] = directory
    struct.pack_into("<3I", image, block_size * 4, 20000404, 123, info_age)
    image[block_size * 4 + 12:block_size * 4 + 28] = bytes(range(16))
    struct.pack_into("<iII", image, block_size * 5, -1, 19990903, dbi_age)
    return image


def cleanup_fixture(delegated=False):
    """Tiny call graphs with result write, owned wrapper cleanup and return."""
    decoder = Cs(CS_ARCH_X86, CS_MODE_64)
    decoder.detail = True
    call = lambda address, target: b"\xe8" + struct.pack("<i", target - address - 5)
    release = (bytes.fromhex("488b11 48832100 488bca 488b02 488b4010 48ff25") +
               struct.pack("<i", 0x4000 - 0x2018) + b"\xcc\xcc\xcc")
    destructor = (bytes.fromhex("4883ec28 48833900 7405") + call(0x300A, 0x2000) +
                  bytes.fromhex("4883c428 c3"))
    screen = bytes.fromhex("f3410f7f06")
    if not delegated:
        screen += bytes.fromhex("48833e00 7408")
    screen += bytes.fromhex("488bce")
    screen += call(0x1000 + len(screen), 0x3000 if delegated else 0x2000)
    screen += bytes.fromhex("498bc6 c3")
    addresses = {UNKNOWN_RELEASE: 0x2000, UNKNOWN_DESTRUCTOR: 0x3000, GUARD_DISPATCH: 0x4000}
    image = SimpleNamespace(role="tray", exact=addresses.__getitem__,
                            bytes=lambda rva, size: release[:size],
                            section=lambda *args: None,
                            function=lambda rva: list(decoder.disasm(destructor, rva)))
    return image, list(decoder.disasm(screen, 0x1000)), release, decoder


def transform_cleanup_fixture(stack_displacement=0xB0, double_owned=False, extra_release=None):
    image, _, _, decoder = cleanup_fixture(delegated=True)
    call = lambda address, target: b"\xe8" + struct.pack("<i", target - address - 5)
    screen = bytes.fromhex("488365a000 4c8d45a0 488d55b0 488bce")
    screen += call(0x1000 + len(screen), 0x5000)
    screen += bytes.fromhex("90 48837da000 7409 488d4da0")
    screen += call(0x1000 + len(screen), 0x2000)
    if extra_release is not None:
        screen += extra_release
        screen += call(0x1000 + len(screen), 0x2000)
    screen += bytes.fromhex("f3410f7f06")
    screen += bytes.fromhex("488bce") if double_owned else bytes.fromhex("488d4d") + bytes([stack_displacement])
    screen += call(0x1000 + len(screen), 0x3000)
    screen += bytes.fromhex("90 488bce")
    screen += call(0x1000 + len(screen), 0x3000)
    screen += bytes.fromhex("498bc6 c3")
    original_exact = image.exact
    image.exact = lambda name: 0x5000 if name == TRANSFORM_TO_VISUAL else original_exact(name)
    original_function = image.function
    transform = bytes.fromhex("498bf0 488bda 488b4c2430 48890b 488bc3 c3")
    image.function = lambda rva: list(decoder.disasm(transform, rva)) if rva == 0x5000 else original_function(rva)
    return image, list(decoder.disasm(screen, 0x1000)), decoder


def source_field_fixture(inline=False):
    decoder = Cs(CS_ARCH_X86, CS_MODE_64)
    decoder.detail = True
    # Use the observed field initialization and invocation sequences; no system
    # image is needed to test wrong-object and ambiguous-field rejections.
    if not inline:
        code = bytes.fromhex("488d7920 488d5008 488bcf e800100000")
        code += bytes.fromhex("488d542420 488bcf")
        code += b"\xe8" + struct.pack("<i", 0x5000 - 0x1000 - len(code) - 5)
    else:
        code = bytes.fromhex("488d7120 498d53e8 488bce e800100000 84c0 7520 "
                             "488364242000 488b0e 488b01 488d542420 488b4030 e800100000 "
                             "90909090909090 cc")
    instructions = list(decoder.disasm(code, 0x1000))
    thunk_rva = [ins.operands[0].imm for ins in instructions if ins.mnemonic == "call"][-1]
    def thunk_bytes(rva, count):
        if rva != thunk_rva:
            raise EvidenceError("Unknown synthetic thunk")
        return b"\xe9" + struct.pack("<i", 0x5000 - rva - 5)
    image = SimpleNamespace(role="shell", bytes=thunk_bytes, exact=lambda name: 0x5000,
                            section=lambda *args: None)
    return instructions, decoder, image


def source_alias_fixture():
    decoder = Cs(CS_ARCH_X86, CS_MODE_64)
    decoder.detail = True
    call = lambda address, target: b"\xe8" + struct.pack("<i", target - address - 5)
    getter = call(0x1000, 0x3000) + b"\xc3"
    factory = bytes.fromhex("498bd9 4c89442428 49832100 4c8d4c2428 4c8bc2 488d542420")
    factory += call(0x2000 + len(factory), 0x5000) + b"\xc3"
    bodies = {0x1000: getter, 0x2000: factory, 0x3000: b"\xc3", 0x4000: b"\xc3", 0x5000: b"\xc3"}
    image = Image.__new__(Image)
    image.role = "shell"
    image.symbols = [{"decorated_name": name, "rva": rva} for name, rva in (
        (RAW_GET_CONTENT, 0x1000), (RAW_FACTORY_CREATE, 0x2000),
        (RAW_IMPL_CONTENT, 0x3000), (RAW_IMPL_CONTENT, 0x4000), (FACTORY_IMPLEMENTATION, 0x5000))]
    image.pointer = {0x6000 + 64: 0x1000, 0x7000 + 48: 0x2000}.__getitem__
    image.function = lambda rva: list(decoder.disasm(bodies[rva], rva))
    return image, bodies, decoder


def borrowed_body_fixture(delegated=False):
    """Literal instruction fixtures; no DLL, symbol server or local evidence reads."""
    decoder = Cs(CS_ARCH_X86, CS_MODE_64)
    decoder.detail = True
    if not delegated:
        start = 0x2CBB3C
        raw = bytes.fromhex("""
            4055 53 56 57 4156 488bec 4881ec80000000 0f29742470 488b05af343e00 4833c4 488945e0
            4d8bf1 498bf8 488bf2 488d55c8 498bc8 e81b030000 90 488d55c0 488bc8 e86eb3d3ff 90
            4c8bc0 488d55a0 488bcf e89219fdff 488bd8 488bcf e8cb57fcff 0f57f6 f20f5af0
            488bcf e8f4ebffff 0f57c9 f20f5ac8 8365b000 8365b400 f30f1175b8 f30f114dbc
            0f57c0 0f1145d0 488b0b 488b01 0f1045b0 660f7f45b0 4c8d45d0 488d55b0 488b4048
            ff152b0f3000 85c0 7908 8bc8 e8b40fdaff 90 48837da000 740a 488d4da0 e8d78bd5ff
            90 48837dc000 740a 488d4dc0 e8c68bd5ff 90 48837dc800 7409 488d4dc8 e8b58bd5ff
            488d4da0 e8ec010000 90 488d4da0 e8f612d6ff 0f57f6 f20f5af0 48837da000 7409
            488d4da0 e88b8bd5ff f30f104dd0 f30f59ce f30f2cc1 8906 f30f1055d4 f30f59d6
            f30f2cc2 894604 f30f1045d8 f30f59c6 f30f58c1 f30f2cd0 895608 f30f104ddc
            f30f59ce f30f58ca f30f2cd1 89560c 41b902000000 4c8bc6 33d2 498bce ff1510d44200
            488bc6 488b4de0 4833cc e821150a00 0f28742470 4881c480000000 415e 5f 5e 5b 5d c3
        """)
        values = (0x2CBE90, 0x6EF0, 0x29D524, 0x291368, 0x2CA7A0, 0x2CF34,
                  0x2CBE20, 0x6CBAC, 0x247E0, 0x36D1E0, 0x6AF008, 0x5CCB18)
        names = dict(BORROWED_TARGETS)
        slot = 0x6F90C0
    else:
        start = 0x2C60A4
        raw = bytes.fromhex("""
            4055 53 56 57 4156 488bec 4883ec70 0f29742460 488b054aff2600 4833c4 488945e8
            4d8bf1 498bf8 488bf2 488d55b0 498bc8 e81a010000 90 488d55d0 488bc8 e8ed51d4ff
            90 4c8bc0 488d55c8 488bcf e87107f9ff 488bd8 488bcf e8061ef6ff 0f57f6 f20f5af0
            488bcf e8b71ceeff 0f57c9 f20f5ac8 8365b800 8365bc00 f30f1175c0 f30f114dc4
            4c8d45b8 488d55d8 488bcb e8521defff 90 488d4dc8 e85cf4d3ff 90 488d4dd0
            e852f4d3ff 90 488d4db0 e848f4d3ff 488d4db0 e897dafdff 90 488bc8 e832e8fdff
            0f57f6 f20f5af0 488d4db0 e826f4d3ff f30f104dd8 f30f59ce f30f2cc1 8906
            f30f1055dc f30f59d6 f30f2cc2 894604 f30f1045e0 f30f59c6 f30f58c1 f30f2cd0
            895608 f30f104de4 f30f59ce f30f58ca f30f2cd1 89560c 41b902000000 4c8bc6 33d2
            498bce ff15abcf2a00 488bc6 488b4de8 4833cc e82c38fbff 0f28742460 4883c470
            415e 5f 5e 5b 5d c3
        """)
        values = (0x2C61F4, 0xB2D4, 0x256868, 0x227F08, 0x1A7DC8, 0x2A499C,
                  0x2A3BF8, 0x595FC, 0x84E0, 0x279A10, 0x536008, 0x479B00, 0x1B7E8C, 0x55A0)
        names = {**BORROWED_TARGETS, "display_factory": DISPLAY_CURRENT_VIEW,
                 "transform_bounds": TRANSFORM_BOUNDS, "destructor": UNKNOWN_DESTRUCTOR}
        slot = 0x573180
    addresses = {names[key]: value for key, value in zip(names, values)}
    image = SimpleNamespace(exact=addresses.__getitem__, import_slot=lambda dll, name: slot)
    return image, list(decoder.disasm(raw, start)), decoder


def returned_wrapper_fixture():
    decoder = Cs(CS_ARCH_X86, CS_MODE_64)
    decoder.detail = True
    raw = bytes.fromhex("""
        4053 4883ec40 488b0513132700 4833c4 4889442430 4889542428 488bda 488b09 4885c9 7505
        48210a eb23 488364242800 4c8d442428 488d1544311c00 e8b342fbff 85c0 781e
        488b4c2428 48890b 488bc3 488b4c2430 4833cc e8cb4cfbff 4883c440 5b c3 8bc8 e8aa48d9ff cc
    """)
    addresses = {name: 0x2C4CE8 for name in AS_FRAMEWORK_ELEMENT.values()}
    addresses.update({"__security_check_cookie": 0x279A10, BORROWED_TARGETS["throw_hresult"]: 0x595FC,
                      UNKNOWN_DESTRUCTOR: 0x55A0, "__security_cookie": 0x536008, GUARD_DISPATCH: 0x479B00,
                      ICON_QUERY_INTERFACE: 0x278FDC, ICON_QUERY_INNER: 0x287748})
    query = bytes.fromhex("""
        48895c2408 4889742410 57 4883ec20 488bd9 498bf8 488b4918 488bf2 4885c9 751e 488bcb
        e843e70000 3d02400080 751b 488b4b08 4885c9 7412 4c8bc7 488bd6 488b01 488b00
        ff15d90a2000 488b5c2430 488b742438 4883c420 5f c3
    """)
    bodies = {0x2C4CE8: raw, 0x278FDC: query, 0x287748: b"\xc3"}
    image = SimpleNamespace(role="tray", exact=addresses.__getitem__,
                            bytes=lambda rva, size, executable: bytes.fromhex("9bd091a3994a7c4b9d8d6fa5d01f6fbf"),
                            function=lambda rva: list(decoder.disasm(bodies[rva], rva)))
    return image, raw, decoder


def borrowed_caller_fixture(returned=False, zero_register=False, clobber_zero=False):
    call = lambda address, target: b"\xe8" + struct.pack("<i", target - address - 5)
    image, _, decoder = returned_wrapper_fixture()
    raw = bytes.fromhex("33ff") if zero_register else b""
    if clobber_zero:
        raw += bytes.fromhex("488bf8")
    if returned:
        raw += bytes.fromhex("488d542428 488d4c2420")
        raw += call(0x1000 + len(raw), 0x2C4CE8) + bytes.fromhex("90 4c8bcf 4c8bc0 488d542430")
    else:
        raw += bytes.fromhex("4c8bcf 4c8d442420 488d542430")
    raw += call(0x1000 + len(raw), 0x2000) + b"\x90"
    if returned:
        raw += bytes.fromhex("488d4c2428")
        raw += call(0x1000 + len(raw), 0x55A0)
    else:
        raw += bytes.fromhex("48397c2420" if zero_register else "48837c242000")
        raw += bytes.fromhex("740a 488d4c2420")
        raw += call(0x1000 + len(raw), 0x3000)
    raw += b"\xc3"
    code = list(decoder.disasm(raw, 0x1000))
    index = next(i for i, ins in enumerate(code) if ins.mnemonic == "call" and ins.operands[0].imm == 0x2000)
    return image, code, index, decoder


class Rejections(unittest.TestCase):
    def test_raw_source_aliases_are_disambiguated_by_vtable_and_direct_call(self):
        image, _, _ = source_alias_fixture()
        getter, _, implementation, _, bindings = anchored_source_functions(image, 0x6000, 0x7000, [])
        self.assertEqual((getter, implementation), (0x1000, 0x3000))
        self.assertTrue(bindings["implementation_getter"]["uses_raw_alias"])
        self.assertEqual(bindings["implementation_getter"]["raw_names"], [RAW_IMPL_CONTENT])
        # The original overloaded raw names stay intact; no decorated aliases are invented.
        self.assertFalse(any(s["decorated_name"] == IMPL_CONTENT for s in image.symbols))
        with self.assertRaises(EvidenceError):
            image.exact(RAW_IMPL_CONTENT)

    def test_source_aliases_reject_conflicting_decorated_symbols_and_similar_names(self):
        image, _, _ = source_alias_fixture()
        image.symbols.append({"decorated_name": GET_CONTENT, "rva": 0x9000})
        with self.assertRaises(EvidenceError):
            anchored_source_functions(image, 0x6000, 0x7000, [])
        image.symbols = [{"decorated_name": RAW_GET_CONTENT + "Extra", "rva": 0x1000}]
        with self.assertRaises(EvidenceError):
            anchored_symbol(image, (GET_CONTENT, RAW_GET_CONTENT), 0x1000, "child_vtable_slot_8")

    def test_source_alias_requires_the_exact_vtable_slot_and_unique_implementation_call(self):
        image, bodies, _ = source_alias_fixture()
        image.pointer = {0x6000 + 64: 0x3000, 0x7000 + 48: 0x2000}.__getitem__
        with self.assertRaises(EvidenceError):
            anchored_source_functions(image, 0x6000, 0x7000, [])
        image, bodies, _ = source_alias_fixture()
        bodies[0x1000] = bodies[0x1000][:-1] + b"\xe8" + struct.pack("<i", 0x4000 - 0x1005 - 5) + b"\xc3"
        with self.assertRaisesRegex(EvidenceError, "one independently named"):
            anchored_source_functions(image, 0x6000, 0x7000, [])

    def test_raw_factory_alias_requires_exact_argument_flow_and_implementation(self):
        for old, new in (("4c8bc2", "4c8bc1"), ("49832100", "49832000")):
            image, bodies, _ = source_alias_fixture()
            bodies[0x2000] = bodies[0x2000].replace(bytes.fromhex(old), bytes.fromhex(new), 1)
            with self.assertRaises(EvidenceError):
                anchored_source_functions(image, 0x6000, 0x7000, [])
        image, _, _ = source_alias_fixture()
        image.symbols[-1]["rva"] = 0x9000
        with self.assertRaises(EvidenceError):
            anchored_source_functions(image, 0x6000, 0x7000, [])

    def test_rdi_source_requires_exact_content_consumer_and_preserved_field_pointer(self):
        code, decoder, image = source_field_fixture(False)
        image.exact = lambda name: 0x5008
        with self.assertRaises(EvidenceError):
            implementation_source_field(image, code, [])
        code, decoder, image = source_field_fixture(False)
        raw = b"".join(ins.bytes for ins in code)
        wrong = raw.replace(bytes.fromhex("488d542420"), bytes.fromhex("488bfa9090"), 1)
        with self.assertRaises(EvidenceError):
            implementation_source_field(image, list(decoder.disasm(wrong, 0x1000)), [])

    def test_both_complete_borrowed_screen_bodies_are_verified(self):
        for delegated in (False, True):
            image, code, _ = borrowed_body_fixture(delegated)
            targets, _, chosen = match_borrowed_body(image, code)
            self.assertEqual(chosen, delegated)
            self.assertEqual(targets["map_window_points"], 0x573180 if delegated else 0x6F90C0)

    def test_borrowed_screen_rejects_argument_swaps_output_aliases_and_changed_return(self):
        for delegated in (False, True):
            image, code, decoder = borrowed_body_fixture(delegated)
            raw = b"".join(ins.bytes for ins in code)
            for old, new in (("4d8bf1", "4d8bf0"), ("498bf8", "498bf9"), ("488bf2", "488bf1"),
                             ("8906", "8907"), ("89560c", "895608"), ("488bc6", "488bc7")):
                with self.subTest(delegated=delegated, mutated=old):
                    wrong = raw.replace(bytes.fromhex(old), bytes.fromhex(new), 1)
                    self.assertNotEqual(wrong, raw)
                    with self.assertRaises(EvidenceError):
                        match_borrowed_body(image, list(decoder.disasm(wrong, code[0].address)))

    def test_borrowed_screen_rejects_nonlocal_cleanup_wrong_guard_and_extra_calls(self):
        for delegated in (False, True):
            image, code, decoder = borrowed_body_fixture(delegated)
            raw = b"".join(ins.bytes for ins in code)
            old = "488d4dc8" if delegated else "488d4da0"
            for wrong in (raw.replace(bytes.fromhex(old), bytes.fromhex("488d4f20"), 1), raw + b"\xe8\0\0\0\0"):
                with self.assertRaises(EvidenceError):
                    match_borrowed_body(image, list(decoder.disasm(wrong, code[0].address)))
        image, code, decoder = borrowed_body_fixture()
        raw = b"".join(ins.bytes for ins in code)
        for branch in ("7400", "7409", "750a"):
            wrong = raw.replace(bytes.fromhex("740a"), bytes.fromhex(branch), 1)
            with self.assertRaises(EvidenceError):
                match_borrowed_body(image, list(decoder.disasm(wrong, code[0].address)))

    def test_borrowed_screen_requires_named_callees_and_exact_map_import(self):
        for delegated in (False, True):
            image, code, _ = borrowed_body_fixture(delegated)
            original = image.exact
            image.exact = lambda name: original(name) + (8 if name == BORROWED_TARGETS["xaml_root"] else 0)
            with self.assertRaises(EvidenceError):
                match_borrowed_body(image, code)
            image.exact = original
            image.import_slot = lambda dll, name: 0x9000
            with self.assertRaises(EvidenceError):
                match_borrowed_body(image, code)

    def test_named_import_does_not_accept_another_dll_name_or_duplicate_slot(self):
        image = Image.__new__(Image)
        image.identity = {"image_size": 0x9000}
        base = 0x180000000
        entry = SimpleNamespace(dll=b"USER32.dll", imports=[SimpleNamespace(name=b"MapWindowPoints", address=base + 0x4000)])
        image.pe = SimpleNamespace(OPTIONAL_HEADER=SimpleNamespace(ImageBase=base), DIRECTORY_ENTRY_IMPORT=[entry])
        self.assertEqual(image.import_slot("user32.dll", "MapWindowPoints"), 0x4000)
        for dll, name in (("kernel32.dll", "MapWindowPoints"), ("user32.dll", "ClientToScreen")):
            with self.assertRaises(EvidenceError):
                image.import_slot(dll, name)
        entry.imports.append(SimpleNamespace(name=b"MapWindowPoints", address=base + 0x5000))
        with self.assertRaises(EvidenceError):
            image.import_slot("user32.dll", "MapWindowPoints")

    def test_borrowed_caller_retains_same_wrapper_and_nonoverlapping_result(self):
        _, code, index, _ = borrowed_caller_fixture()
        proof = borrowed_caller_window(code, index, 0x3000)
        self.assertEqual(proof["borrowed_wrapper_stack_location"], "[rsp + 0x20]")
        self.assertEqual(proof["result_stack_location"], "[rsp + 0x30]")

    def test_borrowed_caller_rejects_wrong_wrapper_cleanup_null_guard_or_result(self):
        _, code, index, decoder = borrowed_caller_fixture()
        raw = b"".join(ins.bytes for ins in code)
        for old, new in (("488d4c2420", "488d4c2428"), ("48837c242000", "48837c242800"),
                         ("740a", "7400"), ("488d542430", "488d542420"), ("4c8bcf", "4c8bc7")):
            wrong = raw.replace(bytes.fromhex(old), bytes.fromhex(new), 1)
            with self.assertRaises(EvidenceError):
                borrowed_caller_window(list(decoder.disasm(wrong, 0x1000)), index, 0x3000)

    def test_zero_register_caller_guard_requires_uninterrupted_zero_origin(self):
        _, code, index, decoder = borrowed_caller_fixture(zero_register=True)
        proof = borrowed_caller_window(code, index, 0x3000)
        self.assertEqual(proof["null_guard_zero_register_origin_rva"], 0x1000)
        raw = b"".join(ins.bytes for ins in code)
        mutated = list(decoder.disasm(raw.replace(bytes.fromhex("33ff"), bytes.fromhex("33f6"), 1), 0x1000))
        with self.assertRaises(EvidenceError):
            borrowed_caller_window(mutated, index, 0x3000)
        _, code, index, _ = borrowed_caller_fixture(zero_register=True, clobber_zero=True)
        with self.assertRaises(EvidenceError):
            borrowed_caller_window(code, index, 0x3000)

    def test_conversion_helper_returns_its_original_hidden_wrapper(self):
        image, _, _ = returned_wrapper_fixture()
        proof = returned_wrapper_helper(image, AS_FRAMEWORK_ELEMENT["ChevronIconView"], [])
        self.assertEqual(proof["rva"], 0x2C4CE8)

    def test_conversion_helper_rejects_different_return_capture_and_branch(self):
        for old, new in (("488bda", "488bd9"), ("488bc3", "488bc7"), ("eb23", "eb20"), ("781e", "7800")):
            image, raw, decoder = returned_wrapper_fixture()
            wrong = raw.replace(bytes.fromhex(old), bytes.fromhex(new), 1)
            image.function = lambda rva: list(decoder.disasm(wrong, rva))
            with self.assertRaises(EvidenceError):
                returned_wrapper_helper(image, AS_FRAMEWORK_ELEMENT["ChevronIconView"], [])
        image, _, _ = returned_wrapper_fixture()
        image.bytes = lambda *args: bytes(16)
        with self.assertRaisesRegex(EvidenceError, "exact IFrameworkElement IID"):
            returned_wrapper_helper(image, AS_FRAMEWORK_ELEMENT["ChevronIconView"], [])

    def test_conversion_cannot_call_destructor_instead_of_exact_query_interface(self):
        image, raw, decoder = returned_wrapper_fixture()
        body = image.function(0x2C4CE8)
        call = body[15]
        wrong = raw.replace(call.bytes, b"\xe8" + struct.pack("<i", 0x55A0 - call.address - 5), 1)
        original = image.function
        image.function = lambda rva: list(decoder.disasm(wrong, rva)) if rva == 0x2C4CE8 else original(rva)
        with self.assertRaisesRegex(EvidenceError, "exact call/branch target"):
            returned_wrapper_helper(image, AS_FRAMEWORK_ELEMENT["ChevronIconView"], [])

    def test_query_interface_requires_slot_zero_exact_cfg_and_exact_inner_method(self):
        for old, new in (("488b00", "488b01"), ("4c8bc7", "4c8bc6"), ("751e", "751b")):
            image, _, decoder = returned_wrapper_fixture()
            original = image.function
            raw = b"".join(ins.bytes for ins in original(0x278FDC))
            wrong = raw.replace(bytes.fromhex(old), bytes.fromhex(new), 1)
            image.function = lambda rva: list(decoder.disasm(wrong, rva)) if rva == 0x278FDC else original(rva)
            with self.assertRaises(EvidenceError):
                returned_wrapper_helper(image, AS_FRAMEWORK_ELEMENT["ChevronIconView"], [])
        for name in (GUARD_DISPATCH, ICON_QUERY_INNER):
            image, _, _ = returned_wrapper_fixture()
            original = image.exact
            image.exact = lambda queried: original(queried) + (8 if queried == name else 0)
            with self.assertRaises(EvidenceError):
                returned_wrapper_helper(image, AS_FRAMEWORK_ELEMENT["ChevronIconView"], [])

    def test_returned_wrapper_caller_releases_its_exact_conversion_output(self):
        image, code, index, _ = borrowed_caller_fixture(True)
        proof = borrowed_returned_caller_window(image, code, index, next(iter(BORROWED_CALLERS)), [])
        self.assertEqual(proof["borrowed_wrapper_stack_location"], "[rsp + 0x28]")

    def test_returned_wrapper_caller_rejects_wrong_conversion_borrow_or_cleanup(self):
        image, code, index, decoder = borrowed_caller_fixture(True)
        raw = b"".join(ins.bytes for ins in code)
        for old, new in (("488d542428", "488d542420"), ("4c8bc0", "4c8bc3"),
                         ("488d4c2428", "488d4c2420"), ("488d542430", "488d542428")):
            wrong = raw.replace(bytes.fromhex(old), bytes.fromhex(new), 1)
            with self.assertRaises(EvidenceError):
                borrowed_returned_caller_window(image, list(decoder.disasm(wrong, 0x1000)), index,
                                                next(iter(BORROWED_CALLERS)), [])

    def test_coordinate_result_register_variants_require_all_fields_and_preserved_pointer(self):
        decoder = Cs(CS_ARCH_X86, CS_MODE_64)
        decoder.detail = True
        raw = bytes.fromhex("498bd8 488bfa 488bf1 f30f1107 f30f114f04 488bc7 c3")
        self.assertEqual(coordinate_member(list(decoder.disasm(raw, 0x1000)), "invocation_point"),
                         "rbx_input_rdi_result_v1")
        for old, new in (("488bfa", "488bf9"), ("f30f114f04", "f30f114b04"), ("488bc7", "488bc3")):
            with self.assertRaises(EvidenceError):
                coordinate_member(list(decoder.disasm(raw.replace(bytes.fromhex(old), bytes.fromhex(new)), 0x1000)),
                                  "invocation_point")

    def test_byte_hit_inside_an_immediate_is_not_a_decoded_direct_caller(self):
        image = Image.__new__(Image)
        data = bytearray(b"\xcc" * 64)
        data[:11] = bytes.fromhex("48b8e8f90f0000909090c3")
        data[32:38] = b"\xe8" + struct.pack("<i", 0x2000 - 0x1020 - 5) + b"\xc3"
        image.bytes = lambda rva, count: bytes(data[rva - 0x1000:rva - 0x1000 + count])
        section = SimpleNamespace(VirtualAddress=0x1000, Misc_VirtualSize=64, SizeOfRawData=64,
                                  Characteristics=0x60000020)
        image.pe = SimpleNamespace(sections=[section], DIRECTORY_ENTRY_EXCEPTION=[
            SimpleNamespace(struct=SimpleNamespace(BeginAddress=0x1000, EndAddress=0x100B)),
            SimpleNamespace(struct=SimpleNamespace(BeginAddress=0x1020, EndAddress=0x1026))])
        accepted, rejected = image.direct_callers(0x2000)
        self.assertEqual([row["call_rva"] for row in accepted], [0x1020])
        self.assertEqual([row["call_rva"] for row in rejected], [0x1002])

    def test_both_owned_wrapper_cleanup_call_graphs_are_verified(self):
        for delegated, recipe in ((False, "inline_null_check_release_v1"),
                                  (True, "delegated_null_check_release_v1")):
            image, code, _, _ = cleanup_fixture(delegated)
            self.assertEqual(owned_cleanup(image, code, [])["recipe"], recipe)

    def test_distinct_transform_temporary_and_owned_wrapper_are_both_verified(self):
        image, code, _ = transform_cleanup_fixture()
        result = owned_cleanup(image, code, [])
        self.assertEqual(result["recipe"], "delegated_with_transform_temporary_v1")
        self.assertEqual(result["cleanup_call_rva"], code[-3].address)

    def test_extra_cleanup_cannot_destroy_owned_wrapper_twice_or_another_local(self):
        for options in ({"double_owned": True}, {"stack_displacement": 0xB8}):
            image, code, _ = transform_cleanup_fixture(**options)
            with self.assertRaises(EvidenceError):
                owned_cleanup(image, code, [])

    def test_transform_wrapper_must_have_the_reviewed_initialization(self):
        image, code, decoder = transform_cleanup_fixture()
        raw = b"".join(ins.bytes for ins in code)
        for wrong in (raw.replace(bytes.fromhex("488365a000"), bytes.fromhex("488365a001"), 1),
                      raw.replace(bytes.fromhex("488d55b0"), bytes.fromhex("488d55b8"), 1),
                      raw.replace(bytes.fromhex("488bce"), bytes.fromhex("488bcf"), 1)):
            with self.assertRaises(EvidenceError):
                owned_cleanup(image, list(decoder.disasm(wrong, 0x1000)), [])
        initializer = next(ins for ins in code if ins.mnemonic == "call")
        wrong = raw.replace(initializer.bytes, b"\xe8" + struct.pack("<i", 0x2000 - initializer.address - 5), 1)
        with self.assertRaises(EvidenceError):
            owned_cleanup(image, list(decoder.disasm(wrong, 0x1000)), [])

    def test_transform_cleanup_rejects_extra_owned_or_temporary_direct_release(self):
        for argument in (bytes.fromhex("488bce"), bytes.fromhex("488d4da0")):
            image, code, _ = transform_cleanup_fixture(extra_release=argument)
            with self.assertRaisesRegex(EvidenceError, "exactly one direct call"):
                owned_cleanup(image, code, [])

    def test_transform_direct_release_must_preserve_the_owned_input(self):
        image, code, decoder = transform_cleanup_fixture()
        raw = b"".join(ins.bytes for ins in code)
        # Keep one direct call, but pass RSI instead of the reviewed local.
        wrong = raw.replace(bytes.fromhex("488d4da0"), bytes.fromhex("488bce90"), 1)
        with self.assertRaises(EvidenceError):
            owned_cleanup(image, list(decoder.disasm(wrong, 0x1000)), [])

    def test_transform_direct_release_requires_the_exact_local_null_guard(self):
        image, code, decoder = transform_cleanup_fixture()
        raw = b"".join(ins.bytes for ins in code)
        for original, replacement in (("48837da000", "48837da800"),
                                      ("488d4da0", "488d4da8"),
                                      ("7409", "7400"), ("7409", "740e")):
            wrong = raw.replace(bytes.fromhex(original), bytes.fromhex(replacement), 1)
            with self.assertRaises(EvidenceError):
                owned_cleanup(image, list(decoder.disasm(wrong, 0x1000)), [])

    def test_transform_input_temporary_requires_one_exact_direct_release(self):
        image, code, decoder = transform_cleanup_fixture()
        raw = b"".join(ins.bytes for ins in code)
        release = next(ins for ins in code if ins.mnemonic == "call" and
                       ins.operands[0].imm == image.exact(UNKNOWN_RELEASE))
        wrong = raw.replace(release.bytes, b"\x90" * release.size, 1)
        with self.assertRaisesRegex(EvidenceError, "exactly one direct call"):
            owned_cleanup(image, list(decoder.disasm(wrong, 0x1000)), [])

    def test_both_source_field_register_allocations_are_verified(self):
        for inline, recipe in ((False, "rdi_content_helper_v1"), (True, "rsi_inline_content_v1")):
            code, _, image = source_field_fixture(inline)
            self.assertEqual(implementation_source_field(image, code, []), (0x20, recipe))

    def test_source_field_cannot_flow_through_a_different_object(self):
        code, decoder, image = source_field_fixture(True)
        raw = b"".join(ins.bytes for ins in code)
        for original, replacement in (("488bce", "488bcf"), ("488b0e", "488b0f"),
                                      ("488b4030", "488b4038"), ("7520", "7500")):
            wrong = raw.replace(bytes.fromhex(original), bytes.fromhex(replacement), 1)
            with self.assertRaises(EvidenceError):
                implementation_source_field(image, list(decoder.disasm(wrong, 0x1000)), [])

    def test_inline_content_must_reach_the_exact_cfg_dispatch(self):
        code, decoder, image = source_field_fixture(True)
        raw = b"".join(ins.bytes for ins in code)
        content_call = [ins for ins in code if ins.mnemonic == "call"][-1]
        wrong = raw[:content_call.address - 0x1000] + bytes.fromhex("ffd3 909090") + raw[content_call.address - 0x1000 + 5:]
        with self.assertRaises(EvidenceError):
            implementation_source_field(image, list(decoder.disasm(wrong, 0x1000)), [])
        image.exact = lambda name: 0x5008
        with self.assertRaises(EvidenceError):
            implementation_source_field(image, code, [])

    def test_multiple_source_field_candidates_are_ambiguous(self):
        code, decoder, image = source_field_fixture(False)
        raw = b"".join(ins.bytes for ins in code) + bytes.fromhex("488d7128")
        with self.assertRaises(EvidenceError):
            implementation_source_field(image, list(decoder.disasm(raw, 0x1000)), [])

    def test_wrong_release_vtable_slot_is_rejected(self):
        image, code, release, _ = cleanup_fixture()
        wrong = release.replace(bytes.fromhex("488b4010"), bytes.fromhex("488b4018"))
        image.bytes = lambda rva, size: wrong[:size]
        with self.assertRaises(EvidenceError):
            owned_cleanup(image, code, [])

    def test_cleanup_of_a_different_object_is_rejected(self):
        for delegated in (False, True):
            image, code, _, decoder = cleanup_fixture(delegated)
            wrong = b"".join(ins.bytes for ins in code).replace(bytes.fromhex("488bce"), bytes.fromhex("488bcf"))
            with self.assertRaises(EvidenceError):
                owned_cleanup(image, list(decoder.disasm(wrong, 0x1000)), [])

    def test_cleanup_null_branch_must_skip_only_the_release(self):
        image, code, _, decoder = cleanup_fixture()
        wrong = b"".join(ins.bytes for ins in code).replace(bytes.fromhex("7408"), bytes.fromhex("740b"))
        with self.assertRaises(EvidenceError):
            owned_cleanup(image, list(decoder.disasm(wrong, 0x1000)), [])

    def test_release_leaf_must_clear_the_consumed_wrapper(self):
        image, code, release, _ = cleanup_fixture()
        wrong = release.replace(bytes.fromhex("48832100"), bytes.fromhex("48832101"))
        image.bytes = lambda rva, size: wrong[:size]
        with self.assertRaises(EvidenceError):
            owned_cleanup(image, code, [])

    def test_release_leaf_cannot_jump_to_an_unrelated_dispatch(self):
        image, code, _, _ = cleanup_fixture()
        original = image.exact
        image.exact = lambda name: 0x4008 if name == GUARD_DISPATCH else original(name)
        with self.assertRaises(EvidenceError):
            owned_cleanup(image, code, [])

    def test_public_pdb_info_age_is_not_mistaken_for_image_dbi_age(self):
        with TemporaryDirectory() as directory:
            path = Path(directory) / "sample.pdb"
            path.write_bytes(pdb_fixture())
            identity = pdb_identity(path)
            self.assertEqual((identity["info_age"], identity["dbi_age"]), (3, 1))
            self.assertEqual(identity["guid"], list(range(16)))

    def test_truncated_or_out_of_bounds_pdb_is_rejected(self):
        with TemporaryDirectory() as directory:
            path = Path(directory) / "bad.pdb"
            data = pdb_fixture()
            for bad in (data[:100], data[:-512]):
                path.write_bytes(bad)
                with self.assertRaises(EvidenceError):
                    pdb_identity(path)
            struct.pack_into("<I", data, 512 * 3 + 20, 100000)
            path.write_bytes(data)
            with self.assertRaises(EvidenceError):
                pdb_identity(path)

    def test_mismatched_cached_pdb_is_not_replaced_or_used(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "sample.pdb" / "KEY1" / "sample.pdb"
            path.parent.mkdir(parents=True)
            path.write_bytes(pdb_fixture(dbi_age=2))
            image = SimpleNamespace(pdb_name="sample.pdb", pdb_key="KEY1",
                                    identity={"pdb_guid": list(range(16)), "pdb_age": 1})
            original = path.read_bytes()
            with self.assertRaises(EvidenceError):
                acquire_pdb(image, root, offline=True)
            self.assertEqual(path.read_bytes(), original)

    def test_paths_cannot_leave_the_input_tree(self):
        with TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            for relative in ("../taskbar.dll", "System32/../../taskbar.dll", "/Windows/a.dll", "C:/Windows/a.dll"):
                with self.assertRaises(EvidenceError):
                    within(root, relative)

    def test_multiple_tray_modules_require_explicit_role(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            package = root / "SystemApps" / "MicrosoftWindows.Client.Core_cw5n1h2txyewy"
            package.mkdir(parents=True)
            for name in ("Taskbar.View.dll", "SystemTray.dll"):
                (package / name).touch()
            selected, _, errors = choices(root, {})
            self.assertNotIn("tray", selected)
            self.assertTrue(any(error.get("role") == "tray" for error in errors))
            chosen = package.relative_to(root).as_posix() + "/Taskbar.View.dll"
            selected, _, _ = choices(root, {"tray": chosen})
            self.assertEqual(selected["tray"], chosen)

    def test_distinct_symbol_addresses_are_ambiguous(self):
        image = Image.__new__(Image)
        image.symbols = [{"decorated_name": "exact", "rva": 4096}, {"decorated_name": "exact", "rva": 8192}]
        with self.assertRaises(EvidenceError):
            image.exact("exact")
        with self.assertRaises(EvidenceError):
            image.exact("missing")

    def test_writable_or_truncated_function_sections_are_rejected(self):
        image = Image.__new__(Image)
        image.identity = {"image_size": 0x4000}
        image.data = bytes(0x500)
        section = SimpleNamespace(VirtualAddress=0x1000, Misc_VirtualSize=0x100,
                                  SizeOfRawData=0x100, PointerToRawData=0x400, Characteristics=0xE0000020)
        image.pe = SimpleNamespace(sections=[section])
        with self.assertRaises(EvidenceError):
            image.section(0x1000, 16, True)
        section.Characteristics = 0x60000020
        with self.assertRaises(EvidenceError):
            image.section(0x10F8, 16, True)

    def test_changed_abi_and_unbounded_field_are_not_guessed(self):
        decoder = Cs(CS_ARCH_X86, CS_MODE_64)
        code = list(decoder.disasm(bytes.fromhex("49 8b f1 48 8b da 48 8b f9"), 0x1000)) # input from r9, not r8
        with self.assertRaises(EvidenceError):
            find_sequence(code, (("mov", "rsi, r8"), ("mov", "rbx, rdx"), ("mov", "rdi, rcx")))
        for value in (0, 3, 264):
            with self.assertRaises(EvidenceError):
                aligned_offset(value)


if __name__ == "__main__":
    unittest.main()
