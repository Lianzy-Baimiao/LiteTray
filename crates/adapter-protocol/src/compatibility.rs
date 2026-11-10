//! Shared, built-in compatibility evidence. No Windows calls, downloads or
//! external profile overrides. Matching a disk image is not live activation.
use crate::{HostLayout, ModuleIdentity};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
mod recognition;
pub use recognition::{
    MatchBasis, Recognized, Resolved, ResolvedTray, Role, assemble, execution_fingerprint,
    recognize,
};

const PROFILE_BYTES: &[&[u8]] = &[
    include_bytes!("../profiles/windows11-26300.9711-x64.json"),
    include_bytes!("../profiles/windows11-26300.9818-x64.json"),
    include_bytes!("../profiles/windows11-26200.8457-x64.json"),
    include_bytes!("../profiles/windows11-26200.8037-x64.json"),
    include_bytes!("../profiles/windows11-26100.1742-x64.json"),
    include_bytes!("../profiles/windows11-22631.2861-x64.json"),
    include_bytes!("../profiles/windows11-22621.1702-x64.json"),
    include_bytes!("../profiles/windows11-22000.318-x64-clock.json"),
    #[cfg(feature = "compatibility-lab")]
    include_bytes!("../profiles/lab/windows11-28000.3365-x64-notification.json"),
    include_bytes!("../profiles/windows11-28000.1-x64-notification.json"),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceRecipe {
    XamlExplorerHostChildV1,
    /// Earlier child implementation with an XFG getter and an image-backed
    /// writable vtable. Every callable slot is checked before querying it.
    XamlExplorerHostChildXfgV1,
}
/// Private interface identity is independent of the child field layout.
/// Both identities have appeared on the reviewed 13-slot XFG layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildInterface {
    XamlExplorerHostChildEef8V1,
    XamlExplorerHostChildEc97V1,
}
impl ChildInterface {
    pub const fn iid(self) -> u128 {
        match self {
            Self::XamlExplorerHostChildEef8V1 => 0xeef8f632_6c0d_5fcb_b6a1_354819e51f3b,
            Self::XamlExplorerHostChildEc97V1 => 0xec97afd0_5c0d_581b_9508_074773c6f1e0,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Topology {
    SystemTrayFrameV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayAbi {
    /// Free function consuming a by-value FrameworkElement wrapper.
    SystemTrayCoordinatesX64V1,
    /// DragDropManager member retaining the caller's borrowed wrapper.
    /// Its opaque first register precedes the hidden RECT result pointer.
    SystemTrayDragDropBorrowedCoordinatesX64V1,
}

/// Reviewed consumers of the coordinate entry. Calling convention alone does
/// not establish which interactions that entry participates in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayGeometry {
    NotificationAreaBounds,
    DragDropHitTesting,
    LegacyUnclassified,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLayout {
    pub recipe: SourceRecipe,
    /// Omission preserves the original built-in profile and its serialized hash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_interface: Option<ChildInterface>,
    pub host_child_offset: u32,
    pub child_source_offset: u32,
    pub child_vtable_rva: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factory_vtable_rva: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_vtable_entries: Option<[SourceVtableEntry; 13]>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceVtableEntry {
    pub rva: u32,
    pub prologue: [u8; 16],
}
/// The early primary notification island owns a separate shared host and HWND.
/// All addresses belong to Taskbar.dll, independently of the shell child ABI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotificationAreaLayout {
    pub component_offset: u32,
    pub component_vtable_rva: u32,
    pub shared_host_offset: u32,
    pub child_offset: u32,
    pub parent_offset: u32,
    pub copy_host: SourceVtableEntry,
    pub control_vtable_rva: u32,
    pub control_entries: [SourceVtableEntry; 3],
}
impl NotificationAreaLayout {
    pub fn validate(&self, image_size: u32) -> Result<(), String> {
        for (offset, limit) in [
            (self.component_offset, 0x400),
            (self.shared_host_offset, 0x400),
            (self.child_offset, 0x100),
            (self.parent_offset, 0x100),
        ] {
            if !(8..=limit).contains(&offset) || !offset.is_multiple_of(8) {
                return Err("通知区宿主字段超出已实现解析范围".into());
            }
        }
        if self.child_offset == self.parent_offset {
            return Err("通知区子接口与父窗口字段重叠".into());
        }
        for (rva, bytes) in [
            (self.component_vtable_rva, 3 * 8),
            (self.control_vtable_rva, 3 * 8),
        ] {
            if rva < 4096
                || !rva.is_multiple_of(8)
                || rva.checked_add(bytes).is_none_or(|end| end > image_size)
            {
                return Err("通知区宿主虚表超出 Taskbar 模块范围".into());
            }
        }
        for entry in std::iter::once(&self.copy_host).chain(&self.control_entries) {
            if entry.rva < 4096 || entry.rva.checked_add(16).is_none_or(|end| end > image_size) {
                return Err("通知区宿主函数超出 Taskbar 模块范围".into());
            }
        }
        Ok(())
    }

    pub fn verify_control_vtable(&self, image_base: u64, table: &[u8]) -> Result<(), String> {
        if table.len() != self.control_entries.len() * 8 {
            return Err("通知区引用计数虚表长度不一致".into());
        }
        for (slot, expected) in table.as_chunks::<8>().0.iter().zip(&self.control_entries) {
            if image_base.checked_add(u64::from(expected.rva)) != Some(u64::from_le_bytes(*slot)) {
                return Err("通知区引用计数虚表入口已变化".into());
            }
        }
        Ok(())
    }
}
impl SourceLayout {
    pub fn interface_contract(&self) -> ChildInterface {
        self.child_interface.unwrap_or(match self.recipe {
            SourceRecipe::XamlExplorerHostChildV1 => ChildInterface::XamlExplorerHostChildEc97V1,
            SourceRecipe::XamlExplorerHostChildXfgV1 => ChildInterface::XamlExplorerHostChildEef8V1,
        })
    }
    /// The same bounded equality is used for preferred-base disk pointers and
    /// ASLR-relocated live pointers. No pointer supplied by the table is called.
    pub fn verify_child_vtable(&self, image_base: u64, table: &[u8]) -> Result<(), String> {
        let entries = self
            .child_vtable_entries
            .as_ref()
            .ok_or("该 source 方案没有逐项虚表证据")?;
        if self.recipe != SourceRecipe::XamlExplorerHostChildXfgV1
            || table.len() != entries.len() * 8
        {
            return Err("子宿主虚表方案或长度不一致".into());
        }
        for (slot, expected) in table.as_chunks::<8>().0.iter().zip(entries) {
            let actual = u64::from_le_bytes(*slot);
            if image_base.checked_add(u64::from(expected.rva)) != Some(actual) {
                return Err("子宿主虚表入口已变化或不属于已核对的模块".into());
            }
        }
        Ok(())
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tray: Option<Module>,
    pub host: HostLayout,
    pub source: SourceLayout,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notification_area: Option<NotificationAreaLayout>,
    pub topology: Topology,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tray_abi: Option<TrayAbi>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tray_geometry: Option<TrayGeometry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tray_entries: Option<[TrayEntry; 3]>,
}
/// A callable tray recipe exists only when all four evidence fields exist.
/// Clock evidence cannot supply a placeholder ABI or function address.
pub struct TrayEvidence<'a> {
    pub module: &'a Module,
    pub abi: TrayAbi,
    pub geometry: TrayGeometry,
    pub entries: &'a [TrayEntry; 3],
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
    pub fn modules(&self) -> Vec<&Module> {
        [&self.taskbar, &self.shell]
            .into_iter()
            .chain(self.tray.as_ref())
            .collect()
    }
    pub fn tray_evidence(&self) -> Result<Option<TrayEvidence<'_>>, String> {
        match (
            &self.tray,
            self.tray_abi,
            self.tray_geometry,
            &self.tray_entries,
        ) {
            (None, None, None, None) => Ok(None),
            (Some(module), Some(abi), Some(geometry), Some(entries)) => Ok(Some(TrayEvidence {
                module,
                abi,
                geometry,
                entries,
            })),
            _ => Err("托盘资料必须同时包含模块、ABI、用途和全部入口".into()),
        }
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
        let tray = self.tray_evidence()?;
        if tray.as_ref().is_some_and(|tray| {
            !matches!(
                (tray.abi, tray.geometry),
                (
                    TrayAbi::SystemTrayDragDropBorrowedCoordinatesX64V1,
                    TrayGeometry::DragDropHitTesting
                ) | (
                    TrayAbi::SystemTrayCoordinatesX64V1,
                    TrayGeometry::NotificationAreaBounds | TrayGeometry::LegacyUnclassified
                )
            )
        }) {
            return Err("托盘坐标入口的 ABI 与已验证用途不一致".into());
        }
        if self.taskbar.name != "taskbar.dll"
            || self.taskbar.relative_path != "System32/taskbar.dll"
            || self.shell.name != "twinui.pcshell.dll"
            || self.shell.relative_path != "System32/twinui.pcshell.dll"
            || tray.as_ref().is_some_and(|tray| {
                !matches!(
                    (
                        tray.module.name.as_str(),
                        tray.module.relative_path.as_str()
                    ),
                    (
                        "SystemTray.dll",
                        "SystemApps/MicrosoftWindows.Client.Core_cw5n1h2txyewy/SystemTray.dll"
                    ) | (
                        "Taskbar.View.dll",
                        "SystemApps/MicrosoftWindows.Client.Core_cw5n1h2txyewy/Taskbar.View.dll"
                    )
                )
            })
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
        match self.source.recipe {
            SourceRecipe::XamlExplorerHostChildV1 => {
                if self.host.element_offset.is_none()
                    || self.source.factory_vtable_rva.is_none()
                    || self.source.child_vtable_entries.is_some()
                    || self.source.interface_contract()
                        != ChildInterface::XamlExplorerHostChildEc97V1
                {
                    return Err("现代子宿主必须包含已验证的工厂虚表".into());
                }
            }
            SourceRecipe::XamlExplorerHostChildXfgV1 => {
                if self.host.element_offset.is_some()
                    || self.source.factory_vtable_rva.is_some()
                    || self.source.child_vtable_entries.is_none()
                {
                    return Err("早期子宿主必须包含完整虚表入口且不能猜测工厂".into());
                }
            }
        }
        if let Some(notification) = &self.notification_area {
            if self.source.recipe != SourceRecipe::XamlExplorerHostChildXfgV1 || tray.is_some() {
                return Err("通知区独立宿主仅属于已实现的早期无托盘方案".into());
            }
            notification.validate(self.taskbar.identity.image_size)?;
        }
        let child_slots = if self.source.child_vtable_entries.is_some() {
            13
        } else {
            6
        };
        for (rva, slots) in std::iter::once((self.source.child_vtable_rva, child_slots))
            .chain(self.source.factory_vtable_rva.map(|rva| (rva, 6)))
        {
            if rva < 4096
                || !rva.is_multiple_of(8)
                || rva
                    .checked_add(slots * 8)
                    .is_none_or(|end| end > self.shell.identity.image_size)
            {
                return Err("宿主虚表超出模块范围".into());
            }
        }
        for entry in self.source.child_vtable_entries.iter().flatten() {
            if entry.rva < 4096
                || entry
                    .rva
                    .checked_add(16)
                    .is_none_or(|end| end > self.shell.identity.image_size)
            {
                return Err("子宿主虚表函数超出模块范围".into());
            }
        }
        for (module, index, entry) in tray.iter().flat_map(|tray| {
            tray.entries
                .iter()
                .enumerate()
                .map(|(index, entry)| (tray.module, index, entry))
        }) {
            let end = entry
                .section_start
                .checked_add(entry.section_size)
                .ok_or("托盘节区溢出")?;
            if entry.role != TrayRole::ALL[index]
                || !entry.executable
                || entry.section_size == 0
                || end > module.identity.image_size
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
/// Default builds include nine reviewed profiles, including primary-clock-only
/// support. `compatibility-lab` adds the later 26H1 candidate pending acceptance.
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

    fn notification_profile() -> Profile {
        Profile::decode(include_bytes!(
            "../profiles/windows11-22000.318-x64-clock.json"
        ))
        .unwrap()
    }

    #[test]
    fn child_interface_is_independent_of_xfg_layout_and_keeps_old_profile_bytes() {
        for bytes in PROFILE_BYTES {
            let profile = Profile::decode(bytes).unwrap();
            if profile.id == "windows11-28000.1-x64-notification" {
                assert_eq!(
                    profile.source.child_interface,
                    Some(ChildInterface::XamlExplorerHostChildEc97V1)
                );
            } else {
                assert!(profile.source.child_interface.is_none());
            }
            assert_eq!(
                serde_json::to_value(&profile).unwrap(),
                serde_json::from_slice::<serde_json::Value>(bytes).unwrap()
            );
        }
        let early = notification_profile();
        assert_eq!(
            early.source.interface_contract(),
            ChildInterface::XamlExplorerHostChildEef8V1
        );
        assert_eq!(
            early.source.interface_contract().iid(),
            0xeef8f632_6c0d_5fcb_b6a1_354819e51f3b
        );
        // A synthetic protocol fixture, not a new supported module profile.
        let mut later = serde_json::to_value(&early).unwrap();
        later["source"]["child_interface"] = "xaml_explorer_host_child_ec97_v1".into();
        let decoded = Profile::decode(&serde_json::to_vec(&later).unwrap()).unwrap();
        assert_eq!(decoded.source.recipe, early.source.recipe);
        assert_eq!(
            decoded.source.interface_contract(),
            ChildInterface::XamlExplorerHostChildEc97V1
        );
        assert_eq!(
            decoded.source.interface_contract().iid(),
            0xec97afd0_5c0d_581b_9508_074773c6f1e0
        );
        later["source"]["child_interface"] = "unknown_interface".into();
        assert!(Profile::decode(&serde_json::to_vec(&later).unwrap()).is_err());
        let mut modern = Profile::decode(PROFILE_BYTES[0]).unwrap();
        modern.source.child_interface = Some(ChildInterface::XamlExplorerHostChildEef8V1);
        assert!(modern.validate().is_err());
    }

    #[test]
    fn next_notification_candidate_is_lab_only_and_keeps_reviewed_consumer() {
        let candidate = Profile::decode(include_bytes!(
            "../profiles/lab/windows11-28000.3365-x64-notification.json"
        ))
        .unwrap();
        assert_eq!(
            profiles().unwrap().iter().any(|p| p.id == candidate.id),
            cfg!(feature = "compatibility-lab")
        );
        assert_eq!(
            profile_for_taskbar(&candidate.taskbar.identity).is_ok(),
            cfg!(feature = "compatibility-lab")
        );
        assert_eq!(
            candidate.tray_abi,
            Some(TrayAbi::SystemTrayCoordinatesX64V1)
        );
        assert_eq!(
            candidate.tray_geometry,
            Some(TrayGeometry::NotificationAreaBounds)
        );
        let entries = candidate.tray_entries.as_ref().unwrap();
        assert_eq!(entries[2].rva, 0xb86ec);
        assert!(entries.iter().all(|entry| entry.rva != 0xa8320));
        assert!(candidate.notification_area.is_none());
        assert_eq!(
            candidate.source.recipe,
            SourceRecipe::XamlExplorerHostChildV1
        );
        let mut wrong = candidate.clone();
        wrong.tray_geometry = Some(TrayGeometry::DragDropHitTesting);
        assert!(wrong.validate().is_err());
    }

    #[test]
    fn base_notification_profile_keeps_its_own_module_and_interface_contract() {
        let candidate = Profile::decode(include_bytes!(
            "../profiles/windows11-28000.1-x64-notification.json"
        ))
        .unwrap();
        assert!(profile_for_taskbar(&candidate.taskbar.identity).is_ok());
        assert_eq!(candidate.tray.as_ref().unwrap().name, "Taskbar.View.dll");
        assert_eq!(
            candidate.source.interface_contract(),
            ChildInterface::XamlExplorerHostChildEc97V1
        );
        assert_eq!(
            candidate.tray_abi,
            Some(TrayAbi::SystemTrayCoordinatesX64V1)
        );
        assert_eq!(
            candidate.tray_geometry,
            Some(TrayGeometry::NotificationAreaBounds)
        );
        assert_eq!(candidate.tray_entries.as_ref().unwrap()[2].rva, 0x5006c);
        for module in candidate.modules() {
            assert_module_identity_requirements(module);
        }
        let updated = Profile::decode(include_bytes!(
            "../profiles/lab/windows11-28000.3365-x64-notification.json"
        ))
        .unwrap();
        for (base, later) in candidate.modules().into_iter().zip(updated.modules()) {
            assert!(!base.matches(&later.sha256, &later.identity));
        }
        let mut mixed = candidate.clone();
        mixed.host = updated.host;
        assert!(mixed.validate().is_err());
    }

    #[test]
    fn notification_evidence_is_optional_and_cannot_enter_a_modern_or_tray_recipe() {
        let early = notification_profile();
        assert_eq!(early.source.host_child_offset, 0x20);
        assert_eq!(early.notification_area.as_ref().unwrap().child_offset, 0x28);
        let mut previous = early.clone();
        previous.notification_area = None;
        previous.validate().unwrap();
        assert!(
            serde_json::to_value(&previous)
                .unwrap()
                .get("notification_area")
                .is_none()
        );
        for bytes in &PROFILE_BYTES[..7] {
            let mut modern = Profile::decode(bytes).unwrap();
            let encoded = serde_json::to_value(&modern).unwrap();
            assert_eq!(
                encoded,
                serde_json::from_slice::<serde_json::Value>(bytes).unwrap()
            );
            modern.tray = None;
            modern.tray_abi = None;
            modern.tray_geometry = None;
            modern.tray_entries = None;
            modern.notification_area = early.notification_area.clone();
            assert!(modern.validate().is_err());
        }
        let modern = &profiles().unwrap()[0];
        let mut mixed = early;
        mixed.tray = modern.tray.clone();
        mixed.tray_abi = modern.tray_abi;
        mixed.tray_geometry = modern.tray_geometry;
        mixed.tray_entries = modern.tray_entries.clone();
        assert!(mixed.validate().is_err());
    }

    #[test]
    fn notification_fields_functions_and_tables_must_stay_bounded() {
        let original = serde_json::to_value(notification_profile()).unwrap();
        for field in [
            "component_offset",
            "shared_host_offset",
            "child_offset",
            "parent_offset",
        ] {
            for invalid in [0, 7, 9, 0x1008, u32::MAX] {
                let mut value = original.clone();
                value["notification_area"][field] = invalid.into();
                assert!(
                    Profile::decode(&serde_json::to_vec(&value).unwrap()).is_err(),
                    "{field}: {invalid}"
                );
            }
        }
        for field in ["component_vtable_rva", "control_vtable_rva"] {
            for invalid in [0, 0x1001, u32::MAX - 7] {
                let mut value = original.clone();
                value["notification_area"][field] = invalid.into();
                assert!(Profile::decode(&serde_json::to_vec(&value).unwrap()).is_err());
            }
        }
        let size = original["taskbar"]["identity"]["image_size"]
            .as_u64()
            .unwrap() as u32;
        for entry in 0..4 {
            for invalid in [0xfff, size - 8, u32::MAX - 7] {
                let mut value = original.clone();
                let layout = &mut value["notification_area"];
                let target = if entry == 0 {
                    &mut layout["copy_host"]
                } else {
                    &mut layout["control_entries"][entry - 1]
                };
                target["rva"] = invalid.into();
                assert!(Profile::decode(&serde_json::to_vec(&value).unwrap()).is_err());
            }
        }
        let mut overlapping = notification_profile();
        let layout = overlapping.notification_area.as_mut().unwrap();
        layout.parent_offset = layout.child_offset;
        assert!(overlapping.validate().is_err());
        let mut truncated = original;
        truncated["notification_area"]["control_entries"]
            .as_array_mut()
            .unwrap()
            .pop();
        assert!(Profile::decode(&serde_json::to_vec(&truncated).unwrap()).is_err());
    }

    #[test]
    fn notification_control_table_requires_all_three_exact_relocated_entries() {
        let layout = notification_profile().notification_area.unwrap();
        for base in [0x180000000u64, 0x7ff700000000] {
            let table = layout
                .control_entries
                .iter()
                .flat_map(|entry| (base + u64::from(entry.rva)).to_le_bytes())
                .collect::<Vec<_>>();
            layout.verify_control_vtable(base, &table).unwrap();
            for index in 0..3 {
                let mut changed = table.clone();
                changed[index * 8] ^= 1;
                assert!(layout.verify_control_vtable(base, &changed).is_err());
            }
            assert!(
                layout
                    .verify_control_vtable(base + 0x10000, &table)
                    .is_err()
            );
            assert!(layout.verify_control_vtable(u64::MAX, &table).is_err());
            assert!(layout.verify_control_vtable(base, &table[..23]).is_err());
            let mut extended = table;
            extended.extend_from_slice(&[0; 8]);
            assert!(layout.verify_control_vtable(base, &extended).is_err());
        }
    }

    #[test]
    fn clock_only_evidence_needs_no_fabricated_tray_entry() {
        let original = serde_json::to_value(&profiles().unwrap()[0]).unwrap();
        let fields = ["tray", "tray_abi", "tray_geometry", "tray_entries"];
        let mut clock = original.clone();
        for field in fields {
            clock.as_object_mut().unwrap().remove(field);
        }
        let profile = Profile::decode(&serde_json::to_vec(&clock).unwrap()).unwrap();
        assert_eq!(profile.modules().len(), 2);
        assert_eq!(profile.host, profiles().unwrap()[0].host);
        // Partial evidence must never become a callable tray recipe.
        for mask in 1..15 {
            let mut partial = clock.clone();
            for (index, field) in fields.iter().enumerate() {
                if mask & (1 << index) != 0 {
                    partial[*field] = original[*field].clone();
                }
            }
            assert!(Profile::decode(&serde_json::to_vec(&partial).unwrap()).is_err());
        }
    }

    #[test]
    fn early_child_table_requires_every_reviewed_relocated_slot() {
        let mut source = profiles().unwrap()[0].source.clone();
        source.recipe = SourceRecipe::XamlExplorerHostChildXfgV1;
        source.factory_vtable_rva = None;
        let entries = std::array::from_fn(|slot| SourceVtableEntry {
            rva: 0x2000 + slot as u32 * 32,
            prologue: [slot as u8; 16],
        });
        source.child_vtable_entries = Some(entries.clone());
        for base in [0x180000000u64, 0x7ff700000000] {
            let table = entries
                .iter()
                .flat_map(|entry| (base + u64::from(entry.rva)).to_le_bytes())
                .collect::<Vec<_>>();
            source.verify_child_vtable(base, &table).unwrap();
            for slot in 0..entries.len() {
                let mut replaced = table.clone();
                replaced[slot * 8] ^= 1;
                assert!(
                    source.verify_child_vtable(base, &replaced).is_err(),
                    "slot {slot}"
                );
            }
            assert!(source.verify_child_vtable(base + 65536, &table).is_err());
            assert!(
                source
                    .verify_child_vtable(base, &table[..table.len() - 1])
                    .is_err()
            );
            let mut extended = table.clone();
            extended.extend_from_slice(&0u64.to_le_bytes());
            assert!(source.verify_child_vtable(base, &extended).is_err());
            assert!(source.verify_child_vtable(u64::MAX, &table).is_err());
        }
        source.recipe = SourceRecipe::XamlExplorerHostChildV1;
        assert!(source.verify_child_vtable(0x180000000, &[0; 104]).is_err());
        source.child_vtable_entries = None;
        assert!(source.verify_child_vtable(0x180000000, &[0; 104]).is_err());
    }

    fn changed_identities(original: &ModuleIdentity) -> Vec<ModuleIdentity> {
        let mut changed = vec![original.clone(); 5];
        changed[0].machine = 0xaa64;
        changed[1].timestamp ^= 1;
        changed[2].image_size += 4096;
        changed[3].pdb_age += 1;
        changed[4].pdb_guid[0] ^= 0xff;
        changed
    }

    fn assert_module_identity_requirements(module: &Module) {
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

    #[test]
    fn embedded_profiles_are_complete_and_selected_by_exact_taskbar_identity() {
        let profiles = profiles().unwrap();
        let expected_ids = [
            "windows11-26300.9711-x64",
            "windows11-26300.9818-x64",
            "windows11-26200.8457-x64",
            "windows11-26200.8037-x64",
            "windows11-26100.1742-x64",
            "windows11-22631.2861-x64",
            "windows11-22621.1702-x64",
            "windows11-22000.318-x64-clock",
            #[cfg(feature = "compatibility-lab")]
            "windows11-28000.3365-x64-notification",
            "windows11-28000.1-x64-notification",
        ];
        assert_eq!(profiles.len(), expected_ids.len());
        for id in expected_ids {
            let profile = profiles.iter().find(|profile| profile.id == id).unwrap();
            let encoded = serde_json::to_vec(profile).unwrap();
            assert_eq!(Profile::decode(&encoded).unwrap().host, profile.host);
            assert_eq!(profile.tray.is_some(), profile.tray_entries.is_some());
            assert_eq!(
                profile_for_taskbar(&profile.taskbar.identity).unwrap().id,
                id
            );
            for changed in changed_identities(&profile.taskbar.identity) {
                assert!(profile_for_taskbar(&changed).is_err());
            }
        }
    }
    #[test]
    fn promoted_vm_profiles_keep_all_module_identities_separate() {
        let candidates = [
            (
                "windows11-28000.1-x64-notification",
                &include_bytes!("../profiles/windows11-28000.1-x64-notification.json")[..],
            ),
            (
                "windows11-26200.8037-x64",
                &include_bytes!("../profiles/windows11-26200.8037-x64.json")[..],
            ),
            (
                "windows11-26100.1742-x64",
                &include_bytes!("../profiles/windows11-26100.1742-x64.json")[..],
            ),
            (
                "windows11-22631.2861-x64",
                &include_bytes!("../profiles/windows11-22631.2861-x64.json")[..],
            ),
            (
                "windows11-22621.1702-x64",
                &include_bytes!("../profiles/windows11-22621.1702-x64.json")[..],
            ),
        ]
        .map(|(id, bytes)| {
            let profile = Profile::decode(bytes).unwrap();
            assert_eq!(profile.id, id);
            assert_eq!(profile.tray.as_ref().unwrap().name, "Taskbar.View.dll");
            profile
        });
        let registry = profiles().unwrap();
        let ids = registry
            .iter()
            .map(|profile| profile.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            [
                "windows11-26300.9711-x64",
                "windows11-26300.9818-x64",
                "windows11-26200.8457-x64",
                "windows11-26200.8037-x64",
                "windows11-26100.1742-x64",
                "windows11-22631.2861-x64",
                "windows11-22621.1702-x64",
                "windows11-22000.318-x64-clock",
                #[cfg(feature = "compatibility-lab")]
                "windows11-28000.3365-x64-notification",
                "windows11-28000.1-x64-notification",
            ]
        );
        for candidate in &candidates {
            assert_eq!(
                profile_for_taskbar(&candidate.taskbar.identity).unwrap().id,
                candidate.id
            );
            for changed in changed_identities(&candidate.taskbar.identity) {
                assert!(profile_for_taskbar(&changed).is_err());
            }
            for module in candidate.modules() {
                assert_module_identity_requirements(module);
            }
            for other in registry.iter().filter(|profile| profile.id != candidate.id) {
                let mut mixed_host = candidate.clone();
                mixed_host.host = other.host.clone();
                assert!(mixed_host.validate().is_err());
                let mut reverse_host = other.clone();
                reverse_host.host = candidate.host.clone();
                assert!(reverse_host.validate().is_err());
                for (other_module, candidate_module) in
                    other.modules().into_iter().zip(candidate.modules())
                {
                    assert!(
                        !other_module.matches(&candidate_module.sha256, &candidate_module.identity)
                    );
                    assert!(
                        !candidate_module.matches(&other_module.sha256, &other_module.identity)
                    );
                    assert!(
                        !candidate_module.matches(&other_module.sha256, &candidate_module.identity)
                    );
                    assert!(
                        !candidate_module.matches(&candidate_module.sha256, &other_module.identity)
                    );
                    assert!(!other_module.matches_loaded(
                        &serde_json::to_value(&candidate_module.identity).unwrap()
                    ));
                    assert!(
                        !candidate_module
                            .matches_loaded(&serde_json::to_value(&other_module.identity).unwrap())
                    );
                }
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
            profile.tray.as_mut().unwrap().name = name.into();
            profile.tray.as_mut().unwrap().relative_path = path.into();
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
    fn tray_abi_contract_preserves_borrowed_member_identity() {
        let mut profile = profiles().unwrap()[0].clone();
        for (abi, name) in [
            (
                TrayAbi::SystemTrayCoordinatesX64V1,
                "system_tray_coordinates_x64_v1",
            ),
            (
                TrayAbi::SystemTrayDragDropBorrowedCoordinatesX64V1,
                "system_tray_drag_drop_borrowed_coordinates_x64_v1",
            ),
        ] {
            profile.tray_abi = Some(abi);
            profile.tray_geometry = Some(match abi {
                TrayAbi::SystemTrayCoordinatesX64V1 => TrayGeometry::NotificationAreaBounds,
                TrayAbi::SystemTrayDragDropBorrowedCoordinatesX64V1 => {
                    TrayGeometry::DragDropHitTesting
                }
            });
            let mut encoded = serde_json::to_value(&profile).unwrap();
            assert_eq!(encoded["tray_abi"], name);
            assert_eq!(
                Profile::decode(&serde_json::to_vec(&encoded).unwrap())
                    .unwrap()
                    .tray_abi,
                Some(abi)
            );
            // The new calling convention still requires all three reviewed
            // roles; it cannot turn missing screen-rectangle evidence into a
            // valid profile or infer an ABI from the function's address.
            encoded["tray_entries"].as_array_mut().unwrap().pop();
            assert!(Profile::decode(&serde_json::to_vec(&encoded).unwrap()).is_err());
        }
        for unknown in ["system_tray_drag_drop_borrowed_coordinates_x64_v2", "auto"] {
            let mut encoded = serde_json::to_value(&profile).unwrap();
            encoded["tray_abi"] = serde_json::json!(unknown);
            assert!(Profile::decode(&serde_json::to_vec(&encoded).unwrap()).is_err());
        }
    }
    #[test]
    fn tray_geometry_requires_its_reviewed_calling_convention() {
        let original = profiles().unwrap()[0].clone();
        for abi in [
            TrayAbi::SystemTrayCoordinatesX64V1,
            TrayAbi::SystemTrayDragDropBorrowedCoordinatesX64V1,
        ] {
            for geometry in [
                TrayGeometry::NotificationAreaBounds,
                TrayGeometry::DragDropHitTesting,
                TrayGeometry::LegacyUnclassified,
            ] {
                let mut profile = original.clone();
                profile.tray_abi = Some(abi);
                profile.tray_geometry = Some(geometry);
                let expected = matches!(
                    (abi, geometry),
                    (
                        TrayAbi::SystemTrayCoordinatesX64V1,
                        TrayGeometry::NotificationAreaBounds | TrayGeometry::LegacyUnclassified
                    ) | (
                        TrayAbi::SystemTrayDragDropBorrowedCoordinatesX64V1,
                        TrayGeometry::DragDropHitTesting
                    )
                );
                assert_eq!(
                    Profile::decode(&serde_json::to_vec(&profile).unwrap()).is_ok(),
                    expected,
                    "{abi:?}: {geometry:?}"
                );
            }
        }
    }
    #[test]
    fn disk_and_loaded_identities_are_independent_requirements() {
        for profile in profiles().unwrap() {
            for module in profile.modules() {
                assert_module_identity_requirements(module);
            }
        }
        let profiles = profiles().unwrap();
        for old_profile in profiles {
            for new_profile in profiles
                .iter()
                .filter(|profile| profile.id != old_profile.id)
            {
                for (old, new) in old_profile.modules().into_iter().zip(new_profile.modules()) {
                    assert!(!old.matches(&new.sha256, &new.identity));
                    assert!(!old.matches_loaded(&serde_json::to_value(&new.identity).unwrap()));
                }
            }
        }
    }
}
