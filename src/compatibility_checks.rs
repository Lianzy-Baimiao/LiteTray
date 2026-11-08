//! Synthetic PE files exercise the disk collector without requiring either
//! Windows build to be installed. They are never production compatibility data.
use super::*;
use litetray_adapter_protocol::{SymbolAddress, SymbolRole};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Fixture {
    profile: Profile,
    images: [Vec<u8>; 3],
}
impl Fixture {
    fn new(version: u8) -> Self {
        let images = std::array::from_fn(|role| {
            let mut bytes = pe::fixture();
            bytes[0x88..0x8c]
                .copy_from_slice(&(u32::from(version) * 10 + role as u32).to_le_bytes());
            bytes[0xa44..0xa54].fill(version);
            bytes[0xa45] = role as u8;
            bytes
        });
        let mut profile = compatibility::profiles().unwrap()[0].clone();
        profile.id = format!("synthetic-{version}");
        profile.system_label = format!("Synthetic compatibility test {version}");
        for (module, bytes) in [&mut profile.taskbar, &mut profile.shell, &mut profile.tray]
            .into_iter()
            .zip(&images)
        {
            let image = pe::identity(bytes).unwrap();
            module.identity = identity(&image);
            module.sha256 = image.sha256;
        }
        profile.host.module = profile.taskbar.identity.clone();
        profile.host.element_offset = 0x48;
        profile.host.symbols = SymbolRole::ALL
            .into_iter()
            .zip([0x2020, 0x2030, 0x1100, 0x1120, 0x1140, 0x1160])
            .map(|(role, rva)| {
                let (code, section) = pe::rva_bytes(&images[0], rva, 16).unwrap();
                SymbolAddress {
                    role,
                    rva,
                    section_start: section.rva,
                    section_size: section.virtual_size,
                    executable: role.executable(),
                    prologue: if role.executable() {
                        code.to_vec()
                    } else {
                        vec![]
                    },
                }
            })
            .collect();
        profile.source.child_vtable_rva = 0x2040;
        profile.source.factory_vtable_rva = 0x2080;
        for (entry, rva) in profile
            .tray_entries
            .iter_mut()
            .zip([0x1100, 0x1120, 0x1160])
        {
            let (code, section) = pe::rva_bytes(&images[2], rva, 16).unwrap();
            entry.rva = rva;
            entry.section_start = section.rva;
            entry.section_size = section.virtual_size;
            entry.prologue.copy_from_slice(code);
        }
        profile.validate().unwrap();
        Self { profile, images }
    }
}

struct SystemRoot(PathBuf);
impl SystemRoot {
    fn new(images: [&[u8]; 3]) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "litetray-profile-check-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        // Refuse to reuse an existing directory; cleanup removes only the
        // exact files and empty directories created by this fixture.
        std::fs::create_dir(&root).unwrap();
        for (relative, bytes) in MODULE_PATHS.into_iter().zip(images) {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        Self(root)
    }
}
impl Drop for SystemRoot {
    fn drop(&mut self) {
        for relative in MODULE_PATHS {
            let _ = std::fs::remove_file(self.0.join(relative));
        }
        for relative in [
            "System32",
            "SystemApps/MicrosoftWindows.Client.Core_cw5n1h2txyewy",
            "SystemApps",
        ] {
            let _ = std::fs::remove_dir(self.0.join(relative));
        }
        let _ = std::fs::remove_dir(&self.0);
    }
}

#[test]
fn disk_preflight_returns_the_selected_profile_layout_for_each_version() {
    for fixture in [Fixture::new(17), Fixture::new(18)] {
        let root = SystemRoot::new(fixture.images.each_ref().map(Vec::as_slice));
        let result = collect_selected(&root.0, Some(26300), |_| Ok(&fixture.profile));
        assert_eq!(
            result.report.profile_id.as_deref(),
            Some(fixture.profile.id.as_str())
        );
        assert!(result.report.profile_selection_reason.is_none());
        assert!(result.report.all_profile_images_matched);
        assert_eq!(
            result.host_layout(true, true).unwrap(),
            fixture.profile.host
        );
    }
}

#[test]
fn unknown_taskbar_does_not_attribute_other_modules_to_a_default_profile() {
    let fixture = Fixture::new(19);
    let root = SystemRoot::new(fixture.images.each_ref().map(Vec::as_slice));
    let result = collect(&root.0, Some(26300)).unwrap();
    assert_eq!(result.profile_id, None);
    assert!(serde_json::to_value(&result).unwrap()["profile_id"].is_null());
    assert!(
        result
            .modules
            .iter()
            .all(|module| module.status == ModuleStatus::UnknownImage)
    );
    assert!(
        result
            .features
            .iter()
            .all(|feature| feature.status == FeatureStatus::UnsupportedModules)
    );
    assert!(preflight(&root.0, true, true).is_err());

    let ambiguous = collect_selected(&root.0, Some(26300), |_| Err("ambiguous fixture".into()));
    assert_eq!(ambiguous.report.profile_id, None);
    assert_eq!(
        ambiguous.report.profile_selection_reason.as_deref(),
        Some("ambiguous fixture")
    );
    assert!(ambiguous.host_layout(true, true).is_err());
    assert!(ambiguous.host_layout(false, false).is_err());
}

