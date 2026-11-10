//! Disk inspection is a background operation, independent of page lifetime.
//! Recorded acceptance is presented separately and never changes disk status.
use super::*;
use litetray::compatibility::{
    self as inventory, Feature, FeatureStatus, Report, SupportProfile, ValidationScenario,
    ValidationStatus,
};
use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

pub(super) const PAGE: usize = 11;
pub(super) const CHECK: u16 = 950;
pub(super) const EXPORT: u16 = 951;
pub(super) const DETAILS: u16 = 952;
pub(super) const CLOCK_SETTINGS: u16 = 957;
pub(super) const TRAY_SETTINGS: u16 = 958;
pub(super) const OPEN_REPORT: u16 = 959;
const OS: u16 = 953;
const SUMMARY: u16 = 954;
const MESSAGE: u16 = 955;
const EXPORT_MESSAGE: u16 = 956;
const FEATURE_STATUS: u16 = 960;
const FEATURE_NOTE: u16 = 964;
const NEXT_STEP: u16 = 970;
const DETAIL_TEXT: u16 = 972;
const FEATURES: [Feature; 4] = [
    Feature::ClockAppearance,
    Feature::ClockInput,
    Feature::TrayMirroring,
    Feature::SystemAreas,
];

#[derive(Debug, serde::Serialize)]
pub(super) struct OperatingSystem {
    pub build: u32,
    pub ubr: Option<u32>,
    pub display_version: Option<String>,
    pub edition: Option<String>,
    pub architecture: String,
}
impl OperatingSystem {
    fn current() -> Self {
        use windows_sys::Win32::System::{Registry::*, SystemInformation::*};
        let key = wide("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion");
        let string = |name: &str| unsafe {
            let mut buffer = [0u16; 256];
            let mut bytes = std::mem::size_of_val(&buffer) as u32;
            let status = RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                wide(name).as_ptr(),
                RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY,
                null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut bytes,
            );
            (status == ERROR_SUCCESS)
                .then(|| {
                    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
                    String::from_utf16_lossy(&buffer[..end])
                })
                .filter(|value| !value.is_empty())
        };
        let mut ubr = 0u32;
        let mut bytes = 4u32;
        let revision = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                wide("UBR").as_ptr(),
                RRF_RT_REG_DWORD | RRF_SUBKEY_WOW6464KEY,
                null_mut(),
                (&mut ubr as *mut u32).cast(),
                &mut bytes,
            )
        };
        let mut system: SYSTEM_INFO = unsafe { std::mem::zeroed() };
        unsafe {
            GetNativeSystemInfo(&mut system);
        }
        let architecture = match unsafe { system.Anonymous.Anonymous.wProcessorArchitecture } {
            PROCESSOR_ARCHITECTURE_AMD64 => "x64",
            PROCESSOR_ARCHITECTURE_ARM64 => "ARM64",
            PROCESSOR_ARCHITECTURE_INTEL => "x86",
            _ => "unknown",
        };
        Self {
            build: diagnostics::windows_build(),
            ubr: (revision == ERROR_SUCCESS).then_some(ubr),
            display_version: string("DisplayVersion"),
            edition: string("EditionID"),
            architecture: architecture.into(),
        }
    }
    fn label(&self) -> String {
        let system = if self.build >= 22000 {
            "Windows 11"
        } else if self.build >= 10240 {
            "Windows 10"
        } else {
            "Windows"
        };
        let version = self
            .ubr
            .map(|ubr| format!("{}.{}", self.build, ubr))
            .unwrap_or_else(|| self.build.to_string());
        [
            system.to_owned(),
            self.edition.clone().unwrap_or_default(),
            self.display_version.clone().unwrap_or_default(),
            version,
            self.architecture.clone(),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
    }
    fn short_label(&self) -> String {
        let system = if self.build >= 22000 {
            "Windows 11"
        } else if self.build >= 10240 {
            "Windows 10"
        } else {
            "Windows"
        };
        match self.display_version.as_deref() {
            Some(version) => format!("{system} {version}"),
            None => system.into(),
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub(super) struct Snapshot {
    pub checked_at: chrono::DateTime<Utc>,
    pub os: OperatingSystem,
    pub report: Report,
    pub supported_profiles: Vec<SupportProfile>,
}
pub(super) fn collect_current() -> Result<Snapshot, String> {
    let os = OperatingSystem::current();
    let root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .ok_or("SystemRoot 不可用")?;
    let report = inventory::collect(&root, Some(os.build))?;
    let supported_profiles = inventory::support_catalog()?;
    Ok(Snapshot {
        checked_at: Utc::now(),
        os,
        report,
        supported_profiles,
    })
}

#[derive(Default)]
pub(super) struct State {
    pub snapshot: Option<Arc<Snapshot>>,
    pub attempted: bool,
    pub busy: bool,
    pub error: Option<String>,
    pub last_attempt: Option<chrono::DateTime<Utc>>,
    pub details_open: bool,
    pub export_busy: bool,
    pub export_path: Option<PathBuf>,
    pub export_error: Option<String>,
}
impl State {
    fn finish(&mut self, result: Result<Box<Snapshot>, String>) {
        self.busy = false;
        match result {
            Ok(snapshot) => {
                self.snapshot = Some(Arc::from(snapshot));
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
    fn check_message(&self) -> String {
        let current = if self.busy {
            tr(if self.snapshot.is_some() {
                "正在重新检查 · 下方为上次结果"
            } else {
                "正在检查，请稍候…"
            })
        } else if self.error.is_some() {
            tr(if self.snapshot.is_some() {
                "本次检查失败 · 下方为上次结果"
            } else {
                "检查失败，请重试"
            })
        } else if self.snapshot.is_some() {
            tr("检查完成")
        } else {
            tr("尚未检查")
        };
        let last = self
            .snapshot
            .as_ref()
            .map(|snapshot| {
                format!(
                    "{} {}",
                    tr("上次检查："),
                    snapshot
                        .checked_at
                        .with_timezone(&Local)
                        .format("%m-%d %H:%M")
                )
            })
            .unwrap_or_default();
        if last.is_empty() {
            current.into()
        } else {
            format!("{current} · {last}")
        }
    }
    fn export_message(&self) -> String {
        if self.export_busy {
            tr("正在保存诊断报告…").into()
        } else if self.export_error.is_some() {
            tr("报告保存失败，请重试。具体原因见诊断详情。").into()
        } else if self.export_path.is_some() {
            tr("报告已保存，可打开文件夹查看。").into()
        } else {
            String::new()
        }
    }
    pub(super) fn footer(&self) -> String {
        if self.export_busy || self.export_error.is_some() || self.export_path.is_some() {
            self.export_message()
        } else {
            tr("此检查不会更改你的设置。").into()
        }
    }
    fn summary(&self) -> (&'static str, &'static str) {
        if self.busy {
            return (
                tr("正在检查任务栏兼容性…"),
                tr("稍等片刻，完成后会列出各项功能的检查结果。"),
            );
        }
        if self.error.is_some() {
            return (
                tr("未能完成检查"),
                tr("请重新检查。日历、日程和提醒仍可使用。"),
            );
        }
        let Some(snapshot) = self.snapshot.as_deref() else {
            return (
                tr("检查这台电脑的任务栏"),
                tr("检查后，你可以选择要启用的时钟和托盘功能。"),
            );
        };
        let ready = |feature| {
            feature_result(Some(&snapshot.report), feature)
                .is_some_and(|item| item.status == FeatureStatus::DiskMatchedLiveCheckRequired)
        };
        if FEATURES.into_iter().all(ready) {
            (
                tr("可尝试启用任务栏功能"),
                tr("先到时钟设置中启用。下方列出了仍需留意的使用限制。"),
            )
        } else if ready(Feature::ClockAppearance) {
            (
                tr("可以先试用任务栏时钟"),
                tr("部分功能暂不可用，不影响你单独设置时钟。"),
            )
        } else if FEATURES.into_iter().any(|feature| {
            feature_result(Some(&snapshot.report), feature).is_none_or(|item| {
                matches!(
                    item.status,
                    FeatureStatus::MissingModules | FeatureStatus::InspectionFailed
                )
            })
        }) {
            (
                tr("还不能确认任务栏兼容性"),
                tr("有些系统文件没能完成检查。请重试，或保存报告帮助排查。"),
            )
        } else {
            (
                tr("任务栏功能暂时用不了"),
                tr("可以保存诊断报告帮助排查。日历、日程和提醒仍可使用。"),
            )
        }
    }
    fn diagnostic_details(&self) -> String {
        let mut sections = Vec::new();
        if let Some(error) = &self.error {
            sections.push(format!("{} {}", tr("检测失败："), error));
        }
        if let Some(error) = &self.export_error {
            sections.push(format!("{} {}", tr("报告保存失败："), error));
        }
        if let Some(path) = &self.export_path {
            sections.push(format!("{} {}", tr("报告已保存："), path.display()));
        }
        if let Some(snapshot) = self.snapshot.as_deref() {
            sections.push(details_text(snapshot));
        }
        sections.join("\r\n\r\n")
    }
}

fn feature_name(feature: Feature) -> &'static str {
    tr(match feature {
        Feature::ClockAppearance => "任务栏时钟",
        Feature::ClockInput => "时钟点击与音量",
        Feature::TrayMirroring => "副屏托盘图标",
        Feature::SystemAreas => "系统图标与菜单",
    })
}
fn feature_result(report: Option<&Report>, feature: Feature) -> Option<&inventory::FeatureReport> {
    report?.features.iter().find(|item| item.feature == feature)
}
fn feature_status(report: Option<&Report>, feature: Feature) -> &'static str {
    let Some(item) = feature_result(report, feature) else {
        return tr(if report.is_some() {
            "未能确认"
        } else {
            "等待检查"
        });
    };
    tr(match item.status {
        FeatureStatus::DiskMatchedLiveCheckRequired => {
            if item
                .baseline_limitations
                .contains(&"notification_click_crash_report_unresolved")
            {
                "有已知问题"
            } else if item.baseline_limitations.is_empty() {
                "可尝试启用"
            } else {
                "仍需实测"
            }
        }
        FeatureStatus::UnsupportedModules => "暂不支持",
        FeatureStatus::MissingModules => "缺少系统组件",
        FeatureStatus::InspectionFailed => "未能确认",
    })
}
fn limitation_text(key: &str) -> &'static str {
    tr(match key {
        "notification_click_crash_report_unresolved" => "有通知时点击可能崩溃，尚未确认修复。",
        "third_party_hover_popups_require_separate_validation" => "第三方图标的悬停窗口仍需测试。",
        "system_icon_and_cross_monitor_menu_validation_incomplete" => {
            "系统图标和跨屏菜单仍需测试。"
        }
        _ => "部分操作仍需验证，详见诊断详情。",
    })
}
fn feature_note(report: Option<&Report>, feature: Feature) -> &'static str {
    if let Some(item) = feature_result(report, feature) {
        match item.status {
            FeatureStatus::UnsupportedModules => return tr("当前任务栏版本还没有适配。"),
            FeatureStatus::MissingModules => {
                return tr("未找到所需的 Windows 文件，请稍后重新检查。");
            }
            FeatureStatus::InspectionFailed => return tr("没能读出检查结果，请重新检查。"),
            FeatureStatus::DiskMatchedLiveCheckRequired => {
                if let Some(key) = item
                    .baseline_limitations
                    .iter()
                    .find(|key| **key == "notification_click_crash_report_unresolved")
                    .or_else(|| item.baseline_limitations.first())
                {
                    return limitation_text(key);
                }
            }
        }
    }
    tr(match feature {
        Feature::ClockAppearance => "自定义时间、日期、天气和字体；启用时会再检查实际连接。",
        Feature::ClockInput => "点击打开日历，滚轮调节音量。",
        Feature::TrayMirroring => "在外接屏显示主屏的托盘图标。",
        Feature::SystemAreas => "副屏上的系统图标和右键菜单。",
    })
}
fn scenario_name(scenario: ValidationScenario) -> &'static str {
    tr(match scenario {
        ValidationScenario::ModuleLayout => "模块与布局",
        ValidationScenario::ClockUpdateRestore => "时钟更新与恢复",
        ValidationScenario::TrayPrepareStop => "托盘准备与停止",
        ValidationScenario::TrayMirroring => "副屏托盘复制",
        ValidationScenario::PointerInteractions => "点击、音量与菜单",
        ValidationScenario::MultiMonitor => "主副屏时钟与布局",
        ValidationScenario::MixedDpi => "混合 DPI 与设备变化",
    })
}
fn validation_status(status: ValidationStatus) -> &'static str {
    tr(match status {
        ValidationStatus::Tested => "已有实测（限定范围）",
        ValidationStatus::Historical => "历史记录，当前包未复测",
        ValidationStatus::NotRun => "尚未验收",
    })
}
fn selected_profile(snapshot: &Snapshot) -> Option<&SupportProfile> {
    // Partial disk matches do not inherit a profile's acceptance history.
    snapshot
        .report
        .all_profile_images_matched
        .then(|| {
            snapshot
                .supported_profiles
                .iter()
                .find(|profile| Some(&profile.profile_id) == snapshot.report.profile_id.as_ref())
        })
        .flatten()
}
fn evidence_text(snapshot: Option<&Snapshot>) -> String {
    let Some(snapshot) = snapshot else {
        return tr("检测后显示所匹配资料的实测记录。").into();
    };
    let Some(profile) = selected_profile(snapshot) else {
        return tr("本机尚未完整匹配已收录资料，不能套用其他模块的实测结果。\r\n独立日历、日程等功能不依赖任务栏接管。").into();
    };
    profile
        .evidence
        .iter()
        .map(|item| {
            format!(
                "{}：{}",
                scenario_name(item.scenario),
                validation_status(item.status)
            )
        })
        .collect::<Vec<_>>()
        .join("\r\n")
}
fn catalog_text(snapshot: Option<&Snapshot>) -> String {
    let Some(snapshot) = snapshot else {
        return tr("检测后显示本版本内置的模块资料。").into();
    };
    snapshot
        .supported_profiles
        .iter()
        .map(|profile| {
            let clock = profile
                .evidence
                .iter()
                .find(|item| item.scenario == ValidationScenario::ClockUpdateRestore);
            format!(
                "{}\r\n{} · {} {}",
                profile.system_label,
                profile.profile_id,
                tr("时钟验收："),
                clock
                    .map(|item| validation_status(item.status))
                    .unwrap_or(tr("尚未验收"))
            )
        })
        .collect::<Vec<_>>()
        .join("\r\n\r\n")
}
fn details_text(snapshot: &Snapshot) -> String {
    let recognized = snapshot
        .report
        .modules
        .iter()
        .filter(|module| module.status == inventory::ModuleStatus::Matched)
        .count();
    let equivalent = snapshot
        .report
        .modules
        .iter()
        .filter(|module| {
            module.recognition.as_ref().is_some_and(|item| {
                item.basis
                    == litetray_adapter_protocol::compatibility::MatchBasis::ExecutionEquivalent
            })
        })
        .count();
    let mut lines = vec![
        snapshot.os.label(),
        format!("{} {}", tr("检测时间："), snapshot.checked_at.to_rfc3339()),
        format!("{} {recognized}/{} · {} {equivalent}", tr("已识别模块："), snapshot.report.modules.len(), tr("实现等价：")),
        tr("磁盘识别后，启用时仍会核验运行中的模块与控件。托盘暂不兼容时，已兼容的时钟可继续运行。").into(),
        format!("\r\n{}\r\n{}", tr("已登记的实测范围"), evidence_text(Some(snapshot))),
    ];
    if let Some(reason) = &snapshot.report.profile_selection_reason {
        lines.push(format!("{} {reason}", tr("资料选择：")));
    }
    for module in &snapshot.report.modules {
        lines.push(format!("\r\n{} · {:?}", module.name, module.status));
        if let Some(image) = &module.image {
            lines.push(format!("SHA-256: {}", image.sha256));
        }
        if let Some(reason) = &module.reason {
            lines.push(reason.clone());
        }
        if let Some(recognition) = &module.recognition {
            lines.push(format!(
                "{} {}",
                tr("识别方式："),
                tr(match recognition.basis {
                    litetray_adapter_protocol::compatibility::MatchBasis::ExactImage =>
                        "完整模块匹配",
                    litetray_adapter_protocol::compatibility::MatchBasis::ExecutionEquivalent =>
                        "实现等价（仅允许已定义的元数据变化）",
                })
            ));
            lines.push(recognition.implementation.clone());
            lines.push(format!(
                "{} {}",
                tr("参考实现："),
                recognition.evidence_profile
            ));
        } else {
            lines.push(tr(match module.status {
                inventory::ModuleStatus::Missing => "处理建议：确认系统组件完整，完成 Windows 更新后重新检测。",
                inventory::ModuleStatus::InvalidImage | inventory::ModuleStatus::ReadFailed => "处理建议：模块读取或结构核验失败；完成系统更新后重新检测，并保留报告中的具体错误。",
                _ => "处理建议：此模块需要新的实现模板；导出报告可提供身份与失败原因。各功能是否可用以上方检查结果为准。",
            }).into());
        }
    }
    if let Some(plan) = &snapshot.report.implementation {
        lines.push(format!(
            "\r\n{} {:?} / {:?}\r\nHostChild: {:#x}; ChildSource: {:#x}; Tray: {:?}",
            tr("当前实现组合："),
            plan.source_recipe,
            plan.topology,
            plan.host_child_offset,
            plan.child_source_offset,
            plan.tray_abi
        ));
        if let Some(notification) = &plan.notification_area {
            lines.push(format!(
                "NotificationHostChild: {:#x}; NotificationParent: {:#x}",
                notification.child_offset, notification.parent_offset
            ));
        }
        if let Some(geometry) = plan.tray_geometry {
            use litetray_adapter_protocol::compatibility::TrayGeometry;
            lines.push(tr(match geometry {
                TrayGeometry::NotificationAreaBounds => "坐标入口用途：通知区域与图标边界。拖放使用另一路径，完整交互仍待验收。",
                TrayGeometry::DragDropHitTesting => "坐标入口用途：托盘拖放命中测试。已与 Copilot 按钮的坐标入口区分，完整交互仍待验收。",
                TrayGeometry::LegacyUnclassified => "坐标入口用途：历史实现，尚未完成调用用途分类。",
            }).into());
        }
    }
    if let Some(profile) = selected_profile(snapshot) {
        for item in &profile.evidence {
            lines.push(format!(
                "\r\n{} · {}\r\n{}\r\n{}",
                scenario_name(item.scenario),
                validation_status(item.status),
                tr(item.scope),
                tr(item.detail)
            ));
        }
    }
    for feature in &snapshot.report.features {
        for reason in &feature.reasons {
            lines.push(format!("{}: {reason}", feature_name(feature.feature)));
        }
        for key in &feature.baseline_limitations {
            lines.push(format!(
                "{}: {} [{key}]",
                feature_name(feature.feature),
                limitation_text(key)
            ));
        }
    }
    lines.push(format!(
        "\r\n{}\r\n{}",
        tr("已收录的模块资料"),
        catalog_text(Some(snapshot))
    ));
    lines.join("\r\n")
}

