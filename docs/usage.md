# 使用与开发说明

[返回项目介绍](../README.md)

## 日程、闹钟与计时

在大窗口的“日程与待办”直接选择类型 Tab，再点击“新建”；填写后点击对应类型的保存按钮。各类型暂未保存的输入在切换时保留。日历中点击加号会带入选定日期并打开弹层内编辑区，点击安排可直接修改对应条目。支持不重复、每天、工作日（周一至周五）、每周、每年，以及最多提前七天提醒。全天事项默认 09:00 提醒；生日和纪念日默认按公历每年重复，2 月 29 日在非闰年按 2 月 28 日处理。

“闹钟与专注”提供闹钟管理入口、番茄钟和多个定时提醒。番茄钟默认 25 分钟专注、5 分钟短休息，每四轮安排 15 分钟长休息；可自行调整。阶段结束后通知并准备下一阶段，等待手动开始。计时器支持暂停、继续和删除，重启后恢复进度。

“铃声”页可选 10 种 Windows 闹钟音、静音或本地 WAV 音乐，设置 5–120 秒响铃时长；试听 5 秒自动停止，也可在任一计时 Tab 点击“停止响铃”。使用 Windows 当前系统音量，不更改系统音量设置。

数据保存在 `%LOCALAPPDATA%\LiteTray\agenda.json`，无云同步。通知需要 LiteTray 后台运行，并遵循 Windows 通知设置。退出后不会唤醒电脑；下次启动补发到期计时器和最近 24 小时内未发送的日程提醒，最近 50 条通知留在本机。无法读取的日程文件会保留，并阻止覆盖。当前支持 **农历显示**，生日/纪念日的农历重复尚未实现。

## 使用与设置

默认打开设置，关闭设置窗口会收至托盘；从托盘菜单或常规设置退出程序。配置保存在 `%LOCALAPPDATA%\LiteTray`。开机启动默认关闭。

先在“多屏托盘”选择显示范围，再到“时钟显示”点击“启用任务栏时钟”。连接流程核对系统模块、任务栏窗口、屏幕、原生时钟元素与会话端点，收到 Active 后才显示运行中。“停用并恢复”需原生端确认 Closed，并保存停用状态。启用状态会保存，`--background` 在不显示设置窗口的情况下保持时钟与天气运行。“测试独立时钟预览”用于调样式，不影响真实任务栏。

时钟宽度留空时按两行实际内容自动扩展和收缩（配置中 `width: 0`）；需要固定宽度时可填 80–600。自动模式直接使用 XAML 的 Auto 布局，不按字符数估算。多屏页勾选“复制主屏托盘图标和隐藏图标入口”并保存，副屏使用主屏的原生数据模型、显隐及内容宽度；时钟和图标复制可以独立开启。源屏保留原位图标。已增加原生输入坐标修正、隐藏图标弹层定位和系统菜单的跨 XAML 根代理。QQNT 延迟悬停消息遗漏已补回并通过原生窗口回归检查；QQNT 实际悬停弹层、双击、右键、控制中心左键弹层和拖放仍需应用级交互验收。

实验原位路径当前只接受 Windows 11 build 26300.9711 x64 已核对的 taskbar.dll 与 twinui.pcshell.dll 哈希。其他系统版本拒绝套用私有偏移。Windows 10、系统升级、多屏热插拔、混合 DPI、自动隐藏与原系统点击行为仍需兼容验证。DLL 为保证回调生命周期，会固定加载到 Explorer 结束；停止成功后释放本次时钟属性、点击及私有宿主引用。

开发版标题包含当前版本号。从其他目录运行的旧 LiteTray 不会自动升级；新版检测到旧实例时会明确显示双方路径，避免静默打开旧界面。`LITETRAY_DATA_DIR` 可指定独立开发配置目录，凭据管理器仍按 Windows 用户共享。

## 天气来源

| 来源 | 认证 | 数据用途 |
| --- | --- | --- |
| Open-Meteo | 常规非商业免费接口无需 Key | 当前预估与三日预报 |
| MET Norway | 无需 Key，带可识别 User-Agent | 逐小时预报托底 |
| 和风天气 | API Host + API Key | 当前观测 |
| OpenWeather | API Key | 当前观测 |
| wttr.in | 无需 Key | 最后备用 |

支持调整顺序和刷新间隔。来源失败或过期后依次尝试，全部失败时使用有效缓存，并标记非实时。首次失败显示暂无数据；不会用示例值填充正式界面。天气服务 Key 存放于 Windows 凭据管理器，不写入配置 JSON。更新检查直接匿名读取公开 Releases 与 Tags，不读取凭据。各服务的发布用途和额度须遵守其条款。

## 构建与检查

需要 Rust stable 与 Windows C++ 链接工具。支持 MSVC，以及本机 GNU MinGW 开发工具链。

```powershell
.\tools\Invoke-Cargo.ps1 build --workspace --release --locked
.\tools\Invoke-Cargo.ps1 test --workspace --all-targets --locked
.\tools\Invoke-Cargo.ps1 -CargoArguments @('clippy','--workspace','--all-targets','--locked','--','-D','warnings')
.\tools\Test-XamlClock.ps1 -BinaryPath <release目录>\LiteTray.exe
.\tools\Test-ShellSource.ps1 -BinaryPath <release目录>\LiteTray.exe
.\tools\Render-Panel.ps1
.\tools\Render-UI.ps1
```