#[test]
fn known_taskbar_identity_with_changed_hash_cannot_select_a_profile() {
    let mut fixture = Fixture::new(20);
    fixture.images[0][0x500] ^= 1;
    assert_eq!(
        identity(&pe::identity(&fixture.images[0]).unwrap()),
        fixture.profile.taskbar.identity
    );
    let root = SystemRoot::new(fixture.images.each_ref().map(Vec::as_slice));
    let result = collect_selected(&root.0, Some(26300), |_| Ok(&fixture.profile));
    assert_eq!(result.report.profile_id, None);
    assert!(!result.report.all_profile_images_matched);
    assert!(
        result
            .report
            .modules
            .iter()
            .all(|module| module.status == ModuleStatus::UnknownImage)
    );
    assert!(result.host_layout(true, false).is_err());
    assert!(result.host_layout(false, true).is_err());
}

#[test]
fn matching_taskbar_hash_does_not_replace_the_full_identity_check() {
    let mut fixture = Fixture::new(26);
    fixture.profile.taskbar.identity.timestamp ^= 1;
    fixture.profile.host.module = fixture.profile.taskbar.identity.clone();
    fixture.profile.validate().unwrap();
    let root = SystemRoot::new(fixture.images.each_ref().map(Vec::as_slice));
    let result = collect_selected(&root.0, Some(26300), |_| Ok(&fixture.profile));
    assert_eq!(result.report.profile_id, None);
    assert!(result.host_layout(true, true).is_err());
}

#[test]
fn mixing_shell_versions_cannot_combine_profiles() {
    let old = Fixture::new(21);
    let new = Fixture::new(22);
    let root = SystemRoot::new([&old.images[0], &new.images[1], &old.images[2]]);
    let result = collect_selected(&root.0, Some(26300), |_| Ok(&old.profile));
    assert_eq!(
        result.report.profile_id.as_deref(),
        Some(old.profile.id.as_str())
    );
    assert_eq!(result.report.modules[0].status, ModuleStatus::Matched);
    assert_eq!(result.report.modules[1].status, ModuleStatus::UnknownImage);
    assert_eq!(result.report.modules[2].status, ModuleStatus::Matched);
    assert!(
        result
            .report
            .features
            .iter()
            .all(|feature| feature.status == FeatureStatus::UnsupportedModules)
    );
    assert!(result.host_layout(true, false).is_err());
    assert!(result.host_layout(false, true).is_err());
}

#[test]
fn mixing_tray_versions_blocks_only_features_that_need_the_tray() {
    let old = Fixture::new(23);
    let new = Fixture::new(24);
    let root = SystemRoot::new([&old.images[0], &old.images[1], &new.images[2]]);
    let result = collect_selected(&root.0, Some(26300), |_| Ok(&old.profile));
    assert!(!result.report.all_profile_images_matched);
    assert_eq!(result.report.modules[2].status, ModuleStatus::UnknownImage);
    assert_eq!(result.host_layout(true, false).unwrap(), old.profile.host);
    assert!(result.host_layout(false, true).is_err());
    assert!(result.host_layout(true, true).is_err());
}

#[test]
fn correct_module_hash_does_not_skip_layout_verification() {
    let mut fixture = Fixture::new(25);
    fixture.profile.host.symbols[2].prologue[0] ^= 1;
    fixture.profile.validate().unwrap();
    let root = SystemRoot::new(fixture.images.each_ref().map(Vec::as_slice));
    let result = collect_selected(&root.0, Some(26300), |_| Ok(&fixture.profile));
    assert_eq!(
        result.report.profile_id.as_deref(),
        Some(fixture.profile.id.as_str())
    );
    assert_eq!(result.report.modules[0].status, ModuleStatus::InvalidImage);
    assert!(
        result
            .report
            .features
            .iter()
            .all(|feature| feature.status == FeatureStatus::InspectionFailed)
    );
    assert!(result.host_layout(true, true).is_err());
    assert!(result.host_layout(false, false).is_err());
}