fn export_snapshot(snapshot: &Snapshot, directory: &Path) -> Result<PathBuf, String> {
    // Do not rerun detection while exporting: this report is the displayed
    // snapshot, including its timestamp and the documented evidence limits.
    let value = serde_json::json!({
        "schema":1,"kind":"litetray_compatibility_check",
        "checked_at":snapshot.checked_at,"os":snapshot.os,
        "compatibility":snapshot.report,"supported_profiles":snapshot.supported_profiles,
    });
    let bytes = serde_json::to_vec_pretty(&value).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    static NEXT_EXPORT: AtomicU64 = AtomicU64::new(0);
    for _ in 0..32 {
        let sequence = NEXT_EXPORT.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(
            "compatibility-{}-{}-{sequence}.json",
            Utc::now().format("%Y%m%d-%H%M%S-%3f"),
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(&bytes)
                    .and_then(|_| file.sync_all())
                    .map_err(|error| error.to_string())?;
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("无法创建独立报告文件".into())
}

impl App {
    pub(super) unsafe fn compatibility_settings(&mut self, width: i32) {
        unsafe {
            let total = width - 260;
            let os = self.control("STATIC", "", OS, SS_LEFT as u32, 224, 92, total - 172, 24);
            SetPropW(os, wide("LiteTray.Muted").as_ptr(), 1usize as HANDLE);
            self.button("重新检查", CHECK, 224 + total - 144, 84, 144);
            let summary = self.control("STATIC", "", SUMMARY, SS_LEFT as u32, 224, 132, total, 30);
            widgets::heading(summary, self.dpi);
            self.control("STATIC", "", NEXT_STEP, SS_LEFT as u32, 224, 174, total, 36);
            let action = self.button("前往时钟设置", CLOCK_SETTINGS, 224, 220, 196);
            SetPropW(action, wide("LiteTray.Primary").as_ptr(), 1usize as HANDLE);
            self.button("多屏托盘设置", TRAY_SETTINGS, 432, 220, 196);
            let message = self.control("STATIC", "", MESSAGE, SS_LEFT as u32, 224, 266, total, 22);
            SetPropW(message, wide("LiteTray.Muted").as_ptr(), 1usize as HANDLE);
            self.divider(224, 302, total);
            for (index, feature) in FEATURES.into_iter().enumerate() {
                let y = 318 + index as i32 * 58;
                self.label(feature_name(feature), 224, y, total - 220, 24);
                self.control(
                    "STATIC",
                    "",
                    FEATURE_STATUS + index as u16,
                    SS_RIGHT as u32,
                    224 + total - 204,
                    y,
                    204,
                    24,
                );
                let note = self.control(
                    "STATIC",
                    "",
                    FEATURE_NOTE + index as u16,
                    SS_LEFT as u32,
                    224,
                    y + 26,
                    total,
                    30,
                );
                SetPropW(note, wide("LiteTray.Muted").as_ptr(), 1usize as HANDLE);
            }
            self.divider(224, 550, total);
            self.button("查看诊断详情", DETAILS, 224, 566, 184);
            self.button("保存诊断报告", EXPORT, 224 + total - 368, 566, 184);
            self.button("打开报告文件夹", OPEN_REPORT, 224 + total - 172, 566, 172);
            self.control(
                "STATIC",
                "",
                EXPORT_MESSAGE,
                SS_LEFT as u32,
                224,
                612,
                total,
                38,
            );
            let details = self.control(
                "EDIT",
                "",
                DETAIL_TEXT,
                WS_TABSTOP
                    | WS_VSCROLL
                    | ES_MULTILINE as u32
                    | ES_READONLY as u32
                    | ES_AUTOVSCROLL as u32,
                224,
                662,
                total,
                280,
            );
            SendMessageW(details, EM_SETLIMITTEXT, 512 * 1024, 0);
            self.sync_compatibility_controls();
        }
    }
    pub(super) unsafe fn sync_compatibility_controls(&self) {
        unsafe {
            if self.page != PAGE {
                return;
            }
            let state = &self.compatibility;
            let snapshot = state.snapshot.as_deref();
            let has_details =
                snapshot.is_some() || state.error.is_some() || state.export_error.is_some();
            let can_try_clock = !state.busy
                && state.error.is_none()
                && feature_result(
                    snapshot.map(|value| &value.report),
                    Feature::ClockAppearance,
                )
                .is_some_and(|item| item.status == FeatureStatus::DiskMatchedLiveCheckRequired);
            let primary = if can_try_clock {
                CLOCK_SETTINGS
            } else if state.busy || state.error.is_some() || snapshot.is_none() {
                CHECK
            } else {
                EXPORT
            };
            for id in [CLOCK_SETTINGS, CHECK, EXPORT] {
                if id == primary {
                    SetPropW(
                        self.field(id),
                        wide("LiteTray.Primary").as_ptr(),
                        1usize as HANDLE,
                    );
                } else {
                    RemovePropW(self.field(id), wide("LiteTray.Primary").as_ptr());
                }
                InvalidateRect(self.field(id), null(), 0);
            }
            EnableWindow(self.field(CHECK), i32::from(!state.busy));
            EnableWindow(
                self.field(EXPORT),
                i32::from(snapshot.is_some() && !state.busy && !state.export_busy),
            );
            EnableWindow(self.field(DETAILS), i32::from(has_details));
            EnableWindow(
                self.field(OPEN_REPORT),
                i32::from(state.export_path.is_some() && !state.export_busy),
            );
            set_text(
                self.field(CHECK),
                if state.busy {
                    "检查中…"
                } else {
                    "重新检查"
                },
            );
            set_text(
                self.field(EXPORT),
                if state.export_busy {
                    "保存中…"
                } else {
                    "保存诊断报告"
                },
            );
            set_text(
                self.field(OS),
                &snapshot
                    .map(|value| value.os.short_label())
                    .unwrap_or_else(|| tr("这台电脑").into()),
            );
            let (summary, next_step) = state.summary();
            set_text(self.field(SUMMARY), summary);
            set_text(self.field(NEXT_STEP), next_step);
            set_text(self.field(MESSAGE), &state.check_message());
            set_text(self.field(EXPORT_MESSAGE), &state.export_message());
            set_text(self.field(STATUS), &state.footer());
            for (index, feature) in FEATURES.into_iter().enumerate() {
                set_text(
                    self.field(FEATURE_STATUS + index as u16),
                    feature_status(snapshot.map(|value| &value.report), feature),
                );
                set_text(
                    self.field(FEATURE_NOTE + index as u16),
                    feature_note(snapshot.map(|value| &value.report), feature),
                );
            }
            set_text(
                self.field(DETAILS),
                if state.details_open {
                    "收起诊断详情"
                } else {
                    "查看诊断详情"
                },
            );
            if state.details_open {
                set_text(self.field(DETAIL_TEXT), &state.diagnostic_details());
            }
            ShowWindow(
                self.field(EXPORT_MESSAGE),
                if state.details_open && !state.export_message().is_empty() {
                    SW_SHOWNA
                } else {
                    SW_HIDE
                },
            );
            ShowWindow(
                self.field(DETAIL_TEXT),
                if state.details_open && has_details {
                    SW_SHOWNA
                } else {
                    SW_HIDE
                },
            );
            // A retry can temporarily hide error-only details. A new result
            // must restore their scroll range even if the page was rebuilt
            // while that background check was running.
            page::finish(self.page_host);
        }
    }
    pub(super) unsafe fn start_compatibility_check(&mut self) {
        unsafe {
            if self.compatibility.busy {
                return;
            }
            self.compatibility.attempted = true;
            self.compatibility.busy = true;
            self.compatibility.error = None;
            self.compatibility.last_attempt = Some(Utc::now());
            self.sync_compatibility_controls();
            let sender = self.tx.clone();
            let window = self.hwnd as usize;
            if let Err(error) = thread::Builder::new()
                .name("compatibility-check".into())
                .spawn(move || {
                    let result = collect_current().map(Box::new);
                    if sender.send(Event::Compatibility(result)).is_ok() {
                        PostMessageW(window as HWND, WM_RESULT, 0, 0);
                    }
                })
            {
                self.compatibility.finish(Err(error.to_string()));
                self.sync_compatibility_controls();
            }
        }
    }
    pub(super) unsafe fn receive_compatibility(&mut self, result: Result<Box<Snapshot>, String>) {
        unsafe {
            self.compatibility.finish(result);
            self.sync_compatibility_controls();
        }
    }
    pub(super) unsafe fn export_compatibility(&mut self) {
        unsafe {
            if self.compatibility.busy || self.compatibility.export_busy {
                return;
            }
            let Some(snapshot) = self.compatibility.snapshot.clone() else {
                return;
            };
            self.compatibility.export_busy = true;
            self.compatibility.export_error = None;
            self.sync_compatibility_controls();
            let directory = config::data_dir().join("reports");
            let sender = self.tx.clone();
            let window = self.hwnd as usize;
            if let Err(error) = thread::Builder::new()
                .name("compatibility-export".into())
                .spawn(move || {
                    let result = export_snapshot(&snapshot, &directory);
                    if sender.send(Event::CompatibilityExport(result)).is_ok() {
                        PostMessageW(window as HWND, WM_RESULT, 0, 0);
                    }
                })
            {
                self.compatibility.export_busy = false;
                self.compatibility.export_error = Some(error.to_string());
                self.sync_compatibility_controls();
            }
        }
    }
    pub(super) unsafe fn receive_compatibility_export(&mut self, result: Result<PathBuf, String>) {
        unsafe {
            self.compatibility.export_busy = false;
            match result {
                Ok(path) => {
                    self.compatibility.export_path = Some(path);
                    self.compatibility.export_error = None;
                }
                Err(error) => self.compatibility.export_error = Some(error),
            }
            self.sync_compatibility_controls();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Snapshot {
        // Missing files are a real disk collector result, not a synthetic pass.
        let missing = std::env::temp_dir().join(format!(
            "litetray-compatibility-missing-{}",
            std::process::id()
        ));
        Snapshot {
            checked_at: Utc::now(),
            os: OperatingSystem {
                build: 26300,
                ubr: Some(9818),
                display_version: Some("26H2".into()),
                edition: Some("Professional".into()),
                architecture: "x64".into(),
            },
            report: inventory::collect(&missing, Some(26300)).unwrap(),
            supported_profiles: inventory::support_catalog().unwrap(),
        }
    }
    #[test]
    fn disk_match_never_claims_runtime_acceptance_and_partial_match_has_no_evidence() {
        i18n::set_language(Language::En);
        let mut value = snapshot();
        for feature in &mut value.report.features {
            feature.status = FeatureStatus::DiskMatchedLiveCheckRequired;
        }
        for feature in FEATURES {
            let item = feature_result(Some(&value.report), feature).unwrap();
            if item.baseline_limitations.is_empty() {
                assert_eq!(
                    feature_status(Some(&value.report), feature),
                    tr("可尝试启用")
                );
            } else {
                assert_ne!(
                    feature_status(Some(&value.report), feature),
                    tr("可尝试启用")
                );
                assert!(!feature_note(Some(&value.report), feature).is_empty());
            }
        }
        assert!(
            feature_note(Some(&value.report), Feature::ClockInput)
                .to_ascii_lowercase()
                .contains("crash")
        );
        assert!(details_text(&value).contains("notification_click_crash_report_unresolved"));
        let tested_ids: Vec<_> = value
            .supported_profiles
            .iter()
            .filter(|profile| {
                matches!(
                    profile.profile_id.as_str(),
                    "windows11-26300.9818-x64"
                        | "windows11-26200.8457-x64"
                        | "windows11-26200.8037-x64"
                        | "windows11-26100.1742-x64"
                        | "windows11-22621.1702-x64"
                        | "windows11-22631.2861-x64"
                )
            })
            .map(|profile| profile.profile_id.clone())
            .collect();
        assert_eq!(tested_ids.len(), 6);
        for id in tested_ids {
            let tested = 3;
            value.report.profile_id = Some(id);
            value.report.all_profile_images_matched = false;
            assert!(selected_profile(&value).is_none());
            assert!(evidence_text(Some(&value)).contains("cannot apply"));
            value.report.all_profile_images_matched = true;
            let evidence = evidence_text(Some(&value));
            assert!(evidence.contains("Not validated"));
            assert!(evidence.contains("Tray mirroring"));
            assert!(evidence.contains("Recorded test (limited scope)"));
            assert_eq!(
                evidence.matches("Recorded test (limited scope)").count(),
                tested
            );
            assert_eq!(evidence.matches("Not validated").count(), 7 - tested);
        }
        for id in ["windows11-26100.1742-x64", "windows11-26200.8037-x64"] {
            value.report.profile_id = Some(id.into());
            let details = details_text(&value);
            assert!(details.contains("5.15.1"));
            assert!(details.contains("WHPX"));
            assert!(details.contains("not a live check"));
        }
        for (id, system) in [
            ("windows11-22621.1702-x64", "22H2 22621.1702"),
            ("windows11-22631.2861-x64", "23H2 22631.2861"),
        ] {
            value.report.profile_id = Some(id.into());
            let details = details_text(&value);
            assert!(details.contains(system));
            assert!(details.contains("frozen test package"));
            assert!(details.contains("56 DIP"));
            assert!(details.contains("Tray sessions and model replication were not activated."));
        }
        i18n::set_language(Language::ZhCn);
    }
    #[test]
    fn failed_rescan_retains_timestamp_and_last_report_without_claiming_fresh_success() {
        i18n::set_language(Language::En);
        let mut state = State::default();
        state.finish(Ok(Box::new(snapshot())));
        let previous = state.snapshot.as_ref().unwrap().checked_at;
        state.busy = true;
        state.finish(Err("fixture failure".into()));
        assert!(!state.busy);
        assert_eq!(state.snapshot.as_ref().unwrap().checked_at, previous);
        assert!(
            state
                .check_message()
                .contains(tr("本次检查失败 · 下方为上次结果"))
        );
        assert!(state.check_message().contains(tr("上次检查：")));
        assert_eq!(state.summary().0, tr("未能完成检查"));
        assert!(state.diagnostic_details().contains("fixture failure"));
        i18n::set_language(Language::ZhCn);
    }
    #[test]
    fn reports_are_independent_files_and_keep_displayed_disk_boundary() {
        let value = snapshot();
        let dir = std::env::temp_dir().join(format!(
            "litetray-compatibility-export-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let first = export_snapshot(&value, &dir).unwrap();
        let before = std::fs::read(&first).unwrap();
        let second = export_snapshot(&value, &dir).unwrap();
        assert_ne!(first, second);
        assert_eq!(std::fs::read(&first).unwrap(), before);
        let result: serde_json::Value = serde_json::from_slice(&before).unwrap();
        assert_eq!(result["compatibility"]["scope"], "disk_only");
        assert_eq!(result["compatibility"]["explorer_accessed"], false);
        assert_eq!(result["compatibility"]["all_profile_images_matched"], false);
        assert_eq!(result["checked_at"], serde_json::json!(value.checked_at));
        std::fs::remove_file(first).unwrap();
        std::fs::remove_file(second).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
