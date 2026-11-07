//! Settings composition. The persistent window shell owns navigation; each
//! page is prepared while hidden, then swapped into the content viewport.
use super::*;
const SS_ENDELLIPSIS: u32 = 0x4000;

#[derive(PartialEq, Eq)]
pub(super) struct PageKey {
    pub page: usize,
    layout: (i32, i32, u32),
    clock: Vec<u8>,
}
impl PageKey {
    pub fn new(page: usize, layout: (i32, i32, u32), config: &Config) -> Option<Self> {
        matches!(page, 1 | 7 | 10).then(|| Self {
            page,
            layout,
            clock: serde_json::to_vec(&config.clock).expect("clock settings are serializable"),
        })
    }
}
pub(super) struct CachedPage {
    pub key: PageKey,
    pub window: HWND,
    pub children: Vec<HWND>,
    pub fields: HashMap<u16, HWND>,
}

pub(super) const PRESET: u16 = 830;
pub(super) const WEATHER_FORMAT: u16 = 221;
pub(super) const WEATHER_PRESET: u16 = 222;
pub(super) const WEATHER_FIELD: u16 = 223;
pub(super) const WEATHER_PRECISION: u16 = 224;
const WEATHER_SAMPLE: u16 = 225;
pub(super) const OPEN_WEATHER_FORMAT: u16 = 226;
pub(super) const PRESETS: [(&str, &str, &str); 4] = [
    ("Windows 简洁", "%time%", "%date%"),
    ("日期与星期", "%time%", "%month2%/%day2% %weekday_short%"),
    ("时间与天气", "%time%  %weekday_short%", "%weather%"),
    ("紧凑年月日", "%time%", "%date%"),
];
impl App {
    pub(super) unsafe fn tray_settings(&mut self, c: &Config, width: i32) {
        unsafe {
            let total = width - 260;
            let half = (total - 24) / 2;
            self.section("时钟与图标", 224, 88, total);
            self.label("时钟显示范围", 224, 137, half, 24);
            self.combo(
                &["所有显示器", "仅主显示器", "指定显示器"],
                match c.tray.scope {
                    MonitorScope::All => 0,
                    MonitorScope::Primary => 1,
                    MonitorScope::Selected => 2,
                },
                301,
                248 + half,
                130,
                half,
            );
            self.check(
                "复制主屏托盘图标到副屏",
                c.tray.enabled && c.tray.show_icons,
                308,
                224,
                185,
                total,
            );
            self.caption(
                "图标顺序、显隐和宽度跟随主屏，按实际内容排列。",
                224,
                225,
                total,
                24,
            );
            self.divider(224, 273, total);
            self.section("选择显示器", 224, 292, total - 160);
            self.button("刷新", 307, 224 + total - 112, 286, 112);
            self.caption(
                "使用“指定显示器”时，在下面选择需要显示时钟的屏幕。",
                224,
                335,
                total,
                24,
            );
            self.monitor_rows = monitors::choices(&self.monitor_snapshot, &c.tray.selected_devices);
            let list = self.control(
                "LISTBOX",
                "",
                305,
                WS_TABSTOP | WS_VSCROLL | LBS_NOTIFY as u32 | LBS_MULTIPLESEL as u32,
                224,
                377,
                total,
                132,
            );
            for (index, row) in self.monitor_rows.iter().enumerate() {
                SendMessageW(list, LB_ADDSTRING, 0, wide(&row.label).as_ptr() as isize);
                SendMessageW(list, LB_SETSEL, usize::from(row.selected), index as isize);
            }
            self.control(
                "STATIC",
                &self.monitor_summary(),
                306,
                SS_LEFT as u32,
                224,
                531,
                total,
                44,
            );
        }
    }

