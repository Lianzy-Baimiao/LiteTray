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
const OS: u16 = 953;
const PROFILE: u16 = 954;
const MESSAGE: u16 = 955;
const EXPORT_MESSAGE: u16 = 956;
const FEATURE_STATUS: u16 = 960;
const EVIDENCE: u16 = 970;
const CATALOG: u16 = 971;
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
            tr("正在检测本机模块…").into()
        } else if let Some(error) = &self.error {
            format!("{} {}", tr("检测失败："), tr(error))
        } else if self.snapshot.is_some() {
            tr("检测完成 · 以下为磁盘匹配结果").into()
        } else {
            tr("尚未检测").into()
        };
        let last = self
            .snapshot
            .as_ref()
            .map(|snapshot| {
                format!(
                    "{} {}",
                    tr("上次完成："),
                    snapshot
                        .checked_at
                        .with_timezone(&Local)
                        .format("%Y-%m-%d %H:%M:%S")
                )
            })
            .or_else(|| {
                self.last_attempt.map(|at| {
                    format!(
                        "{} {}",
                        tr("本次开始："),
                        at.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S")
                    )
                })
            })
            .unwrap_or_default();
        format!("{current}\r\n{last}")
    }
    fn export_message(&self) -> String {
        if self.export_busy {
            tr("正在保存独立报告…").into()
        } else if let Some(error) = &self.export_error {
            format!("{} {}", tr("报告保存失败："), tr(error))
        } else if let Some(path) = &self.export_path {
            format!("{} {}", tr("报告已保存："), path.display())
        } else {
            tr("报告保存在本机配置目录的 reports 文件夹，每次生成新文件。").into()
        }
    }
    pub(super) fn footer(&self) -> String {
        if self.export_busy || self.export_error.is_some() || self.export_path.is_some() {
            self.export_message()
        } else {
            tr("兼容性检测只读取本机文件，不接管 Explorer。").into()
        }
    }
}

