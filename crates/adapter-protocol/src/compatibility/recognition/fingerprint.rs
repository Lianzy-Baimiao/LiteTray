//! A deliberately narrow equivalence relation: identical file bytes after
//! normalizing enumerated, non-executable metadata. All section headers/RVAs,
//! instructions, imports, initialized data, vtables, unwind/relocation/CFG data
//! and non-version resources remain in the digest. This is not pattern search.
use crate::pe;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, ops::Range};

pub(super) const ALGORITHM: &str = "litetray-execution-equivalence-v1";
const EXECUTE: u32 = 0x2000_0000;
const WRITE: u32 = 0x8000_0000;
const MAX_FILE: usize = 128 * 1024 * 1024;

fn word(bytes: &[u8], offset: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset.checked_add(2).ok_or("字段溢出")?)
            .ok_or("字段截断")?
            .try_into()
            .map_err(|_| "字段截断")?,
    ))
}
fn dword(bytes: &[u8], offset: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset.checked_add(4).ok_or("字段溢出")?)
            .ok_or("字段截断")?
            .try_into()
            .map_err(|_| "字段截断")?,
    ))
}
fn span(start: usize, length: usize, limit: usize) -> Result<Range<usize>, String> {
    let end = start.checked_add(length).ok_or("区间溢出")?;
    if length == 0 || end > limit {
        return Err("区间为空或超出图像".into());
    }
    Ok(start..end)
}
fn overlap(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}
fn align4(value: usize) -> Result<usize, String> {
    value
        .checked_add(3)
        .map(|v| v & !3)
        .ok_or_else(|| "对齐溢出".into())
}