    pub(super) unsafe fn general_settings(&mut self, c: &Config, width: i32) {
        unsafe {
            let total = width - 260;
            let half = (total - 24) / 2;
            self.section("外观", 224, 88, total);
            self.label("颜色模式", 224, 140, half, 24);
            self.combo(
                &["跟随 Windows", "浅色", "深色"],
                match c.appearance {
                    Appearance::System => 0,
                    Appearance::Light => 1,
                    Appearance::Dark => 2,
                },
                501,
                248 + half,
                133,
                half,
            );
            self.caption(
                "强调色、界面字体与高对比度随系统变化。",
                224,
                187,
                total,
                24,
            );
            self.divider(224, 233, total);
            self.section("启动与后台运行", 224, 252, total);
            self.check(
                "登录 Windows 时启动 LiteTray",
                c.start_with_windows,
                502,
                224,
                299,
                total,
            );
            self.caption("关闭窗口后继续计时和提醒。\r\n退出 LiteTray 后停止提醒，已保存的数据仍保留在本机。", 224, 342, total, 44);
            self.divider(224, 410, total);
            self.section("数据与维护", 224, 429, total);
            let button = (total - 32) / 3;
            for (i, (name, id)) in [
                ("打开配置目录", 503),
                ("导出诊断信息", 504),
                ("退出 LiteTray", EXIT),
            ]
            .into_iter()
            .enumerate()
            {
                self.button(name, id, 224 + i as i32 * (button + 16), 477, button);
            }
            self.caption(
                "配置、日程和提醒记录均保存在本机配置目录。",
                224,
                533,
                total,
                24,
            );
        }
    }

    pub(super) unsafe fn weather_settings(&mut self, c: &Config, width: i32) {
        unsafe {
            let total = width - 260;
            let half = (total - 24) / 2;
            let right = 248 + half;
            let small = (half - 16) / 2;
            self.section("位置", 224, 88, total - 180);
            self.check(
                "启用天气",
                c.weather.enabled,
                400,
                224 + total - 150,
                84,
                150,
            );
            self.location_controls(total);
            self.divider(224, 273, total);
            self.section("来源优先级", 224, 287, half);
            self.caption("按列表顺序请求，失败后自动切换。", 224, 320, half, 24);
            let list = self.control(
                "LISTBOX",
                "",
                404,
                WS_TABSTOP | WS_VSCROLL | LBS_NOTIFY as u32,
                224,
                356,
                half,
                100,
            );
            for provider in &c.weather.providers {
                SendMessageW(
                    list,
                    LB_ADDSTRING,
                    0,
                    wide(provider.label()).as_ptr() as isize,
                );
            }
            SendMessageW(list, LB_SETCURSEL, 0, 0);
            self.combo(&Provider::ALL.map(|p| p.label()), 0, 405, right, 300, half);
            for (i, (name, id)) in [("添加", 406), ("移除", 407), ("上移", 408), ("下移", 409)]
                .into_iter()
                .enumerate()
            {
                self.button(
                    name,
                    id,
                    right + i as i32 * (half + 8) / 4,
                    346,
                    (half - 24) / 4,
                );
            }
            self.edit(
                "刷新 / 分钟",
                &c.weather.refresh_minutes.to_string(),
                414,
                right,
                393,
                small,
            );
            self.edit(
                "超时 / 秒",
                &c.weather.timeout_seconds.to_string(),
                415,
                right + small + 16,
                393,
                small,
            );
            self.divider(224, 467, total);
            self.section("可选服务凭据", 224, 478, total);
            self.combo(&["和风天气", "OpenWeather"], 0, 410, 224, 518, 160);
            self.control(
                "EDIT",
                "",
                411,
                WS_TABSTOP | ES_AUTOHSCROLL as u32 | ES_PASSWORD as u32,
                400,
                518,
                total - 344,
                30,
            );
            SendMessageW(
                self.field(411),
                EM_SETCUEBANNER,
                0,
                wide("API Key").as_ptr() as isize,
            );
            self.button("保存", 412, 224 + total - 152, 516, 68);
            self.button("删除", 413, 224 + total - 68, 516, 68);
            self.label("和风 API Host", 224, 571, 160, 24);
            self.control(
                "EDIT",
                &c.weather.qweather_host,
                416,
                WS_TABSTOP | ES_AUTOHSCROLL as u32,
                400,
                566,
                total - 176,
                30,
            );
            SendMessageW(
                self.field(416),
                EM_SETCUEBANNER,
                0,
                wide("仅使用和风天气时填写").as_ptr() as isize,
            );
        }
    }

