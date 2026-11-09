//! Shared bounds-checked PE reader used by disk detection and the adapter.
pub use litetray_adapter_protocol::pe::*;
#[cfg(test)]
pub(crate) fn fixture() -> Vec<u8> {
    let mut b = vec![0u8; 0x1000];
    b[..2].copy_from_slice(b"MZ");
    b[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
    b[0x80..0x84].copy_from_slice(b"PE\0\0");
    b[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
    b[0x86..0x88].copy_from_slice(&2u16.to_le_bytes());
    b[0x94..0x96].copy_from_slice(&240u16.to_le_bytes());
    b[0x98..0x9a].copy_from_slice(&0x20bu16.to_le_bytes());
    b[0xd0..0xd4].copy_from_slice(&0x3000u32.to_le_bytes());
    b[0x104..0x108].copy_from_slice(&16u32.to_le_bytes());
    b[0x138..0x13c].copy_from_slice(&0x2200u32.to_le_bytes());
    b[0x13c..0x140].copy_from_slice(&28u32.to_le_bytes());
    for (header, rva, raw, permissions) in [
        (0x188, 0x1000u32, 0x200u32, 0x60000020u32),
        (0x1b0, 0x2000, 0x800, 0x40000040),
    ] {
        for (offset, value) in [
            (8, 0x600u32),
            (12, rva),
            (16, 0x600),
            (20, raw),
            (36, permissions),
        ] {
            b[header + offset..header + offset + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    for (offset, value) in [(12, 2u32), (16, 32), (20, 0x2240), (24, 0xa40)] {
        b[0xa00 + offset..0xa00 + offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    b[0xa40..0xa44].copy_from_slice(b"RSDS");
    b[0xa44..0xa54].copy_from_slice(&[1; 16]);
    b[0xa54..0xa58].copy_from_slice(&1u32.to_le_bytes());
    b[0xa58..0xa60].copy_from_slice(b"test.pdb");
    // Include the terminating NUL in the CodeView directory's length.
    b[0xa10..0xa14].copy_from_slice(&33u32.to_le_bytes());
    b[0x300..0x380].fill(0x90);
    b[0x340..0x348].copy_from_slice(&[0x48, 0x83, 0xec, 0x28, 0x48, 0x83, 0xc1, 0x48]);
    b
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_truncated_and_invalid_images_without_panicking() {
        for bytes in [vec![], b"MZ".to_vec(), vec![0; 512], {
            let mut b = vec![0; 64];
            b[..2].copy_from_slice(b"MZ");
            b[0x3c..].copy_from_slice(&u32::MAX.to_le_bytes());
            b
        }] {
            assert!(identity(&bytes).is_err());
        }
    }
    #[test]
    fn maps_executable_and_readonly_sections_without_crossing_boundaries() {
        let b = fixture();
        let image = identity(&b).unwrap();
        assert_eq!(image.pdb_name, "test.pdb");
        assert_eq!(image.image_size, 0x3000);
        let (code, section) = rva_bytes(&b, 0x1140, 16).unwrap();
        assert_eq!(&code[..3], &[0x48, 0x83, 0xec]);
        assert_eq!(section.characteristics & 0x20000000, 0x20000000);
        assert_eq!(
            rva_bytes(&b, 0x2000, 8).unwrap().1.characteristics & 0x20000000,
            0
        );
        for (rva, length) in [
            (0x15ff, 2),
            (0x1600, 1),
            (u32::MAX, 16),
            (0x1000, usize::MAX),
        ] {
            assert!(rva_bytes(&b, rva, length).is_err());
        }
    }
}
