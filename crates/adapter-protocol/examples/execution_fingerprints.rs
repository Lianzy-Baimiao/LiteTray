//! Generate or verify built-in fingerprints using locally retained Microsoft
//! module images. No network, PDB parsing, process access or system writes.
use litetray_adapter_protocol::{
    compatibility::{MatchBasis, Role, execution_fingerprint, profiles, recognize},
    pe,
};
use serde_json::json;
use std::{collections::BTreeMap, path::PathBuf};

fn main() -> Result<(), String> {
    let mut roots = vec![];
    let mut exact_only = vec![];
    let mut verify = false;
    for arg in std::env::args().skip(1) {
        if arg == "--verify" {
            verify = true;
        } else if let Some(profile) = arg.strip_prefix("--exact-only=") {
            exact_only.push(profile.to_string());
        } else {
            roots.push(PathBuf::from(arg));
        }
    }
    if roots.is_empty() {
        return Err("Pass Windows module roots and explicitly name any --exact-only=<profile-id> without retained bytes".into());
    }
    let registry = profiles()?;
    if exact_only
        .iter()
        .any(|id| !registry.iter().any(|p| &p.id == id))
    {
        return Err("Unknown exact-only profile".into());
    }
    let paths = registry
        .iter()
        .flat_map(|p| {
            p.modules()
                .into_iter()
                .map(|module| module.relative_path.clone())
        })
        .collect::<std::collections::BTreeSet<_>>();
    let mut images = BTreeMap::new();
    for root in roots {
        for path in &paths {
            let file = root.join(path);
            let Ok(bytes) = std::fs::read(&file) else {
                continue;
            };
            let identity = pe::identity(&bytes)?;
            images
                .entry(identity.sha256.clone())
                .or_insert((file, identity, bytes));
        }
    }
    let mut entries = vec![];
    let mut checked = 0;
    let mut changed_sections = 0;
    for profile in registry {
        for role in Role::ALL {
            let Some(module) = role.module(profile) else {
                continue;
            };
            let Some((path, identity, bytes)) = images.get(&module.sha256) else {
                if !exact_only.contains(&profile.id) {
                    return Err(format!(
                        "Missing original bytes: {} {:?} {}",
                        profile.id, role, module.sha256
                    ));
                }
                entries.push(json!({
                    "profile_id": profile.id, "role": role, "sha256": module.sha256,
                    "fingerprint": null,
                    "unavailable_reason": "Original PE bytes were not retained; exact-image recognition only."
                }));
                continue;
            };
            if module.identity.machine != identity.machine
                || module.identity.timestamp != identity.timestamp
                || module.identity.image_size != identity.image_size
                || module.identity.pdb_guid != identity.pdb_guid
                || module.identity.pdb_age != identity.pdb_age
            {
                return Err(format!("Baseline identity mismatch: {}", path.display()));
            }
            let fingerprint =
                execution_fingerprint(bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            entries.push(json!({
                "profile_id": profile.id, "role": role, "sha256": module.sha256,
                "fingerprint": fingerprint
            }));
            if verify {
                let exact = recognize(role, &module.relative_path, bytes)?;
                if exact.basis != MatchBasis::ExactImage {
                    return Err("Exact image did not match exactly".into());
                }
                let pe_offset = u32::from_le_bytes(bytes[0x3c..0x40].try_into().unwrap()) as usize;
                let mut metadata = bytes.clone();
                metadata[pe_offset + 8] ^= 1;
                let unknown = recognize(role, &module.relative_path, &metadata)?;
                if unknown.basis != MatchBasis::ExecutionEquivalent
                    || unknown.module.sha256 == module.sha256
                    || unknown.module.identity.timestamp == module.identity.timestamp
                {
                    return Err("Metadata-only image did not return its current identity".into());
                }
                for section in pe::sections(bytes)? {
                    if section.raw_size == 0 {
                        continue;
                    }
                    let mut mutated = bytes.clone();
                    // Test the end of every file-backed section, away from the
                    // deliberately ignored metadata. Padding also remains hashed.
                    let offset = (section.raw_offset + section.raw_size - 1) as usize;
                    mutated[offset] ^= 0x80;
                    if execution_fingerprint(&mutated).is_ok_and(|v| v == fingerprint)
                        || recognize(role, &module.relative_path, &mutated).is_ok()
                    {
                        return Err(format!(
                            "Changed section accepted: {} {offset:x}",
                            path.display()
                        ));
                    }
                    changed_sections += 1;
                }
                checked += 1;
            }
        }
    }
    if verify {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema": 1, "real_module_images": checked,
                "metadata_only_unknown_images_accepted": checked,
                "changed_sections_rejected": changed_sections,
                "exact_only_profiles_without_original_bytes": exact_only,
                "live_activation_performed": false
            }))
            .map_err(|e| e.to_string())?
        );
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema": 1, "algorithm": "litetray-execution-equivalence-v1", "entries": entries
            }))
            .map_err(|e| e.to_string())?
        );
    }
    Ok(())
}