    pub(super) unsafe fn migration_settings(&mut self, width: i32, height: i32) {
        unsafe {
            let total = width - 260;
            self.section("导入 Windhawk 配置", 224, 88, total - 180);
            self.button("选择 JSON 文件", 701, 224 + total - 172, 82, 172);
            self.caption(
                "选择导出文件后，在下方检查转换结果，再采用到草稿。",
                224,
                131,
                total,
                24,
            );
            self.divider(224, 174, total);
            let text = self.migration.as_ref().map(migration::Report::text).unwrap_or_else(|| "还没有导入配置\r\n\r\n在 Windhawk 中导出设置为 JSON 文件，然后选择该文件。\r\n\r\n这里会列出可迁移的时钟和托盘设置，以及需要手动调整的项目。".into());
            let report = self.control(
                "EDIT",
                "",
                704,
                WS_TABSTOP | ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_READONLY as u32,
                224,
                196,
                total,
                (height - 492).max(236),
            );
            SendMessageW(report, EM_SETLIMITTEXT, 2 * 1024 * 1024, 0);
            SetWindowTextW(report, wide(&text).as_ptr());
            let y = (height - 274).max(452);
            self.button("采用到草稿", 702, 224, y, 160);
            self.button("导出转换报告", 703, 400, y, 168);
            EnableWindow(self.field(702), self.migration.is_some() as i32);
            EnableWindow(self.field(703), self.migration.is_some() as i32);
            self.caption(
                "采用后可继续调整，点击底部“保存设置”后生效。",
                224,
                y + 54,
                total,
                24,
            );
        }
    }

