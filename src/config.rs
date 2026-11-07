use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub schema_version: u32,
    pub clock: ClockConfig,
    pub tray: TrayConfig,
    pub weather: WeatherConfig,
    pub updates: UpdateConfig,
    pub appearance: Appearance,
    pub start_with_windows: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            clock: ClockConfig::default(),
            tray: TrayConfig::default(),
            weather: WeatherConfig::default(),
            updates: UpdateConfig::default(),
            appearance: Appearance::System,
            start_with_windows: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClockConfig {
    pub top_line: String,
    pub bottom_line: String,
    pub date_format: String,
    pub time_format: String,
    pub weather_format: String,
    pub show_seconds: bool,
    pub show_lunar: bool,
    pub show_almanac: bool,
    pub font_family: String,
    pub font_size: u32,
    pub width: u32,
    pub click_action: ClickAction,
    pub volume_scroll: bool,
    pub volume_mute: bool,
    pub top_color: String,
    pub bottom_color: String,
    pub alignment: litetray_adapter_protocol::clock::Alignment,
    pub tooltip_line: String,
    pub top_style: litetray_adapter_protocol::clock::LineStyle,
    pub bottom_style: litetray_adapter_protocol::clock::LineStyle,
}
impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            top_line: "%date% %weekday%  %time%".into(),
            bottom_line: "%weather%".into(),
            date_format: "yyyy/MM/dd".into(),
            time_format: "HH:mm".into(),
            weather_format: "%weather_condition% %weather_temp%".into(),
            show_seconds: true,
            show_lunar: true,
            show_almanac: true,
            font_family: "Microsoft YaHei UI".into(),
            font_size: 12,
            width: 0,
            click_action: ClickAction::CalendarWeather,
            volume_scroll: true,
            volume_mute: true,
            top_color: String::new(),
            bottom_color: String::new(),
            alignment: Default::default(),
            tooltip_line: String::new(),
            top_style: Default::default(),
            bottom_style: Default::default(),
        }
    }
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClickAction {
    #[default]
    CalendarWeather,
    SystemCalendar,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TrayConfig {
    pub enabled: bool,
    pub clock_enabled: bool,
    pub scope: MonitorScope,
    pub selected_devices: Vec<String>,
    pub show_icons: bool,
    /// Opt-in because this correction loads a hook DLL into QQ's tray thread.
    pub qq_popup_compatibility: bool,
    pub show_control_center: bool,
    pub copied_taskbars_only: bool,
}
impl Default for TrayConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            clock_enabled: false,
            scope: MonitorScope::All,
            selected_devices: vec![],
            show_icons: true,
            qq_popup_compatibility: false,
            show_control_center: true,
            copied_taskbars_only: false,
        }
    }
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorScope {
    #[default]
    All,
    Primary,
    Selected,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    OpenMeteo,
    MetNorway,
    QWeather,
    OpenWeather,
    Wttr,
}
impl Provider {
    pub const ALL: [Self; 5] = [
        Self::OpenMeteo,
        Self::MetNorway,
        Self::QWeather,
        Self::OpenWeather,
        Self::Wttr,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::OpenMeteo => "Open-Meteo",
            Self::MetNorway => "MET Norway",
            Self::QWeather => "和风天气",
            Self::OpenWeather => "OpenWeather",
            Self::Wttr => "wttr.in",
        }
    }
    pub fn credential_target(self) -> Option<&'static str> {
        match self {
            Self::QWeather => Some("LiteTray/weather/qweather"),
            Self::OpenWeather => Some("LiteTray/weather/openweather"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WeatherConfig {
    pub enabled: bool,
    pub city: String,
    pub latitude: f64,
    pub longitude: f64,
    pub providers: Vec<Provider>,
    pub refresh_minutes: u32,
    pub timeout_seconds: u64,
    pub max_data_age_minutes: i64,
    pub cache_max_age_hours: i64,
    pub qweather_host: String,
}
impl Default for WeatherConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            city: "深圳 · 龙华".into(),
            latitude: 22.696,
            longitude: 114.045,
            providers: vec![Provider::OpenMeteo, Provider::MetNorway, Provider::Wttr],
            refresh_minutes: 10,
            timeout_seconds: 10,
            max_data_age_minutes: 180,
            cache_max_age_hours: 24,
            qweather_host: String::new(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateConfig {
    pub repository: String,
    pub include_prerelease: bool,
    pub check_on_startup: bool,
}
impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            repository: "Lianzy-Baimiao/LiteTray".into(),
            include_prerelease: false,
            check_on_startup: false,
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "不支持配置版本 {}；当前支持 {}",
                self.schema_version, SCHEMA_VERSION
            ));
        }
        if !self.weather.latitude.is_finite()
            || !(-90.0..=90.0).contains(&self.weather.latitude)
            || !self.weather.longitude.is_finite()
            || !(-180.0..=180.0).contains(&self.weather.longitude)
        {
            return Err("经纬度无效".into());
        }
        if !(5..=180).contains(&self.weather.refresh_minutes) {
            return Err("天气刷新间隔应为 5–180 分钟".into());
        }
        if !(3..=30).contains(&self.weather.timeout_seconds) {
            return Err("请求超时应为 3–30 秒".into());
        }
        if !(15..=1440).contains(&self.weather.max_data_age_minutes)
            || !(1..=72).contains(&self.weather.cache_max_age_hours)
        {
            return Err("天气新鲜度或缓存时限无效".into());
        }
        if self.weather.enabled && self.weather.providers.is_empty() {
            return Err("请至少启用一个天气来源".into());
        }
        let mut providers = std::collections::HashSet::new();
        if self.weather.providers.iter().any(|p| !providers.insert(p)) {
            return Err("天气来源不能重复".into());
        }
        if self.weather.city.trim().is_empty() || self.weather.city.len() > 128 {
            return Err("城市名称无效".into());
        }
        if !(8..=32).contains(&self.clock.font_size)
            || self.clock.width != 0 && !(80..=600).contains(&self.clock.width)
        {
            return Err("时钟字号或宽度超出范围".into());
        }
        litetray_adapter_protocol::clock::validate_font_family(&self.clock.font_family)?;
        crate::clock::validate_pattern(&self.clock.date_format, true)?;
        crate::clock::validate_pattern(&self.clock.time_format, false)?;
        crate::clock::weather_format::validate(&self.clock.weather_format)?;
        self.clock.top_style.validate()?;
        self.clock.bottom_style.validate()?;
        if self.clock.top_line.len() > 1024
            || self.clock.bottom_line.len() > 1024
            || self.clock.tooltip_line.len() > 1024
            || self.clock.font_family.len() > 128
        {
            return Err("时钟配置内容过长".into());
        }
        for color in [&self.clock.top_color, &self.clock.bottom_color] {
            if !color.is_empty() {
                litetray_adapter_protocol::clock::validate_color(color)?;
            }
        }
        for text in [
            &self.clock.top_line,
            &self.clock.bottom_line,
            &self.clock.tooltip_line,
        ] {
            crate::clock::validate_template(text)?;
        }
        validate_repository(&self.updates.repository)?;
        if self.weather.providers.contains(&Provider::QWeather) {
            validate_qweather_host(&self.weather.qweather_host)?;
        }
        if self.tray.scope == MonitorScope::Selected && self.tray.selected_devices.is_empty() {
            return Err("请选择至少一个显示器".into());
        }
        crate::monitors::validate_preferences(&self.tray.selected_devices)?;
        Ok(())
    }
}
pub fn validate_repository(repository: &str) -> Result<(), String> {
    let parts: Vec<_> = repository.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|p| {
            p.is_empty()
                || *p == "."
                || *p == ".."
                || !p
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        })
    {
        return Err("GitHub 仓库应填写 owner/repository".into());
    }
    Ok(())
}
pub fn validate_qweather_host(host: &str) -> Result<(), String> {
    if host.is_empty()
        || !host.ends_with(".qweatherapi.com")
        || !host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c))
    {
        return Err("和风 API Host 应为控制台分配的 *.qweatherapi.com 域名".into());
    }
    Ok(())
}

