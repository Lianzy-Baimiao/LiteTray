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
                     ICON_QUERY_INTERFACE, ICON_QUERY_INNER, TRAY_SYMBOLS, tray_layout,
                     XFG_DISPATCH, RAW_UNKNOWN_EQUAL, THROW_HR_DIAGNOSTIC,
                     aligned_offset, anchored_symbol, anchored_source_functions, borrowed_caller_window,
                     borrowed_returned_caller_window, borrowed_named_cleanup_thunks, coordinate_member,
                     find_sequence, implementation_source_field, match_borrowed_body, owned_cleanup, owned_caller_window,
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


def source_xfg_fixture():
    """Reviewed bounded 22000 getter/helper bodies; no Windows image access."""
    decoder = Cs(CS_ARCH_X86, CS_MODE_64)
    decoder.detail = True
    bodies = {
        0xdcd50: bytes.fromhex("""
            488bc4 48895810 48897018 57 4883ec20 488bda 488b7c2428 4883600800
            488d7120 488d5008 488bce e8cb000000 84c0 0f854f0e1a00 488364243000
            488b0e 488b01 49ba70f3555029ae158b 488d542430 488b4030 ff151ed35800
            85c0 781c 488b442430 488903 488bc3 488b5c2438 488b742440 4883c420
            5f c3 cc 8bc8 e87b4e2400 cc
        """),
        0xdce48: bytes.fromhex("""
            48895c2420 57 4883ec20 488b09 488bfa 483b0a 741d 4885c9 740a
            48833a00 0f85840d1a00 32c0 488b5c2448 4883c420 5f c3 cc b001 ebf0
        """),
        0x27dbd4: bytes.fromhex("41b99f130780 4c8d051f404100 ba45000000 488bcf e8c6bbe6ff cc"),
    }
    image = Image.__new__(Image)
    image.role = "shell"
    image.symbols = [{"decorated_name": name, "rva": rva} for name, rva in (
        (RAW_UNKNOWN_EQUAL, 0xdce48), (XFG_DISPATCH, 0x66a0c8),
        (THROW_HR_DIAGNOSTIC, 0xe97b4), ("?throw_hresult@winrt@@YAXUhresult@1@@Z", 0x321c4c))]
    image.function = lambda rva: list(decoder.disasm(bodies[rva], rva))
    def section(rva, count, executable):
        if (rva, count, executable) not in ((0x691c00, 1, False), (0x66a0c8, 8, False)):
            raise EvidenceError("Unknown fixture read-only evidence")
    image.section = section
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
    image = Image.__new__(Image)
    image.symbols = [{"decorated_name": name, "rva": rva} for name, rva in addresses.items()]
    image.import_slot = lambda dll, name: slot
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