    unsafe fn section(&mut self, text: &str, x: i32, y: i32, width: i32) {
        unsafe {
            let hwnd = self.label(text, x, y, width, 26);
            widgets::heading(hwnd, self.dpi);
        }
    }
    unsafe fn caption(&mut self, text: &str, x: i32, y: i32, width: i32, height: i32) {
        unsafe {
            let hwnd = self.label(text, x, y, width, height);
            SetPropW(hwnd, wide("LiteTray.Muted").as_ptr(), 1usize as HANDLE);
        }
    }
    unsafe fn divider(&mut self, x: i32, y: i32, width: i32) {
        unsafe {
            let hwnd = self.label("", x, y, width, 1);
            widgets::divider(hwnd);
        }
    }
    unsafe fn clock_sample(&mut self, c: &Config, width: i32) {
        unsafe {
            let hwnd = self.control(
                "STATIC",
                "时钟预览",
                widgets::PREVIEW,
                0,
                224,
                84,
                width - 260,
                72,
            );
            widgets::preview(hwnd, &c.clock, self.dark(), self.dpi);
        }
    }
    unsafe fn preset_choices(&mut self, c: &Config, total: i32) {
        unsafe {
            for (index, (name, top, bottom)) in PRESETS.iter().enumerate() {
                let hwnd = self.button(
                    name,
                    PRESET + index as u16,
                    224 + index as i32 * (total + 8) / 4,
                    168,
                    (total - 24) / 4,
                );
                if c.clock.top_line == *top && c.clock.bottom_line == *bottom {
                    SetPropW(hwnd, wide("LiteTray.Selected").as_ptr(), 1usize as HANDLE);
                }
            }
        }
    }
    unsafe fn font_picker(
        &mut self,
        value: Option<&str>,
        id: u16,
        x: i32,
        y: i32,
        width: i32,
        inherit: bool,
    ) {
        unsafe {
            let mut names = widgets::fonts().to_vec();
            if let Some(value) = value.filter(|v| !v.is_empty())
                && !names.iter().any(|n| n == value)
            {
                names.insert(0, value.into());
            }
            if inherit {
                names.insert(0, "沿用公共字体".into());
            }
            let index = value
                .and_then(|v| names.iter().position(|n| n == v))
                .unwrap_or(0);
            self.combo(
                &names.iter().map(String::as_str).collect::<Vec<_>>(),
                index,
                id,
                x,
                y,
                width,
            );
            SendMessageW(
                self.field(id),
                CB_SETDROPPEDWIDTH,
                self.scale(width.max(320)) as usize,
                0,
            );
            SendMessageW(self.field(id), CB_SETMINVISIBLE, 10, 0);
        }
    }
    unsafe fn size_picker(
        &mut self,
        value: Option<u32>,
        id: u16,
        x: i32,
        y: i32,
        width: i32,
        inherit: bool,
    ) {
        unsafe {
            let mut sizes = (8..=32).map(|size| size.to_string()).collect::<Vec<_>>();
            if inherit {
                sizes.insert(0, "沿用".into());
            }
            let index = value
                .and_then(|value| sizes.iter().position(|s| s == &value.to_string()))
                .unwrap_or(0);
            self.combo(
                &sizes.iter().map(String::as_str).collect::<Vec<_>>(),
                index,
                id,
                x,
                y,
                width,
            );
        }
    }
    pub(super) unsafe fn clock_settings(&mut self, c: &Config, width: i32) {
        unsafe {
            let total = width - 260;
            let half = (total - 24) / 2;
            let right = 248 + half;
            self.clock_sample(c, width);
            self.preset_choices(c, total);
            self.edit("第一行 · 内容模板", &c.clock.top_line, 201, 224, 216, half);
            self.edit(
                "第二行 · 内容模板",
                &c.clock.bottom_line,
                202,
                right,
                216,
                half,
            );
            self.caption(
                "%time% 时间 · %date% 日期 · %weekday% 星期 · %weather% 天气",
                224,
                278,
                total - 150,
                22,
            );
            self.button("设置天气格式", OPEN_WEATHER_FORMAT, width - 172, 274, 136);
            for (id, value, x) in [
                (214, c.clock.date_format.clone(), 224),
                (215, clock::effective_time_pattern(&c.clock), right),
            ] {
                self.control(
                    "STATIC",
                    "",
                    id + 2,
                    SS_LEFT as u32 | SS_ENDELLIPSIS,
                    x,
                    310,
                    half,
                    22,
                );
                let edit = self.control(
                    "EDIT",
                    &value,
                    id,
                    WS_TABSTOP | ES_AUTOHSCROLL as u32,
                    x,
                    335,
                    half,
                    30,
                );
                SendMessageW(edit, EM_SETLIMITTEXT, 128, 0);
            }
            for (id, y) in [(218, 373), (219, 397)] {
                let hint = self.control(
                    "STATIC",
                    "",
                    id,
                    SS_LEFT as u32 | SS_ENDELLIPSIS,
                    224,
                    y,
                    total,
                    22,
                );
                SetPropW(hint, wide("LiteTray.Muted").as_ptr(), 1usize as HANDLE);
            }
            self.update_clock_samples();
            self.divider(224, 434, total);
            self.label("点击时钟", 224, 449, half, 22);
            self.combo(
                &["日历与天气", "系统日历"],
                usize::from(c.clock.click_action == ClickAction::SystemCalendar),
                208,
                224,
                476,
                half,
            );
            self.edit(
                "时钟宽度 · 留空随内容",
                &if c.clock.width == 0 {
                    String::new()
                } else {
                    c.clock.width.to_string()
                },
                207,
                right,
                449,
                half,
            );
            self.check("日历显示农历", c.clock.show_lunar, 204, 224, 522, half);
            self.check(
                "日期悬停显示黄历",
                c.clock.show_almanac,
                220,
                right,
                522,
                half,
            );
            let button = (total - 32) / 3;
            for (i, (label, id)) in [
                ("预览原生时钟", 209),
                ("启用任务栏时钟", 212),
                ("恢复系统时钟", 213),
            ]
            .into_iter()
            .enumerate()
            {
                self.button(label, id, 224 + i as i32 * (button + 16), 564, button);
            }
            // The persistent footer carries connection status; a long adapter
            // diagnostic must not push the actual settings out of the page.
        }
    }