#[test]
fn support_catalog_keeps_recorded_scope_separate_from_built_in_support() {
    let catalog = support_catalog().unwrap();
    let profiles = compatibility::profiles().unwrap();
    assert_eq!(catalog.len(), profiles.len());
    for profile in profiles {
        let entry = catalog
            .iter()
            .find(|entry| entry.profile_id == profile.id)
            .unwrap();
        assert_eq!(entry.architecture, "x64");
        assert_eq!(
            serde_json::to_value(&entry.modules).unwrap(),
            serde_json::to_value(profile.modules()).unwrap()
        );
        assert_eq!(entry.evidence.len(), 7);
        assert!(entry.evidence.iter().all(|evidence| {
            !evidence.scope.is_empty()
                && !evidence.detail.is_empty()
                && (evidence.status == ValidationStatus::NotRun
                    || (evidence.recorded_on.is_some() && evidence.reference.is_some()))
        }));
        for evidence in &entry.evidence {
            let expected = if profile.id == "windows11-26300.9711-x64" {
                if matches!(
                    evidence.scenario,
                    ValidationScenario::PointerInteractions | ValidationScenario::MixedDpi
                ) {
                    ValidationStatus::NotRun
                } else {
                    ValidationStatus::Historical
                }
            } else if matches!(
                profile.id.as_str(),
                "windows11-26300.9818-x64"
                    | "windows11-26200.8457-x64"
                    | "windows11-26100.1742-x64"
            ) && matches!(
                evidence.scenario,
                ValidationScenario::ModuleLayout
                    | ValidationScenario::ClockUpdateRestore
                    | ValidationScenario::TrayPrepareStop
            ) {
                ValidationStatus::Tested
            } else {
                ValidationStatus::NotRun
            };
            assert_eq!(
                evidence.status, expected,
                "{} {:?}",
                profile.id, evidence.scenario
            );
        }
    }
    let historical = catalog
        .iter()
        .find(|entry| entry.profile_id == "windows11-26300.9711-x64")
        .unwrap();
    assert!(
        historical
            .evidence
            .iter()
            .all(|evidence| evidence.status != ValidationStatus::Tested)
    );
    let sandbox = catalog
        .iter()
        .find(|entry| entry.profile_id == "windows11-26300.9818-x64")
        .unwrap();
    for scenario in [
        ValidationScenario::TrayMirroring,
        ValidationScenario::PointerInteractions,
        ValidationScenario::MultiMonitor,
        ValidationScenario::MixedDpi,
    ] {
        let evidence = sandbox
            .evidence
            .iter()
            .find(|evidence| evidence.scenario == scenario)
            .unwrap();
        assert_eq!(evidence.status, ValidationStatus::NotRun);
        assert!(evidence.recorded_on.is_none());
    }
    let clock = sandbox
        .evidence
        .iter()
        .find(|evidence| evidence.scenario == ValidationScenario::ClockUpdateRestore)
        .unwrap();
    assert_eq!(clock.status, ValidationStatus::Tested);
    let encoded = serde_json::to_value(sandbox).unwrap();
    assert!(
        encoded["evidence"].as_array().unwrap().iter().any(|item| {
            item["scenario"] == "pointer_interactions" && item["status"] == "not_run"
        })
    );
}

#[test]
fn detection_summary_preserves_disk_limits_and_feature_independence() {
    let old = Fixture::new(27);
    let new = Fixture::new(28);
    let root = SystemRoot::new([&old.images[0], &old.images[1], &new.images[2]]);
    let inventory = collect_selected(&root.0, Some(26300), |_| Ok(&old.profile));
    let report = &inventory.report;
    let before = serde_json::to_value(report).unwrap();
    let summary = report.summary();
    assert_eq!(summary.profile_id, Some(old.profile.id.as_str()));
    assert_eq!(summary.matched_module_count, 2);
    assert_eq!(summary.required_module_count, 3);
    assert!(!summary.all_profile_images_matched);
    assert!(!summary.loaded_modules_checked && !summary.runtime_topology_checked);
    assert_eq!(summary.scope, "disk_only");
    assert_eq!(summary.os_build, Some(26300));
    assert_eq!(
        summary.features[0].status,
        FeatureStatus::DiskMatchedLiveCheckRequired
    );
    assert_eq!(
        summary.features[2].status,
        FeatureStatus::UnsupportedModules
    );
    assert_eq!(
        serde_json::to_value(summary).unwrap()["features"],
        before["features"]
    );
    assert_eq!(serde_json::to_value(report).unwrap(), before);
    assert!(inventory.host_layout(true, false).is_ok());
    assert!(inventory.host_layout(false, true).is_err());

    let unknown = collect(&root.0, Some(26300)).unwrap();
    let summary = serde_json::to_value(unknown.summary()).unwrap();
    assert!(summary["profile_id"].is_null());
    assert!(summary["profile_selection_reason"].is_string());
    assert_eq!(summary["matched_module_count"], 0);
    assert_eq!(summary["all_profile_images_matched"], false);
}
