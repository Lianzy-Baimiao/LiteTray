//! Shared, built-in compatibility evidence. No Windows calls, downloads or
//! external profile overrides. Matching a disk image is not live activation.
use crate::{HostLayout, ModuleIdentity};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const PROFILE_BYTES: &[&[u8]] = &[
    include_bytes!("../profiles/windows11-26300.9711-x64.json"),
    include_bytes!("../profiles/windows11-26300.9818-x64.json"),
    #[cfg(feature = "compatibility-lab")]
    include_bytes!("../profiles/lab/windows11-26200.8457-x64.json"),
];
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
            || self.taskbar.relative_path != "System32/taskbar.dll"
            || self.shell.name != "twinui.pcshell.dll"
            || self.shell.relative_path != "System32/twinui.pcshell.dll"
            || !matches!(
                (self.tray.name.as_str(), self.tray.relative_path.as_str()),
                (
                    "SystemTray.dll",
                    "SystemApps/MicrosoftWindows.Client.Core_cw5n1h2txyewy/SystemTray.dll"
                ) | (
                    "Taskbar.View.dll",
                    "SystemApps/MicrosoftWindows.Client.Core_cw5n1h2txyewy/Taskbar.View.dll"
                )
            )
        {
            return Err("当前解析方案不支持该模块角色或系统路径".into());
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

fn decode_profiles(sources: &[&[u8]]) -> Result<Vec<Profile>, String> {
    if sources.is_empty() {
        return Err("没有内置兼容资料".into());
    }
    let profiles = sources
        .iter()
        .map(|bytes| Profile::decode(bytes))
        .collect::<Result<Vec<_>, _>>()?;
    let mut ids = std::collections::BTreeSet::new();
    if profiles.iter().any(|profile| !ids.insert(&profile.id)) {
        return Err("内置兼容资料标识重复".into());
    }
    Ok(profiles)
}

/// Validated, embedded evidence only. Registry order is not a compatibility
/// preference: callers must select an exact module identity, never a default.
/// Lab candidates enter only through the explicit `compatibility-lab` feature.
pub fn profiles() -> Result<&'static [Profile], String> {
    static PROFILES: OnceLock<Result<Vec<Profile>, String>> = OnceLock::new();
    PROFILES
        .get_or_init(|| decode_profiles(PROFILE_BYTES))
        .as_ref()
        .map(Vec::as_slice)
        .map_err(Clone::clone)
}

fn select_taskbar<'a>(
    profiles: &'a [Profile],
    identity: &ModuleIdentity,
) -> Result<&'a Profile, String> {
    identity.validate()?;
    let mut matching = profiles
        .iter()
        .filter(|profile| profile.taskbar.identity == *identity);
    let profile = matching
        .next()
        .ok_or("尚无与 taskbar.dll 完整身份匹配的内置兼容资料")?;
    if matching.next().is_some() {
        return Err("多个内置兼容资料匹配同一 taskbar.dll 身份，拒绝选择".into());
    }
    Ok(profile)
}

