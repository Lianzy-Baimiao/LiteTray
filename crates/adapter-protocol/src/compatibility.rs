//! Shared, built-in compatibility evidence. No Windows calls, downloads or
//! external profile overrides. Matching a disk image is not live activation.
use crate::{HostLayout, ModuleIdentity};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const PROFILE_BYTES: &[u8] = include_bytes!("../profiles/windows11-26300.9711-x64.json");
const MAX_PROFILE: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Module {
    pub name: String,
    pub relative_path: String,
    pub sha256: String,
    pub identity: ModuleIdentity,
}
impl Module {
    pub fn matches(&self, sha256: &str, identity: &ModuleIdentity) -> bool {
        self.sha256 == sha256 && self.identity == *identity
    }
    /// Loaded-image reports contain additional fields. Compare every identity
    /// field explicitly; a correct disk hash cannot prove loaded-image identity.
    pub fn matches_loaded(&self, value: &serde_json::Value) -> bool {
        value["machine"] == self.identity.machine
            && value["timestamp"] == self.identity.timestamp
            && value["image_size"] == self.identity.image_size
            && value["pdb_age"] == self.identity.pdb_age
            && value["pdb_guid"] == serde_json::json!(self.identity.pdb_guid)
    }
    fn validate(&self) -> Result<(), String> {
        self.identity.validate()?;
        let safe = |part: &str| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        };
        if self.relative_path.len() > 256
            || !self.relative_path.split('/').all(safe)
            || self.relative_path.rsplit('/').next() != Some(self.name.as_str())
            || !safe(&self.name)
            || !self.name.ends_with(".dll")
            || self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err("兼容资料的模块路径或哈希无效".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceRecipe {
    XamlExplorerHostChildV1,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Topology {
    SystemTrayFrameV1,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayAbi {
    SystemTrayCoordinatesX64V1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLayout {
    pub recipe: SourceRecipe,
    pub host_child_offset: u32,
    pub child_source_offset: u32,
    pub child_vtable_rva: u32,
    pub factory_vtable_rva: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayRole {
    InvocationPoint,
    IconBounds,
    ScreenRect,
}
impl TrayRole {
    pub const ALL: [Self; 3] = [Self::InvocationPoint, Self::IconBounds, Self::ScreenRect];
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrayEntry {
    pub role: TrayRole,
    pub rva: u32,
    pub section_start: u32,
    pub section_size: u32,
    pub executable: bool,
    pub prologue: [u8; 16],
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub schema: u32,
    pub id: String,
    pub system_label: String,
    pub taskbar: Module,
    pub shell: Module,
    pub tray: Module,
    pub host: HostLayout,
    pub source: SourceLayout,
    pub topology: Topology,
    pub tray_abi: TrayAbi,
    pub tray_entries: [TrayEntry; 3],
}
impl Profile {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.is_empty() || bytes.len() > MAX_PROFILE {
            return Err("兼容资料尺寸无效".into());
        }
        let profile: Self =
            serde_json::from_slice(bytes).map_err(|e| format!("兼容资料格式无效：{e}"))?;
        profile.validate()?;
        Ok(profile)
    }
    pub fn modules(&self) -> [&Module; 3] {
        [&self.taskbar, &self.shell, &self.tray]
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1
            || self.id.is_empty()
            || self.id.len() > 80
            || !self
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b".-_".contains(&c))
            || self.system_label.is_empty()
            || self.system_label.len() > 160
        {
            return Err("兼容资料版本或标识无效".into());
        }
        for module in self.modules() {
            module.validate()?;
        }
        if self.taskbar.name != "taskbar.dll"
            || self.shell.name != "twinui.pcshell.dll"
            || self.tray.name != "SystemTray.dll"
        {
            return Err("当前解析方案不支持该模块角色".into());
        }
        self.host.validate()?;
        if self.host.module != self.taskbar.identity {
            return Err("宿主符号与兼容模块身份不一致".into());
        }
        for offset in [
            self.source.host_child_offset,
            self.source.child_source_offset,
        ] {
            if !(8..=256).contains(&offset) || !offset.is_multiple_of(8) {
                return Err("宿主字段超出已实现解析范围".into());
            }
        }
        for rva in [self.source.child_vtable_rva, self.source.factory_vtable_rva] {
            if rva < 4096
                || !rva.is_multiple_of(8)
                || rva
                    .checked_add(6 * 8)
                    .is_none_or(|end| end > self.shell.identity.image_size)
            {
                return Err("宿主虚表超出模块范围".into());
            }
        }
        for (index, entry) in self.tray_entries.iter().enumerate() {
            let end = entry
                .section_start
                .checked_add(entry.section_size)
                .ok_or("托盘节区溢出")?;
            if entry.role != TrayRole::ALL[index]
                || !entry.executable
                || entry.section_size == 0
                || end > self.tray.identity.image_size
                || entry.rva < entry.section_start
                || entry.rva.checked_add(16).is_none_or(|v| v > end)
            {
                return Err("托盘入口角色、节区或 ABI 无效".into());
            }
        }
        Ok(())
    }
}

pub fn builtin() -> Result<&'static Profile, String> {
    static PROFILE: OnceLock<Result<Profile, String>> = OnceLock::new();
    PROFILE
        .get_or_init(|| Profile::decode(PROFILE_BYTES))
        .as_ref()
        .map_err(Clone::clone)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_profile_is_complete_and_round_trips() {
        let profile = builtin().unwrap();
        let encoded = serde_json::to_vec(profile).unwrap();
        assert_eq!(Profile::decode(&encoded).unwrap().host, profile.host);
        assert_eq!(profile.tray_entries.len(), 3);
    }
    #[test]
    fn malformed_profiles_cannot_supply_paths_abis_or_out_of_bounds_addresses() {
        let original: serde_json::Value = serde_json::from_slice(PROFILE_BYTES).unwrap();
        for (path, value) in [
            ("/schema", serde_json::json!(2)),
            ("/source/recipe", serde_json::json!("scan_any_pointer")),
            ("/tray_abi", serde_json::json!("arm64")),
            ("/shell/relative_path", serde_json::json!("../evil.dll")),
            (
                "/shell/relative_path",
                serde_json::json!("C:/Windows/twinui.pcshell.dll"),
            ),
            ("/shell/sha256", serde_json::json!("bad")),
            ("/taskbar/identity/pdb_age", serde_json::json!(2)),
            ("/source/host_child_offset", serde_json::json!(3)),
            ("/source/child_vtable_rva", serde_json::json!(u32::MAX)),
            ("/tray_entries/0/rva", serde_json::json!(u32::MAX)),
            ("/tray_entries/0/role", serde_json::json!("screen_rect")),
            ("/tray_entries/0/executable", serde_json::json!(false)),
        ] {
            let mut wrong = original.clone();
            *wrong.pointer_mut(path).unwrap() = value;
            assert!(
                Profile::decode(&serde_json::to_vec(&wrong).unwrap()).is_err(),
                "{path}"
            );
        }
        assert!(Profile::decode(b"{}").is_err());
        assert!(Profile::decode(&vec![b' '; MAX_PROFILE + 1]).is_err());
    }
    #[test]
    fn disk_and_loaded_identities_are_independent_requirements() {
        let module = &builtin().unwrap().taskbar;
        assert!(module.matches(&module.sha256, &module.identity));
        assert!(!module.matches(&"0".repeat(64), &module.identity));
        let mut loaded = serde_json::to_value(&module.identity).unwrap();
        assert!(module.matches_loaded(&loaded));
        loaded["pdb_age"] = serde_json::json!(module.identity.pdb_age + 1);
        assert!(!module.matches_loaded(&loaded));
        assert!(!module.matches_loaded(&serde_json::json!({})));
    }
}