struct Image<'a> {
    bytes: &'a [u8],
    pe: usize,
    optional: usize,
    sections: Vec<pe::ImageSection>,
    directories: Vec<(u32, u32)>,
    headers: usize,
    raw_end: usize,
}
impl<'a> Image<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.len() > MAX_FILE {
            return Err("模块超过检查上限".into());
        }
        let identity = pe::identity(bytes)?;
        if identity.machine != 0x8664 {
            return Err("实现指纹仅支持 x64".into());
        }
        let pe = dword(bytes, 0x3c)? as usize;
        let optional = pe + 24;
        let optional_size = word(bytes, pe + 20)? as usize;
        let count = dword(bytes, optional + 108)? as usize;
        if count != 16 || optional_size != 240 {
            return Err("未知 PE32+ 目录布局".into());
        }
        let sections = pe::sections(bytes)?;
        let headers = dword(bytes, optional + 60)? as usize;
        if sections.is_empty()
            || headers < optional + optional_size + sections.len() * 40
            || headers > bytes.len()
        {
            return Err("PE 头或节区范围无效".into());
        }
        let mut raw = vec![];
        let mut virtual_ranges = vec![];
        for section in &sections {
            if section.raw_size != 0 {
                if (section.raw_offset as usize) < headers {
                    return Err("节区覆盖 PE 头".into());
                }
                raw.push(span(
                    section.raw_offset as usize,
                    section.raw_size as usize,
                    bytes.len(),
                )?);
            }
            let size = section.virtual_size.max(section.raw_size) as usize;
            if size != 0 {
                if (section.rva as usize) < headers {
                    return Err("节区虚拟地址覆盖 PE 头".into());
                }
                virtual_ranges.push(span(
                    section.rva as usize,
                    size,
                    identity.image_size as usize,
                )?);
            }
        }
        for ranges in [&mut raw, &mut virtual_ranges] {
            ranges.sort_by_key(|r| r.start);
            if ranges.windows(2).any(|r| overlap(&r[0], &r[1])) {
                return Err("PE 节区重叠".into());
            }
        }
        let raw_end = raw.last().map_or(headers, |r| r.end);
        let directories = (0..count)
            .map(|index| {
                let offset = optional + 112 + index * 8;
                Ok((dword(bytes, offset)?, dword(bytes, offset + 4)?))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let image = Self {
            bytes,
            pe,
            optional,
            sections,
            directories,
            headers,
            raw_end,
        };
        for (index, &(rva, size)) in image.directories.iter().enumerate() {
            if index != 4 && (rva != 0 || size != 0) {
                if rva == 0 || size == 0 {
                    return Err("PE 目录地址和长度不一致".into());
                }
                image.map(rva, size as usize, false)?;
            }
        }
        Ok(image)
    }
    fn map(&self, rva: u32, length: usize, readonly: bool) -> Result<Range<usize>, String> {
        for section in &self.sections {
            let Some(delta) = rva.checked_sub(section.rva) else {
                continue;
            };
            if delta >= section.raw_size {
                continue;
            }
            if readonly && section.characteristics & (EXECUTE | WRITE) != 0 {
                return Err("元数据位于执行或可写节区".into());
            }
            if (delta as usize)
                .checked_add(length)
                .is_none_or(|end| end > section.raw_size as usize)
            {
                return Err("元数据跨越节区".into());
            }
            return span(
                section.raw_offset as usize + delta as usize,
                length,
                self.bytes.len(),
            );
        }
        Err("PE 目录无文件映射".into())
    }
    fn certificate_start(&self) -> Result<usize, String> {
        let (offset, size) = self.directories[4];
        if offset == 0 && size == 0 {
            if self.bytes.len() != self.raw_end {
                return Err("文件末尾存在未知附加数据".into());
            }
            return Ok(self.bytes.len());
        }
        let offset = offset as usize;
        let size = size as usize;
        let end = offset.checked_add(size).ok_or("证书区间溢出")?;
        if size < 8
            || !offset.is_multiple_of(8)
            || offset < self.raw_end
            || offset - self.raw_end > 7
            || self
                .bytes
                .get(self.raw_end..offset)
                .is_none_or(|v| v.iter().any(|b| *b != 0))
            || end != self.bytes.len()
        {
            return Err("证书不是独立、对齐的文件尾部".into());
        }
        let mut cursor = offset;
        let mut count = 0;
        while cursor < end {
            let length = dword(self.bytes, cursor)? as usize;
            if length < 8
                || word(self.bytes, cursor + 4)? != 0x200
                || word(self.bytes, cursor + 6)? != 2
                || cursor.checked_add(length).is_none_or(|n| n > end)
            {
                return Err("WIN_CERTIFICATE 结构无效".into());
            }
            let padded = length.checked_add(7).ok_or("证书长度溢出")? & !7;
            let next = cursor.checked_add(padded).ok_or("证书长度溢出")?;
            if next > end || self.bytes[cursor + length..next].iter().any(|b| *b != 0) {
                return Err("证书填充无效".into());
            }
            cursor = next;
            count += 1;
            if count > 16 {
                return Err("证书数量超限".into());
            }
        }
        Ok(offset)
    }
    fn debug_ranges(&self, ranges: &mut Vec<Range<usize>>) -> Result<Vec<Range<usize>>, String> {
        let (rva, size) = self.directories[6];
        if size == 0 || size > 4096 || size % 28 != 0 {
            return Err("调试目录长度无效".into());
        }
        let directory = self.map(rva, size as usize, true)?;
        let mut data_ranges = vec![];
        let mut rsds_count = 0;
        for entry in (directory.start..directory.end).step_by(28) {
            ranges.push(entry + 4..entry + 8); // IMAGE_DEBUG_DIRECTORY timestamp.
            let kind = dword(self.bytes, entry + 12)?;
            let size = dword(self.bytes, entry + 16)? as usize;
            if size == 0 {
                continue;
            }
            let rva = dword(self.bytes, entry + 20)?;
            let raw = dword(self.bytes, entry + 24)? as usize;
            let data = self.map(rva, size, true)?;
            if data.start != raw
                || overlap(&directory, &data)
                || data_ranges.iter().any(|other| overlap(other, &data))
            {
                return Err("调试数据地址冲突或重叠".into());
            }
            data_ranges.push(data.clone());
            match kind {
                2 => {
                    if !(25..=4096).contains(&size)
                        || &self.bytes[data.start..data.start + 4] != b"RSDS"
                    {
                        return Err("未知 CodeView 结构".into());
                    }
                    ranges.push(data.start + 4..data.start + 24); // GUID and age, not the path.
                    rsds_count += 1;
                }
                16 => {
                    // IMAGE_DEBUG_TYPE_REPRO: fixed length plus a content digest.
                    if size != 36 || dword(self.bytes, data.start)? != 32 {
                        return Err("未知可复现构建调试摘要".into());
                    }
                    ranges.push(data.start + 4..data.end);
                }
                19 => {
                    let value = &self.bytes[data.clone()];
                    let algorithm = if value.starts_with(b"SHA256\0") && size == 7 + 32 {
                        7
                    } else if value.starts_with(b"SHA1\0") && size == 5 + 20 {
                        5
                    } else {
                        return Err("未知 PDB 校验和结构".into());
                    };
                    ranges.push(data.start + algorithm..data.end);
                }
                // POGO/function ranges, VC feature counts and extended DLL
                // characteristics (e.g. CET) are deliberately NOT normalized.
                _ => {}
            }
        }
        if rsds_count != 1 {
            return Err("模块没有唯一 RSDS 身份".into());
        }
        data_ranges.push(directory);
        Ok(data_ranges)
    }
}