pub fn data_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("LITETRAY_DATA_DIR") {
        return PathBuf::from(path);
    }
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LiteTray")
}
pub fn load(path: &Path) -> Result<Config, String> {
    match fs::read(path) {
        Ok(bytes) => {
            if bytes.len() > 1024 * 1024 {
                return Err("配置文件过大".into());
            }
            let config: Config =
                serde_json::from_slice(&bytes).map_err(|e| format!("配置解析失败：{e}"))?;
            config.validate()?;
            Ok(config)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(format!("配置读取失败：{e}")),
    }
}
pub fn save(path: &Path, config: &Config) -> Result<(), String> {
    config.validate()?;
    atomic_write(
        path,
        &serde_json::to_vec_pretty(config).map_err(|e| e.to_string())?,
    )
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut f = fs::File::create(&temporary).map_err(|e| e.to_string())?;
    f.write_all(bytes)
        .and_then(|_| f.sync_all())
        .map_err(|e| e.to_string())?;
    drop(f);
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };
        let from: Vec<_> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
    }
    #[cfg(not(windows))]
    fs::rename(temporary, path).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clock_style_roundtrip_defaults_and_validation() {
        let mut c: Config = serde_json::from_str(r#"{"clock":{"top_line":"%time%"}}"#).unwrap();
        assert!(c.clock.top_color.is_empty() && c.clock.tooltip_line.is_empty());
        assert_eq!(c.clock.top_style, Default::default());
        assert_eq!(c.clock.bottom_style, Default::default());
        c.clock.top_color = "#00FFff".into();
        c.clock.bottom_color = "#80443322".into();
        c.clock.tooltip_line = "%date%\n%weather%".into();
        c.clock.alignment = litetray_adapter_protocol::clock::Alignment::Right;
        c.clock.top_style = litetray_adapter_protocol::clock::LineStyle {
            font_family: Some("Consolas".into()),
            font_size: Some(20),
            alignment: Some(litetray_adapter_protocol::clock::Alignment::Native),
        };
        c.clock.bottom_style.font_size = Some(10);
        c.validate().unwrap();
        let restored: Config = serde_json::from_slice(&serde_json::to_vec(&c).unwrap()).unwrap();
        assert_eq!(restored.clock.tooltip_line, c.clock.tooltip_line);
        assert_eq!(restored.clock.alignment, c.clock.alignment);
        assert_eq!(restored.clock.top_style, c.clock.top_style);
        assert_eq!(restored.clock.bottom_style, c.clock.bottom_style);
        c.clock.bottom_style.font_size = Some(40);
        assert!(c.validate().is_err());
        c.clock.bottom_style.font_size = None;
        c.clock.tooltip_line = "%unsupported%".into();
        assert!(c.validate().is_err());
        c.clock.tooltip_line.clear();
        c.clock.top_color = "red".into();
        assert!(c.validate().is_err());
    }
    #[test]
    fn defaults_are_valid_and_missing_fields_migrate() {
        Config::default().validate().unwrap();
        let c: Config = serde_json::from_str("{}").unwrap();
        c.validate().unwrap();
        assert!(!c.tray.qq_popup_compatibility);
        let old: Config =
            serde_json::from_str(r#"{"tray":{"enabled":true,"show_icons":true}}"#).unwrap();
        assert!(
            !old.tray.qq_popup_compatibility,
            "existing tray mirroring must not opt into QQ injection"
        );
        assert!(old.clock.volume_scroll && old.clock.volume_mute);
    }
    #[test]
    fn rejects_invalid_coords_and_duplicate_providers() {
        let mut c = Config::default();
        c.weather.latitude = f64::NAN;
        assert!(c.validate().is_err());
        c.weather.latitude = 20.;
        c.weather.providers = vec![Provider::OpenMeteo, Provider::OpenMeteo];
        assert!(c.validate().is_err());
    }
    #[test]
    fn rejects_host_and_repo_injection() {
        for r in ["a/b/c", "../x", "x/y?token=z", "https://github.com/a/b"] {
            assert!(validate_repository(r).is_err());
        }
        assert!(validate_qweather_host("evil.test/path.qweatherapi.com").is_err());
        assert!(validate_qweather_host("abc.qweatherapi.com").is_ok());
    }
    #[test]
    fn refuses_future_schema() {
        let c = Config {
            schema_version: 999,
            ..Config::default()
        };
        assert!(c.validate().is_err());
    }
}