fn feature_name(feature: Feature) -> &'static str {
    tr(match feature {
        Feature::ClockAppearance => "时钟外观",
        Feature::ClockInput => "时钟点击与音量",
        Feature::TrayMirroring => "副屏托盘复制",
        Feature::SystemAreas => "系统区域与菜单",
    })
}
fn feature_status(report: Option<&Report>, feature: Feature) -> &'static str {
    let Some(report) = report else {
        return tr("等待本机检测");
    };
    let Some(item) = report.features.iter().find(|item| item.feature == feature) else {
        return tr("检查结果缺失，不能确认支持");
    };
    tr(match item.status {
        FeatureStatus::DiskMatchedLiveCheckRequired => "模块匹配 · 运行时仍需验证",
        FeatureStatus::UnsupportedModules => "模块未收录 · 暂不支持接管",
        FeatureStatus::MissingModules => "缺少所需模块",
        FeatureStatus::InspectionFailed => "模块检查失败",
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
    let mut lines = vec![format!(
        "{} {}",
        tr("检测时间："),
        snapshot.checked_at.to_rfc3339()
    )];
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
    }
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
            let half = (total - 24) / 2;
            self.section("本机兼容性", 224, 88, total - 330);
            self.button("检测兼容性", CHECK, 224 + total - 312, 82, 152);
            self.button("导出报告", EXPORT, 224 + total - 148, 82, 148);
            self.control("STATIC", "", OS, SS_LEFT as u32, 224, 132, total, 28);
            self.control("STATIC", "", PROFILE, SS_LEFT as u32, 224, 168, total, 44);
            self.control(
                "EDIT",
                "",
                MESSAGE,
                WS_TABSTOP
                    | WS_VSCROLL
                    | ES_MULTILINE as u32
                    | ES_READONLY as u32
                    | ES_AUTOVSCROLL as u32,
                224,
                220,
                total,
                54,
            );
            self.caption(
                "只读取本机系统文件；不接管 Explorer，不下载符号，也不更改设置。",
                224,
                290,
                total,
                44,
            );
            self.divider(224, 346, total);
            self.section("四项功能的模块匹配", 224, 366, total);
            for (index, feature) in FEATURES.into_iter().enumerate() {
                let x = 224 + (index as i32 % 2) * (half + 24);
                let y = 410 + (index as i32 / 2) * 80;
                self.label(feature_name(feature), x, y, half, 26);
                self.control(
                    "STATIC",
                    "",
                    FEATURE_STATUS + index as u16,
                    SS_LEFT as u32,
                    x,
                    y + 32,
                    half,
                    48,
                );
            }
            self.caption(
                "磁盘匹配不是本机实测通过；启用接管时仍会检查运行中的模块和控件归属。",
                224,
                584,
                total,
                44,
            );
            self.divider(224, 640, total);
            self.section("已登记的实测范围", 224, 660, total);
            self.control(
                "EDIT",
                "",
                EVIDENCE,
                WS_TABSTOP
                    | WS_VSCROLL
                    | ES_MULTILINE as u32
                    | ES_READONLY as u32
                    | ES_AUTOVSCROLL as u32,
                224,
                702,
                total,
                172,
            );
            self.caption("实测范围以所选资料的记录为准。单屏时钟和托盘准备通过，不代表点击、悬浮、音量、托盘复制与多屏已验收。", 224, 890, total, 64);
            self.divider(224, 968, total);
            self.section("已收录的模块资料", 224, 988, total);
            self.control(
                "EDIT",
                "",
                CATALOG,
                WS_TABSTOP
                    | WS_VSCROLL
                    | ES_MULTILINE as u32
                    | ES_READONLY as u32
                    | ES_AUTOVSCROLL as u32,
                224,
                1030,
                total,
                134,
            );
            self.caption("Windows 11 的多个版本可能共用任务栏结构。支持范围按精确模块判断，不按 23H2、24H2 或 25H2 标签放行。", 224, 1180, total, 64);
            let export = self.control(
                "EDIT",
                "",
                EXPORT_MESSAGE,
                WS_TABSTOP | ES_MULTILINE as u32 | ES_READONLY as u32 | ES_AUTOVSCROLL as u32,
                224,
                1260,
                total,
                48,
            );
            SendMessageW(export, EM_SETLIMITTEXT, 32768, 0);
            self.button("展开检测详情", DETAILS, 224, 1328, 196);
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
                1380,
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
            EnableWindow(self.field(CHECK), i32::from(!state.busy));
            EnableWindow(
                self.field(EXPORT),
                i32::from(snapshot.is_some() && !state.busy && !state.export_busy),
            );
            EnableWindow(self.field(DETAILS), i32::from(snapshot.is_some()));
            set_text(
                self.field(CHECK),
                if state.busy {
                    "检测中…"
                } else {
                    "检测兼容性"
                },
            );
            set_text(
                self.field(EXPORT),
                if state.export_busy {
                    "保存中…"
                } else {
                    "导出报告"
                },
            );
            set_text(
                self.field(OS),
                &snapshot
                    .map(|value| value.os.label())
                    .unwrap_or_else(|| tr("正在读取当前 Windows 信息…").into()),
            );
            let profile = snapshot
                .map(|value| match value.report.profile_id.as_deref() {
                    Some(id) if value.report.all_profile_images_matched => {
                        format!("{} {id}", tr("精确匹配的模块资料："))
                    }
                    Some(id) => format!("{} {id}", tr("部分模块匹配，适用功能见下方：")),
                    None => tr("本机模块尚未匹配已收录资料").into(),
                })
                .unwrap_or_else(|| tr("模块资料：等待检测").into());
            set_text(self.field(PROFILE), &profile);
            set_text(self.field(MESSAGE), &state.check_message());
            set_text(self.field(EXPORT_MESSAGE), &state.export_message());
            set_text(self.field(STATUS), &state.footer());
            for (index, feature) in FEATURES.into_iter().enumerate() {
                set_text(
                    self.field(FEATURE_STATUS + index as u16),
                    feature_status(snapshot.map(|value| &value.report), feature),
                );
            }
            set_text(self.field(EVIDENCE), &evidence_text(snapshot));
            set_text(self.field(CATALOG), &catalog_text(snapshot));
            set_text(
                self.field(DETAILS),
                if state.details_open {
                    "收起检测详情"
                } else {
                    "展开检测详情"
                },
            );
            if state.details_open
                && let Some(snapshot) = snapshot
            {
                set_text(self.field(DETAIL_TEXT), &details_text(snapshot));
            }
            ShowWindow(
                self.field(DETAIL_TEXT),
                if state.details_open && snapshot.is_some() {
                    SW_SHOWNA
                } else {
                    SW_HIDE
                },
            );
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
            assert_eq!(
                feature_status(Some(&value.report), feature),
                "Modules match · runtime checks still required"
            );
        }
        let tested_ids: Vec<_> = value
            .supported_profiles
            .iter()
            .filter(|profile| {
                matches!(
                    profile.profile_id.as_str(),
                    "windows11-26300.9818-x64"
                        | "windows11-26200.8457-x64"
                        | "windows11-26100.1742-x64"
                )
            })
            .map(|profile| profile.profile_id.clone())
            .collect();
        for id in tested_ids {
            value.report.profile_id = Some(id);
            value.report.all_profile_images_matched = false;
            assert!(selected_profile(&value).is_none());
            assert!(evidence_text(Some(&value)).contains("cannot apply"));
            value.report.all_profile_images_matched = true;
            let evidence = evidence_text(Some(&value));
            assert!(evidence.contains("Not validated"));
            assert!(evidence.contains("Tray mirroring"));
            assert!(evidence.contains("Recorded test (limited scope)"));
            assert_eq!(evidence.matches("Recorded test (limited scope)").count(), 3);
            assert_eq!(evidence.matches("Not validated").count(), 4);
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
        assert!(state.check_message().contains("Check failed:"));
        assert!(state.check_message().contains("Last completed:"));
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
