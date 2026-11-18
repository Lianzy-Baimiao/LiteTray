//! Settings and flyout appearance. Taskbar typography has its own settings.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InterfaceStyle {
    #[default]
    RefinedNative,
    SoftSkeuomorphic,
    LiquidGlass,
}

impl InterfaceStyle {
    pub const ALL: [Self; 3] = [
        Self::RefinedNative,
        Self::SoftSkeuomorphic,
        Self::LiquidGlass,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::RefinedNative => "精修原生",
            Self::SoftSkeuomorphic => "轻拟物质感",
            Self::LiquidGlass => "液态玻璃",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CornerStyle {
    #[default]
    Rounded,
    Square,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct InterfaceConfig {
    pub style: InterfaceStyle,
    pub corners: CornerStyle,
    /// Percent of the window tint removed to expose the system backdrop.
    pub transparency: u32,
    /// Percent of the fill of buttons and inputs under their sheen; glyphs
    /// stay opaque.
    pub card_clarity: u32,
    /// None follows the Windows message font, including after a theme change.
    pub font_family: Option<String>,
    /// Logical pixels at 96 DPI; the layout scales with the text.
    pub font_size: u32,
}

impl Default for InterfaceConfig {
    fn default() -> Self {
        Self {
            style: InterfaceStyle::RefinedNative,
            corners: CornerStyle::Rounded,
            transparency: 60,
            card_clarity: 35,
            font_family: None,
            font_size: 14,
        }
    }
}

impl InterfaceConfig {
    pub const MIN_FONT_SIZE: u32 = 12;
    pub const MAX_FONT_SIZE: u32 = 20;

    pub fn validate(&self) -> Result<(), String> {
        if self.transparency > 100 || self.card_clarity > 100 {
            return Err("透明程度和控件清晰度应为 0–100%".into());
        }
        if !(Self::MIN_FONT_SIZE..=Self::MAX_FONT_SIZE).contains(&self.font_size) {
            return Err("界面字号应为 12–20".into());
        }
        if let Some(family) = &self.font_family {
            litetray_adapter_protocol::clock::validate_font_family(family)?;
            if family.chars().any(char::is_control) {
                return Err("界面字体名称不能包含控制字符".into());
            }
        }
        Ok(())
    }

    pub fn background_alpha(&self) -> u8 {
        (255 * (100 - self.transparency.min(100)) / 100) as u8
    }

    pub fn card_alpha(&self) -> u8 {
        // The glass sheen keeps controls discernible at zero clarity. Full
        // clarity stops well short of opaque: a solid slab is not glass.
        (255 * self.card_clarity.min(100) * 30 / 10000) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn older_profiles_keep_native_appearance_and_taskbar_typography() {
        let config: Config = serde_json::from_str(
            r#"{"appearance":"dark","clock":{"font_family":"Microsoft YaHei UI","font_size":16},"start_with_windows":true}"#,
        )
        .unwrap();
        assert_eq!(config.interface, InterfaceConfig::default());
        assert_eq!(config.clock.font_family, "Microsoft YaHei UI");
        assert_eq!(config.clock.font_size, 16);
        assert!(config.start_with_windows);
        config.validate().unwrap();
    }

    #[test]
    fn appearance_round_trips_independent_transparency_and_font_choices() {
        let config = Config {
            interface: InterfaceConfig {
                style: InterfaceStyle::LiquidGlass,
                corners: CornerStyle::Square,
                transparency: 85,
                card_clarity: 20,
                font_family: Some("Microsoft YaHei UI".into()),
                font_size: 20,
            },
            ..Default::default()
        };
        let restored: Config =
            serde_json::from_slice(&serde_json::to_vec(&config).unwrap()).unwrap();
        assert_eq!(restored.interface, config.interface);
        restored.validate().unwrap();
        let mut changed = restored.interface.clone();
        changed.transparency = 0;
        assert_ne!(
            changed.background_alpha(),
            restored.interface.background_alpha()
        );
        assert_eq!(changed.card_alpha(), restored.interface.card_alpha());
    }

    #[test]
    fn rejects_invalid_interface_values_before_saving() {
        for (size, transparency, clarity) in
            [(11, 45, 75), (21, 45, 75), (14, 101, 75), (14, 45, 101)]
        {
            let config = InterfaceConfig {
                font_size: size,
                transparency,
                card_clarity: clarity,
                ..Default::default()
            };
            assert!(config.validate().is_err());
        }
        for family in ["", " ", "bad\0font", "bad\nfont"] {
            let config = InterfaceConfig {
                font_family: Some(family.into()),
                ..Default::default()
            };
            assert!(config.validate().is_err(), "{family:?}");
        }
    }
}