本机 GNU 产物在 `%USERPROFILE%\.cargo-target\litetray\release`。MSVC 可直接使用 `cargo`，默认产物在 `target\release`。`tools/Package.ps1` 将 EXE、同版本 DLL、说明及 SHA-256 清单放入指定目录。

生成 Windows 11 x64 安装包（需要 Inno Setup 7 或 6.5+）：

```powershell
.\tools\Build-Setup.ps1
# 使用已构建的同版本完整包：
.\tools\Build-Setup.ps1 -SkipBuild -BinaryPath .\dist\LiteTray-ui5101-weather-icons\LiteTray.exe
```

输出命名为 `dist/LiteTray-<版本>-Setup-x64.exe`（当前为 `LiteTray-5.11.6-Setup-x64.exe`），旁边包含同名 SHA-256 校验文件和版本清单。中文安装向导默认按用户安装至 `%LOCALAPPDATA%\Programs\LiteTray`，创建开始菜单入口，可选桌面图标；无需管理员权限。升级正常退出旧宿主，使用按内容哈希区分的版本目录，已有开机启动项会指向新版。安装和卸载保留 `%LOCALAPPDATA%\LiteTray` 下的配置、日程及缓存；卸载不会删除用户凭据。安装程序不关闭 Windhawk，也不重启 Explorer。详见 [安装包说明](installer.md)。

`Test-XamlClock` 与 `Test-ShellSource` 只创建自身控件，不访问 Explorer。当前 124 项自动测试通过，包含设置首次打开与最小化恢复、100 次跨入口打开、弹层内六类日程持久化、错误输入、删除、翻页、草稿保留，以及日程通知去重、计时器重启、日历切换重绘、农历闰月、放假补班与离线缓存回归。设置激活测试在独立进程及非输入桌面上运行，不切换用户桌面。任务栏适配代码未在 5.3/5.4 改动，5.2 的 153 项原生 XAML 检查覆盖集合变化、替换/恢复、Auto 宽度、输入路由、菜单祖先接口和绑定恢复。界面渲染覆盖十个页面的浅深色、所有计时 Tab、最小窗口尺寸、外层滚轮隔离及日历／弹层编辑区的多 DPI 状态。`--self-test-app-icon` 检查可执行文件图标资源及不同 DPI 的加载尺寸。

2026-11-06 用户重新授权后，实际 GUI 启动与 Explorer 接入均成功。新增显式命令 `--test-secondary-clock <report.json>`：不打开窗口、不发送鼠标键盘输入，只向已核对的副屏原生时钟写入 6 次测试内容，检查系统刷新后仍保持，随后停止并检查恢复。没有副屏时拒绝修改。该命令确实会暂时更改副屏任务栏，不属于隔离自检；测试期间应关闭其他 LiteTray 原位连接。

`--test-secondary-tray <report.json>` 在同一后台流程中额外检查副屏生成的原生应用图标数量、独立复制模式和停止恢复；用长短天气文字检查原生宽度确实自动变化。此命令不模拟鼠标点击，不能代替菜单和回调位置验收。

实机后台实例持续显示秒和真实天气；主动结束该测试进程后，原生 watchdog 在 2 秒后的检查中已恢复系统时间。宿主工作集约 21.3 MiB、私有内存约 3.3 MiB，运行两分多钟累计 CPU 时间约 0.06 秒；这仅是一次宿主观察，不包括 Explorer 内的适配器，也不是与 Windhawk 的同口径对比。由于用户正在主屏游戏，后续测试没有发送输入；真实鼠标点击、跨屏菜单和 Explorer 重启仍不计为验收通过。

## 配置迁移与命令行

`tools/Export-WindhawkProfile.ps1` 只读导出两个模块的设置，原设置不变。设置中的“配置迁移”页先预览逐字段转换报告，采用草稿并保存后写入 LiteTray。无法转换的格式保留原行并报告原因。迁移不会自动停用 Windhawk 或启用任务栏连接。

常用参数：`--settings`、`--calendar`、`--background`、`--clock-preview`、`--diagnose [file]`、`--weather-check [file]`、`--check-updates`、`--preview-windhawk <profile.json> [new-report.json]`。原生开发检查与只读探测参数见 `--help`。

`--show-secondary [calendar|agenda|timers|clock|appearance]` 在副屏显示指定页面，不激活窗口，不改保存的配置。日常打开请使用 `--settings` 或托盘入口。

`cargo run --example tray_windows -- 60` 可记录 60 秒实际弹窗的进程、窗口类和几何，不读取窗口内容、不移动鼠标。重定向为 JSONL 后，用 `tools/Test-TrayPopupPlacement.ps1 -ReportPath <file>` 检查副屏悬停观测；没有真实悬停样本返回 INCONCLUSIVE，不能视为通过。

- [完整产品规格](implementation-plan.md)
- [开发与验证记录](progress.md)
- [任务栏 source 依据与限制](taskbar-source-binding.md)
- [原生 ABI 说明](../crates/taskbar-adapter/ABI.md)
- [第三方声明](../THIRD-PARTY-NOTICES.md)

