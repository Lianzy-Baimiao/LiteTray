//! UI language selection and an embedded catalog. Only presentation labels
//! enter this module; persisted user text and protocol values stay unchanged.
use serde::{Deserialize, Serialize};
use std::{cell::Cell, collections::HashMap, sync::OnceLock};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    System,
    ZhCn,
    En,
}

impl Language {
    pub fn resolve(self) -> Self {
        match self {
            Self::System => {
                #[cfg(windows)]
                let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
                #[cfg(not(windows))]
                let id = 0x0409;
                Self::from_windows_language(id)
            }
            value => value,
        }
    }

    fn from_windows_language(id: u16) -> Self {
        if id & 0x03ff == 0x0004 {
            Self::ZhCn
        } else {
            Self::En
        }
    }
}

// Native windows and their labels belong to the UI thread. Tests can select
// a language on their isolated UI thread without mutating other test cases.
thread_local! { static CURRENT: Cell<Language> = const { Cell::new(Language::ZhCn) }; }

pub fn set_language(language: Language) {
    CURRENT.with(|current| current.set(language.resolve()));
}

pub fn english() -> bool {
    CURRENT.with(|current| current.get() == Language::En)
}

pub fn tr(text: &str) -> &str {
    if !english() {
        return text;
    }
    static ENGLISH: OnceLock<HashMap<String, String>> = OnceLock::new();
    ENGLISH
        .get_or_init(|| {
            serde_json::from_str(include_str!("../locales/en.json"))
                .expect("embedded English UI catalog must be valid")
        })
        .get(text)
        .map(String::as_str)
        .unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_windows_ui_language_with_english_fallback() {
        for id in [0x0804, 0x0404, 0x0c04] {
            assert_eq!(Language::from_windows_language(id), Language::ZhCn);
        }
        for id in [0x0409, 0x0809, 0x0411, 0x0407, 0] {
            assert_eq!(Language::from_windows_language(id), Language::En);
        }
        assert_eq!(Language::default(), Language::System);
        assert_eq!(Language::En.resolve(), Language::En);
    }

    #[test]
    fn embedded_catalog_translates_labels_and_leaves_unknown_text_intact() {
        set_language(Language::En);
        assert_eq!(tr("保存设置"), "Save settings");
        assert_eq!(tr("时钟显示"), "Clock content");
        assert_eq!(tr("兼容性"), "Compatibility");
        assert_eq!(tr("检测兼容性"), "Run check");
        assert_eq!(
            tr("模块匹配 · 运行时仍需验证"),
            "Modules match · runtime checks still required"
        );
        assert_eq!(tr("自己的会议 · %date%"), "自己的会议 · %date%");
        let catalog: HashMap<String, String> =
            serde_json::from_str(include_str!("../locales/en.json")).unwrap();
        assert!(catalog.len() >= 250);
        assert!(
            catalog
                .iter()
                .all(|(key, value)| !key.is_empty() && !value.is_empty())
        );
        set_language(Language::ZhCn);
        assert_eq!(tr("保存设置"), "保存设置");
    }
}
