# 界面语言 / UI languages

在“常规设置 → 界面语言”中选择“跟随系统 / 简体中文 / English”，点击“保存设置”应用。首次运行及旧版配置默认跟随 Windows 显示语言；中文系统使用简体中文，其他尚未提供翻译的语言回退为英文。

In **General → Language**, choose **Use system setting / 简体中文 / English**, then save. New installations and older settings files default to the Windows display language. Chinese Windows uses Simplified Chinese; other languages currently fall back to English.

语言选择写入配置中的 `language`，值为 `system`、`zh_cn` 或 `en`。切换语言后刷新设置窗口和面板，原有时钟模板、城市名、日程标题和备注保持原文。星期及天气状况等内置显示字段按界面语言显示。农历、黄历、上游节假日公告和部分底层诊断保留原文；不使用在线翻译，也不上传用户文本。

The `language` preference is stored as `system`, `zh_cn` or `en`. Switching refreshes the UI while keeping existing clock templates, city names, entry titles and notes unchanged. Built-in weekday and weather-condition fields follow the UI language. Lunar/almanac content, upstream holiday notices and some low-level diagnostics retain their original text. No online translation is used and user text is not uploaded.

GitHub 不会按访问者本机语言自动选择 README，请使用顶部的 [简体中文](../README.md) / [English](../README.en.md) 链接。

## 翻译维护 / Maintaining translations

- 界面语言解析与查找：`src/i18n.rs`。Windows 显示语言通过 `GetUserDefaultUILanguage` 读取。
- 英文词条：`locales/en.json`。中文原文为查找键，缺失翻译保留原文。
- 仅在标签、选项和其他展示入口翻译；不能对用户输入框、保存数据或协议值作全局替换。
- `tools/Render-UI.ps1 -Language en` 可生成英文原生界面预览。
- 原生测试验证语言保存、切换、缓存刷新、模板保留、继承选项往返，以及明暗主题下的英文控件边界。
