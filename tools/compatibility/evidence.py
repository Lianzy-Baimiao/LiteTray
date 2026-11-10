"""Bounded PE/PDB inspection and serial, exact DbgHelp enumeration.

The Windows modules are read as files. SymLoadModuleEx registers debug data;
this module never calls LoadLibrary on an inspected DLL or opens Explorer.
"""
from __future__ import annotations

import ctypes as ct
from ctypes import wintypes as wt
import hashlib
import math
from pathlib import Path
import re
import struct
import urllib.request
import uuid

import pefile
from capstone import Cs, CS_ARCH_X86, CS_MODE_64
from capstone.x86_const import X86_OP_IMM

MAX_IMAGE = 128 * 1024 * 1024
MAX_PDB = 512 * 1024 * 1024
MASKS = {
    "taskbar": ("*TaskbarHost*", "*TaskBand*", "*Decref*", "*XamlExplorerHostChildWindow*",
                "*TrayUI*", "*CSecondaryTray*", "__guard_xfg_dispatch_icall_fptr",
                "__security_cookie", "__security_check_cookie", "?throw_hresult@winrt@@YAXUhresult@1@@Z",
                "?Width@?$consume_Windows_UI_Xaml_IFrameworkElement@UIFrameworkElement@Xaml@UI@Windows@winrt@@@impl@winrt@@QEBA@XZ",
                "?Height@?$consume_Windows_UI_Xaml_IFrameworkElement@UIFrameworkElement@Xaml@UI@Windows@winrt@@@impl@winrt@@QEBA@XZ"),
    "shell": ("*XamlExplorerHost*", "*IDesktopWindowXamlSource*", "_guard_dispatch_icall",
              "__guard_xfg_dispatch_icall_fptr", "winrt::Windows::Foundation::operator==",
              "?Throw_Hr@in1diag3@details@wil@@YAXPEAXIPEBDJ@Z",
              "?throw_hresult@winrt@@YAXUhresult@1@@Z",
              "?unconditional_release_ref@IUnknown@Foundation@Windows@winrt@@AEAAXXZ"),
    "tray": ("*GetInvocationPointRelativeToScreen*", "*GetIconBoundsRelativeToScreen*",
             "*GetScreenRectFromXamlElement*", "*SystemTrayFrame*", "*NotificationAreaIconsDataModel*",
             "??1IUnknown@Foundation@Windows@winrt@@QEAA@XZ",
             "?unconditional_release_ref@IUnknown@Foundation@Windows@winrt@@AEAAXXZ",
             "__guard_dispatch_icall_fptr", "*TransformToVisual*",
             "*ScreenRectForElement*",
             "?ToggleEdgeCopilot@CopilotIcon@implementation@SystemTray@winrt@@AEAAXXZ",
             "?OnPointerEntered@CopilotIcon@implementation@SystemTray@winrt@@QEAAXUPointerRoutedEventArgs@Input@Xaml@UI@Windows@4@@Z",
             "?OnPointerExited@CopilotIcon@implementation@SystemTray@winrt@@QEAAXUPointerRoutedEventArgs@Input@Xaml@UI@Windows@4@@Z",
             "?HitTestChevron@DragDropManager@implementation@SystemTray@winrt@@AEAA_NUtagPOINT@@@Z",
             "?HitTestNotifyIcons@DragDropManager@implementation@SystemTray@winrt@@AEAAPEAUNotifyIconView@234@UtagPOINT@@@Z",
             "?GetRelativeDropPosition@DragDropManager@implementation@SystemTray@winrt@@AEAA?AW4MoveTargetRelativePosition@@AEBUNotifyIconView@234@UtagPOINT@@@Z",
             "?XamlRoot@?$consume_Windows_UI_Xaml_IUIElement10@UFrameworkElement@Xaml@UI@Windows@winrt@@@impl@winrt@@QEBA@XZ",
             "?Content@?$consume_Windows_UI_Xaml_IXamlRoot@UIXamlRoot@Xaml@UI@Windows@winrt@@@impl@winrt@@QEBA@XZ",
             "?ActualWidth@?$consume_Windows_UI_Xaml_IFrameworkElement@UIFrameworkElement@Xaml@UI@Windows@winrt@@@impl@winrt@@QEBA@XZ",
             "?ActualHeight@?$consume_Windows_UI_Xaml_IFrameworkElement@UIFrameworkElement@Xaml@UI@Windows@winrt@@@impl@winrt@@QEBA@XZ",
             "?RawPixelsPerViewPixel@?$consume_Windows_Graphics_Display_IDisplayInformation2@UDisplayInformation@Display@Graphics@Windows@winrt@@@impl@winrt@@QEBA@XZ",
             "??$call_factory_cast@P6A?AUDisplayInformation@Display@Graphics@Windows@winrt@@AEBUIDisplayInformationStatics@2345@@ZU12345@U62345@V<lambda_0a1437f4f9e4e85de4e92d9eedff3a11>@@@impl@winrt@@YA?A_P$$QEAV<lambda_0a1437f4f9e4e85de4e92d9eedff3a11>@@@Z",
             "?throw_hresult@winrt@@YAXUhresult@1@@Z", "__security_cookie", "__security_check_cookie"),
}
MASKS["tray"] += (
    "?GetForCurrentView@DisplayInformation@Display@Graphics@Windows@winrt@@SA@XZ",
    "?TransformBounds@?$consume_Windows_UI_Xaml_Media_IGeneralTransform@UIGeneralTransform@Media@Xaml@UI@Windows@winrt@@@impl@winrt@@QEBA@AEBURect@Foundation@Windows@3@@Z",
    "??$as@UFrameworkElement@Xaml@UI@Windows@winrt@@@?$com_ptr@UChevronIconView@implementation@SystemTray@winrt@@@winrt@@QEBA?A_PXZ",
    "??$as@UFrameworkElement@Xaml@UI@Windows@winrt@@@?$com_ptr@UNotifyIconView@implementation@SystemTray@winrt@@@winrt@@QEBA?A_PXZ",
    "?QueryInterface@?$root_implements@UIconView@implementation@SystemTray@winrt@@U134@UINotifyPropertyChanged@Data@Xaml@UI@Windows@4@Ucomposable@4@Ucomposing@4@UIControlOverrides@Controls@7894@UIControlOverrides6@Controls@7894@UIFrameworkElementOverrides@7894@UIFrameworkElementOverrides2@7894@UIUIElementOverrides@7894@UIUIElementOverrides7@7894@UIUIElementOverrides8@7894@UIUIElementOverrides9@7894@@impl@winrt@@QEAAHAEBUguid@3@PEAPEAX@Z",
    "?query_interface@?$root_implements@UIconView@implementation@SystemTray@winrt@@U134@UINotifyPropertyChanged@Data@Xaml@UI@Windows@4@Ucomposable@4@Ucomposing@4@UIControlOverrides@Controls@7894@UIControlOverrides6@Controls@7894@UIFrameworkElementOverrides@7894@UIFrameworkElementOverrides2@7894@UIUIElementOverrides@7894@UIUIElementOverrides7@7894@UIUIElementOverrides8@7894@UIUIElementOverrides9@7894@@impl@winrt@@AEAAHAEBUguid@3@PEAPEAX@Z",
)


