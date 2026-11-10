"""The cached later 13-slot source uses ec97, independently of field layout."""
import json
from pathlib import Path
import unittest

from evidence import EvidenceError
from layouts import xfg_source_layout
from test_early_xfg import fixture

LATE = json.loads((Path(__file__).parent / 'fixtures/xfg-source-ec97-v1.json').read_text(encoding='utf-8'))


class LateXfg(unittest.TestCase):
    def test_actual_late_source_keeps_field_recipe_and_selects_its_proven_iid(self):
        host, shell = fixture(LATE)
        source = xfg_source_layout(host, shell, [])
        self.assertEqual(source, LATE['source'])
        self.assertEqual(source['recipe_candidate'], 'xaml_explorer_host_child_xfg_v1')
        self.assertEqual(source['child_interface'], 'xaml_explorer_host_child_ec97_v1')
        self.assertEqual(len(source['child_vtable_entries']), 13)
        self.assertEqual(source['evidence']['child_query_interface']['iid'], 'ec97afd0-5c0d-581b-9508-074773c6f1e0')
        self.assertTrue(source['evidence']['child_query_interface']['owned_reference'])

    def test_named_guid_and_enumerated_interfaces_must_agree(self):
        for rva, replacement in ((0x6d3780, bytes.fromhex('32f6f8ee0d6ccb5fb6a1354819e51f3b')),
                                 (0x6d3780, bytes(16)), (0x693938, bytes(32))):
            with self.subTest(rva=hex(rva)):
                host, shell = fixture(LATE)
                self.assertIn(rva, shell.data_blocks)
                shell.data_blocks[rva] = replacement
                with self.assertRaises(EvidenceError):
                    xfg_source_layout(host, shell, [])

    def test_late_query_owned_result_and_getter_transfer_are_not_inferred_from_iid(self):
        for rva, before, after in (
            (0x17b440, '488d41f0', '488d41e8'),
            (0xfae70, '498d4110', '498d4118'),
            (0xf7f74, 'f0480fb14b08', 'f0480fb14b10'),
            (0xf7f74, '488d4a01', '488d4a02'),
            (0x2b141c, 'f00fc1441218', 'f00fc1441220'),
            (0xf9d00, '48832000', '48832001'),
            (0xf9d00, '48890b', '48890f'),
        ):
            with self.subTest(rva=hex(rva), before=before):
                host, shell = fixture(LATE)
                old = shell.bodies[rva]
                shell.bodies[rva] = old.replace(bytes.fromhex(before), bytes.fromhex(after), 1)
                self.assertNotEqual(old, shell.bodies[rva])
                with self.assertRaises(EvidenceError):
                    xfg_source_layout(host, shell, [])

    def test_late_release_contract_rejects_wrong_counters_destructor_or_free(self):
        for rva, before, after in (
            (0x38090, '498b08', '498b48'),
            (0x38090, '488d5101', '488d5102'),
            (0x245c86, 'f00fc1540918', 'f00fc1540920'),
            (0xf7b90, '488d41f0', '488d41e8'),
            (0xf7da8, '488b4108', '488b4110'),
            (0xf7ed8, '488d50ff', '488d50fe'),
            (0xf7ed8, 'f00fc15918', 'f00fc15920'),
            (0x41e580, 'f00fc1791c', 'f00fc17918'),
            (0x188ee8, 'ff25b2184e00', 'ff25b3184e00'),
        ):
            with self.subTest(rva=hex(rva), before=before):
                host, shell = fixture(LATE)
                old = shell.bodies[rva]
                shell.bodies[rva] = old.replace(bytes.fromhex(before), bytes.fromhex(after), 1)
                self.assertNotEqual(old, shell.bodies[rva])
                with self.assertRaises(EvidenceError):
                    xfg_source_layout(host, shell, [])
        host, shell = fixture(LATE)
        shell.table[0x7aa328] += 1
        with self.assertRaises(EvidenceError):
            xfg_source_layout(host, shell, [])


if __name__ == '__main__':
    unittest.main()