    pub(super) unsafe fn update_clock_samples(&self) {
        unsafe {
            if self.page != 1 {
                return;
            }
            let mut c = self.draft.clock.clone();
            c.date_format = self.text(214);
            c.time_format = self.text(215);
            c.show_seconds = false;
            for (date, id, pattern, label, token, help) in [
                (
                    true,
                    216,
                    &c.date_format,
                    "日期格式",
                    "%date%",
                    "yyyy / yy 年 · MM 或 mm 月 · dd 日 · ddd / dddd 星期；例如 yyyymmdd",
                ),
                (
                    false,
                    217,
                    &c.time_format,
                    "时间格式",
                    "%time%",
                    "HH 24小时 · hh 12小时 · mm 分 · ss 秒 · tt 上午/下午；例如 HH:mm:ss",
                ),
            ] {
                match clock::validate_pattern(pattern, date) {
                    Ok(()) => {
                        set_text(
                            self.field(id),
                            &format!("{label} · {}", clock::format(token, &c, Local::now(), "")),
                        );
                        set_text(self.field(id + 2), help);
                    }
                    Err(error) => {
                        set_text(self.field(id), &format!("{label} · 请检查格式"));
                        set_text(self.field(id + 2), &error);
                    }
                }
            }
        }
    }
    pub(super) unsafe fn weather_format_settings(&mut self, c: &Config, width: i32) {
        unsafe {
            let total = width - 260;
            let half = (total - 24) / 2;
            self.clock_sample(c, width);
            self.label("天气预设", 224, 178, 110, 22);
            let mut presets = vec!["自定义"];
            presets.extend(clock::weather_format::PRESETS.iter().map(|(name, _)| *name));
            self.combo(&presets, 0, WEATHER_PRESET, 340, 173, 220);
            self.caption("选择后可继续编辑，保存后应用到任务栏。", 578, 178, total - 354, 22);
            self.edit("%weather% · 天气内容模板", &c.clock.weather_format, WEATHER_FORMAT, 224, 220, total);
            SendMessageW(self.field(WEATHER_FORMAT), EM_SETLIMITTEXT, 1024, 0);
            self.label("插入属性 · 添加到光标位置", 224, 294, half, 22);
            let mut fields = vec!["选择天气属性".to_string()];
            fields.extend(clock::weather_format::FIELDS.iter().map(|(token, label, _)| format!("{label}  %{token}%")));
            self.combo(&fields.iter().map(String::as_str).collect::<Vec<_>>(), 0, WEATHER_FIELD, 224, 321, half);
            SendMessageW(self.field(WEATHER_FIELD), CB_SETDROPPEDWIDTH, self.scale(470) as usize, 0);
            SendMessageW(self.field(WEATHER_FIELD), CB_SETMINVISIBLE, 12, 0);
            self.label("数值精度 · 也可单独写 :0、:1、:2", 248 + half, 294, half, 22);
            self.combo(&["保留各属性精度", "整数", "1 位小数", "2 位小数"], 0,
                WEATHER_PRECISION, 248 + half, 321, half);
            self.control("STATIC", "", WEATHER_SAMPLE, SS_LEFT as u32 | SS_ENDELLIPSIS,
                224, 369, total, 26);
            self.divider(224, 411, total);
            self.section("如何组合", 224, 429, total);
            self.caption("文字、空格和分隔符都可以自由输入。数值自带单位，例如：", 224, 469, total, 24);
            self.label("%weather_temp:1% → 23.4°C    %weather_humidity% → 68%", 224, 501, total, 24);
            self.caption("这些属性也可直接放入时钟第一行、第二行或悬停提示。", 224, 537, total, 24);
            self.caption("来源缺少的属性显示 —；今日温差与小时降雨取对应日期预报。", 224, 569, total, 24);
            self.update_weather_sample();
        }
    }
    pub(super) unsafe fn update_weather_sample(&self) {
        if self.page != 10 { return; }
        unsafe {
            let template = self.text(WEATHER_FORMAT);
            let summary = match clock::weather_format::validate(&template) {
                Ok(()) => {
                    let now = Local::now();
                    format!("示例 · {}", clock::weather_format::format(&template,
                        Some(&clock::weather_format::sample(now)), now))
                }
                Err(error) => error,
            };
            set_text(self.field(WEATHER_SAMPLE), &summary);
            let preset = clock::weather_format::PRESETS.iter()
                .position(|(_, value)| *value == template).map_or(0, |index| index + 1);
            SendMessageW(self.field(WEATHER_PRESET), CB_SETCURSEL, preset, 0);
        }
    }
    pub(super) unsafe fn choose_weather_format(&mut self, id: u16) {
        unsafe {
            let index = self.selected(id);
            match id {
                WEATHER_PRESET => {
                    if let Some((_, template)) = index.checked_sub(1).and_then(|i| clock::weather_format::PRESETS.get(i)) {
                        set_text(self.field(WEATHER_FORMAT), template);
                    }
                }
                WEATHER_FIELD => {
                    if let Some((token, _, numeric)) = index.checked_sub(1).and_then(|i| clock::weather_format::FIELDS.get(i)) {
                        let precision = self.selected(WEATHER_PRECISION);
                        let token = if *numeric && precision > 0 { format!("%{token}:{}%", precision - 1) }
                            else { format!("%{token}%") };
                        SendMessageW(self.field(WEATHER_FORMAT), EM_REPLACESEL, 1, wide(&token).as_ptr() as isize);
                        SetFocus(self.field(WEATHER_FORMAT));
                    }
                    SendMessageW(self.field(WEATHER_FIELD), CB_SETCURSEL, 0, 0);
                }
                WEATHER_PRECISION if index > 0 => {
                    let template = clock::weather_format::with_precision(&self.text(WEATHER_FORMAT), index - 1);
                    set_text(self.field(WEATHER_FORMAT), &template);
                }
                _ => {}
            }
            if self.collect_page().is_ok() {
                widgets::update_preview(self.field(widgets::PREVIEW), &self.draft.clock);
                self.update_weather_sample();
            }
        }
    }
    pub(super) unsafe fn style_settings(&mut self, c: &Config, width: i32) {
        unsafe {
            use litetray_adapter_protocol::clock::Alignment;
            let align = |a| match a {
                Alignment::Native => 0,
                Alignment::Left => 1,
                Alignment::Center => 2,
                Alignment::Right => 3,
            };
            let total = width - 260;
            let half = (total - 24) / 2;
            self.clock_sample(c, width);
            self.label("公共字体", 224, 178, half, 22);
            self.font_picker(Some(&c.clock.font_family), 205, 224, 205, half, false);
            self.label("字号", 248 + half, 178, 80, 22);
            self.size_picker(Some(c.clock.font_size), 206, 248 + half, 205, 80, false);
            self.label("公共对齐", 344 + half, 178, half - 96, 22);
            self.combo(
                &["保留原样", "左对齐", "居中", "右对齐"],
                align(c.clock.alignment),
                803,
                344 + half,
                205,
                half - 96,
            );
            self.divider(224, 253, total);
            for (title, line, color, color_id, font_id, x) in [
                (
                    "第一行",
                    &c.clock.top_style,
                    &c.clock.top_color,
                    801,
                    806,
                    224,
                ),
                (
                    "第二行",
                    &c.clock.bottom_style,
                    &c.clock.bottom_color,
                    802,
                    809,
                    248 + half,
                ),
            ] {
                self.section(title, x, 270, half);
                self.font_picker(
                    line.font_family.as_deref(),
                    font_id,
                    x,
                    306,
                    half - 96,
                    true,
                );
                self.size_picker(line.font_size, font_id + 1, x + half - 80, 306, 80, true);
                let color_width = half - 220;
                self.edit("文字颜色", color, color_id, x, 356, color_width);
                SendMessageW(
                    self.field(color_id),
                    EM_SETCUEBANNER,
                    0,
                    wide("跟随系统").as_ptr() as isize,
                );
                self.button(
                    "选色",
                    if color_id == 801 { 812 } else { 813 },
                    x + color_width + 8,
                    379,
                    56,
                );
                self.label("对齐", x + half - 144, 356, 144, 22);
                self.combo(
                    &["沿用公共对齐", "保留原样", "左对齐", "居中", "右对齐"],
                    line.alignment.map(|a| align(a) + 1).unwrap_or(0),
                    font_id + 2,
                    x + half - 144,
                    381,
                    144,
                );
            }
            self.caption(
                "选择“沿用”继承公共设置；颜色留空时跟随 Windows。",
                224,
                425,
                total,
                22,
            );
            self.divider(224, 462, total);
            self.label("鼠标悬停提示 · 留空使用系统提示", 224, 477, total, 22);
            let tip = self.control(
                "EDIT",
                &c.clock.tooltip_line.replace('\n', "\r\n"),
                804,
                WS_TABSTOP | ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_WANTRETURN as u32,
                224,
                505,
                total,
                48,
            );
            SendMessageW(tip, EM_SETLIMITTEXT, 1024, 0);
            SendMessageW(
                tip,
                EM_SETCUEBANNER,
                0,
                wide("留空使用系统提示；支持内容模板中的变量与换行").as_ptr() as isize,
            );
            self.button("预览原生时钟", 209, 224, 562, half);
            self.button("重置样式", 805, 248 + half, 562, half);
        }
    }

