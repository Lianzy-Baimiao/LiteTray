"""Mutation tests for explicit early XFG host/source contracts; no OS access."""
import copy
import json
from pathlib import Path
import struct
from types import SimpleNamespace
import unittest

from capstone import Cs, CS_ARCH_X86, CS_MODE_64
from evidence import EvidenceError, Image
from layouts import CHILD_TABLE, xfg_host_layout, xfg_source_layout


FIXTURE = json.loads((Path(__file__).parent / 'fixtures/xfg-source-v1.json').read_text(encoding='utf-8'))


def fixture():
    decoder = Cs(CS_ARCH_X86, CS_MODE_64)
    decoder.detail = True
    images = {}
    for role in ('taskbar', 'shell'):
        image = Image.__new__(Image)
        image.role = role
        image.identity = copy.deepcopy(FIXTURE['host']['module'])
        image.symbols = copy.deepcopy(FIXTURE[role]['symbols'])
        image.bodies = {int(rva): bytes.fromhex(raw) for rva, raw in FIXTURE[role]['code'].items()}
        image.function = lambda rva, image=image: list(decoder.disasm(image.bodies[rva], rva))
        image.bytes = lambda rva, count, executable=True, image=image: image.bodies[rva][:count]
        image.section = lambda rva, count, executable: {
            'section_start': 4096, 'section_size': 0x800000, 'executable': executable}
        images[role] = image
    host, shell = images['taskbar'], images['shell']
    host.pointers = {1983392 + 0x3f0: 0x49790, 1996344 + 0x118: 0x52180}
    host.pointer = host.pointers.__getitem__
    child = shell.exact(CHILD_TABLE)
    shell.table = {child + i * 8: e['rva'] for i, e in enumerate(FIXTURE['source']['child_vtable_entries'])}
    shell.pe = SimpleNamespace(
        OPTIONAL_HEADER=SimpleNamespace(ImageBase=0x180000000),
        sections=[SimpleNamespace(VirtualAddress=child, Misc_VirtualSize=104, SizeOfRawData=104,
                                  Name=b'.data\0\0\0', Characteristics=0xc0000040)],
        DIRECTORY_ENTRY_BASERELOC=[SimpleNamespace(entries=[SimpleNamespace(rva=r, type=10) for r in shell.table])],
        get_data=lambda rva, count: struct.pack('<Q', 0x180000000 + shell.table[rva])[:count],
    )
    return host, shell


class EarlyXfg(unittest.TestCase):
    def test_complete_host_and_source_prove_five_roles_and_all_relocated_slots(self):
        host, shell = fixture()
        result = xfg_host_layout(host, [])
        self.assertEqual([s['role'] for s in result['symbols']],
                         ['primary_site', 'secondary_site', 'primary_host', 'secondary_host', 'release_host'])
        self.assertNotIn('element_offset', result)
        source = xfg_source_layout(host, shell, [])
        self.assertEqual(source, FIXTURE['source'])
        self.assertNotIn('factory_vtable_rva', source)

    def test_host_rejects_wrong_return_wrapper_refcount_or_xfg_contract(self):
        mutations = (
            (0x4a7d0, '488bda', '488bd9'),
            (0x57120, '488b4938', '488b4940'),
            (0x4a7d0, '488b80f0030000', '488b80e8030000'),
            (0x57120, '49ba70215d1457074d98', '49ba71215d1457074d98'),
            (0x49790, '498bc8', '498bc9'),
            (0x52180, '498bc0', '498bc1'),
            (0x513c, 'f0ff4008', 'f0ff400c'),
            (0x513c, '7404', '7403'),
            (0x513c, '48894108', '48894110'),
            (0x25644, 'f00fc14108', 'f00fc1410c'),
            (0x25644, '488b4008', '488b4010'),
            (0x25644, '75d7', '75d6'),
        )
        for rva, before, after in mutations:
            with self.subTest(rva=hex(rva), before=before):
                host, _ = fixture()
                changed = host.bodies[rva].replace(bytes.fromhex(before), bytes.fromhex(after), 1)
                self.assertNotEqual(changed, host.bodies[rva])
                host.bodies[rva] = changed
                with self.assertRaises(EvidenceError):
                    xfg_host_layout(host, [])

    def test_host_virtual_slot_must_point_to_the_exact_named_getter(self):
        for slot in (1983392 + 0x3f0, 1996344 + 0x118):
            host, _ = fixture()
            host.pointers[slot] += 1
            with self.assertRaisesRegex(EvidenceError, 'exact named getter'):
                xfg_host_layout(host, [])

    def test_source_rejects_changed_focus_and_getter_transfer_cleanup(self):
        mutations = (
            ('taskbar', 0x12c23c, '4883c120', '4883c128'),
            ('taskbar', 0x12c23c, '7405', '7404'),
            ('taskbar', 0x9500c, '488b4060', '488b4068'),
            ('taskbar', 0x9500c, 'c644242000', 'c644242001'),
            ('shell', 0xdcd00, '488d41f0', '488d41e8'),
            ('shell', 0xdcd00, '48832000', '48832001'),
            ('shell', 0xdcd00, '48890b', '48890f'),
            ('shell', 0xdcd00, 'ebf3', 'ebf2'),
            ('shell', 0x27dbc2, '488d4c2430', '488d4c2438'),
            ('shell', 0x27dbc2, 'e87425e2ff', 'e87525e2ff'),
            ('shell', 0x27dbc2, 'e968f1e5ff', 'e969f1e5ff'),
        )
        for role, rva, before, after in mutations:
            with self.subTest(role=role, rva=hex(rva), before=before):
                host, shell = fixture()
                image = host if role == 'taskbar' else shell
                changed = image.bodies[rva].replace(bytes.fromhex(before), bytes.fromhex(after), 1)
                self.assertNotEqual(changed, image.bodies[rva])
                image.bodies[rva] = changed
                with self.assertRaises(EvidenceError):
                    xfg_source_layout(host, shell, [])

    def test_source_table_requires_exact_range_flags_and_each_unique_dir64(self):
        for mutation in ('flags', 'name', 'size', 'missing', 'type', 'duplicate'):
            with self.subTest(mutation=mutation):
                host, shell = fixture()
                section = shell.pe.sections[0]
                entries = shell.pe.DIRECTORY_ENTRY_BASERELOC[0].entries
                if mutation == 'flags':
                    section.Characteristics |= 0x20000000
                elif mutation == 'name':
                    section.Name = b'.other'
                elif mutation == 'size':
                    section.SizeOfRawData -= 1
                elif mutation == 'missing':
                    entries.pop()
                elif mutation == 'type':
                    entries[3].type = 3
                else:
                    entries.append(copy.copy(entries[2]))
                with self.assertRaises(EvidenceError):
                    xfg_source_layout(host, shell, [])

    def test_source_table_rejects_noncode_target_and_wrong_getter_slot(self):
        host, shell = fixture()
        shell.table[shell.exact(CHILD_TABLE) + 8 * 8] = 0xdcd50
        with self.assertRaises(EvidenceError):
            xfg_source_layout(host, shell, [])
        host, shell = fixture()
        original = shell.bytes
        def reject_noncode(rva, count, executable=True):
            if rva == FIXTURE['source']['child_vtable_entries'][2]['rva']:
                raise EvidenceError('Non-executable table target')
            return original(rva, count, executable)
        shell.bytes = reject_noncode
        with self.assertRaisesRegex(EvidenceError, 'Non-executable'):
            xfg_source_layout(host, shell, [])


if __name__ == '__main__':
    unittest.main()