struct ResourceLeaf {
    data: Range<usize>,
    version: bool,
}
struct Resources<'a, 'b> {
    image: &'a Image<'b>,
    base: Range<usize>,
    visited: BTreeSet<usize>,
    structures: Vec<Range<usize>>,
    leaves: Vec<ResourceLeaf>,
}
impl Resources<'_, '_> {
    fn relative(&self, offset: usize, size: usize) -> Result<Range<usize>, String> {
        let value = span(
            self.base.start.checked_add(offset).ok_or("资源地址溢出")?,
            size,
            self.base.end,
        )?;
        Ok(value)
    }
    fn walk(&mut self, offset: usize, depth: usize, version: bool) -> Result<(), String> {
        if depth > 2 || self.visited.len() >= 4096 || !self.visited.insert(offset) {
            return Err("资源目录循环、共享或超限".into());
        }
        let header = self.relative(offset, 16)?;
        let bytes = self.image.bytes;
        let names = word(bytes, header.start + 12)? as usize;
        let ids = word(bytes, header.start + 14)? as usize;
        let count = names + ids;
        if count > 4096 {
            return Err("资源目录条目超限".into());
        }
        let table = self.relative(offset, 16 + count * 8)?;
        self.structures.push(table.clone());
        for index in 0..count {
            let entry = header.start + 16 + index * 8;
            let name = dword(bytes, entry)?;
            if name & 0x8000_0000 != 0 {
                let name_offset = (name & 0x7fff_ffff) as usize;
                let first = self.relative(name_offset, 2)?;
                let length = word(bytes, first.start)? as usize;
                self.structures
                    .push(self.relative(name_offset, 2 + length * 2)?);
            }
            let version = if depth == 0 { name == 16 } else { version };
            let target = dword(bytes, entry + 4)?;
            if target & 0x8000_0000 != 0 {
                self.walk((target & 0x7fff_ffff) as usize, depth + 1, version)?;
            } else {
                if depth != 2 || self.leaves.len() >= 4096 {
                    return Err("未知资源树形状".into());
                }
                let descriptor = self.relative(target as usize, 16)?;
                let rva = dword(bytes, descriptor.start)?;
                let length = dword(bytes, descriptor.start + 4)? as usize;
                let data = self.image.map(rva, length, true)?;
                if data.start < self.base.start || data.end > self.base.end {
                    return Err("资源数据不在资源目录范围内".into());
                }
                self.structures.push(descriptor);
                self.leaves.push(ResourceLeaf { data, version });
            }
        }
        Ok(())
    }
}