def coexisting_tray_fixture():
    """Literal owned/borrowed bodies and real caller windows in synthetic PE bounds."""
    image = Image.__new__(Image)
    image.role = "tray"
    image.path = Path("Taskbar.View.dll")
    bodies = {
        0x48346c: bytes.fromhex("""
            48895c240848897424205557415441564157488bec4881ec800000000f29742470488b05ac9b35004833c4488945e04d
            8bf8488bf24c8bf1488955b8488365a0004c8d45a0488d55b0488bcee88fa9beff9048837da0007409488d4da0e8221c
            b8ff488bcee88aaabeff0f57f6f20f5af0488bcee81faabeff0f57c9f20f5ac88365a0008365a400f30f1175a8f30f11
            4dac4c8d45a0488d55c0488d4db0e8b12cd3ff498bcfff1590cc3a008bf8f30f2c4dc041bc60000000458bc48bd0ff15
            887026008945d0f30f2c4dc4458bc48bd7ff15757026008945d4f30f2c4dc8458bc48bd7ff15627026008bd8f30f2c4d
            c0458bc48bd7ff155070260003d8895dd8f30f2c4dcc458bc48bd7ff153b7026008bd8f30f2c4dc4458bc48bd7ff1529
            70260003d8895ddc458d4c24a24c8d45d033d2498bcfff1518cc3a000f1045d0f3410f7f06488d4db0e80611b8ff9048
            8bcee8fd10b8ff498bc6488b4de04833cce8ded5f3ff4c8d9c2480000000498b5b30498b73480f28742470498be3415f
            415e415c5f5dc3
        """),
        0x6dcb0: bytes.fromhex("""
            40555356574156488bec4883ec700f29742460488b0576f376004833c4488945e84d8bf1498bf8488bf2488d55b0498b
            c8e80a5efbff90488d55d0488bc8e891e4f9ff904c8bc0488d55b8488bcfe849010000488bd8488bcfe8520200000f57
            f6f20f5af0488bcfe8e70100000f57c9f20f5ac88365c0008365c400f30f1175c8f30f114dcc0f57c00f1145d8488b0b
            488b010f1045c0660f7f45c04c8d45d8488d55c0488b4048ff1522cd670085c00f88d500000048837db800740a488d4d
            b8e87a73f9ff9048837dd000740a488d4dd0e86973f9ff9048837db0007409488d4db0e85873f9ff488d4db0e85f68fb
            ff90488bc8e8b267fbff0f57f6f20f5af048837db0007409488d4db0e82f73f9fff30f104dd8f30f59cef30f2cc18906
            f30f1055dcf30f59d6f30f2cc2894604f30f1045e0f30f59c6f30f58c1f30f2cd0895608f30f104de4f30f59cef30f58
            caf30f2cd189560c41b9020000004c8bc633d2498bceff15a4237c00488bc6488b4de84833cce8852d35000f28742460
            4883c470415e5f5e5b5dc38bc8e87a0c0500cc
        """),
        0x4892a0: bytes.fromhex("""
            488bc448895810488970184889782055488d68a14881ec00010000488b057e3d35004833c448894547488bd9e88b87ff
            ff488bbbf8000000488b4b084885c9750748214c2420eb25488365ff00488b014c8d45ff488d15e5882700488b00ff15
            7c172600488b45ff48894424204c8bc7488d542420488d4d07e84ea1ffff0f1008488b48088bc166490f7ec8412bc099
            2bc2d1f8418d3c0049c1e82048c1e920412bc88bc1992bc2d1f8418d3400c3
        """),
        0x46c0: bytes.fromhex("""
            4883ec28488339007405e8210a00004883c428c3
        """),
        0x6de4c: bytes.fromhex("""
            405356574883ec50488b05e5f176004833c44889442440498bf0488bda4889542430488364243000488b094885c9747e
            488364243800488b014c8d442438488d1527106900488b00ff15e6cb6700488b4c2438488bf948894c2420488b014c8d
            442430488b16488b8010030000ff15c1cb670085c0782f4885ff740a488d4c2420e81e72f9ff488b4c243048890b488b
            c3488b4c24404833cce8c62c35004883c4505f5e5bc38bc8e8c30b05009033ffeba4
        """),
        0x1b61c0: bytes.fromhex("""
            40534883ec300f57c0488bda0f1102488b09410f10004c8bc2488d542420488b01f30f7f442420488b4048ff158f4853
            0085c07809488bc34883c4305bc38bc8e8b788f0ffcc
        """),
        0x6c984: bytes.fromhex("""
            48895c2418574883ec50488b05ab0677004833c44889442440488bda488bf94883c140488d542428e8b7e02f0090488b
            5424284885d27456488b7f08488d4c2420e8ee090000904c8bcf4c8d442420488d542430e8d31200009048837c242000
            740a488d4c2420e80087f9ff488bd3488d4c2430ff15d2367c009085c00f95c3488d4c2428e8aa0800008ac3eb0232c0
            488b4c24404833cce88f413500488b5c24704883c4505fc3
        """),
        0x6cf04: bytes.fromhex("""
            48895c24185556574154415541564157488bec4883ec70488b051e0177004833c4488945f0488bfa4c8bf94883c13848
            8d55c0e814edffff90488b75c04885f60f848100000080be70020000007405498b0feb04498b4f08498d5f08ff156231
            7c0085c07461498bc780be7002000000480f44c3488b18488bd6488d4db0e8bd030000904c8bcb4c8d45b0488d55d0e8
            180d00009048837db0007409488d4db0e84781f9ff488bd7488d4dd0ff151a317c0085c07411488d4dc0e8f502000048
            8bc6e9500100004885f67409488d4dc0e8df0200004d8b6720498b4728488945d04c3be00f842b010000498b1c244889
            5dc84885db7411488b03488bcb488b4008ff1575da6700904885db7442488365c000488b034c8d45c0488d15d40d6900
            488bcb488b4018ff154fda6700488b45c04885c074094c8db0d0fdffffeb034533f6488365c000498bc64d8beeeb0845
            33f633c04533ed4c8975b84885c0746e498d77084180bd70020000007405498b0feb03488b0eff1548307c0085c0744e
            4180bd7002000000490f45f7488b36498bd6488d4db0e8a5020000904c8bce4c8d45b0488d55e0e8000c00009048837d
            b0007409488d4db0e82f80f9ff488bd7488d4de0ff1502307c0085c0752a4d85f6740a488d4db8e8d8010000904885db
            7409488d4dc8e80180f9ff4983c408488b45d0e9e9feffff488d4db8e8b3010000904885db7409488d4dc8e8dc7ff9ff
            498bc6eb0233c0488b4df04833cce8893a3500488b9c24c00000004883c470415f415e415d415c5f5e5dc3
        """),
        0x6e874: bytes.fromhex("""
            48895c24205556574883ec50488b05b9e776004833c44889442440498bd8488bea33ff4038ba700200007405488b31eb
            04488b7108488b4a084885c9750748897c2420eb2748897c2428488b014c8d442428488d1513336900488b00ff15aac1
            6700488b44242848894424204c8bce4c8d442420488d542430e8bef3ffff9048397c2420740a488d4c2420e8ec67f9ff
            8b4424380344243099b902000000f7f93bd80f9ec20fb6da488bcde83c0000008acb80f1010fb6d183f8010f45d384d2
            400f94c78bc7488b4c24404833cce869223500488b9c24880000004883c4505f5e5dc3
        """),
        0x1000: bytes.fromhex("""
            498bf0488bda488bf9f30f1103f30f114b04488bc3c3
        """),
        0x1100: bytes.fromhex("""
            498bf0488bda488bf9f30f1103f30f114b04f30f115308f30f115b0c488bc3c3
        """),
    }
    leaves = {
        0x50f0: bytes.fromhex("488b1148c70100000000488bca488b02488b401048ff2575596e00"),
        0x6b8230: bytes.fromhex("488d8a28000000e984c494ff"),
        0x6b83e0: bytes.fromhex("488d8a20000000e9d4c294ff"),
        0x6b8c40: bytes.fromhex("488d8a40000000e974ba94ff"),
    }
    addresses = dict(zip((name for key, name in BORROWED_TARGETS.items() if key != "display_factory"),
                         (146160, 49540, 450124, 450400, 450308, 148828, 780988, 20720, 3935152, 8245312, 7252608)))
    addresses.update({
        DISPLAY_CURRENT_VIEW: 0x24600,
        TRANSFORM_BOUNDS: 0x1b61c0,
        UNKNOWN_DESTRUCTOR: 0x46c0,
        TRAY_SYMBOLS[0][1]: 0x1000, TRAY_SYMBOLS[1][1]: 0x1100,
        TRAY_SYMBOLS[2][1]: 0x48346c,
        BORROWED_SCREEN: 0x6dcb0,
        '?ToggleEdgeCopilot@CopilotIcon@implementation@SystemTray@winrt@@AEAAXXZ': 0x4892a0,
    })
    addresses['?HitTestChevron@DragDropManager@implementation@SystemTray@winrt@@AEAA_NUtagPOINT@@@Z'] = 0x6c984
    addresses['?HitTestNotifyIcons@DragDropManager@implementation@SystemTray@winrt@@AEAAPEAUNotifyIconView@234@UtagPOINT@@@Z'] = 0x6cf04
    addresses['?GetRelativeDropPosition@DragDropManager@implementation@SystemTray@winrt@@AEAA?AW4MoveTargetRelativePosition@@AEBUNotifyIconView@234@UtagPOINT@@@Z'] = 0x6e874
    addresses["`winrt::SystemTray::implementation::DragDropManager::ScreenRectForElement'::`1'::dtor$2"] = 0x6b8230
    addresses["`winrt::SystemTray::implementation::DragDropManager::ScreenRectForElement'::`1'::dtor$0"] = 0x6b83e0
    addresses["`winrt::SystemTray::implementation::DragDropManager::ScreenRectForElement'::`1'::dtor$3"] = 0x6b83e0
    addresses["`winrt::SystemTray::implementation::DragDropManager::ScreenRectForElement'::`1'::dtor$1"] = 0x6b8c40
    image.symbols = [{"decorated_name": name, "rva": rva} for name, rva in addresses.items()]
    data = {
        0x701be0: bytes.fromhex("9bd091a3994a7c4b9d8d6fa5d01f6fbf"),
    }
    def read(rva, count, executable=True):
        for start, raw in {**bodies, **leaves, **data}.items():
            if start <= rva and rva + count <= start + len(raw):
                return raw[rva - start:rva - start + count]
        raise EvidenceError("Missing synthetic bytes")
    image.bytes = read
    image.section = lambda *args: {"section_start": 0x1000, "section_size": 0x900000, "executable": True}
    image.identity = {"image_size": 0x900000}
    image.pe = SimpleNamespace(
        OPTIONAL_HEADER=SimpleNamespace(ImageBase=0x180000000),
        sections=[SimpleNamespace(VirtualAddress=rva, Misc_VirtualSize=len(raw), SizeOfRawData=len(raw),
                                  Characteristics=0x60000020) for rva, raw in bodies.items()],
        DIRECTORY_ENTRY_EXCEPTION=[SimpleNamespace(struct=SimpleNamespace(BeginAddress=rva,
            EndAddress=rva + len(raw))) for rva, raw in bodies.items()],
        DIRECTORY_ENTRY_IMPORT=[
            SimpleNamespace(dll=b'api-ms-win-core-largeinteger-l1-1-0.dll', imports=[SimpleNamespace(name=b'MulDiv',
                address=0x180000000 + 0x6ea5b8)]),
            SimpleNamespace(dll=b'USER32.dll', imports=[SimpleNamespace(name=b'GetDpiForWindow',
                address=0x180000000 + 0x8301a8)]),
            SimpleNamespace(dll=b'USER32.dll', imports=[SimpleNamespace(name=b'MapWindowPoints',
                address=0x180000000 + 0x8301c0)]),
        ])
    return image, bodies, data