    pub(super) unsafe fn about_settings(&mut self, c: &Config, width: i32) {
        unsafe {
            let total = width - 260;
            self.section("LiteTray 轻托盘", 224, 88, total - 180);
            self.button("项目主页", 607, 224 + total - 144, 82, 144);
            self.caption(
                &format!(
                    "版本 {} · {UI_REVISION}\r\n让每块屏幕都有顺手的时钟、日历和托盘。",
                    env!("CARGO_PKG_VERSION")
                ),
                224,
                134,
                total,
                48,
            );
            self.divider(224, 209, total);
            self.section("应用更新", 224, 228, total);
            self.control(
                "STATIC",
                &self
                    .update
                    .as_ref()
                    .map(UpdateStatus::text)
                    .unwrap_or_else(|| "可从 GitHub 获取最新的 Releases 与 Tags。".into()),
                608,
                SS_LEFT as u32,
                224,
                270,
                total,
                56,
            );
            self.button("检查更新", UPDATE, 224, 340, 160);
            self.button("查看版本", OPEN_RELEASE, 400, 340, 150);
            self.divider(224, 400, total);
            self.check(
                "启动时检查更新",
                c.updates.check_on_startup,
                603,
                224,
                419,
                (total - 24) / 2,
            );
            self.check(
                "接收预览版本",
                c.updates.include_prerelease,
                602,
                248 + (total - 24) / 2,
                419,
                (total - 24) / 2,
            );
            self.edit("更新仓库", &c.updates.repository, 601, 224, 471, total);
            self.caption(
                "公开仓库无需密钥或登录。\r\n开发中的私有仓库暂不可访问，转为公开后即可检查。",
                224,
                547,
                total,
                44,
            );
        }
    }
}