fn utf16_key(bytes: &[u8], start: usize, end: usize) -> Result<(String, usize), String> {
    let mut values = vec![];
    let mut cursor = start;
    while cursor.checked_add(2).is_some_and(|next| next <= end) {
        let value = word(bytes, cursor)?;
        cursor += 2;
        if value == 0 {
            return Ok((
                String::from_utf16(&values).map_err(|_| "版本键不是 UTF16")?,
                cursor,
            ));
        }
        values.push(value);
        if values.len() > 512 {
            break;
        }
    }
    Err("版本键没有结束符".into())
}
fn version_block(
    bytes: &[u8],
    start: usize,
    limit: usize,
    path: &mut Vec<String>,
    ranges: &mut Vec<Range<usize>>,
    count: &mut usize,
) -> Result<usize, String> {
    *count += 1;
    if *count > 256 || path.len() > 3 {
        return Err("版本资源结构超限".into());
    }
    let length = word(bytes, start)? as usize;
    let value_length = word(bytes, start + 2)? as usize;
    let kind = word(bytes, start + 4)?;
    if length < 8 || kind > 1 {
        return Err("版本资源块无效".into());
    }
    let block = span(start, length, limit)?;
    let (key, key_end) = utf16_key(bytes, start + 6, block.end)?;
    let value_start = align4(key_end)?;
    let value_bytes = if kind == 1 {
        value_length * 2
    } else {
        value_length
    };
    let value_end = value_start.checked_add(value_bytes).ok_or("版本值溢出")?;
    if value_end > block.end {
        return Err("版本资源值截断".into());
    }
    if path.is_empty() {
        if key != "VS_VERSION_INFO"
            || kind != 0
            || value_length != 52
            || dword(bytes, value_start)? != 0xfeef_04bd
            || dword(bytes, value_start + 4)? != 0x0001_0000
        {
            return Err("未知 VS_FIXEDFILEINFO 结构".into());
        }
        // Only file/product version numbers; flags, OS, file type stay hashed.
        ranges.push(value_start + 8..value_start + 24);
    } else if path.len() == 3
        && path[0] == "VS_VERSION_INFO"
        && path[1] == "StringFileInfo"
        && path[2].len() == 8
        && path[2].bytes().all(|v| v.is_ascii_hexdigit())
        && matches!(key.as_str(), "FileVersion" | "ProductVersion")
    {
        if kind != 1 || value_bytes < 2 || word(bytes, value_end - 2)? != 0 {
            return Err("版本号字符串无效".into());
        }
        let words = bytes[value_start..value_end]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|v| u16::from_le_bytes([v[0], v[1]]))
            .collect::<Vec<_>>();
        String::from_utf16(&words).map_err(|_| "版本号字符串不是 UTF16")?;
        if value_bytes > 2 {
            ranges.push(value_start..value_end - 2);
        }
    }
    path.push(key);
    let mut cursor = align4(value_end)?;
    while cursor < block.end {
        if block.end - cursor < 8 && bytes[cursor..block.end].iter().all(|v| *v == 0) {
            break;
        }
        cursor = align4(version_block(
            bytes, cursor, block.end, path, ranges, count,
        )?)?;
    }
    path.pop();
    Ok(block.end)
}

fn resource_ranges(
    image: &Image<'_>,
    ranges: &mut Vec<Range<usize>>,
) -> Result<Option<Range<usize>>, String> {
    let (rva, size) = image.directories[2];
    if rva == 0 && size == 0 {
        return Ok(None);
    }
    let base = image.map(rva, size as usize, true)?;
    let mut resources = Resources {
        image,
        base,
        visited: BTreeSet::new(),
        structures: vec![],
        leaves: vec![],
    };
    resources.walk(0, 0, false)?;
    for (index, leaf) in resources.leaves.iter().enumerate() {
        if resources.structures.iter().any(|r| overlap(r, &leaf.data))
            || resources.leaves[..index]
                .iter()
                .any(|other| overlap(&other.data, &leaf.data))
        {
            return Err("资源数据存在别名或覆盖目录结构".into());
        }
        if leaf.version {
            version_block(
                image.bytes,
                leaf.data.start,
                leaf.data.end,
                &mut vec![],
                ranges,
                &mut 0,
            )?;
        }
    }
    Ok(Some(resources.base))
}

