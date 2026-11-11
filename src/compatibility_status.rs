//! Read-only presentation of exact built-in profiles and recorded acceptance.
//! None of these descriptions or historical results authorize activation.
use super::{FeatureReport, ModuleStatus, Report};
use litetray_adapter_protocol::compatibility::{
    Module, Profile, SourceRecipe, Topology, TrayAbi, TrayGeometry, profiles,
};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationScenario {
    ModuleLayout,
    ClockUpdateRestore,
    TrayPrepareStop,
    TrayMirroring,
    PointerInteractions,
    MultiMonitor,
    MixedDpi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationStatus {
    /// A recorded run in the stated environment, not the current session.
    Tested,
    /// Retained evidence from an earlier package/system; not a fresh rerun.
    Historical,
    /// No completed acceptance of the stated scope is recorded.
    NotRun,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationEvidence {
    pub scenario: ValidationScenario,
    pub status: ValidationStatus,
    pub scope: &'static str,
    pub detail: &'static str,
    pub recorded_on: Option<&'static str>,
    pub reference: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImplementationFamily {
    pub source_recipe: SourceRecipe,
    pub topology: Topology,
    pub tray_abi: Option<TrayAbi>,
    pub tray_geometry: Option<litetray_adapter_protocol::compatibility::TrayGeometry>,
    pub description: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct SupportProfile {
    pub profile_id: String,
    pub system_label: String,
    pub architecture: &'static str,
    pub family: ImplementationFamily,
    /// Exact required modules. No OS build interval is a matching rule.
    pub modules: Vec<Module>,
    pub evidence: Vec<ValidationEvidence>,
    /// Original per-scenario records, before applying a newer reviewed run.
    pub historical_evidence: Vec<ValidationEvidence>,
    /// Exact tested package/run for this recipe; never current-session proof.
    pub recorded_acceptance: Option<&'static RecordedAcceptance>,
    /// Matching runs in recording order, including the latest run above.
    /// A newer partial run does not erase acceptance of other scenarios.
    pub recorded_acceptances: Vec<&'static RecordedAcceptance>,
}

/// A compact projection of a disk report, deliberately excluding acceptance
/// history. UI may show that history beside this result but cannot upgrade it.
#[derive(Debug, Serialize)]
pub struct DetectionSummary<'a> {
    pub scope: &'static str,
    pub os_build: Option<u32>,
    pub collector_architecture: &'static str,
    pub profile_id: Option<&'a str>,
    pub profile_selection_reason: Option<&'a str>,
    pub all_profile_images_matched: bool,
    pub matched_module_count: usize,
    pub required_module_count: usize,
    pub loaded_modules_checked: bool,
    pub runtime_topology_checked: bool,
    pub features: &'a [FeatureReport],
}

impl Report {
    pub fn summary(&self) -> DetectionSummary<'_> {
        DetectionSummary {
            scope: self.scope,
            os_build: self.os_build,
            collector_architecture: self.collector_architecture,
            profile_id: self.profile_id.as_deref(),
            profile_selection_reason: self.profile_selection_reason.as_deref(),
            all_profile_images_matched: self.all_profile_images_matched,
            matched_module_count: self
                .modules
                .iter()
                .filter(|module| module.status == ModuleStatus::Matched)
                .count(),
            required_module_count: self
                .profile_id
                .as_ref()
                .and_then(|id| profiles().ok()?.iter().find(|profile| &profile.id == id))
                .map_or(self.modules.len(), |profile| profile.modules().len()),
            loaded_modules_checked: self.loaded_modules_checked,
            runtime_topology_checked: self.runtime_topology_checked,
            features: &self.features,
        }
    }
}

const SCENARIOS: [(ValidationScenario, &str); 7] = [
    (ValidationScenario::ModuleLayout, "精确模块身份、符号与布局"),
    (
        ValidationScenario::ClockUpdateRestore,
        "真实任务栏时钟更新与停止恢复",
    ),
    (
        ValidationScenario::TrayPrepareStop,
        "托盘区域与坐标入口准备、停止",
    ),
    (
        ValidationScenario::TrayMirroring,
        "副屏托盘模型复制与停止恢复",
    ),
    (
        ValidationScenario::PointerInteractions,
        "日历点击、滚轮音量和托盘菜单/悬停完整矩阵",
    ),
    (
        ValidationScenario::MultiMonitor,
        "真实主副屏时钟、拓扑与托盘几何",
    ),
    (
        ValidationScenario::MixedDpi,
        "多屏混合 DPI 与显示器动态变化",
    ),
];

#[derive(Clone, Copy)]
enum RecordedProfile {
    Previous9711,
    Sandbox9818,
    VirtualMachine8457,
    VirtualMachine8037,
    VirtualMachineLtsc1742,
    VirtualMachine22H21702,
    VirtualMachine23H22861,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoricalRecipe {
    profile_id: String,
    host_source_digest: String,
    tray_digest: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoricalRecipes {
    schema: u32,
    algorithm: String,
    source_commit: String,
    entries: Vec<HistoricalRecipe>,
}
fn historical_recipes() -> Result<&'static [HistoricalRecipe], String> {
    static RECIPES: std::sync::OnceLock<Result<HistoricalRecipes, String>> =
        std::sync::OnceLock::new();
    RECIPES
        .get_or_init(|| {
            let recipes: HistoricalRecipes =
                serde_json::from_str(include_str!("compatibility_historical_recipes.json"))
                    .map_err(|error| error.to_string())?;
            let hex = |value: &str, length| {
                value.len() == length
                    && value
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            };
            let mut ids = std::collections::BTreeSet::new();
            if recipes.schema != 1
                || recipes.algorithm != "litetray-historical-recipe-v1"
                || !hex(&recipes.source_commit, 40)
                || recipes.entries.len() != 7
                || recipes.entries.iter().any(|entry| {
                    entry.profile_id.is_empty()
                        || !ids.insert(&entry.profile_id)
                        || !hex(&entry.host_source_digest, 64)
                        || !hex(&entry.tray_digest, 64)
                })
            {
                return Err("历史验收方案绑定无效".into());
            }
            Ok(recipes)
        })
        .as_ref()
        .map(|recipes| recipes.entries.as_slice())
        .map_err(Clone::clone)
}
fn recipe_digests(profile: &Profile) -> (String, String) {
    use sha2::{Digest, Sha256};
    let digest = |value: serde_json::Value| {
        let mut digest = Sha256::new();
        digest.update(b"litetray-historical-recipe-v1\0");
        // serde_json's default maps sort keys recursively, matching the saved
        // digest generator. Arrays retain ABI/entry order.
        digest.update(serde_json::to_vec(&value).expect("finite recipe fields"));
        format!("{:x}", digest.finalize())
    };
    (
        digest(serde_json::json!({
            "taskbar": profile.taskbar, "shell": profile.shell,
            "host": profile.host, "source": profile.source, "topology": profile.topology,
        })),
        digest(serde_json::json!({
            "tray": profile.tray, "tray_abi": profile.tray_abi, "tray_entries": profile.tray_entries,
        })),
    )
}
fn recorded_profile(profile: &Profile) -> Option<RecordedProfile> {
    // This only selects a historical record. Each scenario below separately
    // verifies its complete historical module identities and implementation.
    match profile.id.as_str() {
        "windows11-26300.9711-x64" => Some(RecordedProfile::Previous9711),
        "windows11-26300.9818-x64" => Some(RecordedProfile::Sandbox9818),
        "windows11-26200.8457-x64" => Some(RecordedProfile::VirtualMachine8457),
        "windows11-26200.8037-x64" => Some(RecordedProfile::VirtualMachine8037),
        "windows11-26100.1742-x64" => Some(RecordedProfile::VirtualMachineLtsc1742),
        "windows11-22621.1702-x64" => Some(RecordedProfile::VirtualMachine22H21702),
        "windows11-22631.2861-x64" => Some(RecordedProfile::VirtualMachine23H22861),
        _ => None,
    }
}

fn historical_evidence(profile: &Profile) -> Vec<ValidationEvidence> {
    let record = recorded_profile(profile);
    let digests = recipe_digests(profile);
    let previous = historical_recipes()
        .ok()
        .and_then(|entries| entries.iter().find(|entry| entry.profile_id == profile.id));
    let host_matches = previous.is_some_and(|entry| entry.host_source_digest == digests.0);
    // The old digest predates purpose classification. Bind that independently
    // without rewriting its saved source commit or any of the seven digests.
    // Old LTSC used Copilot-only geometry, which no supported enum represents.
    let historical_geometry = match record {
        Some(RecordedProfile::Previous9711) => Some(TrayGeometry::LegacyUnclassified),
        Some(
            RecordedProfile::Sandbox9818
            | RecordedProfile::VirtualMachine8457
            | RecordedProfile::VirtualMachine8037,
        ) => Some(TrayGeometry::NotificationAreaBounds),
        Some(RecordedProfile::VirtualMachine22H21702 | RecordedProfile::VirtualMachine23H22861) => {
            Some(TrayGeometry::DragDropHitTesting)
        }
        Some(RecordedProfile::VirtualMachineLtsc1742) | None => None,
    };
    let full_matches = host_matches
        && historical_geometry == profile.tray_geometry
        && previous.is_some_and(|entry| entry.tray_digest == digests.1);
    SCENARIOS
        .into_iter()
        .map(|(scenario, scope)| {
            let mut evidence = ValidationEvidence {
                scenario,
                status: ValidationStatus::NotRun,
                scope,
                detail: "尚无此场景的完整验收记录；模块匹配不代表实测通过。",
                recorded_on: None,
                reference: None,
            };
            match record {
                Some(RecordedProfile::Previous9711) => {
                    evidence.reference = Some("docs/live-clock-acceptance.md");
                    evidence.detail = match scenario {
                        ValidationScenario::ModuleLayout => {
                            evidence.reference = Some("docs/compatibility.md");
                            "保留 9711 三模块身份、匹配符号与布局证据；当前包未在旧系统重新实测。"
                        }
                        ValidationScenario::ClockUpdateRestore => {
                            "历史主副屏时钟更新、自动宽度及停止恢复通过；当前包未在旧系统重新实测。"
                        }
                        ValidationScenario::TrayPrepareStop => {
                            "历史副屏托盘会话准备、启动和停止通过；当前包未在旧系统重新实测。"
                        }
                        ValidationScenario::TrayMirroring => {
                            "历史副屏图标模型、顺序、宽度、对齐及停止恢复通过，不覆盖全部菜单和悬停。"
                        }
                        ValidationScenario::MultiMonitor => {
                            "历史双屏 source、时钟、拓扑和托盘几何通过；不代表混合 DPI 与热插拔通过。"
                        }
                        ValidationScenario::PointerInteractions => {
                            "完整输入矩阵未验收；通知点击问题和跨屏菜单仍有限制，局部反馈不替代完整验收。"
                        }
                        ValidationScenario::MixedDpi => {
                            "混合 DPI、显示器热插拔和休眠恢复尚未完成现场矩阵。"
                        }
                    };
                    if !matches!(
                        scenario,
                        ValidationScenario::PointerInteractions | ValidationScenario::MixedDpi
                    ) {
                        evidence.status = ValidationStatus::Historical;
                        evidence.recorded_on = Some("2026-11-08");
                    }
                }
                Some(RecordedProfile::Sandbox9818) => {
                    evidence.reference = Some("docs/compatibility-9818.md");
                    evidence.detail = match scenario {
                        ValidationScenario::ModuleLayout => {
                            "9818 三模块/PDB、字段、虚表及 ABI 重新核验；Sandbox 磁盘匹配、XAML 164 项和私有宿主 6 项通过。"
                        }
                        ValidationScenario::ClockUpdateRestore => {
                            evidence.scope = "单显示器 Sandbox 的真实主屏时钟（来宾 26100.9818，目标模块与 26300.9818 相同）";
                            "真实文字读回及宽度 240→320 通过；停止后原文字、宽度和可见性恢复，Explorer 身份不变。未独立验收字体、颜色或绑定。"
                        }
                        ValidationScenario::TrayPrepareStop => {
                            "5 类必需托盘区域与三个坐标入口准备、停止通过；未激活托盘复制。"
                        }
                        ValidationScenario::TrayMirroring => {
                            "本轮没有副屏副本，未激活托盘复制会话；入口准备不能代替模型复制和交互验收。"
                        }
                        ValidationScenario::PointerInteractions => {
                            "本轮未注入鼠标或键盘；日历点击、滚轮、中键、真实悬停和菜单定位待验收。"
                        }
                        ValidationScenario::MultiMonitor => {
                            "本轮 Sandbox 只有一块显示器，主副屏完整矩阵未执行。"
                        }
                        ValidationScenario::MixedDpi => {
                            "本轮未执行混合 DPI、显示器热插拔和休眠恢复矩阵。"
                        }
                    };
                    if matches!(
                        scenario,
                        ValidationScenario::ModuleLayout
                            | ValidationScenario::ClockUpdateRestore
                            | ValidationScenario::TrayPrepareStop
                    ) {
                        evidence.status = ValidationStatus::Tested;
                        evidence.recorded_on = Some("2026-11-08");
                    }
                }
                Some(RecordedProfile::VirtualMachine8457) => {
                    evidence.reference = Some("docs/compatibility-8457.md");
                    evidence.detail = match scenario {
                        ValidationScenario::ModuleLayout => {
                            "25H2 三模块/PDB、字段、虚表及 ABI 已核验；独立客体磁盘匹配、XAML 164 项和私有宿主 6 项通过。"
                        }
                        ValidationScenario::ClockUpdateRestore => {
                            evidence.scope = "独立 Windows 11 Pro 25H2 26200.8457 单屏虚拟机，普通权限测试进程";
                            "三次真实文字更新及宽度 240→320 通过；停止后原文字、宽度 55 和可见性恢复，Explorer 身份不变。未独立验收字体、颜色或绑定。"
                        }
                        ValidationScenario::TrayPrepareStop => {
                            "Taskbar.View.dll 的三个坐标入口与 5 类必需托盘区域准备、停止通过；未激活复制。"
                        }
                        ValidationScenario::TrayMirroring => {
                            "本轮虚拟机只有主屏，未激活副屏托盘模型复制；准备通过不代表复制及交互验收。"
                        }
                        ValidationScenario::PointerInteractions => {
                            "本轮验收未注入鼠标或键盘；日历点击、滚轮、中键、真实悬停和菜单定位待验收。"
                        }
                        ValidationScenario::MultiMonitor => {
                            "本轮独立虚拟机只有一块显示器，主副屏完整矩阵未执行。"
                        }
                        ValidationScenario::MixedDpi => {
                            "本轮未执行混合 DPI、显示器热插拔和休眠恢复矩阵。"
                        }
                    };
                    if matches!(
                        scenario,
                        ValidationScenario::ModuleLayout
                            | ValidationScenario::ClockUpdateRestore
                            | ValidationScenario::TrayPrepareStop
                    ) {
                        evidence.status = ValidationStatus::Tested;
                        evidence.recorded_on = Some("2026-11-09");
                    }
                }
                Some(RecordedProfile::VirtualMachine8037) => {
                    evidence.reference = Some("docs/compatibility-next-versions.md");
                    evidence.detail = match scenario {
                        ValidationScenario::ModuleLayout => {
                            "25H2 8037 三模块/PDB、字段、虚表及 ABI 已核验；独立客体磁盘与加载身份匹配，XAML 164 项和私有宿主 6 项通过。"
                        }
                        ValidationScenario::ClockUpdateRestore => {
                            evidence.scope = "独立 Windows 11 Pro 25H2 26200.8037 单屏虚拟机，普通权限测试进程";
                            "三次真实文字更新及宽度 240→320 通过；停止后恢复原文字、尺寸和可见性，Explorer 身份不变。外层等待超时后，原测试进程完整结果经独立复核通过；未独立验收屏幕绘制、原生刷新、字体、颜色或绑定。"
                        }
                        ValidationScenario::TrayPrepareStop => {
                            "Taskbar.View.dll 的三个坐标入口与 5 类必需托盘区域准备、停止通过；未激活复制。"
                        }
                        ValidationScenario::TrayMirroring => {
                            "本轮虚拟机只有主屏，未激活副屏托盘模型复制；准备通过不代表复制及交互验收。"
                        }
                        ValidationScenario::PointerInteractions => {
                            "本轮验收未注入鼠标或键盘；日历点击、滚轮、中键、真实悬停和菜单定位待验收。"
                        }
                        ValidationScenario::MultiMonitor => {
                            "本轮独立虚拟机只有一块显示器，主副屏完整矩阵未执行。"
                        }
                        ValidationScenario::MixedDpi => {
                            "本轮未执行混合 DPI、显示器热插拔和休眠恢复矩阵。"
                        }
                    };
                    if matches!(
                        scenario,
                        ValidationScenario::ModuleLayout
                            | ValidationScenario::ClockUpdateRestore
                            | ValidationScenario::TrayPrepareStop
                    ) {
                        evidence.status = ValidationStatus::Tested;
                        evidence.recorded_on = Some("2026-11-09");
                    }
                }
                Some(RecordedProfile::VirtualMachineLtsc1742) => {
                    evidence.reference = Some("docs/compatibility-next-versions.md");
                    evidence.detail = match scenario {
                        ValidationScenario::ModuleLayout => {
                            "24H2 代 LTSC 三模块/PDB、字段、虚表及 ABI 已核验；独立客体磁盘与加载身份匹配，XAML 164 项和私有宿主 6 项通过。"
                        }
                        ValidationScenario::ClockUpdateRestore => {
                            evidence.scope = "独立 Windows 11 Enterprise LTSC 2024 Evaluation 26100.1742 单屏虚拟机，普通权限测试进程";
                            "三次真实文字更新及宽度 240→320 通过；停止后恢复原文字、尺寸和可见性，Explorer 身份不变。首次读取响应曾失败，后续同包复测通过；未独立验收屏幕绘制、原生刷新、字体、颜色或绑定。"
                        }
                        ValidationScenario::TrayPrepareStop => {
                            "Taskbar.View.dll 的三个坐标入口与 5 类必需托盘区域准备、停止通过；未激活复制。"
                        }
                        ValidationScenario::TrayMirroring => {
                            "本轮虚拟机只有主屏，未激活副屏托盘模型复制；准备通过不代表复制及交互验收。"
                        }
                        ValidationScenario::PointerInteractions => {
                            "本轮验收未注入鼠标或键盘；日历点击、滚轮、中键、真实悬停和菜单定位待验收。"
                        }
                        ValidationScenario::MultiMonitor => {
                            "本轮独立虚拟机只有一块显示器，主副屏完整矩阵未执行。"
                        }
                        ValidationScenario::MixedDpi => {
                            "本轮未执行混合 DPI、显示器热插拔和休眠恢复矩阵。"
                        }
                    };
                    if matches!(
                        scenario,
                        ValidationScenario::ModuleLayout
                            | ValidationScenario::ClockUpdateRestore
                            | ValidationScenario::TrayPrepareStop
                    ) {
                        evidence.status = ValidationStatus::Tested;
                        evidence.recorded_on = Some("2026-11-09");
                    }
                }
                Some(
                    RecordedProfile::VirtualMachine22H21702
                    | RecordedProfile::VirtualMachine23H22861,
                ) => {
                    evidence.reference = Some("docs/compatibility-22h2-23h2.md");
                    let is_22h2 = matches!(record, Some(RecordedProfile::VirtualMachine22H21702));
                    evidence.detail = match scenario {
                        ValidationScenario::ModuleLayout => {
                            if is_22h2 {
                                "22H2 1702 三模块/PDB、字段、虚表及 ABI 已核验；冻结测试包在独立客体中磁盘与加载身份匹配，XAML 164 项和私有宿主 6 项通过。"
                            } else {
                                "23H2 2861 三模块/PDB、字段、虚表及 ABI 已核验；冻结测试包在独立客体中磁盘与加载身份匹配，XAML 164 项和私有宿主 6 项通过。"
                            }
                        }
                        ValidationScenario::ClockUpdateRestore => {
                            evidence.scope = if is_22h2 {
                                "独立 Windows 11 Pro 22H2 22621.1702 单屏虚拟机，冻结测试包、普通权限测试进程"
                            } else {
                                "独立 Windows 11 Pro 23H2 22631.2861 单屏虚拟机，冻结测试包、普通权限测试进程"
                            };
                            "三次真实文字读回及宽度 240→320→320 DIP 通过；时钟停止和最终清理后的时间、日期、容器原始对象逐字段恢复（含 NUL 和方向标记），原宽度 56 DIP 恢复，Explorer 身份不变。未独立验收屏幕绘制、原生刷新、字体、颜色或绑定。"
                        }
                        ValidationScenario::TrayPrepareStop => {
                            "Taskbar.View.dll 的三个坐标入口与 5 类必需托盘区域准备、停止通过；未激活托盘会话或模型复制。"
                        }
                        ValidationScenario::TrayMirroring => {
                            "本轮虚拟机只有主屏，未激活副屏托盘模型复制；准备通过不代表复制及交互验收。"
                        }
                        ValidationScenario::PointerInteractions => {
                            "本轮验收未注入鼠标或键盘；日历点击、滚轮、中键、真实悬停和菜单定位待验收。"
                        }
                        ValidationScenario::MultiMonitor => {
                            "本轮独立虚拟机只有一块显示器，主副屏完整矩阵未执行。"
                        }
                        ValidationScenario::MixedDpi => {
                            "本轮未执行混合 DPI、显示器热插拔和休眠恢复矩阵。"
                        }
                    };
                    if matches!(
                        scenario,
                        ValidationScenario::ModuleLayout
                            | ValidationScenario::ClockUpdateRestore
                            | ValidationScenario::TrayPrepareStop
                    ) {
                        evidence.status = ValidationStatus::Tested;
                        evidence.recorded_on = Some("2026-11-09");
                    }
                }
                None => {}
            }
            let recipe_matches = if scenario == ValidationScenario::ClockUpdateRestore {
                host_matches
            } else {
                full_matches
            };
            if !recipe_matches {
                evidence.status = ValidationStatus::NotRun;
                evidence.recorded_on = None;
                evidence.detail = "历史验收使用的模块、字段、虚表或坐标入口方案与当前实现不同；旧记录不能证明当前场景通过。";
                // Keep the old document as context only when the old host is
                // identical and just the tray implementation was replaced.
                if !host_matches { evidence.reference = None; }
                if host_matches && matches!(record, Some(RecordedProfile::VirtualMachineLtsc1742)) {
                    evidence.detail = "旧包 LTSC 记录使用传值坐标入口；当前改为已核验的借用拖放入口，旧入口的准备/停止记录不适用于当前实现。当前场景待新包独立验收。";
                }
            } else if scenario == ValidationScenario::ClockUpdateRestore
                && matches!(record, Some(RecordedProfile::VirtualMachineLtsc1742))
            {
                evidence.detail = "保留旧包单屏普通权限的时钟三次文字更新、宽度 240→320 及停止恢复记录；其 taskbar/source 模块与完整布局和当前相同。该记录只覆盖时钟，不验证当前已替换的借用托盘入口，也不是当前包复测结果。";
            }
            evidence
        })
        .collect()
}

/// One reviewed run, tied to its tested package and exact implementation.
/// This describes retained evidence; it never authorizes the current process.
#[derive(Debug, Serialize)]
pub struct RecordedAcceptance {
    pub profile_id: &'static str,
    pub recipe_sha256: &'static str,
    pub package_version: &'static str,
    pub package_build_id: &'static str,
    /// None for a frozen development package that was never published.
    pub published_commit: Option<&'static str>,
    pub package_manifest_sha256: &'static str,
    pub executable_sha256: &'static str,
    pub adapter_sha256: &'static str,
    pub popup_sha256: &'static str,
    pub run_id: &'static str,
    pub nonce: &'static str,
    pub archive_sha256: &'static str,
    pub environment_scope: &'static str,
    pub recorded_on: &'static str,
    pub reference: &'static str,
    pub scenarios: &'static [AcceptedScenario],
}

#[derive(Debug, Serialize)]
pub struct AcceptedScenario {
    pub scenario: ValidationScenario,
    pub detail: &'static str,
}

// Only independently reviewed completed runs belong here. This is an evidence
// table, not an OS-version support list. Each scope names its observed edition.
// Append newer runs; retain earlier package identities and their scope.
const RECORDED_ACCEPTANCES: &[RecordedAcceptance] = &[
    RecordedAcceptance {
        profile_id: "windows11-26200.8037-x64",
        recipe_sha256: "c550f7cc9f60f1d6d32897de0402a122c6449325b88380bf471921c24c3223bb",
        package_version: "5.15.1",
        package_build_id: "5.15.1-8e688e0f1c01",
        published_commit: Some("1417f38b8d031ded3db2734fd11e82517f419d38"),
        package_manifest_sha256: "7de3103f1f05859ecf6806e4f8dd192c9189a51f049480f0c02a5a705859a1f1",
        executable_sha256: "837ebebd89c192dff94397565c1498c1007c3a5ae814eb23164a25dcf652e62a",
        adapter_sha256: "ac68fa6dbb6c9cdc0d3a4691aaf30033109a2178ad0e92603c16b84fd0db5b62",
        popup_sha256: "f6171c7f4164965a50b56a985198367691703150037c714d8f9336f83992d659",
        run_id: "sandbox-20261109-202125-52113436",
        nonce: "6d6528a03d864927811e5083c49dba60",
        archive_sha256: "7f8e1323704e2131c95ac57287b919961cc1eb4641db5f986a159cdd2dd1702b",
        environment_scope: "已发布 LiteTray 5.15.1 指定哈希包；独立 Windows 11 Pro 25H2 26200.8037 单屏 WHPX 虚拟机，Medium 普通权限。该记录不是当前程序或其他 edition 的现场复测。",
        recorded_on: "2026-11-09",
        reference: "docs/compatibility-completion-5.15.1.md",
        scenarios: &[
            AcceptedScenario {
                scenario: ValidationScenario::ModuleLayout,
                detail: "六项原生命令和五项仅磁盘副本识别检查通过；实际加载模块匹配、XAML 168 项、私有宿主六项通过。磁盘实现等价与拒绝场景不替代未知模块的现场运行验收。",
            },
            AcceptedScenario {
                scenario: ValidationScenario::ClockUpdateRestore,
                detail: "三次真实文字及宽度 55→240→320→320→55 读回通过；停止和最终清理恢复全部原始时钟字段，原 Explorer、线程与窗口不变。未验收像素、字体、颜色、绑定或持续原生刷新。",
            },
            AcceptedScenario {
                scenario: ValidationScenario::TrayPrepareStop,
                detail: "五类必需托盘区域、当前三个坐标入口的准备与停止通过；未激活托盘复制、菜单或悬停。此前同包 TCG 启动通信失败仍单独保留，WHPX 通过不证明其根因已修复。",
            },
        ],
    },
    RecordedAcceptance {
        profile_id: "windows11-26100.1742-x64",
        recipe_sha256: "28ca9cfd9ba1f805cc3dd1e7c116a81770d497c1c424a742cc856c3739a76e1d",
        package_version: "5.15.1",
        package_build_id: "5.15.1-8e688e0f1c01",
        published_commit: Some("1417f38b8d031ded3db2734fd11e82517f419d38"),
        package_manifest_sha256: "7de3103f1f05859ecf6806e4f8dd192c9189a51f049480f0c02a5a705859a1f1",
        executable_sha256: "837ebebd89c192dff94397565c1498c1007c3a5ae814eb23164a25dcf652e62a",
        adapter_sha256: "ac68fa6dbb6c9cdc0d3a4691aaf30033109a2178ad0e92603c16b84fd0db5b62",
        popup_sha256: "f6171c7f4164965a50b56a985198367691703150037c714d8f9336f83992d659",
        run_id: "sandbox-20261109-205423-3b6aa4b5",
        nonce: "912941652b7f4bcfac0cb32a1389a07f",
        archive_sha256: "ad82c8b3d0bb2d0a9575d7a55490184187a60f8818e372df42b5ed07497a82ba",
        environment_scope: "已发布 LiteTray 5.15.1 指定哈希包；独立 Windows 11 Pro 24H2 26100.1742 单屏 WHPX 虚拟机，Medium 普通权限。实测 edition 为 Professional；LTSC 保留独立旧记录，此结果不是其他 edition 或当前程序的现场复测。",
        recorded_on: "2026-11-09",
        reference: "docs/compatibility-completion-5.15.1.md",
        scenarios: &[
            AcceptedScenario {
                scenario: ValidationScenario::ModuleLayout,
                detail: "六项原生命令和五项仅磁盘副本识别检查通过；实际加载模块匹配、XAML 168 项、私有宿主六项通过。磁盘实现等价与拒绝场景不替代未知模块的现场运行验收。",
            },
            AcceptedScenario {
                scenario: ValidationScenario::ClockUpdateRestore,
                detail: "三次真实文字及宽度 56→240→320→320→56 读回通过；停止和最终清理恢复全部原始时钟字段，原 Explorer、线程与窗口不变。未验收像素、字体、颜色、绑定或持续原生刷新。",
            },
            AcceptedScenario {
                scenario: ValidationScenario::TrayPrepareStop,
                detail: "普通 Pro 24H2 的五类必需托盘区域、当前借用拖放坐标入口准备与停止通过；未激活托盘复制、菜单或悬停。这是新包在 Professional 客体的结果，不沿用旧 LTSC 的传值 Copilot 入口验收。",
            },
        ],
    },
    RecordedAcceptance {
        profile_id: "windows11-22000.318-x64-clock",
        recipe_sha256: "a1f32a2f93d24aee31559764c607297f36490b0b8b873c0a8843380207a0e70a",
        package_version: "5.15.2",
        package_build_id: "lab-package-r6-combined",
        published_commit: None,
        package_manifest_sha256: "c426a98962ac1828549dfe2c02a5a93699cba4c6ebdc7cd86662b736b586551f",
        executable_sha256: "8726580eb110873d76d5cb1a1839d21210aa21db965dc90627ac7955a6bdef06",
        adapter_sha256: "061d36559359862adb91fef60887475e9281678a747dcac001f25c6141cf7f3a",
        popup_sha256: "b89532bf19ecb25f703031d0a99ecfc51449d8a76b7a543d0d67afc3a8bdd3a5",
        run_id: "sandbox-20261110-161111-f88fd698",
        nonce: "888f7f898eff4128a7a61b31dd5e3f70",
        archive_sha256: "f19492341ac90596eca6583a766733c875907c283e85bd34942598d9af7002c0",
        environment_scope: "基于 5.15.2 开发工作区冻结的 lab-package-r6-combined，未发布实验包；独立 Windows 11 Pro 21H2 22000.318 x64 单屏 TCG 虚拟机，Medium 普通权限。只验收主屏时钟，不代表公开 5.15.2、当前程序或其他模块的现场结果。",
        recorded_on: "2026-11-10",
        reference: "docs/compatibility-21h2.md",
        scenarios: &[
            AcceptedScenario {
                scenario: ValidationScenario::ModuleLayout,
                detail: "实际加载的宿主/source、独立通知区窗口归属与 XAML 结构检查通过；按真实单个 DateTimeInnerTextBlock 识别合并时钟，通知铃铛不属于文字容器。此资料不提供托盘坐标入口。",
            },
            AcceptedScenario {
                scenario: ValidationScenario::ClockUpdateRestore,
                detail: "同一文本控件三次完整双行文字及宽度 63→240→320→320→63 读回通过；停止及最终清理恢复原始文字（含方向标记）、布局、宽度、可见性和高度，原 Explorer 保持不变。产品正常退出，随后独立只读检查确认产品和测试进程已结束。未验收持续原生走时、屏幕绘制、字体、颜色、绑定或输入。",
            },
        ],
    },
    RecordedAcceptance {
        profile_id: "windows11-22000.318-x64-clock",
        recipe_sha256: "a1f32a2f93d24aee31559764c607297f36490b0b8b873c0a8843380207a0e70a",
        package_version: "5.16.0",
        package_build_id: "5.16.0-af2ca67da2fd",
        published_commit: Some("0761ab7d642176693eb33c548dfd1643f703ad31"),
        package_manifest_sha256: "40989be9ef3eafc6d528224f6f37e6638f930463478b6678c185a616bdad145a",
        executable_sha256: "477946e5d7403b337b0b19b9177ee7c256f85fc244b228d8c0cfffa0af7d72b9",
        adapter_sha256: "16a2cc8ffcff944806797926992b66bb18469fa644071061a6a86154f0b15509",
        popup_sha256: "cf29dad7037d8eb4de2e2b2474632b62618fd47c5f0b54a9c40c4dd8bed84145",
        run_id: "sandbox-20261110-164157-04be8fb5",
        nonce: "efe8a3ca8fd741cea8d25499651bca74",
        archive_sha256: "ffc4c23741e1dde12de1ea769ca0c7186d6dc0f904398bc7e16f247717423bed",
        environment_scope: "LiteTray 5.16.0 正式二进制、指定哈希的测试包；独立 Windows 11 Pro 21H2 22000.318 x64 单屏 TCG 虚拟机，Medium 普通权限。只验收主屏时钟；最终分发包只补充文档，二进制相同。此记录不是当前会话的现场检查。",
        recorded_on: "2026-11-10",
        reference: "docs/compatibility-release-5.16.0.md",
        scenarios: &[
            AcceptedScenario {
                scenario: ValidationScenario::ModuleLayout,
                detail: "五项只读命令通过；实际加载模块、独立通知区窗口归属及合并时钟结构匹配。发布程序使用默认八组资料；本机型仍仅支持主屏时钟。",
            },
            AcceptedScenario {
                scenario: ValidationScenario::ClockUpdateRestore,
                detail: "合并时间/日期控件三帧文字和宽度 63→240→320→320→63 读回通过；停止后完整原文、节点、布局、可见性和高度恢复。Explorer 保持不变，产品正常退出，工作进程确认退出。未验收持续原生走时、像素绘制或输入。",
            },
        ],
    },
    RecordedAcceptance {
        profile_id: "windows11-26100.1742-x64",
        recipe_sha256: "28ca9cfd9ba1f805cc3dd1e7c116a81770d497c1c424a742cc856c3739a76e1d",
        package_version: "5.16.0",
        package_build_id: "5.16.0-af2ca67da2fd",
        published_commit: Some("0761ab7d642176693eb33c548dfd1643f703ad31"),
        package_manifest_sha256: "40989be9ef3eafc6d528224f6f37e6638f930463478b6678c185a616bdad145a",
        executable_sha256: "477946e5d7403b337b0b19b9177ee7c256f85fc244b228d8c0cfffa0af7d72b9",
        adapter_sha256: "16a2cc8ffcff944806797926992b66bb18469fa644071061a6a86154f0b15509",
        popup_sha256: "cf29dad7037d8eb4de2e2b2474632b62618fd47c5f0b54a9c40c4dd8bed84145",
        run_id: "sandbox-20261110-170638-cdfefae4",
        nonce: "bc7ee9c5858b484b8c3a3933bb03b544",
        archive_sha256: "a9fcfb12ab6312a155c1afa3359da4389455e43e42c11f3f8dd908bebf6d485c",
        environment_scope: "LiteTray 5.16.0 正式二进制、指定哈希的测试包；独立 Windows 11 Pro 24H2 26100.1742 x64 单屏 WHPX 虚拟机，Professional、Medium 普通权限。LTSC 保留独立旧记录；最终分发包只补充文档，二进制相同。此记录不是当前会话的现场检查。",
        recorded_on: "2026-11-10",
        reference: "docs/compatibility-release-5.16.0.md",
        scenarios: &[
            AcceptedScenario {
                scenario: ValidationScenario::ModuleLayout,
                detail: "五项只读命令、三模块/四能力识别和实际分离式时钟检查通过；运行前绑定同一 Explorer、普通权限会话和已冻结程序。",
            },
            AcceptedScenario {
                scenario: ValidationScenario::ClockUpdateRestore,
                detail: "时间/日期各三帧文字及宽度 63→240→320→320→63 读回通过；停止后完整原文（含方向标记和 NUL）、节点、可见性和高度恢复。Explorer 不变，产品及工作进程正常结束。基线原生时间已滞后，本次只证明字段恢复，不证明持续走时、时间准确或像素绘制。",
            },
            AcceptedScenario {
                scenario: ValidationScenario::TrayPrepareStop,
                detail: "五类必需托盘区域及当前坐标入口准备、停止通过，清理收到确认。未激活托盘复制，未验收输入、副屏或混合 DPI。",
            },
        ],
    },
    RecordedAcceptance {
        profile_id: "windows11-26200.8037-x64",
        recipe_sha256: "c550f7cc9f60f1d6d32897de0402a122c6449325b88380bf471921c24c3223bb",
        package_version: "5.16.0",
        package_build_id: "5.16.0-af2ca67da2fd",
        published_commit: Some("0761ab7d642176693eb33c548dfd1643f703ad31"),
        package_manifest_sha256: "40989be9ef3eafc6d528224f6f37e6638f930463478b6678c185a616bdad145a",
        executable_sha256: "477946e5d7403b337b0b19b9177ee7c256f85fc244b228d8c0cfffa0af7d72b9",
        adapter_sha256: "16a2cc8ffcff944806797926992b66bb18469fa644071061a6a86154f0b15509",
        popup_sha256: "cf29dad7037d8eb4de2e2b2474632b62618fd47c5f0b54a9c40c4dd8bed84145",
        run_id: "sandbox-20261110-185055-d3671190",
        nonce: "a8519a76a40c4171a608f907bf1d0844",
        archive_sha256: "9d8858f1b4f4dcb8e3fbe904b1dce804cf5c3b39d7ab685aa7807a4f797c14d9",
        environment_scope: "LiteTray 5.16.0 正式二进制；独立 Windows 11 Pro 25H2 26200.8037 x64 双屏 WHPX 虚拟机，Medium 普通权限。本轮仅补充双屏时钟混合缩放场景，不是当前会话的现场检查。",
        recorded_on: "2026-11-10",
        reference: "docs/compatibility-release-5.16.1.md",
        scenarios: &[AcceptedScenario {
            scenario: ValidationScenario::MixedDpi,
            detail: "通过 Windows 设置将副屏从 100% 改为 125%，实读 DPI 96/96→96/120→96/96；两屏托管文字和 240 DIP 宽度自动恢复，显示布局恢复，正常退出后原生时钟结构恢复并观察到 75 秒跨分钟走时，Explorer 未重启。未测物理插拔、休眠、其他缩放比例或托盘复制。",
        }],
    },
    RecordedAcceptance {
        profile_id: "windows11-28000.1-x64-notification",
        recipe_sha256: "9c78c1468d10688c531a209e6dfccd8a10b6866495d165e0e70152ecb93b883a",
        package_version: "5.16.1",
        package_build_id: "5.16.1-5dc517dab70f",
        published_commit: None,
        package_manifest_sha256: "35afe0bcdff0479a370d4a8368060f72b8b7aee317e07b331a8bfc836b54e50b",
        executable_sha256: "a6537eaa984328d22e9c9109f77e03eda3ca935e46876fa7279411b30b23ce06",
        adapter_sha256: "f5ddb6bae2bc3e2977a66cc43dded0def2a64690db0fe3660821a9df05b4c49b",
        popup_sha256: "b565724ef34551cfd28ca0682c24ffc50306ba33c111fa75bc64da0182481a2b",
        run_id: "sandbox-20261110-224027-782c4b5d",
        nonce: "db768217108544b191e8d99daafef808",
        archive_sha256: "6dc2f59a79dc38834ff88a49e3e5d06aa6a6501c04d1188f3f3ad03af1d76441",
        environment_scope: "基于 5.16.1 开发源码冻结的 compatibility-lab 实验包，未发布；独立 Windows 11 Pro 26H1 28000.1 x64 单屏 WHPX 虚拟机，1280×800、DPI 96、Medium 普通权限。四次主屏日历点击另有通过记录，不代表完整输入矩阵。此记录不是正式 5.16.1、最终默认包、当前会话或 28000.3365 的验收。",
        recorded_on: "2026-11-10",
        reference: "docs/compatibility-26h1.md",
        scenarios: &[
            AcceptedScenario {
                scenario: ValidationScenario::ModuleLayout,
                detail: "前置五项命令及本轮真实主屏事务通过；三份实际加载模块、ec97 子接口、任务栏窗口/线程归属和分离式时钟结构匹配。隔离时钟自测 179 项通过；它不代替 Explorer 内的实际检查。",
            },
            AcceptedScenario {
                scenario: ValidationScenario::ClockUpdateRestore,
                detail: "三帧文字与宽度 63→240→320→320→63 读回通过，停止后完整原文、节点、可见性和尺寸恢复。后台应用正常退出码 0；随后两次原生时间从 10∶41 PM 变为 10∶42 PM，观察相隔 75.294936 秒。原 Explorer、显示布局不变，工作进程确认退出。该观察不等于逐帧绘制或每秒刷新验收。",
            },
            AcceptedScenario {
                scenario: ValidationScenario::TrayPrepareStop,
                detail: "实际通知区域表面与三个坐标入口准备、停止通过，清理收到确认。未激活托盘复制，未验收副屏、菜单、悬停或滚轮音量。",
            },
        ],
    },
];

fn recorded_recipe_digest(profile: &Profile) -> String {
    use sha2::{Digest, Sha256};
    let mut value = serde_json::to_value(profile).expect("finite profile fields");
    // The module label is presentation only. Every other serialized field,
    // including ID, schema, tray geometry and complete modules/entries, binds.
    value
        .as_object_mut()
        .expect("profile is an object")
        .remove("system_label");
    let mut digest = Sha256::new();
    digest.update(b"litetray-recorded-recipe-v1\0");
    digest.update(serde_json::to_vec(&value).expect("finite profile fields"));
    format!("{:x}", digest.finalize())
}

fn recorded_acceptances(profile: &Profile) -> Vec<&'static RecordedAcceptance> {
    let digest = recorded_recipe_digest(profile);
    RECORDED_ACCEPTANCES
        .iter()
        .filter(|record| record.profile_id == profile.id && record.recipe_sha256 == digest)
        .collect()
}

fn recorded_acceptance(profile: &Profile) -> Option<&'static RecordedAcceptance> {
    recorded_acceptances(profile).last().copied()
}

fn evidence(profile: &Profile) -> Vec<ValidationEvidence> {
    let mut results = historical_evidence(profile);
    for record in recorded_acceptances(profile) {
        apply_acceptance(&mut results, record);
    }
    results
}

fn apply_acceptance(results: &mut [ValidationEvidence], record: &RecordedAcceptance) {
    for result in results {
        if let Some(accepted) = record
            .scenarios
            .iter()
            .find(|item| item.scenario == result.scenario)
        {
            *result = ValidationEvidence {
                scenario: accepted.scenario,
                status: ValidationStatus::Tested,
                scope: record.environment_scope,
                detail: accepted.detail,
                recorded_on: Some(record.recorded_on),
                reference: Some(record.reference),
            };
        }
    }
}

fn support_profile(profile: &Profile) -> SupportProfile {
    SupportProfile {
        profile_id: profile.id.clone(),
        system_label: profile.system_label.clone(),
        architecture: if profile.taskbar.identity.machine == 0x8664 {
            "x64"
        } else {
            "unknown"
        },
        family: ImplementationFamily {
            source_recipe: profile.source.recipe,
            topology: profile.topology,
            tray_abi: profile.tray_abi,
            tray_geometry: profile.tray_geometry,
            description: if profile.notification_area.is_some() {
                "这份适配仅提供主屏时钟，暂不支持副屏时钟和托盘复制。启用前会再次确认当前任务栏。"
            } else if profile.tray.is_some() {
                "分别检查时钟和托盘所需的系统模块；启用前再次确认当前任务栏。"
            } else {
                "这份适配资料提供时钟功能，尚未包含托盘复制。启用前会再次确认当前任务栏。"
            },
        },
        modules: profile.modules().into_iter().cloned().collect(),
        evidence: evidence(profile),
        historical_evidence: historical_evidence(profile),
        recorded_acceptance: recorded_acceptance(profile),
        recorded_acceptances: recorded_acceptances(profile),
    }
}

/// Inventory of compiled profiles and their precise, recorded scope. Reading
/// this catalog performs no disk scan, registry access, network or native call.
pub fn support_catalog() -> Result<Vec<SupportProfile>, String> {
    Ok(profiles()?.iter().map(support_profile).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_profiles_and_replaced_images_do_not_inherit_recorded_acceptance() {
        for original in profiles().unwrap() {
            let mut profile = original.clone();
            profile.id = "new-profile-with-existing-layout".into();
            assert!(
                evidence(&profile)
                    .iter()
                    .all(|item| item.status == ValidationStatus::NotRun
                        && item.recorded_on.is_none()
                        && item.reference.is_none())
            );
            for module_index in 0..original.modules().len() {
                let mut profile = original.clone();
                let module = match module_index {
                    0 => &mut profile.taskbar,
                    1 => &mut profile.shell,
                    _ => profile.tray.as_mut().unwrap(),
                };
                module.sha256 = "0".repeat(64);
                assert!(evidence(&profile).iter().all(|item| {
                    if module_index == 2 && item.scenario == ValidationScenario::ClockUpdateRestore
                    {
                        item.status
                            == historical_evidence(original)
                                .into_iter()
                                .find(|previous| previous.scenario == item.scenario)
                                .unwrap()
                                .status
                    } else {
                        item.status == ValidationStatus::NotRun && item.recorded_on.is_none()
                    }
                }));
            }
            for other in profiles()
                .unwrap()
                .iter()
                .filter(|other| other.id != original.id)
            {
                let mut profile = original.clone();
                profile.id.clone_from(&other.id);
                assert!(
                    evidence(&profile)
                        .iter()
                        .all(|item| item.status == ValidationStatus::NotRun
                            && item.recorded_on.is_none()
                            && item.reference.is_none())
                );
            }
        }
    }

    #[test]
    fn mixing_recorded_22h2_and_23h2_modules_does_not_inherit_acceptance() {
        let profiles = profiles().unwrap();
        let recorded = ["windows11-22621.1702-x64", "windows11-22631.2861-x64"]
            .map(|id| profiles.iter().find(|profile| profile.id == id).unwrap());
        for (original, replacement) in [(recorded[0], recorded[1]), (recorded[1], recorded[0])] {
            for module_index in 0..3 {
                let mut mixed = original.clone();
                match module_index {
                    0 => mixed.taskbar = replacement.taskbar.clone(),
                    1 => mixed.shell = replacement.shell.clone(),
                    _ => mixed.tray = replacement.tray.clone(),
                }
                assert!(evidence(&mixed).iter().all(|item| {
                    if module_index == 2 && item.scenario == ValidationScenario::ClockUpdateRestore
                    {
                        item.status == ValidationStatus::Tested
                    } else {
                        item.status == ValidationStatus::NotRun && item.recorded_on.is_none()
                    }
                }));
            }
        }
    }

    #[test]
    fn historical_recipe_digests_match_retained_profiles_except_replaced_ltsc_tray() {
        let retained = historical_recipes().unwrap();
        for previous in retained {
            let profile = profiles()
                .unwrap()
                .iter()
                .find(|profile| profile.id == previous.profile_id)
                .unwrap();
            let current = recipe_digests(profile);
            assert_eq!(
                current.0, previous.host_source_digest,
                "{} host/source",
                profile.id
            );
            assert_eq!(
                current.1 == previous.tray_digest,
                profile.id != "windows11-26100.1742-x64",
                "{} tray",
                profile.id
            );
        }
        let ltsc = profiles()
            .unwrap()
            .iter()
            .find(|profile| profile.id == "windows11-26100.1742-x64")
            .unwrap();
        let records = historical_evidence(ltsc);
        assert_eq!(
            records
                .iter()
                .filter(|item| item.status == ValidationStatus::Tested)
                .count(),
            1
        );
        assert_eq!(
            records
                .iter()
                .filter(|item| item.status == ValidationStatus::NotRun)
                .count(),
            6
        );
        assert_eq!(
            records
                .iter()
                .find(|item| item.status == ValidationStatus::Tested)
                .unwrap()
                .scenario,
            ValidationScenario::ClockUpdateRestore
        );
        for scenario in [
            ValidationScenario::ModuleLayout,
            ValidationScenario::TrayPrepareStop,
        ] {
            let item = records
                .iter()
                .find(|item| item.scenario == scenario)
                .unwrap();
            assert_eq!(item.reference, Some("docs/compatibility-next-versions.md"));
            assert!(item.detail.contains("旧入口"));
            assert!(item.recorded_on.is_none());
        }
    }

    #[test]
    fn changed_host_source_fields_invalidate_every_dependent_scenario() {
        let original = profiles()
            .unwrap()
            .iter()
            .find(|profile| profile.id == "windows11-26200.8037-x64")
            .unwrap();
        assert_eq!(
            evidence(original)
                .iter()
                .filter(|item| item.status == ValidationStatus::Tested)
                .count(),
            4
        );
        for change in 0..5 {
            let mut profile = original.clone();
            match change {
                0 => {
                    profile.host.element_offset = profile.host.element_offset.map(|value| value + 8)
                }
                1 => profile.source.host_child_offset += 8,
                2 => profile.source.child_source_offset += 8,
                3 => profile.source.child_vtable_rva += 8,
                _ => profile.host.symbols[2].prologue[0] ^= 1,
            }
            assert!(
                evidence(&profile)
                    .iter()
                    .all(|item| item.status == ValidationStatus::NotRun
                        && item.recorded_on.is_none()),
                "change {change}"
            );
        }
    }

    #[test]
    fn changed_tray_abi_and_entries_do_not_inherit_tray_acceptance() {
        let original = profiles()
            .unwrap()
            .iter()
            .find(|profile| profile.id == "windows11-26200.8037-x64")
            .unwrap();
        for change in 0..4 {
            let mut profile = original.clone();
            match change {
                0 => profile.tray_abi = Some(TrayAbi::SystemTrayDragDropBorrowedCoordinatesX64V1),
                1 => profile.tray_entries.as_mut().unwrap()[0].rva += 8,
                2 => profile.tray_entries.as_mut().unwrap()[1].prologue[0] ^= 1,
                _ => profile.tray_entries.as_mut().unwrap()[2].rva += 8,
            }
            for item in evidence(&profile) {
                assert_eq!(
                    item.status,
                    if item.scenario == ValidationScenario::ClockUpdateRestore {
                        ValidationStatus::Tested
                    } else {
                        ValidationStatus::NotRun
                    },
                    "change {change} {:?}",
                    item.scenario
                );
            }
        }
    }

    #[test]
    fn notification_acceptance_keeps_lab_history_and_selects_the_published_run() {
        let profile = profiles()
            .unwrap()
            .iter()
            .find(|profile| profile.id == "windows11-22000.318-x64-clock")
            .unwrap();
        let catalog = support_profile(profile);
        let record = catalog.recorded_acceptance.unwrap();
        assert_eq!(record.package_version, "5.16.0");
        assert_eq!(record.package_build_id, "5.16.0-af2ca67da2fd");
        assert_eq!(
            record.published_commit,
            Some("0761ab7d642176693eb33c548dfd1643f703ad31")
        );
        assert_eq!(record.run_id, "sandbox-20261110-164157-04be8fb5");
        assert_eq!(record.nonce, "efe8a3ca8fd741cea8d25499651bca74");
        assert_eq!(record.scenarios.len(), 2);
        assert_eq!(catalog.recorded_acceptances.len(), 2);
        let lab = catalog.recorded_acceptances[0];
        assert_eq!(lab.package_version, "5.15.2");
        assert_eq!(lab.package_build_id, "lab-package-r6-combined");
        assert!(lab.published_commit.is_none());
        assert_eq!(lab.run_id, "sandbox-20261110-161111-f88fd698");
        assert!(lab.environment_scope.contains("未发布实验包"));
        assert!(record.environment_scope.contains("只验收主屏时钟"));
        assert!(catalog.family.description.contains("暂不支持副屏时钟"));
        assert!(
            catalog.historical_evidence.iter().all(|item| {
                item.status == ValidationStatus::NotRun && item.recorded_on.is_none()
            })
        );
        for item in &catalog.evidence {
            let accepted = matches!(
                item.scenario,
                ValidationScenario::ModuleLayout | ValidationScenario::ClockUpdateRestore
            );
            assert_eq!(
                item.status,
                if accepted {
                    ValidationStatus::Tested
                } else {
                    ValidationStatus::NotRun
                }
            );
            if accepted {
                assert_eq!(item.scope, record.environment_scope);
                assert_eq!(item.reference, Some("docs/compatibility-release-5.16.0.md"));
            } else {
                assert!(item.recorded_on.is_none());
                assert!(item.reference.is_none());
            }
        }
        let export = serde_json::to_value(catalog).unwrap();
        assert!(export["recorded_acceptances"][0]["published_commit"].is_null());
        assert_eq!(
            export["recorded_acceptances"][1],
            export["recorded_acceptance"]
        );
        assert_eq!(
            export["recorded_acceptance"]["executable_sha256"],
            "477946e5d7403b337b0b19b9177ee7c256f85fc244b228d8c0cfffa0af7d72b9"
        );
        let published = RECORDED_ACCEPTANCES
            .iter()
            .find(|record| record.profile_id == "windows11-26200.8037-x64")
            .unwrap();
        assert_eq!(
            serde_json::to_value(published).unwrap()["published_commit"],
            "1417f38b8d031ded3db2734fd11e82517f419d38"
        );
    }

    #[test]
    fn notification_acceptance_does_not_survive_a_changed_host_contract() {
        let original = profiles()
            .unwrap()
            .iter()
            .find(|profile| profile.id == "windows11-22000.318-x64-clock")
            .unwrap();
        let mut renamed = original.clone();
        renamed.system_label = "same primary-clock implementation".into();
        assert!(recorded_acceptance(&renamed).is_some());
        for change in 0..7 {
            let mut profile = original.clone();
            if change == 0 {
                profile.notification_area = None;
            } else {
                let notification = profile.notification_area.as_mut().unwrap();
                match change {
                    1 => notification.component_offset += 8,
                    2 => notification.component_vtable_rva += 8,
                    3 => notification.shared_host_offset += 8,
                    4 => notification.child_offset += 8,
                    5 => notification.copy_host.prologue[0] ^= 1,
                    _ => notification.control_entries[2].prologue[0] ^= 1,
                }
            }
            assert!(recorded_acceptance(&profile).is_none(), "change {change}");
            assert!(evidence(&profile).iter().all(|item| {
                item.status == ValidationStatus::NotRun
                    && item.recorded_on.is_none()
                    && item.reference.is_none()
            }));
        }
    }

    #[test]
    fn published_acceptance_preserves_history_and_unrun_scenarios() {
        let profile = profiles()
            .unwrap()
            .iter()
            .find(|profile| profile.id == "windows11-26200.8037-x64")
            .unwrap();
        let catalog = support_profile(profile);
        let record = catalog.recorded_acceptance.unwrap();
        assert_eq!(record.package_version, "5.16.0");
        assert_eq!(
            record.package_manifest_sha256,
            "40989be9ef3eafc6d528224f6f37e6638f930463478b6678c185a616bdad145a"
        );
        assert_eq!(record.run_id, "sandbox-20261110-185055-d3671190");
        assert_eq!(record.scenarios.len(), 1);
        assert_eq!(record.scenarios[0].scenario, ValidationScenario::MixedDpi);
        assert_eq!(catalog.recorded_acceptances.len(), 2);
        assert_eq!(catalog.recorded_acceptances[0].package_version, "5.15.1");
        assert_eq!(
            serde_json::to_value(&catalog.historical_evidence).unwrap(),
            serde_json::to_value(historical_evidence(profile)).unwrap()
        );
        for item in &catalog.evidence {
            let accepted = catalog.recorded_acceptances.iter().rev().find(|run| {
                run.scenarios
                    .iter()
                    .any(|entry| entry.scenario == item.scenario)
            });
            assert_eq!(
                item.status,
                if accepted.is_some() {
                    ValidationStatus::Tested
                } else {
                    ValidationStatus::NotRun
                }
            );
            if let Some(run) = accepted {
                assert_eq!(item.scope, run.environment_scope);
                assert_eq!(item.reference, Some(run.reference));
            }
        }
        let export = serde_json::to_value(&catalog).unwrap();
        assert_eq!(
            export["recorded_acceptance"]["executable_sha256"],
            record.executable_sha256
        );
        let ltsc = profiles()
            .unwrap()
            .iter()
            .find(|profile| profile.id == "windows11-26100.1742-x64")
            .unwrap();
        let pro = support_profile(ltsc);
        let pro_record = pro.recorded_acceptance.unwrap();
        assert_eq!(pro_record.run_id, "sandbox-20261110-170638-cdfefae4");
        assert_eq!(pro_record.package_version, "5.16.0");
        assert_eq!(pro.recorded_acceptances.len(), 2);
        assert_eq!(
            pro.recorded_acceptances[0].run_id,
            "sandbox-20261109-205423-3b6aa4b5"
        );
        assert!(pro_record.environment_scope.contains("Professional"));
        assert_eq!(pro_record.scenarios.len(), 3);
        assert_eq!(
            pro.historical_evidence
                .iter()
                .filter(|item| item.status == ValidationStatus::Tested)
                .count(),
            1
        );
        assert_eq!(
            pro.evidence
                .iter()
                .filter(|item| item.status == ValidationStatus::Tested)
                .count(),
            3
        );
        for item in &pro.evidence {
            if pro_record
                .scenarios
                .iter()
                .any(|entry| entry.scenario == item.scenario)
            {
                assert_eq!(item.scope, pro_record.environment_scope);
            } else {
                assert_eq!(item.status, ValidationStatus::NotRun);
                assert!(item.recorded_on.is_none());
            }
        }
    }

    #[test]
    fn base_26h1_acceptance_keeps_lab_identity_and_limited_scope() {
        let profile = profiles()
            .unwrap()
            .iter()
            .find(|profile| profile.id == "windows11-28000.1-x64-notification")
            .unwrap();
        let catalog = support_profile(profile);
        let record = catalog.recorded_acceptance.unwrap();
        assert_eq!(
            recorded_recipe_digest(profile),
            "9c78c1468d10688c531a209e6dfccd8a10b6866495d165e0e70152ecb93b883a"
        );
        assert_eq!(catalog.recorded_acceptances.len(), 1);
        assert_eq!(record.package_version, "5.16.1");
        assert_eq!(record.package_build_id, "5.16.1-5dc517dab70f");
        assert_eq!(record.published_commit, None);
        assert_eq!(record.run_id, "sandbox-20261110-224027-782c4b5d");
        assert_eq!(record.nonce, "db768217108544b191e8d99daafef808");
        assert_eq!(
            record.archive_sha256,
            "6dc2f59a79dc38834ff88a49e3e5d06aa6a6501c04d1188f3f3ad03af1d76441"
        );
        assert_eq!(
            record.package_manifest_sha256,
            "35afe0bcdff0479a370d4a8368060f72b8b7aee317e07b331a8bfc836b54e50b"
        );
        assert_eq!(
            record.executable_sha256,
            "a6537eaa984328d22e9c9109f77e03eda3ca935e46876fa7279411b30b23ce06"
        );
        assert_eq!(
            record.adapter_sha256,
            "f5ddb6bae2bc3e2977a66cc43dded0def2a64690db0fe3660821a9df05b4c49b"
        );
        assert_eq!(
            record.popup_sha256,
            "b565724ef34551cfd28ca0682c24ffc50306ba33c111fa75bc64da0182481a2b"
        );
        assert!(record.environment_scope.contains("compatibility-lab"));
        assert!(
            catalog.historical_evidence.iter().all(|item| {
                item.status == ValidationStatus::NotRun && item.recorded_on.is_none()
            })
        );
        assert_eq!(record.scenarios.len(), 3);
        for item in &catalog.evidence {
            let tested = matches!(
                item.scenario,
                ValidationScenario::ModuleLayout
                    | ValidationScenario::ClockUpdateRestore
                    | ValidationScenario::TrayPrepareStop
            );
            assert_eq!(
                item.status,
                if tested {
                    ValidationStatus::Tested
                } else {
                    ValidationStatus::NotRun
                }
            );
            assert_eq!(item.recorded_on, tested.then_some("2026-11-10"));
            assert_eq!(
                item.reference,
                tested.then_some("docs/compatibility-26h1.md")
            );
        }
        let exported = serde_json::to_value(&catalog).unwrap();
        assert_eq!(
            exported["recorded_acceptance"]["package_build_id"],
            "5.16.1-5dc517dab70f"
        );
        assert_eq!(
            exported["recorded_acceptances"].as_array().unwrap().len(),
            1
        );
        let later = Profile::decode(include_bytes!(
            "../crates/adapter-protocol/profiles/lab/windows11-28000.3365-x64-notification.json"
        ))
        .unwrap();
        assert!(recorded_acceptances(&later).is_empty());
        assert!(evidence(&later).iter().all(|item| {
            item.status == ValidationStatus::NotRun
                && item.recorded_on.is_none()
                && item.reference.is_none()
        }));
    }

    #[test]
    fn a_partial_rerun_does_not_relabel_other_scenarios_or_discard_their_evidence() {
        let profile = profiles()
            .unwrap()
            .iter()
            .find(|profile| profile.id == "windows11-26100.1742-x64")
            .unwrap();
        let mut results = evidence(profile);
        let previous = serde_json::to_value(&results).unwrap();
        let partial = RecordedAcceptance {
            environment_scope: "Synthetic test of a later clock-only run",
            recorded_on: "2099-01-01",
            reference: "synthetic-regression-fixture",
            scenarios: &[AcceptedScenario {
                scenario: ValidationScenario::ClockUpdateRestore,
                detail: "Only this scenario was rerun",
            }],
            ..*recorded_acceptance(profile).unwrap()
        };
        apply_acceptance(&mut results, &partial);
        for (index, item) in results.iter().enumerate() {
            if item.scenario == ValidationScenario::ClockUpdateRestore {
                assert_eq!(item.scope, partial.environment_scope);
                assert_eq!(item.reference, Some(partial.reference));
                assert_eq!(item.recorded_on, Some(partial.recorded_on));
            } else {
                assert_eq!(serde_json::to_value(item).unwrap(), previous[index]);
            }
        }
    }

    #[test]
    fn recorded_acceptance_binds_entire_recipe_except_label() {
        for id in [
            "windows11-26200.8037-x64",
            "windows11-28000.1-x64-notification",
        ] {
            let original = profiles()
                .unwrap()
                .iter()
                .find(|profile| profile.id == id)
                .unwrap();
            let mut renamed = original.clone();
            renamed.system_label = "same module implementation, clearer label".into();
            assert!(recorded_acceptance(&renamed).is_some());
            for change in 0..11 {
                let mut profile = original.clone();
                match change {
                    0 => profile.schema += 1,
                    1 => profile.id = "new-profile-with-existing-layout".into(),
                    2 => profile.taskbar.sha256 = "0".repeat(64),
                    3 => profile.shell.identity.timestamp ^= 1,
                    4 => profile.tray.as_mut().unwrap().sha256 = "0".repeat(64),
                    5 => {
                        profile.host.element_offset =
                            profile.host.element_offset.map(|value| value + 8)
                    }
                    6 => profile.source.child_vtable_rva += 8,
                    7 => {
                        profile.tray_abi = Some(TrayAbi::SystemTrayDragDropBorrowedCoordinatesX64V1)
                    }
                    8 => profile.tray_geometry = Some(
                        litetray_adapter_protocol::compatibility::TrayGeometry::DragDropHitTesting,
                    ),
                    9 => profile.tray_entries.as_mut().unwrap()[0].prologue[0] ^= 1,
                    _ => {
                        use litetray_adapter_protocol::compatibility::ChildInterface;
                        profile.source.child_interface =
                            Some(match profile.source.interface_contract() {
                                ChildInterface::XamlExplorerHostChildEef8V1 => {
                                    ChildInterface::XamlExplorerHostChildEc97V1
                                }
                                ChildInterface::XamlExplorerHostChildEc97V1 => {
                                    ChildInterface::XamlExplorerHostChildEef8V1
                                }
                            });
                    }
                }
                assert!(
                    recorded_acceptance(&profile).is_none(),
                    "{id} change {change}"
                );
            }
        }
    }

    #[test]
    fn changed_geometry_cannot_reenter_through_historical_acceptance() {
        for original in profiles().unwrap() {
            let previous_clock = historical_evidence(original)
                .into_iter()
                .find(|item| item.scenario == ValidationScenario::ClockUpdateRestore)
                .unwrap()
                .status;
            for geometry in [
                TrayGeometry::NotificationAreaBounds,
                TrayGeometry::DragDropHitTesting,
                TrayGeometry::LegacyUnclassified,
            ] {
                if Some(geometry) == original.tray_geometry {
                    continue;
                }
                let mut changed = original.clone();
                changed.tray_geometry = Some(geometry);
                assert!(recorded_acceptance(&changed).is_none());
                for results in [historical_evidence(&changed), evidence(&changed)] {
                    for item in results {
                        assert_eq!(
                            item.status,
                            if item.scenario == ValidationScenario::ClockUpdateRestore {
                                previous_clock
                            } else {
                                ValidationStatus::NotRun
                            },
                            "{} {geometry:?} {:?}",
                            original.id,
                            item.scenario
                        );
                        if item.scenario != ValidationScenario::ClockUpdateRestore {
                            assert!(item.recorded_on.is_none());
                        }
                    }
                }
            }
        }
    }
}
