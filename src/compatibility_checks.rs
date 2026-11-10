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

fn recognize_from<'a>(
    profiles: &[&'a Profile],
    role: Role,
    path: &str,
    bytes: &[u8],
) -> Result<Recognized<'a>, String> {
    let image = pe::identity(bytes)?;
    let identity = identity(&image);
    let index = match role {
        Role::Taskbar => 0,
        Role::Shell => 1,
        Role::Tray => 2,
    };
    let mut matching = profiles.iter().filter(|profile| {
        let module = profile.modules()[index];
        module.relative_path == path && module.matches(&image.sha256, &identity)
    });
    let evidence = *matching.next().ok_or("unrecognized fixture")?;
    if matching.next().is_some() {
        return Err("ambiguous fixture".into());
    }
    Ok(Recognized {
        role,
        module: evidence.modules()[index].clone(),
        evidence,
        basis: MatchBasis::ExactImage,
    })
}

#[test]
fn disk_preflight_binds_each_module_to_its_own_bytes_and_layout() {
    for fixture in [Fixture::new(17), Fixture::new(18)] {
        let root = SystemRoot::new(fixture.images.each_ref().map(Vec::as_slice));
        let result = collect_selected(&root.0, Some(26300), |role, path, bytes| {
            recognize_from(&[&fixture.profile], role, path, bytes)
        });
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
        assert!(
            result
                .report
                .implementation
                .as_ref()
                .unwrap()
                .tray_abi
                .is_some()
        );
    }
}

#[test]
fn unknown_and_ambiguous_modules_never_choose_a_default() {
    let fixture = Fixture::new(19);
    let root = SystemRoot::new(fixture.images.each_ref().map(Vec::as_slice));
    let result = collect(&root.0, Some(26300)).unwrap();
    assert_eq!(result.profile_id, None);
    assert!(result.implementation.is_none());
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
    let ambiguous = collect_selected(&root.0, Some(26300), |role, path, bytes| {
        recognize_from(&[&fixture.profile, &fixture.profile], role, path, bytes)
    });
    assert_eq!(ambiguous.report.profile_id, None);
    assert_eq!(
        ambiguous.report.modules[0].reason.as_deref(),
        Some("ambiguous fixture")
    );
    assert!(ambiguous.host_layout(true, true).is_err());
    assert!(ambiguous.host_layout(false, false).is_err());
}

#[test]
fn known_taskbar_identity_with_changed_code_keeps_other_roles_recognized_but_cannot_activate() {
    let mut fixture = Fixture::new(20);
    fixture.images[0][0x500] ^= 1;
    assert_eq!(
        identity(&pe::identity(&fixture.images[0]).unwrap()),
        fixture.profile.taskbar.identity
    );
    let root = SystemRoot::new(fixture.images.each_ref().map(Vec::as_slice));
    let result = collect_selected(&root.0, Some(26300), |role, path, bytes| {
        recognize_from(&[&fixture.profile], role, path, bytes)
    });
    assert_eq!(result.report.profile_id, None);
    assert!(!result.report.all_profile_images_matched);
    assert_eq!(result.report.modules[0].status, ModuleStatus::UnknownImage);
    assert_eq!(result.report.modules[1].status, ModuleStatus::Matched);
    assert_eq!(result.report.modules[2].status, ModuleStatus::Matched);
    assert!(result.host_layout(true, false).is_err());
    assert!(result.host_layout(false, true).is_err());
}

#[test]
fn recognition_must_bind_full_current_identity_and_hash() {
    let fixture = Fixture::new(26);
    let root = SystemRoot::new(fixture.images.each_ref().map(Vec::as_slice));
    let result = collect_selected(&root.0, Some(26300), |role, path, bytes| {
        let mut recognized = recognize_from(&[&fixture.profile], role, path, bytes)?;
        recognized.module.identity.timestamp ^= 1;
        Ok(recognized)
    });
    assert!(
        result
            .report
            .modules
            .iter()
            .all(|module| module.status == ModuleStatus::InvalidImage)
    );
    assert!(result.host_layout(true, true).is_err());
}