class Rejections(unittest.TestCase):
    def test_coexisting_geometry_records_both_real_consumers_without_name_precedence(self):
        image, _, _ = coexisting_tray_fixture()
        result = tray_layout(image, [])
        traits = {row["trait_id"]: row for row in result["screen_rect_traits"]}
        self.assertEqual(set(traits), {"copilot_button_owned", "drag_drop_borrowed"})
        self.assertTrue(all(row["status"] == "reviewed" for row in traits.values()))
        self.assertIsNone(result["abi_candidate"])
        self.assertEqual(result["implementation_selection"], "requires_consumer_choice")
        self.assertEqual([row["role"] for row in result["entries"]], ["invocation_point", "icon_bounds"])
        self.assertEqual(result["geometry_capabilities"]["notification_area_bounds"]["status"], "unavailable")
        self.assertEqual(result["geometry_capabilities"]["drag_drop_hit_testing"]["trait_id"], "drag_drop_borrowed")
        self.assertEqual(len(traits["copilot_button_owned"]["usage"]["callers"]), 1)
        self.assertEqual(len(traits["drag_drop_borrowed"]["usage"]["callers"]), 4)

    def test_unknown_borrowed_shape_disables_only_its_capability(self):
        image, bodies, _ = coexisting_tray_fixture()
        rva = image.exact(BORROWED_SCREEN)
        bodies[rva] = bodies[rva].replace(bytes.fromhex("4d8bf1"), bytes.fromhex("4d8bf0"), 1)
        result = tray_layout(image, [])
        traits = {row["trait_id"]: row for row in result["screen_rect_traits"]}
        self.assertEqual(traits["copilot_button_owned"]["status"], "reviewed")
        self.assertEqual(traits["drag_drop_borrowed"]["status"], "unreviewed")
        self.assertIsNone(result["abi_candidate"])
        self.assertEqual(result["geometry_capabilities"]["drag_drop_hit_testing"]["status"], "unreviewed")
        self.assertTrue(traits["drag_drop_borrowed"]["analysis_blockers"])

    def test_nonadjacent_owned_null_diamond_still_requires_exact_query_and_transfer_branches(self):
        decoder = Cs(CS_ARCH_X86, CS_MODE_64)
        decoder.detail = True
        raw = bytes.fromhex("""
            488b5db0 4885db 751a 48215dd0 eb3b 488d55b8 488bce e8f9100000 807dc800 744a ebce
            488365d800 488b03 4c8d45d8 488d155a8f0e00 488bcb 488b00 ff15ee270d00
            488b45d8 488945d0 4c8bc7 488d55d0 488d4db8 e8ea f4ffff 0f1030
        """)
        image = SimpleNamespace(exact=lambda name: 0x18B9D8,
                                bytes=lambda *args: bytes.fromhex("9bd091a3994a7c4b9d8d6fa5d01f6fbf"))
        code = list(decoder.disasm(raw, 0xB91A8))
        index = len(code) - 2
        self.assertEqual(owned_caller_window(image, code, index)["query_interface_slot"], 0)
        for old, new in (("751a", "751b"), ("eb3b", "eb3e"), ("48215dd0", "48215dd8")):
            with self.subTest(mutated=old), self.assertRaises(EvidenceError):
                owned_caller_window(image, list(decoder.disasm(raw.replace(bytes.fromhex(old), bytes.fromhex(new), 1),
                                                             0xB91A8)), index)

    def test_coexisting_signatures_reject_multiple_rvas_same_address_and_overlapping_bounds(self):
        for kind in ("multiple", "same", "overlap"):
            image, bodies, _ = coexisting_tray_fixture()
            owned = image.exact(TRAY_SYMBOLS[2][1])
            if kind == "multiple":
                image.symbols.append({"decorated_name": BORROWED_SCREEN, "rva": 0x9000})
            else:
                borrowed = next(row for row in image.symbols if row["decorated_name"] == BORROWED_SCREEN)
                borrowed["rva"] = owned if kind == "same" else owned + 5
                if kind == "overlap":
                    image.pe.DIRECTORY_ENTRY_EXCEPTION.append(SimpleNamespace(struct=SimpleNamespace(
                        BeginAddress=owned + 5, EndAddress=owned + len(bodies[owned]))))
            with self.subTest(kind=kind), self.assertRaises(EvidenceError):
                tray_layout(image, [])

    def test_owned_consumer_requires_exact_name_iid_query_slot_and_target(self):
        caller_name = "?ToggleEdgeCopilot@CopilotIcon@implementation@SystemTray@winrt@@AEAAXXZ"
        for mutation in ("name", "caller_rva", "iid", "query_slot", "cfg", "wrapper", "branch", "screen_target"):
            image, bodies, data = coexisting_tray_fixture()
            caller_rva = image.exact(caller_name)
            raw = bodies[caller_rva]
            if mutation in ("name", "caller_rva"):
                symbol = next(s for s in image.symbols if s["decorated_name"] == caller_name)
                if mutation == "name":
                    symbol["decorated_name"] += "Extra"
                else:
                    symbol["rva"] += 8
            elif mutation == "iid":
                data[next(iter(data))] = bytes(16)
            elif mutation == "screen_target":
                call = next(i for i in image.function(caller_rva) if i.mnemonic == "call" and
                            i.operands[0].type == 2 and i.operands[0].imm == image.exact(TRAY_SYMBOLS[2][1]))
                wrong = b"\xe8" + struct.pack("<i", image.exact(BORROWED_SCREEN) - call.address - 5)
                bodies[caller_rva] = raw.replace(bytes(call.bytes), wrong, 1)
            else:
                old, new = {"query_slot": ("488b00", "488b01"), "cfg": ("ff157c172600", "ff1584172600"),
                            "wrapper": ("4889442420", "4889442428"), "branch": ("eb25", "eb28")}[mutation]
                bodies[caller_rva] = raw.replace(bytes.fromhex(old), bytes.fromhex(new), 1)
                self.assertNotEqual(bodies[caller_rva], raw)
            with self.subTest(mutation=mutation):
                result = tray_layout(image, [])
                trait = next(t for t in result["screen_rect_traits"] if t["entry"]["decorated_symbol"] == TRAY_SYMBOLS[2][1])
                self.assertEqual(trait["status"], "unreviewed")
                self.assertTrue(any(b["stage"] == "usage" for b in trait["analysis_blockers"]))
                self.assertNotEqual(result["geometry_capabilities"]["copilot_button_geometry"]["status"], "reviewed")

    def test_complete_owned_template_rejects_changed_arithmetic_import_and_extra_instruction(self):
        for mutation in ("arithmetic", "import", "extra"):
            image, bodies, _ = coexisting_tray_fixture()
            rva = image.exact(TRAY_SYMBOLS[2][1])
            raw = bodies[rva]
            if mutation == "arithmetic":
                bodies[rva] = raw.replace(bytes.fromhex("03d8"), bytes.fromhex("03d9"), 1)
            elif mutation == "import":
                image.pe.DIRECTORY_ENTRY_IMPORT[0].imports[0].address += 8
            else:
                bodies[rva] += b"\x90"
                next(e.struct for e in image.pe.DIRECTORY_ENTRY_EXCEPTION if e.struct.BeginAddress == rva).EndAddress += 1
                next(s for s in image.pe.sections if s.VirtualAddress == rva).Misc_VirtualSize += 1
                next(s for s in image.pe.sections if s.VirtualAddress == rva).SizeOfRawData += 1
            with self.subTest(mutation=mutation):
                result = tray_layout(image, [])
                owned = next(t for t in result["screen_rect_traits"] if t["trait_id"] == "copilot_button_owned")
                self.assertEqual(owned["abi_status"], "unreviewed")
                self.assertEqual(result["geometry_capabilities"]["drag_drop_hit_testing"]["status"], "reviewed")

    def test_inline_tail_borrowed_template_checks_tail_branch_named_target_cleanup_and_callers(self):
        for mutation in ("tail", "target", "cleanup", "caller"):
            image, bodies, _ = coexisting_tray_fixture()
            rva = image.exact(BORROWED_SCREEN)
            raw = bodies[rva]
            if mutation == "tail":
                bodies[rva] = raw.replace(bytes.fromhex("0f88d5000000"), bytes.fromhex("0f88d6000000"), 1)
            elif mutation == "target":
                symbol = next(s for s in image.symbols if s["decorated_name"] == DISPLAY_CURRENT_VIEW)
                symbol["rva"] += 8
            elif mutation == "cleanup":
                bodies[rva] = raw.replace(bytes.fromhex("488d4db8"), bytes.fromhex("488d4fb8"), 1)
            else:
                caller = image.exact(next(iter(BORROWED_CALLERS)))
                site = next(s for s in image.direct_callers(rva)[0] if s["function_rva"] == caller)
                cleanup = site["code"][site["index"] + 4]
                offset = cleanup.address - caller
                bodies[caller] = bodies[caller][:offset] + bytes.fromhex("488d4c2428") + bodies[caller][offset + cleanup.size:]
            with self.subTest(mutation=mutation):
                result = tray_layout(image, [])
                trait = next(t for t in result["screen_rect_traits"] if t["trait_id"] == "drag_drop_borrowed")
                self.assertEqual(trait["status"], "unreviewed")
                self.assertEqual(result["geometry_capabilities"]["copilot_button_geometry"]["status"], "reviewed")

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
            self.assertEqual(chosen, "delegated" if delegated else "inline")
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

    def test_xfg_source_field_requires_complete_getter_and_null_helper(self):
        image, bodies, decoder = source_xfg_fixture()
        logs = []
        self.assertEqual(implementation_source_field(image, image.function(0xdcd50), logs),
                         (0x20, "rsi_xfg_inline_content_v1"))
        self.assertEqual({entry["rva"] for entry in logs}, {0xdce48, 0x27dbd4})
        # A raw public name may be shared by overloads. The exact call and the
        # entire helper body select this one, without picking the first alias.
        image.symbols.append({"decorated_name": RAW_UNKNOWN_EQUAL, "rva": 0x5000})
        self.assertEqual(implementation_source_field(image, image.function(0xdcd50), []),
                         (0x20, "rsi_xfg_inline_content_v1"))

    def test_xfg_source_rejects_changed_field_zero_wrapper_abi_and_return(self):
        mutations = (
            ("488d7120", "488d7128"),  # unrelated source member
            ("4883600800", "4883600801"),  # equality helper no longer receives null
            ("488d5008", "488d5010"),  # comparison points to another local
            ("488bce", "488bcf"),
            ("488b0e", "488b0f"),
            ("49ba70f3555029ae158b", "49ba71f3555029ae158b"),  # wrong XFG type
            ("488b4030", "488b4038"),  # wrong COM slot
            ("488d542430", "488d542438"),  # wrong ABI result local
            ("488903", "488907"),  # result written through another object
            ("488bc3", "488bc7"),  # result wrapper not returned
            ("781c", "781b"),  # HRESULT failure jumps inside an instruction
            ("ff151ed35800", "ff151fd35800"),  # another dispatch slot
        )
        for before, after in mutations:
            with self.subTest(before=before):
                image, bodies, decoder = source_xfg_fixture()
                raw = bodies[0xdcd50].replace(bytes.fromhex(before), bytes.fromhex(after), 1)
                self.assertNotEqual(raw, bodies[0xdcd50])
                with self.assertRaises(EvidenceError):
                    implementation_source_field(image, list(decoder.disasm(raw, 0xdcd50)), [])

    def test_xfg_source_rejects_changed_equality_control_flow_and_cold_throw(self):
        mutations = (
            (0xdce48, "483b0a", "483b0b"),
            (0xdce48, "741d", "741c"),
            (0xdce48, "48833a00", "48833a01"),
            (0xdce48, "32c0", "b001"),
            (0xdce48, "b001", "b000"),
            (0x27dbd4, "41b99f130780", "41b99e130780"),
            (0x27dbd4, "e8c6bbe6ff", "e8c7bbe6ff"),
            (0x27dbd4, "4c8d051f404100", "4c8d051e404100"),
        )
        for address, before, after in mutations:
            with self.subTest(address=address, before=before):
                image, bodies, decoder = source_xfg_fixture()
                bodies[address] = bodies[address].replace(bytes.fromhex(before), bytes.fromhex(after), 1)
                with self.assertRaises(EvidenceError):
                    implementation_source_field(image, image.function(0xdcd50), [])

    def test_xfg_source_rejects_missing_or_conflicting_exact_bindings(self):
        for name in (RAW_UNKNOWN_EQUAL, XFG_DISPATCH, THROW_HR_DIAGNOSTIC,
                     "?throw_hresult@winrt@@YAXUhresult@1@@Z"):
            with self.subTest(name=name):
                image, bodies, decoder = source_xfg_fixture()
                image.symbols = [s for s in image.symbols if s["decorated_name"] != name]
                with self.assertRaises(EvidenceError):
                    implementation_source_field(image, image.function(0xdcd50), [])
        image, bodies, decoder = source_xfg_fixture()
        image.symbols.append({"decorated_name": XFG_DISPATCH, "rva": 0x5000})
        with self.assertRaises(EvidenceError):
            implementation_source_field(image, image.function(0xdcd50), [])

    def test_xfg_source_rejects_added_instructions_and_nonreadonly_targets(self):
        image, bodies, decoder = source_xfg_fixture()
        with self.assertRaises(EvidenceError):
            implementation_source_field(image, list(decoder.disasm(bodies[0xdcd50] + b"\x90", 0xdcd50)), [])
        def bad_section(*_args):
            raise EvidenceError("Writable evidence")
        image.section = bad_section
        with self.assertRaisesRegex(EvidenceError, "Writable evidence"):
            implementation_source_field(image, image.function(0xdcd50), [])

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
