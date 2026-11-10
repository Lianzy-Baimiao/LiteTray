//! Recognize each module independently. A fallback accepts only unchanged
//! implementations with narrowly defined metadata changes, never new code/RVAs.
//! Both the host and the injected adapter run this same parser on their own
//! disk reads. No caller-provided profile or OS version chooses a recipe.
use super::{
    Module, NotificationAreaLayout, Profile, SourceLayout, Topology, TrayAbi, TrayEntry,
    TrayGeometry, profiles,
};
use crate::{HostLayout, ModuleIdentity, pe};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

mod fingerprint;
pub use fingerprint::execution_fingerprint;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Taskbar,
    Shell,
    Tray,
}
impl Role {
    pub const ALL: [Self; 3] = [Self::Taskbar, Self::Shell, Self::Tray];
    pub fn module(self, profile: &Profile) -> Option<&Module> {
        match self {
            Self::Taskbar => Some(&profile.taskbar),
            Self::Shell => Some(&profile.shell),
            Self::Tray => profile.tray.as_ref(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchBasis {
    ExactImage,
    ExecutionEquivalent,
}

#[derive(Debug, Clone)]
pub struct Recognized<'a> {
    pub role: Role,
    pub module: Module,
    pub evidence: &'a Profile,
    pub basis: MatchBasis,
}

#[derive(Debug, Clone)]
pub struct ResolvedTray {
    pub module: Module,
    pub abi: TrayAbi,
    pub geometry: TrayGeometry,
    pub entries: [TrayEntry; 3],
}

#[derive(Debug, Clone)]
pub struct Resolved {
    /// Identifies these current module hashes; it is not a historical test ID.
    pub id: String,
    pub taskbar: Module,
    pub shell: Module,
    pub tray: Option<ResolvedTray>,
    pub host: HostLayout,
    pub source: SourceLayout,
    pub notification_area: Option<NotificationAreaLayout>,
    pub topology: Topology,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FingerprintEntry {
    profile_id: String,
    role: Role,
    sha256: String,
    fingerprint: Option<String>,
    #[serde(default)]
    unavailable_reason: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fingerprints {
    schema: u32,
    algorithm: String,
    entries: Vec<FingerprintEntry>,
}
fn fingerprints() -> Result<&'static [FingerprintEntry], String> {
    static VALUES: OnceLock<Result<Fingerprints, String>> = OnceLock::new();
    VALUES
        .get_or_init(|| {
            let data: Fingerprints = serde_json::from_slice(include_bytes!(
                "../../profiles/execution-fingerprints.json"
            ))
            .map_err(|e| format!("实现指纹清单无效：{e}"))?;
            #[cfg(feature = "compatibility-lab")]
            let data = {
                let laboratory: Fingerprints = serde_json::from_slice(include_bytes!(
                    "../../profiles/lab/execution-fingerprints.json"
                ))
                .map_err(|e| format!("实验实现指纹清单无效：{e}"))?;
                if laboratory.schema != data.schema || laboratory.algorithm != data.algorithm {
                    return Err("实验实现指纹清单版本不一致".into());
                }
                let mut combined = data;
                combined.entries.extend(laboratory.entries);
                combined
            };
            let registry = profiles()?;
            if data.schema != 1
                || data.algorithm != fingerprint::ALGORITHM
                || data.entries.len() != registry.iter().map(|p| p.modules().len()).sum::<usize>()
            {
                return Err("实现指纹清单版本或数量无效".into());
            }
            for profile in registry {
                for role in Role::ALL {
                    let Some(module) = role.module(profile) else {
                        continue;
                    };
                    let entries = data
                        .entries
                        .iter()
                        .filter(|e| e.profile_id == profile.id && e.role == role)
                        .collect::<Vec<_>>();
                    if entries.len() != 1
                        || entries[0].sha256 != module.sha256
                        || match &entries[0].fingerprint {
                            Some(value) => {
                                value.len() != 64
                                    || !value
                                        .bytes()
                                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                                    || entries[0].unavailable_reason.is_some()
                            }
                            None => entries[0]
                                .unavailable_reason
                                .as_ref()
                                .is_none_or(|v| v.is_empty()),
                        }
                    {
                        return Err("实现指纹未绑定唯一内置参考模块".into());
                    }
                }
            }
            Ok(data)
        })
        .as_ref()
        .map(|v| v.entries.as_slice())
        .map_err(Clone::clone)
}

fn same_recipe(role: Role, a: &Profile, b: &Profile) -> bool {
    if a.topology != b.topology {
        return false;
    }
    match role {
        Role::Taskbar => {
            a.host.symbols == b.host.symbols
                && a.host.element_offset == b.host.element_offset
                && a.source.recipe == b.source.recipe
                && a.source.host_child_offset == b.source.host_child_offset
                && a.notification_area == b.notification_area
        }
        Role::Shell => {
            a.source.recipe == b.source.recipe
                && a.source.interface_contract() == b.source.interface_contract()
                && a.source.child_source_offset == b.source.child_source_offset
                && a.source.child_vtable_rva == b.source.child_vtable_rva
                && a.source.factory_vtable_rva == b.source.factory_vtable_rva
                && a.source.child_vtable_entries == b.source.child_vtable_entries
        }
        Role::Tray => {
            a.tray_abi == b.tray_abi
                && a.tray_geometry == b.tray_geometry
                && serde_json::to_value(&a.tray_entries).ok()
                    == serde_json::to_value(&b.tray_entries).ok()
        }
    }
}

/// Match the whole module first, then its unchanged execution/data image. The
/// returned module always carries the actual file hash and PE/PDB identity.
pub fn recognize(
    role: Role,
    relative_path: &str,
    bytes: &[u8],
) -> Result<Recognized<'static>, String> {
    let image = pe::identity(bytes)?;
    let identity = ModuleIdentity {
        machine: image.machine,
        timestamp: image.timestamp,
        image_size: image.image_size,
        pdb_guid: image.pdb_guid,
        pdb_age: image.pdb_age,
    };
    identity.validate()?;
    let path = relative_path.replace('\\', "/");
    let candidates = profiles()?
        .iter()
        .filter(|profile| {
            role.module(profile)
                .is_some_and(|module| module.relative_path.eq_ignore_ascii_case(&path))
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err("模块路径不属于已实现的模块角色".into());
    }
    let mut matches = candidates
        .iter()
        .copied()
        .filter(|profile| {
            role.module(profile)
                .is_some_and(|module| module.matches(&image.sha256, &identity))
        })
        .collect::<Vec<_>>();
    let basis = if matches.is_empty() {
        let digest = execution_fingerprint(bytes)?;
        let fingerprints = fingerprints()?;
        matches = candidates
            .into_iter()
            .filter(|profile| {
                fingerprints.iter().any(|entry| {
                    entry.profile_id == profile.id
                        && entry.role == role
                        && entry.fingerprint.as_deref() == Some(digest.as_str())
                })
            })
            .collect();
        MatchBasis::ExecutionEquivalent
    } else {
        MatchBasis::ExactImage
    };
    let evidence = *matches
        .first()
        .ok_or("模块的执行代码、数据或布局尚未识别")?;
    if matches
        .iter()
        .any(|other| !same_recipe(role, evidence, other))
    {
        return Err("相同模块特征对应冲突的解析方案".into());
    }
    let reference = role.module(evidence).ok_or("该资料不包含所需模块角色")?;
    Ok(Recognized {
        role,
        module: Module {
            name: reference.name.clone(),
            relative_path: reference.relative_path.clone(),
            sha256: image.sha256,
            identity,
        },
        evidence,
        basis,
    })
}

/// Combine independently proven module contracts. The taskbar owns the field
/// pointing to the child; the shell owns that child's vtable and source field.
/// Runtime type, root, HWND ownership and loaded-image checks remain mandatory.
pub fn assemble(
    taskbar: &Recognized<'_>,
    shell: &Recognized<'_>,
    tray: Option<&Recognized<'_>>,
) -> Result<Resolved, String> {
    if taskbar.role != Role::Taskbar
        || shell.role != Role::Shell
        || tray.is_some_and(|v| v.role != Role::Tray)
        || taskbar.evidence.source.recipe != shell.evidence.source.recipe
        || taskbar.evidence.topology != shell.evidence.topology
        || tray.is_some_and(|v| v.evidence.topology != taskbar.evidence.topology)
        || (taskbar.evidence.notification_area.is_some() && tray.is_some())
    {
        return Err("模块角色或 XAML 宿主接口契约不一致".into());
    }
    for recognized in [Some(taskbar), Some(shell), tray].into_iter().flatten() {
        recognized.evidence.validate()?;
        recognized.module.validate()?;
        let reference = recognized
            .role
            .module(recognized.evidence)
            .ok_or("该资料不包含所需模块角色")?;
        if recognized.module.name != reference.name
            || recognized.module.relative_path != reference.relative_path
        {
            return Err("模块角色与解析证据路径不一致".into());
        }
    }
    let mut host = taskbar.evidence.host.clone();
    host.module = taskbar.module.identity.clone();
    host.validate()?;
    let notification_area = taskbar.evidence.notification_area.clone();
    if let Some(notification) = &notification_area {
        notification.validate(taskbar.module.identity.image_size)?;
    }
    let source = SourceLayout {
        recipe: taskbar.evidence.source.recipe,
        child_interface: Some(shell.evidence.source.interface_contract()),
        host_child_offset: taskbar.evidence.source.host_child_offset,
        child_source_offset: shell.evidence.source.child_source_offset,
        child_vtable_rva: shell.evidence.source.child_vtable_rva,
        factory_vtable_rva: shell.evidence.source.factory_vtable_rva,
        child_vtable_entries: shell.evidence.source.child_vtable_entries.clone(),
    };
    let mut key = Sha256::new();
    key.update(b"litetray-module-combination-v1\0");
    for recognized in [Some(taskbar), Some(shell), tray] {
        key.update(
            recognized
                .map_or("absent", |v| v.module.sha256.as_str())
                .as_bytes(),
        );
        key.update([0]);
    }
    let tray = tray
        .map(|recognized| {
            let evidence = recognized
                .evidence
                .tray_evidence()?
                .ok_or("时钟资料不包含可调用的托盘方案")?;
            Ok::<_, String>(ResolvedTray {
                module: recognized.module.clone(),
                abi: evidence.abi,
                geometry: evidence.geometry,
                entries: evidence.entries.clone(),
            })
        })
        .transpose()?;
    Ok(Resolved {
        id: format!("modules-{:x}", key.finalize()),
        taskbar: taskbar.module.clone(),
        shell: shell.module.clone(),
        tray,
        host,
        source,
        notification_area,
        topology: taskbar.evidence.topology,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known(role: Role, profile: &Profile) -> Recognized<'_> {
        Recognized {
            role,
            module: role.module(profile).unwrap().clone(),
            evidence: profile,
            basis: MatchBasis::ExactImage,
        }
    }

    #[test]
    fn child_iid_belongs_to_shell_and_conflicting_shell_contracts_are_distinct() {
        use super::super::ChildInterface;
        let early = Profile::decode(include_bytes!(
            "../../profiles/windows11-22000.318-x64-clock.json"
        ))
        .unwrap();
        let mut explicit_early = early.clone();
        explicit_early.source.child_interface = Some(ChildInterface::XamlExplorerHostChildEef8V1);
        assert!(same_recipe(Role::Shell, &early, &explicit_early));
        let mut late = early.clone();
        late.source.child_interface = Some(ChildInterface::XamlExplorerHostChildEc97V1);
        assert!(same_recipe(Role::Taskbar, &early, &late));
        assert!(!same_recipe(Role::Shell, &early, &late));
        let resolved = assemble(
            &known(Role::Taskbar, &early),
            &known(Role::Shell, &late),
            None,
        )
        .unwrap();
        assert_eq!(resolved.source.child_interface, late.source.child_interface);
        assert_eq!(
            resolved.source.interface_contract(),
            ChildInterface::XamlExplorerHostChildEc97V1
        );
        let resolved = assemble(
            &known(Role::Taskbar, &late),
            &known(Role::Shell, &early),
            None,
        )
        .unwrap();
        assert_eq!(
            resolved.source.child_interface,
            Some(ChildInterface::XamlExplorerHostChildEef8V1)
        );
    }

    #[test]
    fn notification_route_is_owned_only_by_the_taskbar_recipe() {
        let notification = Profile::decode(include_bytes!(
            "../../profiles/windows11-22000.318-x64-clock.json"
        ))
        .unwrap();
        let mut previous = notification.clone();
        previous.notification_area = None;
        assert!(!same_recipe(Role::Taskbar, &notification, &previous));
        assert!(same_recipe(Role::Shell, &notification, &previous));
        let taskbar = known(Role::Taskbar, &notification);
        let previous_shell = known(Role::Shell, &previous);
        let resolved = assemble(&taskbar, &previous_shell, None).unwrap();
        assert_eq!(resolved.notification_area, notification.notification_area);
        assert_eq!(resolved.source.host_child_offset, 0x20);
        let previous_taskbar = known(Role::Taskbar, &previous);
        let notification_shell = known(Role::Shell, &notification);
        assert!(
            assemble(&previous_taskbar, &notification_shell, None)
                .unwrap()
                .notification_area
                .is_none()
        );
        let modern = &profiles().unwrap()[0];
        assert!(assemble(&taskbar, &known(Role::Shell, modern), None).is_err());
        assert!(assemble(&taskbar, &previous_shell, Some(&known(Role::Tray, modern))).is_err());

        for mutate in 0..3 {
            let mut changed = notification.clone();
            let layout = changed.notification_area.as_mut().unwrap();
            match mutate {
                0 => layout.parent_offset += 8,
                1 => layout.copy_host.prologue[0] ^= 1,
                _ => layout.control_entries[2].rva += 1,
            }
            changed.validate().unwrap();
            assert!(!same_recipe(Role::Taskbar, &notification, &changed));
        }
    }

    #[test]
    fn notification_layout_is_checked_against_the_current_taskbar_image() {
        let mut profile = Profile::decode(include_bytes!(
            "../../profiles/windows11-22000.318-x64-clock.json"
        ))
        .unwrap();
        profile.notification_area.as_mut().unwrap().copy_host.rva =
            profile.taskbar.identity.image_size - 16;
        profile.validate().unwrap();
        let mut taskbar = known(Role::Taskbar, &profile);
        let shell = known(Role::Shell, &profile);
        // Keep all host sections in range while truncating a notification entry.
        taskbar.module.identity.image_size -= 4096;
        assert!(assemble(&taskbar, &shell, None).is_err());
    }

    #[test]
    fn clock_only_recipe_cannot_be_promoted_into_tray_support() {
        let original = &profiles().unwrap()[0];
        let mut clock = original.clone();
        clock.tray = None;
        clock.tray_abi = None;
        clock.tray_geometry = None;
        clock.tray_entries = None;
        clock.validate().unwrap();
        assert!(Role::Tray.module(&clock).is_none());
        let host = known(Role::Taskbar, &clock);
        let shell = known(Role::Shell, &clock);
        let resolved = assemble(&host, &shell, None).unwrap();
        assert_eq!(resolved.host, clock.host);
        assert!(resolved.tray.is_none());
        let fabricated = Recognized {
            role: Role::Tray,
            module: original.tray.as_ref().unwrap().clone(),
            evidence: &clock,
            basis: MatchBasis::ExactImage,
        };
        assert!(assemble(&host, &shell, Some(&fabricated)).is_err());
    }

    #[test]
    fn every_registered_module_has_an_explicit_fingerprint_or_exact_only_reason() {
        let entries = fingerprints().unwrap();
        assert_eq!(
            entries.len(),
            profiles()
                .unwrap()
                .iter()
                .map(|p| p.modules().len())
                .sum::<usize>()
        );
        assert_eq!(
            entries.iter().filter(|e| e.fingerprint.is_some()).count(),
            20
        );
        assert!(entries.iter().filter(|e| e.fingerprint.is_none()).all(|e| {
            (e.profile_id == "windows11-26300.9711-x64"
                || cfg!(feature = "compatibility-lab")
                    && e.profile_id == "windows11-28000.3365-x64-notification")
                && e.unavailable_reason.as_ref().is_some_and(|v| !v.is_empty())
        }));
    }

    #[test]
    fn composition_keeps_fields_with_the_module_that_defines_them() {
        let registry = profiles().unwrap();
        let old = registry
            .iter()
            .find(|p| p.id == "windows11-22621.1702-x64")
            .unwrap();
        let new = registry
            .iter()
            .find(|p| p.id == "windows11-26300.9818-x64")
            .unwrap();
        let taskbar = known(Role::Taskbar, new);
        let shell = known(Role::Shell, old);
        let tray = known(Role::Tray, old);
        let resolved = assemble(&taskbar, &shell, Some(&tray)).unwrap();
        assert_eq!(resolved.host, new.host);
        assert_eq!(
            resolved.source.host_child_offset,
            new.source.host_child_offset
        );
        assert_eq!(
            resolved.source.child_source_offset,
            old.source.child_source_offset
        );
        assert_eq!(
            resolved.source.child_vtable_rva,
            old.source.child_vtable_rva
        );
        assert_eq!(
            resolved.source.factory_vtable_rva,
            old.source.factory_vtable_rva
        );
        assert_eq!(Some(resolved.tray.as_ref().unwrap().abi), old.tray_abi);
        assert_ne!(resolved.id, new.id);
        assert_ne!(resolved.id, old.id);
        let clock_only = assemble(&taskbar, &shell, None).unwrap();
        assert!(clock_only.tray.is_none());
        assert_eq!(
            clock_only.source.child_vtable_rva,
            resolved.source.child_vtable_rva
        );
        assert_ne!(clock_only.id, resolved.id);
        assert!(assemble(&shell, &taskbar, Some(&tray)).is_err());
        assert!(assemble(&taskbar, &shell, Some(&taskbar)).is_err());
    }

    #[test]
    fn resolved_layout_uses_current_identity_instead_of_reference_identity() {
        let profile = &profiles().unwrap()[0];
        let mut taskbar = known(Role::Taskbar, profile);
        let shell = known(Role::Shell, profile);
        taskbar.module.identity.timestamp ^= 1;
        taskbar.module.identity.pdb_guid[0] ^= 1;
        taskbar.module.sha256 = "a".repeat(64);
        taskbar.basis = MatchBasis::ExecutionEquivalent;
        let resolved = assemble(&taskbar, &shell, None).unwrap();
        assert_eq!(resolved.host.module, taskbar.module.identity);
        assert_ne!(resolved.host.module, profile.taskbar.identity);
        assert_eq!(resolved.host.symbols, profile.host.symbols);
        assert_eq!(resolved.taskbar.sha256, taskbar.module.sha256);
    }
}
