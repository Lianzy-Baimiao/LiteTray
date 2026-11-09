//! Bounds-checked PE identities and section mapping shared by disk inspection
//! and module recognition. An identity alone does not authorize private calls;
//! recognition must prove a complete image or an unchanged implementation.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageIdentity {
    pub machine: u16,
    pub image_size: u32,
    pub timestamp: u32,
    pub sha256: String,
    pub pdb_name: String,
    pub pdb_key: String,
    pub pdb_guid: [u8; 16],
    pub pdb_age: u32,
}
#[derive(Debug)]
pub struct ImageSection {
    pub rva: u32,
    pub virtual_size: u32,
    pub raw_offset: u32,
    pub raw_size: u32,
    pub characteristics: u32,
}
pub fn sections(bytes: &[u8]) -> Result<Vec<ImageSection>, String> {
    let pe = u32_at(bytes, 0x3c)? as usize;
    let count = u16_at(bytes, pe + 6)? as usize;
    if count > 96 {
        return Err("PE 节数过多".into());
    }
    let first = pe + 24 + u16_at(bytes, pe + 20)? as usize;
    (0..count)
        .map(|i| {
            let s = first + i * 40;
            Ok(ImageSection {
                rva: u32_at(bytes, s + 12)?,
                virtual_size: u32_at(bytes, s + 8)?,
                raw_offset: u32_at(bytes, s + 20)?,
                raw_size: u32_at(bytes, s + 16)?,
                characteristics: u32_at(bytes, s + 36)?,
            })
        })
        .collect()
}
pub fn rva_bytes(bytes: &[u8], rva: u32, length: usize) -> Result<(&[u8], ImageSection), String> {
    for section in sections(bytes)? {
        if let Some(offset) = rva.checked_sub(section.rva)
            && offset < section.raw_size
        {
            let end = (offset as usize).checked_add(length).ok_or("RVA 溢出")?;
            if end > section.raw_size as usize {
                return Err("符号超出磁盘节区".into());
            }
            let start = section.raw_offset as usize + offset as usize;
            let slice = bytes
                .get(start..start.checked_add(length).ok_or("RVA 溢出")?)
                .ok_or("符号数据被截断")?;
            return Ok((slice, section));
        }
    }
    Err("符号 RVA 没有对应节区".into())
}
fn u16_at(b: &[u8], offset: usize) -> Result<u16, String> {
    let x = b
        .get(offset..offset.checked_add(2).ok_or("地址溢出")?)
        .ok_or("PE 头被截断")?;
    Ok(u16::from_le_bytes([x[0], x[1]]))
}
fn u32_at(b: &[u8], offset: usize) -> Result<u32, String> {
    let x = b
        .get(offset..offset.checked_add(4).ok_or("地址溢出")?)
        .ok_or("PE 头被截断")?;
    Ok(u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}
pub fn identity(bytes: &[u8]) -> Result<ImageIdentity, String> {
    if bytes.get(..2) != Some(b"MZ") {
        return Err("不是 Windows PE 文件".into());
    }
    let pe = u32_at(bytes, 0x3c)? as usize;
    if bytes.get(pe..pe.checked_add(4).ok_or("地址溢出")?) != Some(b"PE\0\0") {
        return Err("PE 签名无效".into());
    }
    let machine = u16_at(bytes, pe + 4)?;
    let section_count = u16_at(bytes, pe + 6)? as usize;
    if section_count > 96 {
        return Err("PE 节数过多".into());
    }
    let timestamp = u32_at(bytes, pe + 8)?;
    let optional_size = u16_at(bytes, pe + 20)? as usize;
    let optional = pe + 24;
    if u16_at(bytes, optional)? != 0x20b {
        return Err("目前只支持 PE32+（x64/ARM64）".into());
    }
    if optional_size < 112 + 7 * 8 || u32_at(bytes, optional + 108)? < 7 {
        return Err("缺少 PE 调试目录".into());
    }
    let image_size = u32_at(bytes, optional + 56)?;
    let debug_rva = u32_at(bytes, optional + 112 + 6 * 8)?;
    let debug_size = u32_at(bytes, optional + 112 + 6 * 8 + 4)? as usize;
    if debug_size == 0 || debug_size > 4096 || !debug_size.is_multiple_of(28) {
        return Err("PE 调试目录尺寸无效".into());
    }
    let sections = optional + optional_size;
    let mut debug_offset = None;
    for i in 0..section_count {
        let section = sections + i * 40;
        let rva = u32_at(bytes, section + 12)?;
        let raw_size = u32_at(bytes, section + 16)?;
        let raw_offset = u32_at(bytes, section + 20)?;
        if let Some(delta) = debug_rva.checked_sub(rva)
            && delta < raw_size
            && (delta as usize + debug_size) <= raw_size as usize
        {
            debug_offset = Some(raw_offset as usize + delta as usize);
            break;
        }
    }
    let debug = debug_offset.ok_or("无法映射 PE 调试目录")?;
    for i in (0..debug_size).step_by(28) {
        let entry = debug + i;
        if u32_at(bytes, entry + 12)? != 2 {
            continue;
        }
        let length = u32_at(bytes, entry + 16)? as usize;
        let offset = u32_at(bytes, entry + 24)? as usize;
        if !(25..=4096).contains(&length) {
            continue;
        }
        let cv = bytes
            .get(offset..offset.checked_add(length).ok_or("地址溢出")?)
            .ok_or("CodeView 被截断")?;
        if cv.get(..4) != Some(b"RSDS") {
            continue;
        }
        let guid: [u8; 16] = cv[4..20].try_into().map_err(|_| "PDB GUID 无效")?;
        let age = u32_at(cv, 20)?;
        let end = cv[24..]
            .iter()
            .position(|b| *b == 0)
            .ok_or("PDB 名称无结束符")?;
        let path = std::str::from_utf8(&cv[24..24 + end]).map_err(|_| "PDB 名称编码无效")?;
        let name = path.rsplit(['\\', '/']).next().unwrap_or("");
        if !name.to_ascii_lowercase().ends_with(".pdb")
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        {
            return Err("PDB 名称无效".into());
        }
        let key = format!(
            "{:08X}{:04X}{:04X}{}{:X}",
            u32::from_le_bytes(guid[..4].try_into().unwrap()),
            u16::from_le_bytes(guid[4..6].try_into().unwrap()),
            u16::from_le_bytes(guid[6..8].try_into().unwrap()),
            guid[8..]
                .iter()
                .map(|v| format!("{v:02X}"))
                .collect::<String>(),
            age
        );
        return Ok(ImageIdentity {
            machine,
            image_size,
            timestamp,
            sha256: format!("{:x}", Sha256::digest(bytes)),
            pdb_name: name.into(),
            pdb_key: key,
            pdb_guid: guid,
            pdb_age: age,
        });
    }
    Err("模块没有 RSDS 调试签名".into())
}
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
