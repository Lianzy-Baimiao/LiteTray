//! Bounds-checked identities for build-specific adapters. Addresses from one
//! module image must never be reused for a different image or PDB signature.
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
}
