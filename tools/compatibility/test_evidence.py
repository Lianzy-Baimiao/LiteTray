"""Offline rejection tests: no system DLLs, downloads, COM or Explorer access."""
from pathlib import Path
import struct
from tempfile import TemporaryDirectory
from types import SimpleNamespace
import unittest

from capstone import Cs, CS_ARCH_X86, CS_MODE_64

from evidence import EvidenceError, Image, acquire_pdb, pdb_identity, within
from inspect_modules import choices
from layouts import (GUARD_DISPATCH, UNKNOWN_DESTRUCTOR, UNKNOWN_RELEASE,
                     aligned_offset, find_sequence, owned_cleanup)


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


class Rejections(unittest.TestCase):
    def test_both_owned_wrapper_cleanup_call_graphs_are_verified(self):
        for delegated, recipe in ((False, "inline_null_check_release_v1"),
                                  (True, "delegated_null_check_release_v1")):
            image, code, _, _ = cleanup_fixture(delegated)
            self.assertEqual(owned_cleanup(image, code, [])["recipe"], recipe)

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
