//! Settings composition. The persistent window shell owns navigation; each
//! page is prepared while hidden, then swapped into the content viewport.
use super::*;

pub(super) const PRESET: u16 = 830;
pub(super) const PRESETS: [(&str, &str, &str); 4] = [
    ("Windows 简洁", "%time%", "%date%"),
    ("日期与星期", "%time%", "%month2%/%day2% %weekday_short%"),
    ("时间与天气", "%time%  %weekday_short%", "%weather%"),
    ("完整日期", "%date% %weekday%", "%time%"),
];
impl App {
    pub(super) unsafe fn tray_settings(&mut self, c: &Config, width: i32) {
        unsafe {
            let total = width - 260;
            self.section("显示范围", 224, 88, total);
            self.label("选择显示自定义时钟的屏幕", 224, 128, total / 2, 26);
            self.combo(
                &["所有显示器", "仅主显示器", "指定显示器"],
                match c.tray.scope {
                    MonitorScope::All => 0,
                    MonitorScope::Primary => 1,
                    MonitorScope::Selected => 2,
                },
                301,
                224 + total / 2,
                124,
                total / 2,
            );
            self.section("托盘图标", 224, 205, total);
            self.check(
                "将主屏托盘图标复制到副屏",
                c.tray.enabled && c.tray.show_icons,
                308,
                224,
                248,
                total,
            );
            self.label("包含应用图标、隐藏图标入口和系统图标。\r\n顺序、显隐与宽度跟随主屏，按实际内容排列。",224,289,total,58);
            self.section("你的显示器", 224, 390, total - 180);
            self.button("刷新显示器", 307, 224 + total - 154, 382, 154);
            self.label(
                "指定屏幕时可多选；单击一行或按空格切换。",
                224,
                431,
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
                473,
                total,
                128,
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
                620,
                total,
                58,
            );
        }
    }
    pub(super) unsafe fn general_settings(&mut self, c: &Config, width: i32) {
        unsafe {
            let total = width - 260;
            self.section("外观", 224, 88, total);
            self.label("颜色模式", 224, 133, total / 2, 26);
            self.combo(
                &["跟随 Windows", "浅色", "深色"],
                match c.appearance {
                    Appearance::System => 0,
                    Appearance::Light => 1,
                    Appearance::Dark => 2,
                },
                501,
                224 + total / 2,
                128,
                total / 2,
            );
            self.label(
                "强调色、界面字体与高对比度设置跟随 Windows。",
                224,
                183,
                total,
                26,
            );
            self.section("启动与后台运行", 224, 264, total);
            self.check(
                "登录 Windows 时启动 LiteTray",
                c.start_with_windows,
                502,
                224,
                307,
                total,
            );
            self.label("关闭设置窗口后继续运行，日程、闹钟与计时仍会提醒。\r\n退出 LiteTray 后停止提醒；已保存的数据留在本机。",224,351,total,62);
            self.section("数据与维护", 224, 462, total);
            let button = (total - 32) / 3;
            for (i, (name, id)) in [
                ("打开配置目录", 503),
                ("导出诊断信息", 504),
                ("退出 LiteTray", EXIT),
            ]
            .into_iter()
            .enumerate()
            {
                self.button(name, id, 224 + i as i32 * (button + 16), 512, button);
            }
        }
    }
    pub(super) unsafe fn weather_settings(&mut self, c: &Config, width: i32) {
        unsafe {
            let total = width - 260;
            let half = (total - 24) / 2;
            let quarter = (half - 16) / 2;
            self.section("城市与刷新", 224, 88, total);
            self.check(
                "启用天气",
                c.weather.enabled,
                400,
                224 + total - 160,
                84,
                160,
            );
            self.edit("城市名称", &c.weather.city, 401, 224, 136, half);
            self.edit(
                "纬度",
                &c.weather.latitude.to_string(),
                402,
                248 + half,
                136,
                quarter,
            );
            self.edit(
                "经度",
                &c.weather.longitude.to_string(),
                403,
                264 + half + quarter,
                136,
                quarter,
            );
            self.edit(
                "刷新间隔 / 分钟",
                &c.weather.refresh_minutes.to_string(),
                414,
                224,
                208,
                half,
            );
            self.edit(
                "请求超时 / 秒",
                &c.weather.timeout_seconds.to_string(),
                415,
                248 + half,
                208,
                half,
            );
            self.section("天气来源", 224, 300, total);
            self.label(
                "上方优先；连接失败或数据过期时自动切换到下一个来源。",
                224,
                339,
                total,
                25,
            );
            let list = self.control(
                "LISTBOX",
                "",
                404,
                WS_TABSTOP | WS_VSCROLL | LBS_NOTIFY as u32,
                224,
                380,
                half,
                132,
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
            self.combo(
                &Provider::ALL.map(|p| p.label()),
                0,
                405,
                248 + half,
                380,
                half,
            );
            for (i, (name, id)) in [("添加", 406), ("移除", 407), ("上移", 408), ("下移", 409)]
                .into_iter()
                .enumerate()
            {
                self.button(
                    name,
                    id,
                    248 + half + (i % 2) as i32 * (quarter + 16),
                    426 + (i / 2) as i32 * 48,
                    quarter,
                );
            }
            self.section("可选服务凭据", 224, 555, total);
            self.label("Open-Meteo、MET Norway 与 wttr.in 可直接使用。\r\n仅使用和风天气或 OpenWeather 时需要对应服务的 API Key。",224,590,total,56);
            self.combo(&["和风天气", "OpenWeather"], 0, 410, 224, 660, 180);
            self.control(
                "EDIT",
                "",
                411,
                WS_TABSTOP | ES_AUTOHSCROLL as u32 | ES_PASSWORD as u32,
                420,
                660,
                total - 376,
                30,
            );
            self.button("保存", 412, 224 + total - 164, 658, 76);
            self.button("删除", 413, 224 + total - 76, 658, 76);
            self.edit(
                "和风 API Host · 使用其他来源时留空",
                &c.weather.qweather_host,
                416,
                224,
                720,
                total,
            );
        }
    }
    pub(super) unsafe fn migration_settings(&mut self, width: i32, height: i32) {
        unsafe {
            let total = width - 260;
            self.section("从 Windhawk 导入", 224, 88, total);
            self.label(
                "选择导出的 JSON 文件，查看转换结果，再采用到设置草稿。",
                224,
                130,
                total,
                42,
            );
            let button = (total - 32) / 3;
            for (i, (label, id)) in [
                ("选择配置文件", 701),
                ("采用到草稿", 702),
                ("导出迁移报告", 703),
            ]
            .into_iter()
            .enumerate()
            {
                self.button(label, id, 224 + i as i32 * (button + 16), 190, button);
            }
            EnableWindow(self.field(702), self.migration.is_some() as i32);
            EnableWindow(self.field(703), self.migration.is_some() as i32);
            self.section("转换结果", 224, 260, total);
            let text=self.migration.as_ref().map(migration::Report::text).unwrap_or_else(||"尚未选择文件。\r\n\r\n使用 tools/Export-WindhawkProfile.ps1 导出设置，然后在这里导入。\r\n检查转换结果后点击“采用到草稿”，保存设置后生效。".into());
            let report = self.control(
                "EDIT",
                "",
                704,
                WS_TABSTOP | ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_READONLY as u32,
                224,
                304,
                total,
                (height - 484).max(220),
            );
            SendMessageW(report, EM_SETLIMITTEXT, 2 * 1024 * 1024, 0);
            SetWindowTextW(report, wide(&text).as_ptr());
        }
    }
    unsafe fn section(&mut self, text: &str, x: i32, y: i32, width: i32) {
        unsafe {
            let hwnd = self.label(text, x, y, width, 26);
            widgets::heading(hwnd, self.dpi);
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
                90,
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
                    188,
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
            self.clock_sample(c, width);
            self.preset_choices(c, total);
            self.section("显示内容", 224, 242, total);
            self.edit("第一行", &c.clock.top_line, 201, 224, 278, half);
            self.edit("第二行", &c.clock.bottom_line, 202, 248 + half, 278, half);
            self.label(
                "变量：%time% 时间   %date% 日期   %weekday% 星期   %weather% 天气",
                224,
                346,
                total,
                24,
            );
            self.section("文字与交互", 224, 374, total);
            self.label("字体", 224, 412, half, 24);
            self.font_picker(Some(&c.clock.font_family), 205, 224, 438, half, false);
            self.label("字号", 248 + half, 412, 96, 24);
            self.size_picker(Some(c.clock.font_size), 206, 248 + half, 438, 96, false);
            self.edit(
                "宽度 · 留空自动",
                &if c.clock.width == 0 {
                    String::new()
                } else {
                    c.clock.width.to_string()
                },
                207,
                364 + half,
                412,
                half - 116,
            );
            self.check("显示秒", c.clock.show_seconds, 203, 224, 482, 160);
            self.check("日历显示农历", c.clock.show_lunar, 204, 410, 482, 220);
            self.label("点击时钟后打开", 224, 524, half, 24);
            self.combo(
                &["日历与天气", "系统日历"],
                usize::from(c.clock.click_action == ClickAction::SystemCalendar),
                208,
                224,
                550,
                half,
            );
            self.button("预览原生时钟", 209, 248 + half, 549, half);
            self.button("启用任务栏时钟", 212, 224, 600, half);
            self.button("停用并恢复系统时钟", 213, 248 + half, 600, half);
            self.control("STATIC", "", 210, SS_LEFT as u32, 224, 644, total, 52);
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
            self.preset_choices(c, total);
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
                self.section(title, x, 245, half);
                self.label("字体", x, 282, half - 100, 24);
                self.font_picker(
                    line.font_family.as_deref(),
                    font_id,
                    x,
                    308,
                    half - 100,
                    true,
                );
                self.label("字号", x + half - 84, 282, 84, 24);
                self.size_picker(line.font_size, font_id + 1, x + half - 84, 308, 84, true);
                self.edit("文字颜色", color, color_id, x, 346, half - 232);
                self.button(
                    "选色",
                    if color_id == 801 { 812 } else { 813 },
                    x + half - 224,
                    370,
                    56,
                );
                self.label("对齐方式", x + half - 150, 346, 150, 24);
                self.combo(
                    &["沿用公共对齐", "保留原样", "左对齐", "居中", "右对齐"],
                    line.alignment.map(|a| align(a) + 1).unwrap_or(0),
                    font_id + 2,
                    x + half - 150,
                    372,
                    150,
                );
            }
            self.label("公共对齐", 224, 416, half, 24);
            self.combo(
                &["保留原样", "左对齐", "居中", "右对齐"],
                align(c.clock.alignment),
                803,
                224,
                443,
                half,
            );
            self.label(
                "独立设置每一行；选择“沿用”可跟随公共字体与字号。",
                248 + half,
                422,
                half,
                52,
            );
            self.section("鼠标悬停提示", 224, 494, total);
            let tip = self.control(
                "EDIT",
                &c.clock.tooltip_line.replace('\n', "\r\n"),
                804,
                WS_TABSTOP | ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32 | ES_WANTRETURN as u32,
                224,
                530,
                total,
                70,
            );
            SendMessageW(tip, EM_SETLIMITTEXT, 1024, 0);
            self.label(
                "留空使用系统提示。支持显示内容中的全部变量和换行。",
                224,
                610,
                total,
                24,
            );
            self.button("预览原生时钟", 209, 224, 646, half);
            self.button("重置样式", 805, 248 + half, 646, half);
        }
    }
    pub(super) unsafe fn about_settings(&mut self, c: &Config, width: i32) {
        unsafe {
            let total = width - 260;
            self.section("LiteTray 轻托盘", 224, 88, total);
            self.label(
                &format!(
                    "版本 {}  ·  {UI_REVISION}\r\n让每块屏幕都有顺手的时钟、日历和托盘。",
                    env!("CARGO_PKG_VERSION")
                ),
                224,
                130,
                total,
                68,
            );
            self.section("应用更新", 224, 232, total);
            self.control(
                "STATIC",
                &self
                    .update
                    .as_ref()
                    .map(UpdateStatus::text)
                    .unwrap_or_else(|| "点击检查，获取 GitHub 上的最新 Releases 与 Tags。".into()),
                608,
                SS_LEFT as u32,
                224,
                276,
                total,
                68,
            );
            self.button("检查更新", UPDATE, 224, 360, 160);
            self.button("查看版本", OPEN_RELEASE, 402, 360, 150);
            self.button("项目主页", 607, 570, 360, 140);
            self.check(
                "启动时检查更新",
                c.updates.check_on_startup,
                603,
                224,
                430,
                280,
            );
            self.check(
                "接收预览版本",
                c.updates.include_prerelease,
                602,
                522,
                430,
                240,
            );
            self.edit("更新仓库", &c.updates.repository, 601, 224, 493, total);
            self.label("公开仓库直接检查，无需密钥或登录。\r\n开发期间仓库尚未公开时，会显示暂不可访问；公开后自动恢复。",224,574,total,60);
        }
    }
}