fn normalized(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let image = Image::parse(bytes)?;
    let end = image.certificate_start()?;
    let mut ranges = vec![
        image.pe + 8..image.pe + 12,
        image.optional + 64..image.optional + 68,
        image.optional + 112 + 4 * 8..image.optional + 112 + 5 * 8,
    ];
    let debug_regions = image.debug_ranges(&mut ranges)?;
    if let Some(resource_region) = resource_ranges(&image, &mut ranges)?
        && debug_regions
            .iter()
            .any(|region| overlap(region, &resource_region))
    {
        // A byte cannot be both ignorable debug metadata and a resource (for
        // example an XAML/RCDATA payload). Ownership is checked on the complete
        // directory/payload ranges, including bytes we do not normalize.
        return Err("调试数据与资源目录或内容存在别名".into());
    }
    ranges.sort_by_key(|r| r.start);
    if ranges.iter().any(|r| r.is_empty() || r.end > end)
        || ranges.windows(2).any(|r| overlap(&r[0], &r[1]))
    {
        return Err("元数据规范化区间冲突".into());
    }
    // No ignored debug/version bytes may overlap another loader directory.
    // The resource and debug directories own their respective metadata.
    for (index, &(rva, size)) in image.directories.iter().enumerate() {
        if [2, 4, 6].contains(&index) || size == 0 {
            continue;
        }
        let directory = image.map(rva, size as usize, false)?;
        if ranges
            .iter()
            .any(|r| r.start >= image.headers && overlap(r, &directory))
        {
            return Err("元数据与加载器数据重叠".into());
        }
    }
    let mut data = bytes[..end].to_vec();
    for range in ranges {
        data[range].fill(0);
    }
    Ok(data)
}