class EvidenceError(Exception):
    """Expected, fail-closed rejection of an unknown or invalid input."""


def require(condition, message):
    if not condition:
        raise EvidenceError(message)


def bounded_file(path: Path, limit: int) -> bytes:
    require(path.is_file(), f"Missing file: {path}")
    require(0 < path.stat().st_size <= limit, f"File size outside inspection limit: {path}")
    with path.open("rb") as stream:
        data = stream.read(limit + 1)
    require(len(data) <= limit, f"File grew beyond inspection limit: {path}")
    return data


def within(root: Path, relative: str) -> Path:
    parts = relative.replace("\\", "/").split("/")
    require(all(re.fullmatch(r"[A-Za-z0-9._-]+", part) and part not in (".", "..")
                for part in parts), f"Not a safe system-root-relative path: {relative}")
    path = root.joinpath(*parts).resolve()
    require(path.is_relative_to(root.resolve()), f"Path escapes system root: {relative}")
    return path


class Image:
    def __init__(self, role: str, root: Path, relative: str):
        self.role = role
        self.path = within(root, relative)
        self.relative = self.path.relative_to(root.resolve()).as_posix()
        self.data = bounded_file(self.path, MAX_IMAGE)
        self.pe = pefile.PE(data=self.data)
        require(self.pe.FILE_HEADER.Machine == 0x8664 and self.pe.OPTIONAL_HEADER.Magic == 0x20B,
                "Only native x64 PE32+ images have a reviewed analysis recipe")
        require(0 < len(self.pe.sections) <= 96, "Invalid PE section count")
        entries = [entry.struct for entry in getattr(self.pe, "DIRECTORY_ENTRY_DEBUG", [])
                   if entry.struct.Type == 2]
        require(len(entries) == 1, "Expected exactly one CodeView identity")
        entry = entries[0]
        require(25 <= entry.SizeOfData <= 4096, "CodeView size outside bounds")
        start, end = entry.PointerToRawData, entry.PointerToRawData + entry.SizeOfData
        require(0 <= start < end <= len(self.data), "CodeView outside file")
        cv = self.data[start:end]
        require(cv[:4] == b"RSDS" and b"\0" in cv[24:], "Missing RSDS identity or PDB name terminator")
        guid = cv[4:20]
        age = struct.unpack_from("<I", cv, 20)[0]
        name = cv[24:].split(b"\0", 1)[0].decode("utf-8").replace("\\", "/").rsplit("/", 1)[-1]
        require(re.fullmatch(r"[A-Za-z0-9._-]+\.pdb", name, re.IGNORECASE) is not None,
                "Unsafe PDB basename")
        require(age > 0, "PDB age must be nonzero")
        self.pdb_name = name
        self.pdb_key = uuid.UUID(bytes_le=guid).hex.upper() + format(age, "X")
        self.identity = {
            "machine": self.pe.FILE_HEADER.Machine,
            "timestamp": self.pe.FILE_HEADER.TimeDateStamp,
            "image_size": self.pe.OPTIONAL_HEADER.SizeOfImage,
            "pdb_guid": list(guid), "pdb_age": age,
        }
        self.sha256 = hashlib.sha256(self.data).hexdigest()
        self.symbols = []
        self.symbol_proof = None

    def module(self):
        return {"name": self.path.name, "relative_path": self.relative,
                "sha256": self.sha256, "identity": self.identity}

    def section(self, rva: int, count: int, executable: bool):
        require(rva >= 0x1000 and count > 0 and rva + count <= self.identity["image_size"],
                f"RVA outside image: {rva:#x}+{count}")
        found = [section for section in self.pe.sections if section.VirtualAddress <= rva and
                 rva + count <= section.VirtualAddress + min(section.Misc_VirtualSize, section.SizeOfRawData)]
        require(len(found) == 1, f"RVA is absent or ambiguous: {rva:#x}")
        section = found[0]
        require(bool(section.Characteristics & 0x20000000) == executable,
                f"Unexpected executable permission at {rva:#x}")
        require(not section.Characteristics & 0x80000000, f"Writable compatibility evidence at {rva:#x}")
        raw = section.PointerToRawData + rva - section.VirtualAddress
        require(raw + count <= len(self.data), "Truncated raw section")
        return {"section_start": section.VirtualAddress, "section_size": section.Misc_VirtualSize,
                "executable": executable}

    def bytes(self, rva: int, count: int, executable=True):
        self.section(rva, count, executable)
        result = self.pe.get_data(rva, count)
        require(len(result) == count, "Truncated mapped data")
        return result

    def exact(self, decorated: str):
        matches = {symbol["rva"] for symbol in self.symbols if symbol["decorated_name"] == decorated}
        require(len(matches) == 1, f"Missing/ambiguous symbol ({len(matches)} RVAs): {decorated}")
        return matches.pop()

    def import_slot(self, dll: str, name: str):
        """Resolve a named PE import, including delay imports, without loading it."""
        slots = {item.address - self.pe.OPTIONAL_HEADER.ImageBase
                 for table in ("DIRECTORY_ENTRY_IMPORT", "DIRECTORY_ENTRY_DELAY_IMPORT")
                 for entry in getattr(self.pe, table, [])
                 if entry.dll.decode("ascii").casefold() == dll.casefold()
                 for item in entry.imports if item.name and item.name.decode("ascii") == name}
        require(len(slots) == 1, f"Missing/ambiguous exact import: {dll}!{name}")
        slot = slots.pop()
        require(0x1000 <= slot <= self.identity["image_size"] - 8 and slot % 8 == 0,
                "Import slot outside image or unaligned")
        return slot

    def direct_callers(self, target: int):
        """Only accept E8 hits at decoded starts in unique bounded functions."""
        hits = []
        for section in self.pe.sections:
            if not section.Characteristics & 0x20000000 or section.Characteristics & 0x80000000:
                continue
            data = self.bytes(section.VirtualAddress, min(section.Misc_VirtualSize, section.SizeOfRawData))
            cursor = 0
            while (offset := data.find(b"\xe8", cursor)) >= 0:
                cursor = offset + 1
                if offset + 5 > len(data):
                    continue
                rva = section.VirtualAddress + offset
                if rva + 5 + struct.unpack_from("<i", data, offset + 1)[0] == target:
                    hits.append(rva)
                    require(len(hits) <= 1024, "Direct-call candidate limit exceeded")
        accepted, rejected, decoded = [], [], {}
        bounds = [entry.struct for entry in getattr(self.pe, "DIRECTORY_ENTRY_EXCEPTION", [])]
        for site in hits:
            matches = [bound for bound in bounds if bound.BeginAddress <= site < bound.EndAddress]
            if len(matches) != 1:
                rejected.append({"call_rva": site, "reason": "no unique .pdata boundary"})
                continue
            start = matches[0].BeginAddress
            try:
                if start not in decoded:
                    require(len(decoded) < 256, "Direct-call function limit exceeded")
                    decoded[start] = self.function(start)
                code = decoded[start]
                indices = [i for i, ins in enumerate(code) if ins.address == site and
                           ins.mnemonic == "call" and len(ins.operands) == 1 and
                           ins.operands[0].type == X86_OP_IMM and ins.operands[0].imm == target]
                require(len(indices) == 1, "not an aligned decoded direct call")
                accepted.append({"call_rva": site, "function_rva": start, "index": indices[0], "code": code})
            except EvidenceError as error:
                rejected.append({"call_rva": site, "reason": str(error)})
        return accepted, rejected

    def function(self, rva: int):
        matches = [entry.struct for entry in getattr(self.pe, "DIRECTORY_ENTRY_EXCEPTION", [])
                   if entry.struct.BeginAddress == rva]
        require(len(matches) == 1, f"Missing/ambiguous .pdata function boundary at {rva:#x}")
        count = matches[0].EndAddress - rva
        require(1 <= count <= 65536, f"Function size outside recipe limit: {count}")
        code = self.bytes(rva, count)
        decoder = Cs(CS_ARCH_X86, CS_MODE_64)
        decoder.detail = True
        instructions = list(decoder.disasm(code, rva))
        require(sum(ins.size for ins in instructions) == count, f"Incomplete instruction decoding at {rva:#x}")
        return instructions

    def pointer(self, rva: int):
        require(rva % 8 == 0, "Unaligned vtable slot")
        value = struct.unpack("<Q", self.bytes(rva, 8, False))[0] - self.pe.OPTIONAL_HEADER.ImageBase
        self.section(value, 1, True)
        return value

    def recheck(self):
        require(hashlib.sha256(bounded_file(self.path, MAX_IMAGE)).hexdigest() == self.sha256,
                f"Input changed during collection: {self.path}")


