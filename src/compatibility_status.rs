//! Read-only presentation of exact built-in profiles and recorded acceptance.
//! None of these descriptions or historical results authorize activation.
use super::{FeatureReport, ModuleStatus, Report};
use litetray_adapter_protocol::compatibility::{
    Module, Profile, SourceRecipe, Topology, TrayAbi, profiles,
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
    pub tray_abi: TrayAbi,
    pub tray_geometry: litetray_adapter_protocol::compatibility::TrayGeometry,
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
            required_module_count: self.modules.len(),
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

fn evidence(profile: &Profile) -> Vec<ValidationEvidence> {
    let record = recorded_profile(profile);
    let digests = recipe_digests(profile);
    let previous = historical_recipes()
        .ok()
        .and_then(|entries| entries.iter().find(|entry| entry.profile_id == profile.id));
    let host_matches = previous.is_some_and(|entry| entry.host_source_digest == digests.0);
    let full_matches = host_matches && previous.is_some_and(|entry| entry.tray_digest == digests.1);
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
            description: "逐模块核验完整图像或严格实现等价；按宿主、XAML 容器与托盘的接口和调用用途组合。启用时独立核对当前模块和控件归属。",
        },
        modules: profile.modules().into_iter().cloned().collect(),
        evidence: evidence(profile),
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
            for module_index in 0..3 {
                let mut profile = original.clone();
                let module = match module_index {
                    0 => &mut profile.taskbar,
                    1 => &mut profile.shell,
                    _ => &mut profile.tray,
                };
                module.sha256 = "0".repeat(64);
                assert!(evidence(&profile).iter().all(|item| {
                    if module_index == 2 && item.scenario == ValidationScenario::ClockUpdateRestore
                    {
                        item.status
                            == evidence(original)
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
        for profile in profiles().unwrap() {
            let previous = retained
                .iter()
                .find(|entry| entry.profile_id == profile.id)
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
        let records = evidence(ltsc);
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
            3
        );
        for change in 0..5 {
            let mut profile = original.clone();
            match change {
                0 => profile.host.element_offset += 8,
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
                0 => profile.tray_abi = TrayAbi::SystemTrayDragDropBorrowedCoordinatesX64V1,
                1 => profile.tray_entries[0].rva += 8,
                2 => profile.tray_entries[1].prologue[0] ^= 1,
                _ => profile.tray_entries[2].rva += 8,
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
}