/// Stable digest for an unchanged mapped implementation. An unsupported PE
/// shape returns an error; it never falls back to executable-section-only hash.
pub fn execution_fingerprint(bytes: &[u8]) -> Result<String, String> {
    let data = normalized(bytes)?;
    let mut digest = Sha256::new();
    digest.update(ALGORITHM.as_bytes());
    digest.update([0]);
    digest.update(data);
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut bytes = pe::fixture();
        // Make the existing PE identity fixture a fully bounded file image.
        bytes[0xd4..0xd8].copy_from_slice(&0x200u32.to_le_bytes());
        bytes.truncate(0xe00);
        bytes
    }
    #[test]
    fn metadata_only_changes_preserve_the_implementation() {
        let bytes = fixture();
        let original = execution_fingerprint(&bytes).unwrap();
        for (offset, change) in [(0x88, 7), (0xd8, 9), (0xa04, 17), (0xa44, 23), (0xa54, 2)] {
            let mut updated = bytes.clone();
            updated[offset] ^= change;
            assert_eq!(
                execution_fingerprint(&updated).unwrap(),
                original,
                "{offset:x}"
            );
        }
    }
    #[test]
    fn code_data_headers_and_pdb_path_stay_in_the_digest() {
        let bytes = fixture();
        let original = execution_fingerprint(&bytes).unwrap();
        for offset in [0x300, 0x800, 0x198, 0x1ac, 0xa58] {
            let mut updated = bytes.clone();
            updated[offset] ^= 1;
            assert!(
                execution_fingerprint(&updated).map_or(true, |digest| digest != original),
                "{offset:x}"
            );
        }
    }
    #[test]
    fn untrusted_metadata_ranges_cannot_erase_executable_bytes() {
        let mut bytes = fixture();
        // Move CodeView into the executable section. The identity reader accepts
        // the signature, but execution equivalence must reject its placement.
        let cv = bytes[0xa40..0xa61].to_vec();
        bytes[0x400..0x421].copy_from_slice(&cv);
        bytes[0xa14..0xa18].copy_from_slice(&0x1200u32.to_le_bytes());
        bytes[0xa18..0xa1c].copy_from_slice(&0x400u32.to_le_bytes());
        assert!(pe::identity(&bytes).is_ok());
        assert!(execution_fingerprint(&bytes).is_err());
    }
    #[test]
    fn debug_metadata_cannot_alias_a_resource_payload_or_directory() {
        for payload in [0x2200u32, 0x2240] {
            let mut bytes = fixture();
            // This otherwise valid RT_RCDATA tree aliases either the complete
            // debug directory or its RSDS payload. It would hide changing
            // resource bytes behind debug timestamp/GUID normalization.
            bytes[0x118..0x11c].copy_from_slice(&0x2100u32.to_le_bytes());
            bytes[0x11c..0x120].copy_from_slice(&0x400u32.to_le_bytes());
            for directory in [0x900, 0x918, 0x930] {
                bytes[directory + 14..directory + 16].copy_from_slice(&1u16.to_le_bytes());
            }
            for (offset, name, target) in [
                (0x910, 10u32, 0x8000_0018u32),
                (0x928, 1, 0x8000_0030),
                (0x940, 1033, 0x48),
            ] {
                bytes[offset..offset + 4].copy_from_slice(&name.to_le_bytes());
                bytes[offset + 4..offset + 8].copy_from_slice(&target.to_le_bytes());
            }
            bytes[0x948..0x94c].copy_from_slice(&payload.to_le_bytes());
            bytes[0x94c..0x950].copy_from_slice(&28u32.to_le_bytes());
            let image = Image::parse(&bytes).unwrap();
            assert!(image.debug_ranges(&mut vec![]).is_ok());
            assert!(resource_ranges(&image, &mut vec![]).is_ok());
            assert!(execution_fingerprint(&bytes).unwrap_err().contains("别名"));
        }
    }
    #[test]
    fn only_a_valid_nonmapped_certificate_tail_can_be_ignored() {
        let bytes = fixture();
        let original = execution_fingerprint(&bytes).unwrap();
        let mut signed = bytes.clone();
        signed[0x128..0x12c].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
        signed[0x12c..0x130].copy_from_slice(&16u32.to_le_bytes());
        signed.extend_from_slice(&[16, 0, 0, 0, 0, 2, 2, 0, 1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(execution_fingerprint(&signed).unwrap(), original);
        signed[0xe0f] ^= 1;
        assert_eq!(execution_fingerprint(&signed).unwrap(), original);
        signed[0x128..0x12c].copy_from_slice(&0x800u32.to_le_bytes());
        assert!(execution_fingerprint(&signed).is_err());
        let mut overlay = bytes;
        overlay.extend_from_slice(b"unexpected");
        assert!(execution_fingerprint(&overlay).is_err());
    }
    #[test]
    fn malformed_images_and_overlapping_sections_are_rejected() {
        for bytes in [vec![], vec![0; 100], {
            let mut b = fixture();
            b[0x1c4..0x1c8].copy_from_slice(&0x400u32.to_le_bytes());
            b
        }] {
            assert!(execution_fingerprint(&bytes).is_err());
        }
        let bytes = fixture();
        for length in [1, 63, 200, 511, 0xa50] {
            assert!(execution_fingerprint(&bytes[..length]).is_err());
        }
    }
    fn block(key: &str, kind: u16, value: &[u8], children: &[Vec<u8>]) -> Vec<u8> {
        let mut data = vec![0; 6];
        for value in key.encode_utf16().chain([0]) {
            data.extend_from_slice(&value.to_le_bytes());
        }
        while !data.len().is_multiple_of(4) {
            data.push(0);
        }
        data.extend_from_slice(value);
        for child in children {
            while !data.len().is_multiple_of(4) {
                data.push(0);
            }
            data.extend_from_slice(child);
        }
        let length = data.len() as u16;
        data[..2].copy_from_slice(&length.to_le_bytes());
        data[2..4]
            .copy_from_slice(&((value.len() / if kind == 1 { 2 } else { 1 }) as u16).to_le_bytes());
        data[4..6].copy_from_slice(&kind.to_le_bytes());
        data
    }
    fn version_fixture() -> (Vec<u8>, Vec<Range<usize>>) {
        let mut bytes = fixture();
        let value = "1.2.3.4\0"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let file = block("FileVersion", 1, &value, &[]);
        let product = block("ProductVersion", 1, &value, &[]);
        let text = block(
            "StringFileInfo",
            1,
            &[],
            &[block("040904b0", 1, &[], &[file, product])],
        );
        let mut fixed = vec![0; 52];
        fixed[..4].copy_from_slice(&0xfeef_04bdu32.to_le_bytes());
        fixed[4..8].copy_from_slice(&0x0001_0000u32.to_le_bytes());
        let version = block("VS_VERSION_INFO", 0, &fixed, &[text]);
        bytes[0x118..0x11c].copy_from_slice(&0x2300u32.to_le_bytes());
        bytes[0x11c..0x120].copy_from_slice(&(0x60u32 + version.len() as u32).to_le_bytes());
        for directory in [0xb00, 0xb18, 0xb30] {
            bytes[directory + 14..directory + 16].copy_from_slice(&1u16.to_le_bytes());
        }
        for (offset, name, target) in [
            (0xb10, 16u32, 0x8000_0018u32),
            (0xb28, 1, 0x8000_0030),
            (0xb40, 1033, 0x48),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&name.to_le_bytes());
            bytes[offset + 4..offset + 8].copy_from_slice(&target.to_le_bytes());
        }
        bytes[0xb48..0xb4c].copy_from_slice(&0x2360u32.to_le_bytes());
        bytes[0xb4c..0xb50].copy_from_slice(&(version.len() as u32).to_le_bytes());
        bytes[0xb60..0xb60 + version.len()].copy_from_slice(&version);
        let mut ranges = vec![];
        resource_ranges(&Image::parse(&bytes).unwrap(), &mut ranges).unwrap();
        (bytes, ranges)
    }
    #[test]
    fn only_version_numbers_are_normalized_resources_and_templates_stay_hashed() {
        let (bytes, ranges) = version_fixture();
        assert_eq!(ranges.len(), 3); // fixed file/product version + two strings.
        let original = execution_fingerprint(&bytes).unwrap();
        for range in &ranges {
            let mut changed = bytes.clone();
            changed[range.start] ^= 1;
            assert_eq!(execution_fingerprint(&changed).unwrap(), original);
        }
        let mut flags_changed = bytes.clone();
        flags_changed[ranges[0].end] ^= 1;
        assert_ne!(execution_fingerprint(&flags_changed).unwrap(), original);
        let mut template = bytes.clone();
        template[0xb10..0xb14].copy_from_slice(&10u32.to_le_bytes()); // RT_RCDATA, e.g. XAML.
        let template_original = execution_fingerprint(&template).unwrap();
        template[ranges[1].start] ^= 1;
        assert_ne!(execution_fingerprint(&template).unwrap(), template_original);
        let mut overlapping = bytes.clone();
        overlapping[0xb48..0xb4c].copy_from_slice(&0x2300u32.to_le_bytes());
        assert!(execution_fingerprint(&overlapping).is_err());
        let mut alias = bytes;
        alias[0xb18 + 14..0xb18 + 16].copy_from_slice(&2u16.to_le_bytes());
        assert!(execution_fingerprint(&alias).is_err());
    }
}
