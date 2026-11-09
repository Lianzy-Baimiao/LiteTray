//! Disk-only compatibility inventory and preflight. This module never loads
//! DLLs, opens Explorer, reads the user's profile, or contacts a symbol server.
use crate::pe::{self, ImageIdentity};
use litetray_adapter_protocol::{
    HostLayout, ModuleIdentity,
    compatibility::{self, MatchBasis, Module, Profile, Recognized, Resolved, Role, TrayAbi},
};
use serde::Serialize;
use std::{fs::File, io::Read, path::Path};

#[path = "compatibility_status.rs"]
mod status;
pub use status::{
    DetectionSummary, ImplementationFamily, SupportProfile, ValidationEvidence, ValidationScenario,
    ValidationStatus, support_catalog,
};

const MAX_IMAGE: u64 = 128 * 1024 * 1024;
const MODULE_PATHS: [&str; 3] = [
    "System32/taskbar.dll",
    "System32/twinui.pcshell.dll",
    "SystemApps/MicrosoftWindows.Client.Core_cw5n1h2txyewy/SystemTray.dll",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleStatus {
    Matched,
    UnknownImage,
    Missing,
    ReadFailed,
    InvalidImage,
}
#[derive(Debug, Clone, Serialize)]
pub struct ModuleReport {
    pub name: String,
    pub relative_path: String,
    pub status: ModuleStatus,
    pub image: Option<ImageIdentity>,
    pub reason: Option<String>,
    pub recognition: Option<RecognitionReport>,
}
#[derive(Debug, Clone, Serialize)]
pub struct RecognitionReport {
    pub basis: MatchBasis,
    pub evidence_profile: String,
    pub implementation: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    ClockAppearance,
    ClockInput,
    TrayMirroring,
    SystemAreas,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureStatus {
    DiskMatchedLiveCheckRequired,
    UnsupportedModules,
    MissingModules,
    InspectionFailed,
}
#[derive(Debug, Serialize)]
pub struct FeatureReport {
    pub feature: Feature,
    pub status: FeatureStatus,
    pub required_modules: Vec<String>,
    pub reasons: Vec<String>,
    pub baseline_limitations: Vec<&'static str>,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: u32,
    pub scope: &'static str,
    pub os_build: Option<u32>,
    pub collector_architecture: &'static str,
    pub explorer_accessed: bool,
    pub network_accessed: bool,
    pub loaded_modules_checked: bool,
    pub runtime_topology_checked: bool,
    pub profile_id: Option<String>,
    pub profile_selection_reason: Option<String>,
    pub all_profile_images_matched: bool,
    /// A composition of independently recognized modules, not test history.
    pub implementation: Option<ImplementationReport>,
    pub modules: Vec<ModuleReport>,
    pub other_images: Vec<ModuleReport>,
    pub features: Vec<FeatureReport>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ImplementationReport {
    pub id: String,
    pub source_recipe: compatibility::SourceRecipe,
    pub topology: compatibility::Topology,
    pub host_child_offset: u32,
    pub child_source_offset: u32,
    pub tray_abi: Option<TrayAbi>,
    pub tray_geometry: Option<compatibility::TrayGeometry>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    pub clocks: bool,
    pub tray: bool,
    pub notice: Option<String>,
}
impl Report {
    /// A disk match authorizes only the existing in-process preparation, which
    /// independently verifies loaded identities, roots, windows and lifetimes.
    pub fn require(&self, clocks: bool, tray: bool) -> Result<(), String> {
        for report in &self.features {
            let required = match report.feature {
                Feature::ClockAppearance | Feature::ClockInput => clocks,
                Feature::TrayMirroring | Feature::SystemAreas => tray,
            };
            if required && report.status != FeatureStatus::DiskMatchedLiveCheckRequired {
                return Err(format!(
                    "任务栏兼容检查未通过：{}",
                    report.reasons.join("；")
                ));
            }
        }
        Ok(())
    }
    /// Preserve saved preferences, but allow the independently checked clock
    /// to run if only the requested tray implementation is unavailable.
    pub fn negotiate(&self, clocks: bool, tray: bool) -> Result<Capabilities, String> {
        self.require(clocks, false)?;
        let mut enabled = Capabilities {
            clocks,
            tray,
            notice: None,
        };
        if tray && let Err(error) = self.require(false, true) {
            if !clocks {
                return Err(error);
            }
            enabled.tray = false;
            enabled.notice = Some(format!(
                "时钟已启用；托盘复制暂未启用。{error}。保存的托盘设置保持不变，可在兼容性页查看详情。"
            ));
        }
        if !clocks && !tray {
            return Err("没有请求启用的任务栏功能".into());
        }
        Ok(enabled)
    }
}
fn identity(image: &ImageIdentity) -> ModuleIdentity {
    ModuleIdentity {
        machine: image.machine,
        timestamp: image.timestamp,
        image_size: image.image_size,
        pdb_guid: image.pdb_guid,
        pdb_age: image.pdb_age,
    }
}
fn bounded_read(path: &Path) -> Result<Vec<u8>, std::io::Error> {
    let file = File::open(path)?;
    if file.metadata()?.len() > MAX_IMAGE {
        return Err(std::io::Error::other("系统模块超过 128 MiB 检查上限"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_IMAGE + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_IMAGE {
        return Err(std::io::Error::other("读取时系统模块尺寸超限"));
    }
    Ok(bytes)
}
fn section(bytes: &[u8], rva: u32, length: usize, executable: bool) -> Result<&[u8], String> {
    let (bytes, section) = pe::rva_bytes(bytes, rva, length)?;
    if section.characteristics & 0x80000000 != 0
        || (section.characteristics & 0x20000000 != 0) != executable
    {
        return Err("兼容入口的磁盘节区权限不匹配".into());
    }
    Ok(bytes)
}
fn verify_layout(profile: &Profile, module: &Module, bytes: &[u8]) -> Result<(), String> {
    if module.name == profile.taskbar.name {
        for symbol in &profile.host.symbols {
            let code = section(
                bytes,
                symbol.rva,
                if symbol.executable { 16 } else { 8 },
                symbol.executable,
            )?;
            if symbol.executable && code != symbol.prologue {
                return Err("内置宿主符号入口与系统模块不匹配".into());
            }
        }
    } else if module.name == profile.shell.name {
        for rva in [
            profile.source.child_vtable_rva,
            profile.source.factory_vtable_rva,
        ] {
            section(bytes, rva, 6 * 8, false)?;
        }
    } else if module.name == profile.tray.name {
        for entry in &profile.tray_entries {
            if section(bytes, entry.rva, 16, true)? != entry.prologue {
                return Err("内置托盘入口与系统模块不匹配".into());
            }
        }
    }
    Ok(())
}
struct DiskImage {
    report: ModuleReport,
    bytes: Vec<u8>,
}
impl DiskImage {
    fn recognize<'a>(
        mut self,
        role: Role,
        select: &impl Fn(Role, &str, &[u8]) -> Result<Recognized<'a>, String>,
    ) -> (ModuleReport, Option<Recognized<'a>>) {
        if let Some(image) = &self.report.image {
            match select(role, &self.report.relative_path, &self.bytes) {
                Ok(recognized) => {
                    if !recognized.module.matches(&image.sha256, &identity(image)) {
                        self.report.status = ModuleStatus::InvalidImage;
                        self.report.reason =
                            Some("模块识别结果未绑定本次读取的完整身份和哈希".into());
                        return (self.report, None);
                    }
                    match verify_layout(recognized.evidence, &recognized.module, &self.bytes) {
                        Ok(()) => {
                            self.report.status = ModuleStatus::Matched;
                            self.report.recognition = Some(RecognitionReport {
                                basis: recognized.basis,
                                evidence_profile: recognized.evidence.id.clone(),
                                implementation: match role {
                                    Role::Taskbar => format!(
                                        "XAML 宿主；元素偏移 {:#x}，子宿主偏移 {:#x}",
                                        recognized.evidence.host.element_offset,
                                        recognized.evidence.source.host_child_offset
                                    ),
                                    Role::Shell => format!(
                                        "XamlExplorerHostChild；source 偏移 {:#x}",
                                        recognized.evidence.source.child_source_offset
                                    ),
                                    Role::Tray => match recognized.evidence.tray_abi {
                                        TrayAbi::SystemTrayCoordinatesX64V1 => {
                                            "托盘坐标：三参数、传值对象".into()
                                        }
                                        TrayAbi::SystemTrayDragDropBorrowedCoordinatesX64V1 => {
                                            "托盘坐标：四参数、借用对象".into()
                                        }
                                    },
                                },
                            });
                            return (self.report, Some(recognized));
                        }
                        Err(error) => {
                            self.report.status = ModuleStatus::InvalidImage;
                            self.report.reason = Some(error);
                        }
                    }
                }
                Err(error) => self.report.reason = Some(error),
            }
        }
        (self.report, None)
    }
}
fn inspect(root: &Path, relative: &str) -> DiskImage {
    let mut report = ModuleReport {
        name: relative.rsplit('/').next().unwrap_or(relative).into(),
        relative_path: relative.into(),
        status: ModuleStatus::Missing,
        image: None,
        reason: None,
        recognition: None,
    };
    let bytes = match bounded_read(&root.join(relative)) {
        Ok(bytes) => bytes,
        Err(error) => {
            report.status = if error.kind() == std::io::ErrorKind::NotFound {
                ModuleStatus::Missing
            } else {
                ModuleStatus::ReadFailed
            };
            report.reason = Some(error.to_string());
            return DiskImage {
                report,
                bytes: Vec::new(),
            };
        }
    };
    match pe::identity(&bytes) {
        Err(error) => {
            report.status = ModuleStatus::InvalidImage;
            report.reason = Some(error);
        }
        Ok(image) => {
            report.status = ModuleStatus::UnknownImage;
            report.image = Some(image);
        }
    }
    DiskImage { report, bytes }
}
fn features(modules: &[ModuleReport]) -> Vec<FeatureReport> {
    [
        Feature::ClockAppearance,
        Feature::ClockInput,
        Feature::TrayMirroring,
        Feature::SystemAreas,
    ]
    .into_iter()
    .map(|feature| {
        let required = if matches!(feature, Feature::ClockAppearance | Feature::ClockInput) {
            &modules[..2]
        } else {
            modules
        };
        let status = if required.iter().any(|m| {
            matches!(
                m.status,
                ModuleStatus::ReadFailed | ModuleStatus::InvalidImage
            )
        }) {
            FeatureStatus::InspectionFailed
        } else if required.iter().any(|m| m.status == ModuleStatus::Missing) {
            FeatureStatus::MissingModules
        } else if required
            .iter()
            .any(|m| m.status == ModuleStatus::UnknownImage)
        {
            FeatureStatus::UnsupportedModules
        } else {
            FeatureStatus::DiskMatchedLiveCheckRequired
        };
        FeatureReport {
            feature,
            status,
            required_modules: required.iter().map(|m| m.name.clone()).collect(),
            reasons: required
                .iter()
                .filter(|m| m.status != ModuleStatus::Matched)
                .map(|m| {
                    format!(
                        "{}：{}",
                        m.name,
                        m.reason.as_deref().unwrap_or("无法核对模块")
                    )
                })
                .collect(),
            baseline_limitations: match feature {
                Feature::ClockAppearance => vec![],
                Feature::ClockInput => vec!["notification_click_crash_report_unresolved"],
                Feature::TrayMirroring => {
                    vec!["third_party_hover_popups_require_separate_validation"]
                }
                Feature::SystemAreas => vec![
                    "system_icon_and_cross_monitor_menu_validation_incomplete",
                    "notification_click_crash_report_unresolved",
                ],
            },
        }
    })
    .collect()
}
struct Inventory {
    report: Report,
    resolved: Option<Resolved>,
}
impl Inventory {
    fn host_layout(&self, clocks: bool, tray: bool) -> Result<HostLayout, String> {
        self.report.require(clocks, tray)?;
        let resolved = self.resolved.as_ref().ok_or_else(|| {
            self.report
                .profile_selection_reason
                .clone()
                .unwrap_or_else(|| "尚未识别所需任务栏模块的实现特征".into())
        })?;
        if self
            .report
            .modules
            .first()
            .is_none_or(|module| module.status != ModuleStatus::Matched)
        {
            return Err("taskbar.dll 的磁盘布局校验未通过".into());
        }
        Ok(resolved.host.clone())
    }
}
fn collect_selected<'a>(
    system_root: &Path,
    os_build: Option<u32>,
    select: impl Fn(Role, &str, &[u8]) -> Result<Recognized<'a>, String>,
) -> Inventory {
    // Every role is recognized from its own bytes. A template contributes only
    // the fields owned by that module; OS labels and registry order play no part.
    let (taskbar_report, taskbar) =
        inspect(system_root, MODULE_PATHS[0]).recognize(Role::Taskbar, &select);
    let (shell_report, shell) =
        inspect(system_root, MODULE_PATHS[1]).recognize(Role::Shell, &select);
    let mut trays: Vec<_> = [
        MODULE_PATHS[2],
        "SystemApps/MicrosoftWindows.Client.Core_cw5n1h2txyewy/Taskbar.View.dll",
    ]
    .into_iter()
    .map(|path| inspect(system_root, path).recognize(Role::Tray, &select))
    .collect();
    let matched_trays: Vec<_> = trays
        .iter()
        .enumerate()
        .filter(|(_, (_, recognized))| recognized.is_some())
        .map(|(index, _)| index)
        .collect();
    let tray_index = matched_trays.first().copied().unwrap_or_else(|| {
        trays
            .iter()
            .position(|(report, _)| report.status != ModuleStatus::Missing)
            .unwrap_or(0)
    });
    let (mut tray_report, mut tray) = trays.remove(tray_index);
    if matched_trays.len() > 1 {
        tray_report.status = ModuleStatus::InvalidImage;
        tray_report.reason = Some("同时存在多种可识别的托盘模块，无法从磁盘确定实际使用者".into());
        tray_report.recognition = None;
        tray = None;
    }
    let mut modules = vec![taskbar_report, shell_report, tray_report];
    let (resolved, selection_reason) = match (&taskbar, &shell) {
        (Some(taskbar), Some(shell)) => {
            match compatibility::assemble(taskbar, shell, tray.as_ref()) {
                Ok(plan) => (Some(plan), None),
                Err(error) => {
                    // A recognized image alone cannot authorize a mismatched
                    // interface contract between modules.
                    modules[1].status = ModuleStatus::InvalidImage;
                    modules[1].reason = Some(error.clone());
                    (None, Some(error))
                }
            }
        }
        _ => (
            None,
            Some("时钟需要同时识别任务栏宿主与 XAML 容器；请查看各模块的检查结果".into()),
        ),
    };
    // Exact-image history is retained only for the original complete set.
    let profile_id = match (&taskbar, &shell, &tray) {
        (Some(host), Some(shell), Some(tray))
            if [host, shell, tray].iter().all(|item| {
                item.basis == MatchBasis::ExactImage && item.evidence.id == host.evidence.id
            }) =>
        {
            Some(host.evidence.id.clone())
        }
        _ => None,
    };
    let implementation = resolved.as_ref().map(|plan| ImplementationReport {
        id: plan.id.clone(),
        source_recipe: plan.source.recipe,
        topology: plan.topology,
        host_child_offset: plan.source.host_child_offset,
        child_source_offset: plan.source.child_source_offset,
        tray_abi: plan.tray.as_ref().map(|tray| tray.abi),
        tray_geometry: plan.tray.as_ref().map(|tray| tray.geometry),
    });
    let other_images = [
        "explorer.exe",
        "SystemApps/MicrosoftWindows.Client.CBS_cw5n1h2txyewy/Taskbar.View.dll",
        "SystemApps/MicrosoftWindows.Client.CBS_cw5n1h2txyewy/ExplorerExtensions.dll",
    ]
    .into_iter()
    .map(|path| inspect(system_root, path).report)
    .chain(trays.into_iter().map(|(report, _)| report))
    .filter(|m| m.status != ModuleStatus::Missing)
    .collect();
    let report = Report {
        schema: 2,
        scope: "disk_only",
        os_build,
        collector_architecture: std::env::consts::ARCH,
        explorer_accessed: false,
        network_accessed: false,
        loaded_modules_checked: false,
        runtime_topology_checked: false,
        all_profile_images_matched: profile_id.is_some()
            && modules.iter().all(|m| m.status == ModuleStatus::Matched),
        profile_id,
        profile_selection_reason: selection_reason,
        implementation,
        features: features(&modules),
        modules,
        other_images,
    };
    Inventory { report, resolved }
}
fn inventory(system_root: &Path, os_build: Option<u32>) -> Result<Inventory, String> {
    // A malformed built-in registry is a collector error, not an unsupported OS.
    compatibility::profiles()?;
    Ok(collect_selected(
        system_root,
        os_build,
        compatibility::recognize,
    ))
}
pub fn collect(system_root: &Path, os_build: Option<u32>) -> Result<Report, String> {
    Ok(inventory(system_root, os_build)?.report)
}
pub fn preflight(system_root: &Path, clocks: bool, tray: bool) -> Result<HostLayout, String> {
    inventory(system_root, None)?.host_layout(clocks, tray)
}
pub fn negotiate(
    system_root: &Path,
    clocks: bool,
    tray: bool,
) -> Result<(HostLayout, Capabilities), String> {
    let inventory = inventory(system_root, None)?;
    let enabled = inventory.report.negotiate(clocks, tray)?;
    Ok((
        inventory.host_layout(enabled.clocks, enabled.tray)?,
        enabled,
    ))
}

#[cfg(test)]
#[path = "compatibility_checks.rs"]
mod profile_selection_checks;

#[cfg(test)]
mod tests {
    use super::*;
    fn modules() -> Vec<ModuleReport> {
        compatibility::profiles().unwrap()[0]
            .modules()
            .into_iter()
            .map(|m| ModuleReport {
                name: m.name.clone(),
                relative_path: m.relative_path.clone(),
                status: ModuleStatus::Matched,
                image: None,
                reason: None,
                recognition: None,
            })
            .collect()
    }
    #[test]
    fn unknown_tray_does_not_mark_the_clock_incompatible() {
        let mut modules = modules();
        modules[2].status = ModuleStatus::UnknownImage;
        let result = features(&modules);
        assert_eq!(
            result[0].status,
            FeatureStatus::DiskMatchedLiveCheckRequired
        );
        assert_eq!(
            result[1].status,
            FeatureStatus::DiskMatchedLiveCheckRequired
        );
        assert_eq!(result[2].status, FeatureStatus::UnsupportedModules);
        assert_eq!(result[3].status, FeatureStatus::UnsupportedModules);
        modules[0].status = ModuleStatus::UnknownImage;
        assert!(
            features(&modules)
                .iter()
                .all(|f| f.status == FeatureStatus::UnsupportedModules)
        );
    }
    #[test]
    fn missing_and_corrupt_files_report_without_loading_or_requiring_user_configuration() {
        let root = std::env::temp_dir().join(format!("litetray-compat-{}", std::process::id()));
        std::fs::create_dir_all(root.join("System32")).unwrap();
        std::fs::write(root.join("System32/taskbar.dll"), b"invalid PE").unwrap();
        let report = collect(&root, Some(26300)).unwrap();
        assert_eq!(report.modules[0].status, ModuleStatus::InvalidImage);
        assert_eq!(report.modules[1].status, ModuleStatus::Missing);
        assert!(report.require(true, true).is_err());
        assert!(
            !report.explorer_accessed && !report.network_accessed && !report.loaded_modules_checked
        );
        assert!(!report.all_profile_images_matched);
        assert_eq!(report.profile_id, None);
        assert!(report.profile_selection_reason.is_some());
        assert_eq!(report.features[0].status, FeatureStatus::InspectionFailed);
        std::fs::remove_file(root.join("System32/taskbar.dll")).unwrap();
        std::fs::remove_dir(root.join("System32")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn same_build_number_does_not_approve_an_unrecognized_image() {
        let mut modules = modules();
        modules[1].status = ModuleStatus::UnknownImage;
        assert!(
            features(&modules)
                .iter()
                .all(|f| f.status == FeatureStatus::UnsupportedModules)
        );
        modules[1].status = ModuleStatus::ReadFailed;
        assert!(
            features(&modules)
                .iter()
                .all(|f| f.status == FeatureStatus::InspectionFailed)
        );
    }
}
