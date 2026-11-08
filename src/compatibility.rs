//! Disk-only compatibility inventory and preflight. This module never loads
//! DLLs, opens Explorer, reads the user's profile, or contacts a symbol server.
use crate::pe::{self, ImageIdentity};
use litetray_adapter_protocol::{
    HostLayout, ModuleIdentity,
    compatibility::{self, Module, Profile},
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
    pub modules: Vec<ModuleReport>,
    pub other_images: Vec<ModuleReport>,
    pub features: Vec<FeatureReport>,
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
    fn verify(
        mut self,
        expected: Option<(&Profile, &Module)>,
        selection_reason: Option<&str>,
    ) -> ModuleReport {
        if let Some(image) = &self.report.image {
            if let Some((profile, expected)) = expected {
                if expected.matches(&image.sha256, &identity(image)) {
                    match verify_layout(profile, expected, &self.bytes) {
                        Ok(()) => self.report.status = ModuleStatus::Matched,
                        Err(error) => {
                            self.report.status = ModuleStatus::InvalidImage;
                            self.report.reason = Some(error);
                        }
                    }
                } else {
                    self.report.reason = Some(format!(
                        "模块身份或 SHA-256 与所选兼容资料 {} 不匹配",
                        profile.id
                    ));
                }
            } else {
                self.report.reason = selection_reason.map(str::to_owned);
            }
        }
        self.report
    }
}
fn inspect(root: &Path, relative: &str) -> DiskImage {
    let mut report = ModuleReport {
        name: relative.rsplit('/').next().unwrap_or(relative).into(),
        relative_path: relative.into(),
        status: ModuleStatus::Missing,
        image: None,
        reason: None,
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
struct Inventory<'a> {
    report: Report,
    profile: Option<&'a Profile>,
}
impl Inventory<'_> {
    fn host_layout(&self, clocks: bool, tray: bool) -> Result<HostLayout, String> {
        self.report.require(clocks, tray)?;
        let profile = self.profile.ok_or_else(|| {
            self.report
                .profile_selection_reason
                .clone()
                .unwrap_or_else(|| "没有与任务栏磁盘模块匹配的兼容资料".into())
        })?;
        if self
            .report
            .modules
            .first()
            .is_none_or(|module| module.status != ModuleStatus::Matched)
        {
            return Err("taskbar.dll 的磁盘布局校验未通过".into());
        }
        Ok(profile.host.clone())
    }
}
fn collect_selected<'a>(
    system_root: &Path,
    os_build: Option<u32>,
    select: impl FnOnce(&ModuleIdentity) -> Result<&'a Profile, String>,
) -> Inventory<'a> {
    // Select and validate from the same bytes. A later read must never switch
    // taskbar versions while keeping another profile's shell or tray layout.
    let taskbar = inspect(system_root, MODULE_PATHS[0]);
    let selected = taskbar
        .report
        .image
        .as_ref()
        .ok_or_else(|| "无法取得 taskbar.dll 的磁盘身份，未选择兼容资料".into())
        .and_then(|image| {
            let module_identity = identity(image);
            let profile = select(&module_identity)?;
            if profile.taskbar.matches(&image.sha256, &module_identity) {
                Ok(profile)
            } else {
                Err("taskbar.dll 的身份或 SHA-256 与内置兼容资料不匹配".into())
            }
        });
    let (profile, selection_reason) = match selected {
        Ok(profile) => (Some(profile), None),
        Err(error) => (None, Some(error)),
    };
    let mut modules = vec![taskbar.verify(
        profile.map(|profile| (profile, &profile.taskbar)),
        selection_reason.as_deref(),
    )];
    for (index, fallback_path) in MODULE_PATHS.iter().enumerate().skip(1) {
        let expected = profile.map(|profile| (profile, profile.modules()[index]));
        let path = expected.map_or(*fallback_path, |(_, module)| module.relative_path.as_str());
        modules.push(inspect(system_root, path).verify(expected, selection_reason.as_deref()));
    }
    // Inventory known alternate locations without loading them or guessing ABI.
    let other_images = [
        "explorer.exe",
        "SystemApps/MicrosoftWindows.Client.Core_cw5n1h2txyewy/Taskbar.View.dll",
        "SystemApps/MicrosoftWindows.Client.CBS_cw5n1h2txyewy/Taskbar.View.dll",
        "SystemApps/MicrosoftWindows.Client.CBS_cw5n1h2txyewy/ExplorerExtensions.dll",
    ]
    .into_iter()
    .map(|path| inspect(system_root, path).report)
    .filter(|m| m.status != ModuleStatus::Missing)
    .collect();
    let report = Report {
        schema: 1,
        scope: "disk_only",
        os_build,
        collector_architecture: std::env::consts::ARCH,
        explorer_accessed: false,
        network_accessed: false,
        loaded_modules_checked: false,
        runtime_topology_checked: false,
        profile_id: profile.map(|profile| profile.id.clone()),
        profile_selection_reason: selection_reason,
        all_profile_images_matched: profile.is_some()
            && modules.iter().all(|m| m.status == ModuleStatus::Matched),
        features: features(&modules),
        modules,
        other_images,
    };
    Inventory { report, profile }
}
fn inventory(system_root: &Path, os_build: Option<u32>) -> Result<Inventory<'static>, String> {
    // A malformed built-in registry is a collector error, not an unsupported OS.
    compatibility::profiles()?;
    Ok(collect_selected(
        system_root,
        os_build,
        compatibility::profile_for_taskbar,
    ))
}
pub fn collect(system_root: &Path, os_build: Option<u32>) -> Result<Report, String> {
    Ok(inventory(system_root, os_build)?.report)
}
pub fn preflight(system_root: &Path, clocks: bool, tray: bool) -> Result<HostLayout, String> {
    inventory(system_root, None)?.host_layout(clocks, tray)
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
