//! Read-only Windhawk profile conversion. No registry writes, no network,
//! no activation of adapters. Every supplied setting receives a report row.
use crate::config::{Config, MonitorScope};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, io::Read, path::Path};
const CLOCK: &str = "taskbar-clock-customization";
const TRAY: &str = "taskbar-multi-tray";
pub const MAX_PROFILE: usize = 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Profile {
    schema_version: u32,
    modules: BTreeMap<String, Module>,
}
#[derive(Deserialize)]
struct Module {
    version: Option<String>,
    enabled: bool,
    settings: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Imported,
    Adapted,
    NeedsReview,
    Inactive,
}
impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Self::Imported => "已转换",
            Self::Adapted => "近似转换",
            Self::NeedsReview => "需确认",
            Self::Inactive => "未启用/不适用",
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub module: String,
    pub setting: String,
    pub status: Status,
    pub detail: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub schema: u32,
    pub candidate: Config,
    pub entries: Vec<Entry>,
    pub notes: Vec<String>,
}
impl Report {
    pub fn text(&self) -> String {
        let mut out = String::from("Windhawk 配置迁移预览\r\n\r\n");
        for status in [
            Status::Imported,
            Status::Adapted,
            Status::NeedsReview,
            Status::Inactive,
        ] {
            out.push_str(&format!(
                "{}：{} 项\r\n",
                status.label(),
                self.entries.iter().filter(|e| e.status == status).count()
            ));
        }
        out.push_str("\r\n");
        for note in &self.notes {
            out.push_str(note);
            out.push_str("\r\n");
        }
        out.push_str(&format!(
            "\r\n转换后的第一行：{}\r\n转换后的第二行：{}\r\n\r\n",
            self.candidate.clock.top_line, self.candidate.clock.bottom_line
        ));
        for entry in &self.entries {
            out.push_str(&format!(
                "[{}] {} / {}\r\n  {}\r\n",
                entry.status.label(),
                entry.module,
                entry.setting,
                entry.detail
            ));
        }
        out
    }
}
pub fn read(path: &Path, base: &Config) -> Result<Report, String> {
    preview(&read_bytes(path)?, base)
}
pub fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("读取迁移文件失败：{e}"))?;
    let mut bytes = Vec::new();
    file.take((MAX_PROFILE + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_PROFILE {
        return Err("Windhawk 配置文件过大".into());
    }
    Ok(bytes)
}
pub fn preview(bytes: &[u8], base: &Config) -> Result<Report, String> {
    if bytes.len() > MAX_PROFILE {
        return Err("Windhawk 配置文件过大".into());
    }
    base.validate()?;
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    let profile: Profile =
        serde_json::from_slice(bytes).map_err(|_| "不是有效的 LiteTray Windhawk 导出文件")?;
    if profile.schema_version != 1 {
        return Err("不支持此 Windhawk 导出版本".into());
    }
    if !profile.modules.contains_key(CLOCK) && !profile.modules.contains_key(TRAY) {
        return Err("文件中没有目标 Windhawk 模块".into());
    }
    if profile.modules.len() > 32
        || profile
            .modules
            .values()
            .map(|m| m.settings.len())
            .sum::<usize>()
            > 512
    {
        return Err("迁移项目数量过多".into());
    }
    let mut report = Report {
        schema: 1,
        candidate: base.clone(),
        entries: vec![],
        notes: vec![
            "只转换设置偏好；不会禁用 Windhawk，也不会自动启用任务栏接管。".into(),
            "近似转换和需确认项目请逐项核对。采用草稿后，点击“保存设置”才写入 LiteTray 配置。"
                .into(),
            "天气城市和坐标、数据源顺序、凭据、启动项及更新设置保留现有 LiteTray 配置。".into(),
        ],
    };
    for (id, module) in profile.modules {
        if id.len() > 128
            || id.contains(['\r', '\n', '\0'])
            || module.version.as_ref().is_some_and(|v| v.len() > 64)
        {
            return Err("模块标识无效".into());
        }
        for (key, value) in &module.settings {
            if key.len() > 256 || key.contains(['\r', '\n', '\0']) {
                return Err("迁移字段名无效".into());
            }
            let (status, detail) = if !module.enabled {
                (
                    Status::Inactive,
                    "来源模块未启用，保留当前 LiteTray 设置".into(),
                )
            } else if id == CLOCK {
                migrate_clock(key, value, &module.settings, &mut report.candidate)
            } else if id == TRAY {
                migrate_tray(key, value, &module.settings, &mut report.candidate)
            } else {
                (
                    Status::NeedsReview,
                    "不支持此模块；未读取其设置值到候选配置".into(),
                )
            };
            report.entries.push(Entry {
                module: id.clone(),
                setting: key.clone(),
                status,
                detail,
            });
        }
    }
    // Migration preferences never change whether an adapter is running.
    report.candidate.tray.enabled = base.tray.enabled;
    report.candidate.tray.clock_enabled = base.tray.clock_enabled;
    report.candidate.validate()?;
    Ok(report)
}
fn imported(detail: &str) -> (Status, String) {
    (Status::Imported, detail.into())
}
fn review(detail: &str) -> (Status, String) {
    (Status::NeedsReview, detail.into())
}
fn inactive(detail: &str) -> (Status, String) {
    (Status::Inactive, detail.into())
}
fn string(value: &Value) -> Option<&str> {
    value
        .as_str()
        .filter(|v| v.len() <= 4096 && !v.contains('\0'))
}
fn flag(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        _ => match value.as_i64() {
            Some(0) => Some(false),
            Some(1) => Some(true),
            _ => None,
        },
    }
}
fn number(value: &Value, min: u32, max: u32) -> Option<u32> {
    value
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| (min..=max).contains(n))
}
fn setting<'a>(settings: &'a BTreeMap<String, Value>, key: &str) -> Option<&'a str> {
    settings.get(key).and_then(string)
}
fn migrate_tray(
    key: &str,
    value: &Value,
    settings: &BTreeMap<String, Value>,
    config: &mut Config,
) -> (Status, String) {
    match key {
        "monitorMode" => match string(value) {
            Some("all") => {
                config.tray.scope = MonitorScope::All;
                imported("显示范围：所有显示器")
            }
            Some("selected") => review(
                "旧模块使用枚举序号，不能可靠对应长期设备标识；请在多屏托盘页重新选择，当前显示范围保留",
            ),
            _ => review("未知显示范围，保留当前设置"),
        },
        "selectedMonitors" => {
            if setting(settings, "monitorMode") == Some("all") {
                inactive("所有显示器模式不使用此序号列表")
            } else {
                review("请根据当前设备名重新选择显示器；不把旧序号猜测为当前屏幕")
            }
        }
        "components" => match string(value) {
            Some(mode @ ("all" | "tray" | "controlCenter")) => {
                config.tray.show_icons = mode != "controlCenter";
                config.tray.show_control_center = mode != "tray";
                imported("已转换托盘图标及控制中心显示偏好")
            }
            _ => review("未知组件选择，保留当前设置"),
        },
        "componentsScope" => match string(value) {
            Some(scope @ ("allTaskbars" | "copiedTaskbars")) => {
                config.tray.copied_taskbars_only = scope == "copiedTaskbars";
                imported("已转换组件规则的作用范围")
            }
            _ => review("未知组件作用范围，保留当前设置"),
        },
        "enableTreeDump" => inactive("来源模块的调试开关不迁移；LiteTray 单独提供诊断功能"),
        _ => review("暂不支持此托盘设置，未改变候选配置"),
    }
}
fn migrate_clock(
    key: &str,
    value: &Value,
    settings: &BTreeMap<String, Value>,
    config: &mut Config,
) -> (Status, String) {
    match key {
        "TopLine" | "BottomLine" => {
            let Some(text) = string(value) else {
                return review("模板值无效，保留当前模板");
            };
            match convert_template(text, settings) {
                Ok((text, adapted)) if text.len() <= 1024 => {
                    if key == "TopLine" {
                        config.clock.top_line = text
                    } else {
                        config.clock.bottom_line = text
                    };
                    if adapted {
                        (Status::Adapted,"网页天气变量改为 LiteTray 结构化天气摘要；不保留网页抓取、截取和替换规则".into())
                    } else {
                        imported("已转换模板及其日期/时间格式")
                    }
                }
                Ok(_) => review("展开后的模板过长，保留当前模板"),
                Err(reason) => review(&format!("{reason}；保留整行当前模板")),
            }
        }
        "ShowSeconds" => match flag(value) {
            Some(enabled) => {
                config.clock.show_seconds = enabled;
                imported("已转换显示秒偏好；显式秒变量仍按模板显示")
            }
            None => review("显示秒值无效"),
        },
        "Width" => match number(value, 80, 600) {
            Some(width) => {
                config.clock.width = width;
                imported("已转换时钟宽度")
            }
            None => review("宽度不在 80–600 范围内，保留当前宽度"),
        },
        "TimeStyle.FontFamily" | "DateStyle.FontFamily" => {
            let Some(family) = string(value) else {
                return review("字体名称无效");
            };
            if family.is_empty() {
                return inactive("来源使用系统字体，当前 LiteTray 字体保留");
            }
            if let Err(error) = litetray_adapter_protocol::clock::validate_font_family(family) {
                return review(&error);
            }
            let line = if key.starts_with("TimeStyle") {
                &mut config.clock.top_style
            } else {
                &mut config.clock.bottom_style
            };
            line.font_family = Some(family.into());
            imported("已转换此行字体；未安装字体由 Windows 回退显示")
        }
        "TimeStyle.FontSize" | "DateStyle.FontSize" => {
            if value.as_u64() == Some(0) {
                return inactive("来源未指定字号，当前字号保留");
            }
            let Some(size) = number(value, 8, 32) else {
                return review("字号超出 8–32 范围或不是整数，保留当前字号");
            };
            let line = if key.starts_with("TimeStyle") {
                &mut config.clock.top_style
            } else {
                &mut config.clock.bottom_style
            };
            line.font_size = Some(size);
            imported("已转换此行字号")
        }
        "TimeStyle.TextColor" | "DateStyle.TextColor" => {
            let Some(color) = string(value) else {
                return review("颜色值无效");
            };
            if color.is_empty() {
                return inactive("来源保留原生颜色，当前颜色设置保留");
            }
            if let Err(error) = litetray_adapter_protocol::clock::validate_color(color) {
                return review(&error);
            }
            if key.starts_with("TimeStyle") {
                config.clock.top_color = color.into();
            } else {
                config.clock.bottom_color = color.into();
            }
            imported("已转换此行文字颜色")
        }
        "TimeStyle.TextAlignment" | "DateStyle.TextAlignment" => {
            use litetray_adapter_protocol::clock::Alignment;
            if value.as_str() == Some("") {
                return inactive("来源未指定对齐，当前设置保留");
            }
            let alignment = match string(value) {
                Some("Left") => Alignment::Left,
                Some("Center") => Alignment::Center,
                Some("Right") => Alignment::Right,
                _ => return review("暂不支持此对齐方式，保留原设置"),
            };
            let line = if key.starts_with("TimeStyle") {
                &mut config.clock.top_style
            } else {
                &mut config.clock.bottom_style
            };
            line.alignment = Some(alignment);
            imported("已转换此行文字对齐方式")
        }
        "DateFormat" | "TimeFormat" | "WeekdayFormat" => {
            let used = match key {
                "DateFormat" => "%date%",
                "TimeFormat" => "%time%",
                _ => "%weekday%",
            };
            if !["TopLine", "BottomLine"]
                .iter()
                .filter_map(|k| setting(settings, k))
                .any(|v| v.contains(used))
            {
                return inactive("两行模板未引用此格式");
            }
            let Some(format) = string(value) else {
                return review("日期/时间格式值无效");
            };
            if format.is_empty() {
                return (
                    Status::Adapted,
                    "来源采用系统格式，候选模板使用 LiteTray 默认格式".into(),
                );
            }
            match format_pattern(format, key == "TimeFormat") {
                Ok(_) => imported("格式将展开到引用它的模板中；该行是否迁移成功以模板报告为准"),
                Err(e) => review(&e),
            }
        }
        "DateLocale" => match string(value) {
            Some("") | Some("zh-CN") => imported("中文日期标签与本地时间"),
            _ => review("暂不支持该日期区域设置；依赖的日期模板将保留原值"),
        },
        "WeekdayFormatCustom" => {
            if setting(settings, "WeekdayFormat") == Some("custom") {
                review("暂不支持自定义星期名称；依赖的模板保留原值")
            } else {
                inactive("当前星期格式未选用自定义名称")
            }
        }
        "WebContentsUpdateInterval" => match number(value, 5, 180) {
            Some(minutes) => {
                config.weather.refresh_minutes = minutes;
                imported("已转换天气刷新间隔（分钟）")
            }
            None => review("刷新间隔不在 5–180 分钟范围内，保留当前值"),
        },
        "MiddleLine" => match string(value) {
            Some("") | Some("-") => inactive("未启用第三行"),
            _ => review("当前仅提供两行时钟，第三行未迁移"),
        },
        "WebContentWeatherLocation" => {
            review("城市文字无法可靠转换为经纬度；请在天气来源页确认位置，当前城市和坐标保留")
        }
        "WebContentWeatherUnits" => {
            if string(value) == Some("metric") {
                imported("LiteTray 当前使用摄氏度和 km/h")
            } else {
                review("当前仅支持公制天气单位")
            }
        }
        "WebContentsItems[0].Url" => {
            if weather_url(settings) {
                (Status::Adapted,"识别到 wttr.in 天气来源；不复制 URL 或执行网页请求，使用当前 LiteTray 天气配置".into())
            } else {
                review("不导入网页 URL；其内容不能假定为天气")
            }
        }
        key if key.starts_with("WebContentsItems[") => {
            if value.as_str() == Some("") {
                inactive("空网页处理设置")
            } else {
                review("网页抓取、截取及替换规则不迁移；内置天气来源使用结构化解析和中文描述")
            }
        }
        key if key.starts_with("DataCollection.") || key.starts_with("MediaPlayer.") => {
            inactive("硬件/媒体采集设置不迁移；模板若引用相关变量会单独报告")
        }
        "TimeZones[0]" if value.as_str() == Some("") => inactive("使用本地时区"),
        _ if value.as_str() == Some("") => inactive("此项为空，当前实现无对应设置"),
        _ => review("暂不支持此时钟设置，未改变候选配置"),
    }
}
fn weather_url(settings: &BTreeMap<String, Value>) -> bool {
    let Some(text) = setting(settings, "WebContentsItems[0].Url") else {
        return false;
    };
    url::Url::parse(text).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str() == Some("wttr.in")
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
    })
}
fn convert_template(
    text: &str,
    settings: &BTreeMap<String, Value>,
) -> Result<(String, bool), String> {
    if text == "-" {
        return Ok((String::new(), false));
    }
    let mut out = String::new();
    let mut rest = text;
    let mut adapted = false;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        let end = tail.find('%').ok_or("模板变量未闭合")?;
        match &tail[..end] {
            "date" => {
                for key in ["DateLocale", "DateFormat"] {
                    if settings.contains_key(key) && setting(settings, key).is_none() {
                        return Err("日期格式或区域值无效".into());
                    }
                }
                if !matches!(
                    setting(settings, "DateLocale"),
                    None | Some("") | Some("zh-CN")
                ) {
                    return Err("日期区域设置暂不支持".into());
                }
                let format = setting(settings, "DateFormat")
                    .filter(|s| !s.is_empty())
                    .unwrap_or("yyyy/MM/dd");
                out.push_str(&format_pattern(format, false)?);
            }
            "time" => {
                if settings.contains_key("TimeFormat") && setting(settings, "TimeFormat").is_none()
                {
                    return Err("时间格式值无效".into());
                }
                if let Some(format) = setting(settings, "TimeFormat").filter(|s| !s.is_empty()) {
                    out.push_str(&format_pattern(format, true)?)
                } else {
                    out.push_str("%time%")
                }
            }
            "weekday" => {
                if settings.contains_key("WeekdayFormat")
                    && setting(settings, "WeekdayFormat").is_none()
                {
                    return Err("星期格式值无效".into());
                }
                let pattern = setting(settings, "WeekdayFormat")
                    .filter(|s| !s.is_empty())
                    .unwrap_or("dddd");
                if !matches!(pattern, "ddd" | "dddd") {
                    return Err("星期名称格式暂不支持".into());
                }
                out.push_str(&format_pattern(pattern, false)?);
            }
            "web1" | "weather" if weather_url(settings) => {
                out.push_str("%weather%");
                adapted = true;
            }
            "newline" => out.push_str("%newline%"),
            _ => return Err("包含尚不支持或不能确认来源的模板变量".into()),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    crate::clock::validate_template(&out)?;
    Ok((out, adapted))
}

/// Translate a bounded subset of Windows date/time patterns without passing
/// untrusted text to strftime. Quoted literals are parsed, never reinterpreted.
fn format_pattern(pattern: &str, time: bool) -> Result<String, String> {
    let chars = pattern.chars().collect::<Vec<_>>();
    let mut i = 0;
    let mut out = String::new();
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\'' {
            if chars.get(i + 1) == Some(&'\'') {
                out.push('\'');
                i += 2;
                continue;
            }
            i += 1;
            let mut closed = false;
            while i < chars.len() {
                if chars[i] == '\'' {
                    if chars.get(i + 1) == Some(&'\'') {
                        out.push('\'');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    closed = true;
                    break;
                }
                if chars[i] == '%' {
                    out.push_str("%percent%")
                } else {
                    out.push(chars[i]);
                }
                i += 1;
            }
            if !closed {
                return Err("格式中的引号未闭合".into());
            }
            continue;
        }
        if ch.is_ascii_alphabetic() {
            let start = i;
            while chars.get(i) == Some(&ch) {
                i += 1;
            }
            let count = i - start;
            let token = match (time, ch, count) {
                (false, 'y', 2) => "year2",
                (false, 'y', 4) => "year",
                (false, 'M', 1) => "month",
                (false, 'M', 2) => "month2",
                (false, 'd', 1) => "day",
                (false, 'd', 2) => "day2",
                (false, 'd', 3) => "weekday_short",
                (false, 'd', 4) => "weekday",
                (true, 'H', 2) => "hour",
                (true, 'h', 2) => "hour12",
                (true, 'm', 2) => "minute",
                (true, 's', 2) => "second",
                (true, 't', 2) => "ampm",
                _ => return Err("包含暂不支持的日期/时间格式字段".into()),
            };
            out.push('%');
            out.push_str(token);
            out.push('%');
        } else {
            if ch == '%' {
                out.push_str("%percent%")
            } else {
                out.push(ch)
            };
            i += 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone};
    #[test]
    fn imports_independent_colors_and_alignment() {
        let mut base = Config::default();
        base.clock.bottom_color = "#112233".into();
        let report=preview(&profile(serde_json::json!({"TimeStyle.TextColor":"#FF0000","DateStyle.TextColor":"Red\"/>","TimeStyle.TextAlignment":"Right","DateStyle.TextAlignment":"Right"}),serde_json::json!({})),&base).unwrap();
        assert_eq!(report.candidate.clock.top_color, "#FF0000");
        assert_eq!(report.candidate.clock.bottom_color, "#112233");
        assert_eq!(
            report.candidate.clock.top_style.alignment,
            Some(litetray_adapter_protocol::clock::Alignment::Right)
        );
        let report=preview(&profile(serde_json::json!({"TimeStyle.TextAlignment":"Left","DateStyle.TextAlignment":"Right"}),serde_json::json!({})),&base).unwrap();
        assert_eq!(report.candidate.clock.alignment, base.clock.alignment);
        assert_eq!(
            report.candidate.clock.top_style.alignment,
            Some(litetray_adapter_protocol::clock::Alignment::Left)
        );
        assert_eq!(
            report.candidate.clock.bottom_style.alignment,
            Some(litetray_adapter_protocol::clock::Alignment::Right)
        );
        assert!(report.entries.iter().all(|e| e.status == Status::Imported));
    }
    #[test]
    fn imports_distinct_fonts_and_sizes_without_changing_common_defaults() {
        let base = Config::default();
        let report = preview(
            &profile(
                serde_json::json!({
                    "TimeStyle.FontFamily":"Consolas", "TimeStyle.FontSize":20,
                    "DateStyle.FontFamily":"Segoe UI", "DateStyle.FontSize":10
                }),
                serde_json::json!({}),
            ),
            &base,
        )
        .unwrap();
        assert_eq!(
            report.candidate.clock.top_style.font_family.as_deref(),
            Some("Consolas")
        );
        assert_eq!(
            report.candidate.clock.bottom_style.font_family.as_deref(),
            Some("Segoe UI")
        );
        assert_eq!(report.candidate.clock.top_style.font_size, Some(20));
        assert_eq!(report.candidate.clock.bottom_style.font_size, Some(10));
        assert_eq!(report.candidate.clock.font_family, base.clock.font_family);
        assert_eq!(report.candidate.clock.font_size, base.clock.font_size);
        assert!(report.entries.iter().all(|e| e.status == Status::Imported));
        report.candidate.validate().unwrap();
    }
    #[test]
    fn partial_line_migration_preserves_invalid_empty_and_absent_fields() {
        use litetray_adapter_protocol::clock::{Alignment, LineStyle};
        let mut base = Config::default();
        base.clock.top_style = LineStyle {
            font_family: Some("Consolas".into()),
            font_size: Some(18),
            alignment: Some(Alignment::Right),
        };
        base.clock.bottom_style = LineStyle {
            font_family: Some("Arial".into()),
            font_size: Some(14),
            alignment: Some(Alignment::Left),
        };
        let report = preview(
            &profile(
                serde_json::json!({
                    "TimeStyle.FontFamily":"   ", "TimeStyle.FontSize":0,
                    "DateStyle.FontFamily":"", "DateStyle.FontSize":9,
                    "DateStyle.TextAlignment":"Justify"
                }),
                serde_json::json!({}),
            ),
            &base,
        )
        .unwrap();
        assert_eq!(report.candidate.clock.top_style, base.clock.top_style);
        let mut expected = base.clock.bottom_style.clone();
        expected.font_size = Some(9);
        assert_eq!(report.candidate.clock.bottom_style, expected);
        for (key, status) in [
            ("TimeStyle.FontFamily", Status::NeedsReview),
            ("TimeStyle.FontSize", Status::Inactive),
            ("DateStyle.FontFamily", Status::Inactive),
            ("DateStyle.FontSize", Status::Imported),
            ("DateStyle.TextAlignment", Status::NeedsReview),
        ] {
            assert_eq!(
                report
                    .entries
                    .iter()
                    .find(|e| e.setting == key)
                    .unwrap()
                    .status,
                status
            );
        }
        for invalid in [
            serde_json::json!(7),
            serde_json::json!(33),
            serde_json::json!(12.5),
            serde_json::json!("12"),
        ] {
            let report = preview(
                &profile(
                    serde_json::json!({"TimeStyle.FontSize":invalid}),
                    serde_json::json!({}),
                ),
                &base,
            )
            .unwrap();
            assert_eq!(report.candidate.clock.top_style, base.clock.top_style);
            assert_eq!(report.entries[0].status, Status::NeedsReview);
        }
    }
    fn profile(settings: Value, tray: Value) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"modules":{CLOCK:{"version":"1.8.1","enabled":true,"settings":settings},TRAY:{"version":"1.3.0","enabled":true,"settings":tray}}})).unwrap()
    }
    fn current() -> Value {
        serde_json::json!({"TopLine":"%date%  %time%","BottomLine":"%web1%","DateFormat":"yyyy/MM/dd dddd","TimeFormat":"HH':'mm':'ss","DateLocale":"","ShowSeconds":1,"Width":180,"WebContentsUpdateInterval":10,"WebContentsItems[0].Url":"https://wttr.in/Shenzhen+Longhua?format=%c+%t&lang=zh-cn"})
    }
    #[test]
    fn migrates_current_two_line_display_and_reports_every_field() {
        let clock = current();
        let tray = serde_json::json!({"monitorMode":"all","components":"tray","componentsScope":"copiedTaskbars"});
        let report = preview(&profile(clock.clone(), tray.clone()), &Config::default()).unwrap();
        assert_eq!(
            report.entries.len(),
            clock.as_object().unwrap().len() + tray.as_object().unwrap().len()
        );
        assert_eq!(report.candidate.clock.width, 180);
        assert_eq!(report.candidate.clock.bottom_line, "%weather%");
        let date = Local
            .with_ymd_and_hms(2026, 11, 6, 4, 5, 9)
            .single()
            .unwrap();
        assert_eq!(
            crate::clock::format(
                &report.candidate.clock.top_line,
                &report.candidate.clock,
                date,
                ""
            ),
            "2026/11/06 星期五  04:05:09"
        );
        assert!(
            report.candidate.tray.show_icons
                && !report.candidate.tray.show_control_center
                && report.candidate.tray.copied_taskbars_only
        );
        assert!(!report.candidate.tray.enabled && !report.candidate.tray.clock_enabled);
    }
    #[test]
    fn preserves_unrelated_user_configuration_and_unknown_template_whole_line() {
        let mut base = Config::default();
        base.weather.city = "My city".into();
        base.weather.latitude = 40.;
        base.start_with_windows = true;
        let report=preview(&profile(serde_json::json!({"TopLine":"%date% %cpu%","BottomLine":"%web1%","Width":2,"Custom.Secret":"do-not-copy-token"}),serde_json::json!({})),&base).unwrap();
        assert_eq!(report.candidate.clock.top_line, base.clock.top_line);
        assert_eq!(report.candidate.clock.bottom_line, base.clock.bottom_line);
        assert_eq!(report.candidate.clock.width, base.clock.width);
        assert_eq!(report.candidate.weather.city, "My city");
        assert_eq!(report.candidate.weather.latitude, 40.);
        assert!(report.candidate.start_with_windows);
        assert!(
            report
                .entries
                .iter()
                .all(|e| e.status == Status::NeedsReview)
        );
        assert!(
            !serde_json::to_string(&report)
                .unwrap()
                .contains("do-not-copy-token")
        );
    }
    #[test]
    fn refuses_to_guess_monitor_numbers_or_weather_coordinates() {
        let report = preview(
            &profile(
                serde_json::json!({"WebContentWeatherLocation":"Unmapped city"}),
                serde_json::json!({"monitorMode":"selected","selectedMonitors":"2,1"}),
            ),
            &Config::default(),
        )
        .unwrap();
        assert_eq!(report.candidate.tray.scope, MonitorScope::All);
        assert!(report.candidate.tray.selected_devices.is_empty());
        assert!(
            report
                .entries
                .iter()
                .all(|e| e.status == Status::NeedsReview)
        );
    }
    #[test]
    fn rejects_unknown_schema_and_oversized_input() {
        assert!(preview(&vec![b' '; MAX_PROFILE + 1], &Config::default()).is_err());
        assert!(preview(br#"{"schemaVersion":2,"modules":{}}"#, &Config::default()).is_err());
        assert!(preview(br#"{"schemaVersion":1,"modules":{}}"#, &Config::default()).is_err());
    }
    #[test]
    fn does_not_fetch_or_import_arbitrary_weather_urls() {
        for url in [
            "http://wttr.in/x",
            "https://wttr.in.evil.invalid/x",
            "https://user:secret@wttr.in/x",
            "https://evil.invalid/?token=do-not-copy-token",
        ] {
            let report=preview(&profile(serde_json::json!({"BottomLine":"weather %web1%","WebContentsItems[0].Url":url}),serde_json::json!({})),&Config::default()).unwrap();
            assert!(
                report
                    .entries
                    .iter()
                    .all(|e| e.status == Status::NeedsReview)
            );
            assert!(
                !serde_json::to_string(&report)
                    .unwrap()
                    .contains("do-not-copy-token")
            );
        }
    }
    #[test]
    fn inactive_module_does_not_override_settings() {
        let mut v: Value =
            serde_json::from_slice(&profile(current(), serde_json::json!({}))).unwrap();
        v["modules"][CLOCK]["enabled"] = false.into();
        let report = preview(&serde_json::to_vec(&v).unwrap(), &Config::default()).unwrap();
        assert_eq!(report.candidate.clock.width, 280);
        assert!(report.entries.iter().all(|e| e.status == Status::Inactive));
    }
    #[test]
    fn malformed_dependent_format_preserves_whole_template() {
        let mut input = current();
        input["DateFormat"] = 12.into();
        let report = preview(&profile(input, serde_json::json!({})), &Config::default()).unwrap();
        assert_eq!(
            report.candidate.clock.top_line,
            Config::default().clock.top_line
        );
        assert_eq!(
            report
                .entries
                .iter()
                .find(|e| e.setting == "TopLine")
                .unwrap()
                .status,
            Status::NeedsReview
        );
    }
    #[test]
    fn pattern_quotes_and_unknown_fields_are_not_reinterpreted() {
        assert_eq!(
            format_pattern("yyyy'年'MM'月'dd'日'", false).unwrap(),
            "%year%年%month2%月%day2%日"
        );
        assert_eq!(
            format_pattern("HH':'mm '100%'", true).unwrap(),
            "%hour%:%minute% 100%percent%"
        );
        assert_eq!(
            format_pattern("HH''mm 'o''clock'", true).unwrap(),
            "%hour%'%minute% o'clock"
        );
        assert!(format_pattern("MMMM dddd", false).is_err());
        assert!(format_pattern("HH'broken", true).is_err());
    }
}