#[test]
fn compatible_module_contracts_combine_only_their_own_fields_without_inheriting_acceptance() {
    let mut old = Fixture::new(21);
    let mut new = Fixture::new(22);
    old.profile.source.host_child_offset = 0x18;
    old.profile.source.child_source_offset = 0x10;
    new.profile.source.host_child_offset = 0x38;
    new.profile.source.child_source_offset = 0x20;
    new.profile.source.child_vtable_rva = 0x20c0;
    let root = SystemRoot::new([&old.images[0], &new.images[1], &old.images[2]]);
    let result = collect_selected(&root.0, Some(26300), |role, path, bytes| {
        recognize_from(&[&old.profile, &new.profile], role, path, bytes)
    });
    assert!(
        result
            .report
            .modules
            .iter()
            .all(|module| module.status == ModuleStatus::Matched)
    );
    assert_eq!(result.report.profile_id, None);
    assert!(!result.report.all_profile_images_matched);
    assert_eq!(result.host_layout(true, true).unwrap(), old.profile.host);
    let plan = result.resolved.unwrap();
    assert_eq!(plan.source.host_child_offset, 0x18);
    assert_eq!(plan.source.child_source_offset, 0x20);
    assert_eq!(plan.source.child_vtable_rva, 0x20c0);
    assert_eq!(plan.shell.sha256, new.profile.shell.sha256);
    assert_eq!(plan.tray.unwrap().module.sha256, old.profile.tray.sha256);
}

#[test]
fn unknown_tray_degrades_only_requested_tray_and_never_enables_an_unrequested_clock() {
    let old = Fixture::new(23);
    let new = Fixture::new(24);
    let root = SystemRoot::new([&old.images[0], &old.images[1], &new.images[2]]);
    let result = collect_selected(&root.0, Some(26300), |role, path, bytes| {
        recognize_from(&[&old.profile], role, path, bytes)
    });
    assert!(!result.report.all_profile_images_matched);
    assert_eq!(result.report.modules[2].status, ModuleStatus::UnknownImage);
    assert_eq!(result.host_layout(true, false).unwrap(), old.profile.host);
    assert!(result.host_layout(false, true).is_err());
    assert!(result.host_layout(true, true).is_err());
    assert_eq!(
        result.report.negotiate(true, false).unwrap(),
        Capabilities {
            clocks: true,
            tray: false,
            notice: None
        }
    );
    let partial = result.report.negotiate(true, true).unwrap();
    assert!(partial.clocks && !partial.tray);
    assert!(partial.notice.unwrap().contains("托盘复制暂未启用"));
    assert!(result.report.negotiate(false, true).is_err());
    assert!(result.report.negotiate(false, false).is_err());
}

#[test]
fn correct_module_hash_does_not_skip_layout_verification() {
    let mut fixture = Fixture::new(25);
    fixture.profile.host.symbols[2].prologue[0] ^= 1;
    fixture.profile.validate().unwrap();
    let root = SystemRoot::new(fixture.images.each_ref().map(Vec::as_slice));
    let result = collect_selected(&root.0, Some(26300), |role, path, bytes| {
        recognize_from(&[&fixture.profile], role, path, bytes)
    });
    assert_eq!(result.report.profile_id, None);
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
                    | "windows11-26200.8037-x64"
                    | "windows11-26100.1742-x64"
                    | "windows11-22621.1702-x64"
                    | "windows11-22631.2861-x64"
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
fn older_windows11_records_keep_their_exact_single_monitor_lab_scope() {
    let catalog = support_catalog().unwrap();
    for (id, system) in [
        ("windows11-22621.1702-x64", "22H2 22621.1702"),
        ("windows11-22631.2861-x64", "23H2 22631.2861"),
    ] {
        let profile = catalog.iter().find(|entry| entry.profile_id == id).unwrap();
        let tested: Vec<_> = profile
            .evidence
            .iter()
            .filter(|item| item.status == ValidationStatus::Tested)
            .map(|item| item.scenario)
            .collect();
        assert_eq!(
            tested,
            [
                ValidationScenario::ModuleLayout,
                ValidationScenario::ClockUpdateRestore,
                ValidationScenario::TrayPrepareStop,
            ]
        );
        for item in &profile.evidence {
            assert_eq!(item.reference, Some("docs/compatibility-22h2-23h2.md"));
            assert_eq!(
                item.recorded_on,
                (item.status == ValidationStatus::Tested).then_some("2026-11-09")
            );
        }
        let clock = profile
            .evidence
            .iter()
            .find(|item| item.scenario == ValidationScenario::ClockUpdateRestore)
            .unwrap();
        assert!(clock.scope.contains(system));
        assert!(
            clock
                .scope
                .contains("单屏虚拟机，冻结测试包、普通权限测试进程")
        );
        assert!(clock.detail.contains("240→320→320 DIP"));
        assert!(clock.detail.contains("原宽度 56 DIP"));
    }
}

#[test]
fn detection_summary_preserves_disk_limits_and_feature_independence() {
    let old = Fixture::new(27);
    let new = Fixture::new(28);
    let root = SystemRoot::new([&old.images[0], &old.images[1], &new.images[2]]);
    let inventory = collect_selected(&root.0, Some(26300), |role, path, bytes| {
        recognize_from(&[&old.profile], role, path, bytes)
    });
    let report = &inventory.report;
    let before = serde_json::to_value(report).unwrap();
    let summary = report.summary();
    assert_eq!(summary.profile_id, None);
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