/// Selects a single coherent profile from the complete taskbar identity.
/// Disk callers must additionally check its SHA-256; loaded callers must check
/// every required loaded module against this same profile before activation.
pub fn profile_for_taskbar(identity: &ModuleIdentity) -> Result<&'static Profile, String> {
    select_taskbar(profiles()?, identity)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn changed_identities(original: &ModuleIdentity) -> Vec<ModuleIdentity> {
        let mut changed = vec![original.clone(); 5];
        changed[0].machine = 0xaa64;
        changed[1].timestamp ^= 1;
        changed[2].image_size += 4096;
        changed[3].pdb_age += 1;
        changed[4].pdb_guid[0] ^= 0xff;
        changed
    }

    #[test]
    fn embedded_profiles_are_complete_and_selected_by_exact_taskbar_identity() {
        let profiles = profiles().unwrap();
        for id in ["windows11-26300.9711-x64", "windows11-26300.9818-x64"] {
            let profile = profiles.iter().find(|profile| profile.id == id).unwrap();
            let encoded = serde_json::to_vec(profile).unwrap();
            assert_eq!(Profile::decode(&encoded).unwrap().host, profile.host);
            assert_eq!(profile.tray_entries.len(), 3);
            assert_eq!(
                profile_for_taskbar(&profile.taskbar.identity).unwrap().id,
                id
            );
        }
    }
    #[test]
    fn lab_registry_requires_opt_in_and_keeps_all_module_identities_separate() {
        // This fixture is compiled into unit tests, never the default registry.
        let lab = Profile::decode(include_bytes!(
            "../profiles/lab/windows11-26200.8457-x64.json"
        ))
        .unwrap();
        assert_eq!(lab.id, "windows11-26200.8457-x64");
        assert_eq!(lab.tray.name, "Taskbar.View.dll");
        let registry = profiles().unwrap();
        let ids = registry
            .iter()
            .map(|profile| profile.id.as_str())
            .collect::<Vec<_>>();
        if cfg!(feature = "compatibility-lab") {
            assert_eq!(
                ids,
                [
                    "windows11-26300.9711-x64",
                    "windows11-26300.9818-x64",
                    "windows11-26200.8457-x64"
                ]
            );
            assert_eq!(
                profile_for_taskbar(&lab.taskbar.identity).unwrap().id,
                lab.id
            );
            for changed in changed_identities(&lab.taskbar.identity) {
                assert!(profile_for_taskbar(&changed).is_err());
            }
        } else {
            assert_eq!(
                ids,
                ["windows11-26300.9711-x64", "windows11-26300.9818-x64"]
            );
            assert!(profile_for_taskbar(&lab.taskbar.identity).is_err());
        }
        for stable in registry.iter().filter(|profile| profile.id != lab.id) {
            for (stable_module, lab_module) in stable.modules().into_iter().zip(lab.modules()) {
                assert!(!stable_module.matches(&lab_module.sha256, &lab_module.identity));
                assert!(!lab_module.matches(&stable_module.sha256, &stable_module.identity));
                assert!(
                    !stable_module
                        .matches_loaded(&serde_json::to_value(&lab_module.identity).unwrap())
                );
                assert!(
                    !lab_module
                        .matches_loaded(&serde_json::to_value(&stable_module.identity).unwrap())
                );
            }
        }
    }
    #[test]
    fn tray_roles_accept_only_explicit_core_name_path_pairs() {
        let source = profiles().unwrap()[0].clone();
        for (name, path, accepted) in [
            (
                "SystemTray.dll",
                "SystemApps/MicrosoftWindows.Client.Core_cw5n1h2txyewy/SystemTray.dll",
                true,
            ),
            (
                "Taskbar.View.dll",
                "SystemApps/MicrosoftWindows.Client.Core_cw5n1h2txyewy/Taskbar.View.dll",
                true,
            ),
            (
                "SystemTray.dll",
                "SystemApps/MicrosoftWindows.Client.CBS_cw5n1h2txyewy/SystemTray.dll",
                false,
            ),
            (
                "Taskbar.View.dll",
                "SystemApps/MicrosoftWindows.Client.CBS_cw5n1h2txyewy/Taskbar.View.dll",
                false,
            ),
            (
                "ExplorerExtensions.dll",
                "SystemApps/MicrosoftWindows.Client.Core_cw5n1h2txyewy/ExplorerExtensions.dll",
                false,
            ),
            ("Taskbar.View.dll", "System32/Taskbar.View.dll", false),
            (
                "SystemTray.dll",
                "SystemApps/MicrosoftWindows.Client.Core_cw5n1h2txyewy/Taskbar.View.dll",
                false,
            ),
            (
                "Taskbar.View.dll",
                "SystemApps/MicrosoftWindows.Client.Core_cw5n1h2txyewy/SystemTray.dll",
                false,
            ),
        ] {
            let mut profile = source.clone();
            profile.tray.name = name.into();
            profile.tray.relative_path = path.into();
            assert_eq!(profile.validate().is_ok(), accepted, "{name}: {path}");
        }
        for taskbar in [true, false] {
            let mut wrong = source.clone();
            let module = if taskbar {
                &mut wrong.taskbar
            } else {
                &mut wrong.shell
            };
            module.relative_path = format!("SystemApps/{}", module.name);
            assert!(wrong.validate().is_err());
        }
    }
    #[test]
    fn unknown_or_ambiguous_taskbar_identity_never_selects_a_default_profile() {
        let profile = profiles().unwrap()[0].clone();
        for unknown in changed_identities(&profile.taskbar.identity) {
            assert!(profile_for_taskbar(&unknown).is_err());
        }
        let mut duplicate = profile.clone();
        duplicate.id = "ambiguous-test-profile".into();
        assert!(select_taskbar(&[profile.clone(), duplicate], &profile.taskbar.identity).is_err());
        assert!(select_taskbar(&[], &profile.taskbar.identity).is_err());
    }
    #[test]
    fn invalid_registry_entries_or_duplicate_ids_reject_the_registry() {
        assert!(decode_profiles(&[]).is_err());
        assert!(decode_profiles(&[PROFILE_BYTES[0], b"{}"]).is_err());
        assert!(decode_profiles(&[PROFILE_BYTES[0], PROFILE_BYTES[0]]).is_err());
    }
    #[test]
    fn malformed_profiles_cannot_supply_paths_abis_or_out_of_bounds_addresses() {
        let original: serde_json::Value = serde_json::from_slice(PROFILE_BYTES[0]).unwrap();
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
        for profile in profiles().unwrap() {
            for module in profile.modules() {
                assert!(module.matches(&module.sha256, &module.identity));
                assert!(!module.matches(&"0".repeat(64), &module.identity));
                let loaded = serde_json::to_value(&module.identity).unwrap();
                assert!(module.matches_loaded(&loaded));
                for changed in changed_identities(&module.identity) {
                    assert!(!module.matches(&module.sha256, &changed));
                    assert!(!module.matches_loaded(&serde_json::to_value(changed).unwrap()));
                }
                assert!(!module.matches_loaded(&serde_json::json!({})));
            }
        }
        let profiles = profiles().unwrap();
        for (old, new) in profiles[0].modules().into_iter().zip(profiles[1].modules()) {
            assert!(!old.matches(&new.sha256, &new.identity));
            assert!(!old.matches_loaded(&serde_json::to_value(&new.identity).unwrap()));
        }
    }
}
