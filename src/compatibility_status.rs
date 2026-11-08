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
    VirtualMachineLtsc1742,
}

fn recorded_profile(profile: &Profile) -> Option<RecordedProfile> {
    // Evidence is attached to exact images, not a reusable profile label.
    // Replacing images under an existing ID must not inherit acceptance.
    let hashes = profile.modules().map(|module| module.sha256.as_str());
    match (profile.id.as_str(), hashes) {
        (
            "windows11-26300.9711-x64",
            [
                "ca1e335ac1fae5ff57519035302a2f365c4620d3a230f31f1231af7331760741",
                "8ba440eded430a2417692a98b34695a12ad0ee000333dfed07e0c2723e17111c",
                "c5f5260a2a56c4d7b0ee25978aa06c387b5ddffc57c5fe412be429d1289913ce",
            ],
        ) => Some(RecordedProfile::Previous9711),
        (
            "windows11-26300.9818-x64",
            [
                "a52f99f7be8cc1ee360025d1c06c1480a489b1d309bef230c05eb21a106ccf34",
                "1894863a224d22bd8d3ef0e1ba382b4d3d3079e0fa9a1fdeee82f3ebace003b6",
                "9772d19ec44f22ab2fcb976941d6ba6d1e66aabfb9d66367723bb9154ce7769f",
            ],
        ) => Some(RecordedProfile::Sandbox9818),
        (
            "windows11-26200.8457-x64",
            [
                "41326fffd57a2c6e08d1884a698bd9e9dfe90de0614de98733edaebacb5f5403",
                "bc5e4daaace00d2eaff630f0f27078c29a0a56ae77b5cf51d3cfd5a766581f59",
                "a920c97ace9f336855ebfea221101eb03739398c8b75ac6dcf28d82ed645aed5",
            ],
        ) => Some(RecordedProfile::VirtualMachine8457),
        (
            "windows11-26100.1742-x64",
            [
                "b3be279bcb5a7fb63dbdc24cc1c55442a3965b68702a98fbdb6496d91cb40de5",
                "3ef119746b60885b8923c49de95860caca0639aec1e2e7aa0a4113356b2cedb9",
                "734e19a7e749f4112acc1e072ab35a11fdd3ffc04d8fcf54f603593758702d5b",
            ],
        ) => Some(RecordedProfile::VirtualMachineLtsc1742),
        _ => None,
    }
}

fn evidence(profile: &Profile) -> Vec<ValidationEvidence> {
    let record = recorded_profile(profile);
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
                None => {}
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
            description: "同一任务栏实现可复用解析方案；每组模块仍须独立核验完整身份、哈希、布局和运行时归属。系统版本标签或构建区间不授权接管。",
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
                    .all(|item| item.status == ValidationStatus::NotRun)
            );
            for module_index in 0..3 {
                let mut profile = original.clone();
                let module = match module_index {
                    0 => &mut profile.taskbar,
                    1 => &mut profile.shell,
                    _ => &mut profile.tray,
                };
                module.sha256 = "0".repeat(64);
                assert!(
                    evidence(&profile)
                        .iter()
                        .all(|item| item.status == ValidationStatus::NotRun)
                );
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
                        .all(|item| item.status == ValidationStatus::NotRun)
                );
            }
        }
    }
}
