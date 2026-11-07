//! Disk-only compatibility inventory and preflight. This module never loads
//! DLLs, opens Explorer, reads the user's profile, or contacts a symbol server.
use crate::pe::{self, ImageIdentity};
use litetray_adapter_protocol::{
    HostLayout, ModuleIdentity,
    compatibility::{Module, Profile},
};
use serde::Serialize;
use std::{fs::File, io::Read, path::Path};

const MAX_IMAGE: u64 = 128 * 1024 * 1024;

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
    pub profile_id: String,
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
fn inspect(root: &Path, relative: &str, expected: Option<(&Profile, &Module)>) -> ModuleReport {
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
            return report;
        }
    };
    match pe::identity(&bytes) {
        Err(error) => {
            report.status = ModuleStatus::InvalidImage;
            report.reason = Some(error);
        }
        Ok(image) => {
            report.status = ModuleStatus::UnknownImage;
            if let Some((profile, expected)) = expected {
                if expected.matches(&image.sha256, &identity(&image)) {
                    match verify_layout(profile, expected, &bytes) {
                        Ok(()) => report.status = ModuleStatus::Matched,
                        Err(error) => {
                            report.status = ModuleStatus::InvalidImage;
                            report.reason = Some(error);
                        }
                    }
                } else {
                    report.reason = Some("尚无与此模块身份匹配的内置兼容资料".into());
                }
            }
            report.image = Some(image);
        }
    }
    report
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
pub fn collect(system_root: &Path, os_build: Option<u32>) -> Result<Report, String> {
    let profile = litetray_adapter_protocol::compatibility::builtin()?;
    let modules = profile
        .modules()
        .into_iter()
        .map(|module| inspect(system_root, &module.relative_path, Some((profile, module))))
        .collect::<Vec<_>>();
    // Inventory known alternate locations without loading them or guessing ABI.
    let other_images = [
        "explorer.exe",
        "SystemApps/MicrosoftWindows.Client.Core_cw5n1h2txyewy/Taskbar.View.dll",
        "SystemApps/MicrosoftWindows.Client.CBS_cw5n1h2txyewy/Taskbar.View.dll",
        "SystemApps/MicrosoftWindows.Client.CBS_cw5n1h2txyewy/ExplorerExtensions.dll",
    ]
    .into_iter()
    .map(|path| inspect(system_root, path, None))
    .filter(|m| m.status != ModuleStatus::Missing)
    .collect();
    Ok(Report {
        schema: 1,
        scope: "disk_only",
        os_build,
        collector_architecture: std::env::consts::ARCH,
        explorer_accessed: false,
        network_accessed: false,
        loaded_modules_checked: false,
        runtime_topology_checked: false,
        profile_id: profile.id.clone(),
        all_profile_images_matched: modules.iter().all(|m| m.status == ModuleStatus::Matched),
        features: features(&modules),
        modules,
        other_images,
    })
}
pub fn preflight(system_root: &Path, clocks: bool, tray: bool) -> Result<HostLayout, String> {
    let report = collect(system_root, None)?;
    report.require(clocks, tray)?;
    Ok(litetray_adapter_protocol::compatibility::builtin()?
        .host
        .clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn modules() -> Vec<ModuleReport> {
        litetray_adapter_protocol::compatibility::builtin()
            .unwrap()
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