def pdb_identity(path: Path):
    """Read only MSF streams 1 and 3; public PDB info-age can exceed DBI-age."""
    data = bounded_file(path, MAX_PDB)
    require(len(data) >= 56 and data[:32] == b"Microsoft C/C++ MSF 7.00\r\n\x1aDS\0\0\0", "Unsupported PDB/MSF header")
    block_size, _, block_count, directory_size, _, block_map = struct.unpack_from("<6I", data, 32)
    require(block_size in (512, 1024, 2048, 4096, 8192) and
            0 < block_count * block_size <= len(data), "Invalid MSF block geometry")
    require(4 <= directory_size <= min(MAX_PDB // 4, len(data)), "MSF directory size outside limit")

    def take(offset, length):
        require(0 <= offset <= offset + length <= len(data), "MSF read outside file")
        return data[offset:offset + length]

    def blocks_bytes(blocks, length):
        require(all(block < block_count for block in blocks), "MSF block index out of bounds")
        return b"".join(take(block * block_size, block_size) for block in blocks)[:length]

    count = math.ceil(directory_size / block_size)
    require(count * 4 <= block_size, "Unsupported multi-page MSF block map")
    directory = blocks_bytes(struct.unpack(f"<{count}I", take(block_map * block_size, count * 4)), directory_size)
    streams = struct.unpack_from("<I", directory)[0]
    require(4 <= streams <= 100000 and 4 + 4 * streams <= len(directory), "Invalid stream directory")
    lengths = struct.unpack_from(f"<{streams}I", directory, 4)
    cursor = 4 + 4 * streams
    selected = {}
    for index, length in enumerate(lengths):
        count = math.ceil(length / block_size) if length != 0xFFFFFFFF else 0
        require(cursor + count * 4 <= len(directory), "Truncated stream block list")
        if index in (1, 3):
            require(length != 0xFFFFFFFF and length >= (28 if index == 1 else 12), "Missing PDB/DBI stream")
            blocks = struct.unpack_from(f"<{count}I", directory, cursor)
            selected[index] = blocks_bytes(blocks, length)
        cursor += count * 4
    version, signature, info_age = struct.unpack_from("<3I", selected[1])
    return {"version": version, "signature": signature, "info_age": info_age,
            "dbi_age": struct.unpack_from("<I", selected[3], 8)[0], "guid": list(selected[1][12:28]),
            "sha256": hashlib.sha256(data).hexdigest(), "size": len(data)}


def acquire_pdb(image: Image, cache: Path, offline: bool):
    target = within(cache.resolve(), f"{image.pdb_name}/{image.pdb_key}/{image.pdb_name}")
    target.parent.mkdir(parents=True, exist_ok=True)
    url = f"https://msdl.microsoft.com/download/symbols/{image.pdb_name}/{image.pdb_key}/{image.pdb_name}"
    if not target.exists():
        require(not offline, f"Exact PDB unavailable in offline cache: {target}")
        # A failed download never becomes a valid cache entry or replaces one.
        temporary = target.with_name(target.name + f".{uuid.uuid4().hex}.partial")
        try:
            with urllib.request.urlopen(url, timeout=45) as response, temporary.open("xb") as output:
                total = 0
                while chunk := response.read(1024 * 1024):
                    total += len(chunk)
                    require(total <= MAX_PDB, "Symbol download exceeded size limit")
                    output.write(chunk)
            proof = pdb_identity(temporary)
            require(proof["guid"] == image.identity["pdb_guid"] and
                    proof["dbi_age"] == image.identity["pdb_age"], "Downloaded PDB identity mismatch")
            require(not target.exists(), "PDB cache changed concurrently; retry without replacing it")
            temporary.rename(target)
        finally:
            if temporary.exists():
                temporary.unlink()
    proof = pdb_identity(target)
    require(proof["guid"] == image.identity["pdb_guid"] and proof["dbi_age"] == image.identity["pdb_age"],
            "Cached PDB identity mismatch; refusing fallback")
    return target, {"url": url, "path": str(target), "pdb_key": image.pdb_key, **proof}


class SymbolInfo(ct.Structure):
    _fields_ = [("SizeOfStruct", wt.ULONG), ("TypeIndex", wt.ULONG), ("Reserved", ct.c_ulonglong * 2),
                ("Index", wt.ULONG), ("Size", wt.ULONG), ("ModBase", ct.c_ulonglong), ("Flags", wt.ULONG),
                ("Value", ct.c_ulonglong), ("Address", ct.c_ulonglong), ("Register", wt.ULONG),
                ("Scope", wt.ULONG), ("Tag", wt.ULONG), ("NameLen", wt.ULONG), ("MaxNameLen", wt.ULONG),
                ("Name", ct.c_char * 1)]


class ModuleInfo(ct.Structure):
    _fields_ = [("SizeOfStruct", wt.DWORD), ("BaseOfImage", ct.c_ulonglong),
                ("ImageSize", wt.DWORD), ("TimeDateStamp", wt.DWORD), ("CheckSum", wt.DWORD),
                ("NumSyms", wt.DWORD), ("SymType", wt.DWORD), ("ModuleName", ct.c_char * 32),
                ("ImageName", ct.c_char * 256), ("LoadedImageName", ct.c_char * 256),
                ("LoadedPdbName", ct.c_char * 256), ("CVSig", wt.DWORD), ("CVData", ct.c_char * 780),
                ("PdbSig", wt.DWORD), ("PdbSig70", ct.c_ubyte * 16), ("PdbAge", wt.DWORD),
                ("PdbUnmatched", wt.BOOL), ("DbgUnmatched", wt.BOOL), ("LineNumbers", wt.BOOL),
                ("GlobalSymbols", wt.BOOL), ("TypeInfo", wt.BOOL), ("SourceIndexed", wt.BOOL),
                ("Publics", wt.BOOL), ("MachineType", wt.DWORD), ("Reserved", wt.DWORD)]


def enumerate_symbols(image: Image, pdb: Path):
    """Call serially. Each noninvading DbgHelp session gets a unique token."""
    dbg = ct.WinDLL("dbghelp.dll", use_last_error=True)
    token = ct.c_uint64()
    session = wt.HANDLE(ct.addressof(token))
    callback_type = ct.WINFUNCTYPE(wt.BOOL, ct.POINTER(SymbolInfo), wt.ULONG, ct.c_void_p)
    signatures = {
        "SymSetOptions": ([wt.DWORD], wt.DWORD),
        "SymInitializeW": ([wt.HANDLE, wt.LPCWSTR, wt.BOOL], wt.BOOL),
        "SymLoadModuleExW": ([wt.HANDLE, wt.HANDLE, wt.LPCWSTR, wt.LPCWSTR, ct.c_ulonglong, wt.DWORD, ct.c_void_p, wt.DWORD], ct.c_ulonglong),
        "SymCleanup": ([wt.HANDLE], wt.BOOL),
        "SymGetModuleInfo64": ([wt.HANDLE, ct.c_ulonglong, ct.POINTER(ModuleInfo)], wt.BOOL),
        "SymEnumSymbols": ([wt.HANDLE, ct.c_ulonglong, ct.c_char_p, callback_type, ct.c_void_p], wt.BOOL),
        "UnDecorateSymbolName": ([ct.c_char_p, ct.c_char_p, wt.DWORD, wt.DWORD], wt.DWORD),
    }
    for name, (arguments, result) in signatures.items():
        method = getattr(dbg, name)
        method.argtypes, method.restype = arguments, result
    dbg.SymSetOptions(0x400 | 0x200 | 0x80000 | 0x1000)
    require(dbg.SymInitializeW(session, str(pdb.parent), False), f"SymInitializeW failed: {ct.get_last_error()}")
    try:
        base = dbg.SymLoadModuleExW(session, None, str(image.path), None, 0x180000000,
                                    image.identity["image_size"], None, 0)
        require(base != 0, f"SymLoadModuleExW failed: {ct.get_last_error()}")
        symbols, callback_errors = {}, []

        @callback_type
        def collect(pointer, _size, _context):
            try:
                value = pointer.contents
                require(0 < value.NameLen <= 8192 and len(symbols) < 20000, "Symbol enumeration limit exceeded")
                require(value.ModBase == base and base <= value.Address < base + image.identity["image_size"],
                        "Symbol outside registered image")
                raw = ct.string_at(ct.addressof(value) + SymbolInfo.Name.offset, value.NameLen)
                decorated = raw.decode("utf-8")
                buffer = ct.create_string_buffer(16384)
                length = dbg.UnDecorateSymbolName(raw, buffer, len(buffer), 0)
                plain = buffer.value.decode("utf-8", "replace") if length else decorated
                rva = value.Address - base
                symbols[decorated, rva] = {"name": plain, "decorated_name": decorated, "rva": rva,
                                          "size": value.Size, "tag": value.Tag}
                return True
            except Exception as error:
                callback_errors.append(str(error))
                return False

        for mask in MASKS[image.role]:
            succeeded = dbg.SymEnumSymbols(session, base, mask.encode("ascii"), collect, None)
            require(succeeded and not callback_errors,
                    f"Symbol enumeration failed ({mask}): {callback_errors or ct.get_last_error()}")
        info = ModuleInfo()
        info.SizeOfStruct = ct.sizeof(info)
        require(dbg.SymGetModuleInfo64(session, base, ct.byref(info)), "SymGetModuleInfo64 failed")
        require(info.SymType == 3 and not info.PdbUnmatched and not info.DbgUnmatched,
                "Exact PDB debug symbols were not loaded")
        require(list(info.PdbSig70) == image.identity["pdb_guid"] and info.PdbAge == image.identity["pdb_age"],
                "DbgHelp PDB identity mismatch")
        require(info.ImageSize == image.identity["image_size"] and info.TimeDateStamp == image.identity["timestamp"],
                "DbgHelp registered a different PE image")
        image.symbols = sorted(symbols.values(), key=lambda s: (s["rva"], s["decorated_name"]))
        image.symbol_proof = {"pdb_guid": list(info.PdbSig70), "pdb_age": info.PdbAge,
                              "pdb_unmatched": False, "dbg_unmatched": False, "sym_type": info.SymType,
                              "enumerated_symbols": len(symbols)}
    finally:
        dbg.SymCleanup(session)
