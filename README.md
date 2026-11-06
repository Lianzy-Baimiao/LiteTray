# LiteTray · 轻托盘

轻量 Windows 任务栏增强软件：接管 Taskbar multi-tray 与 Taskbar Clock Customization 的目标功能，并提供点击时钟打开的日历、天气面板。

Rust 原生 Windows 应用，GitHub 仓库保持私有。

## 当前实现状态

当前为开发中的原生预览版本：原生设置窗口、托盘入口、日历天气面板、模板配置、多个天气来源、离线缓存、GitHub Releases 更新检查及 Windows 凭据管理器集成已经编写。**Explorer 多屏托盘和原生时钟接管尚未完成，当前 EXE 还不能替代这两个 Windhawk 模块。**

- [产品与实现规格](docs/implementation-plan.md)
- [实现与验证进度](docs/progress.md)
- 本机调查记录在本地 `docs/baseline.md`，包含环境信息，不提交到仓库。
- `tools/Export-WindhawkProfile.ps1`：只读取这两个模块的设置，导出到独立文件，原设置保持不变。
- `local/windhawk-profile.json`：本机导出结果；属于本地配置，不纳入版本控制。

## 只读导出

在项目目录的 PowerShell 中运行：

```powershell
.\tools\Export-WindhawkProfile.ps1
```

默认输出到项目 `local` 目录。已存在输出时不会覆盖，需显式传入 `-Force`。导出不包含模块源码，也不访问天气服务。

设置中的“配置迁移”页可以选择该 JSON，预览每个字段的转换结果，再采用到设置草稿；点击“保存设置”后写入 LiteTray。支持两行日期/时间模板、秒数、共享字体和字号、宽度、天气刷新间隔及多屏组件范围。不能转换的模板会保留整行现有值，报告列出原因。报告可导出到配置目录。

旧配置中的 wttr.in 网页变量可近似转换为内置天气摘要；城市坐标、天气来源顺序和凭据仍使用现有 LiteTray 配置。旧显示器序号需要重新核对。迁移不会改变 Windhawk，也不自动启用尚未完成的任务栏接管。

命令行也支持只读预览：`LiteTray.exe --preview-windhawk <导出文件.json> [新报告.json]`。指定输出时拒绝覆盖已有文件，包括来源文件。

## 构建与运行

需要 Rust stable 和 Windows C++ 链接工具。支持 MSVC（建议用于 CI/发布）和 GNU MinGW（本机开发）。

```powershell
# MSVC / 配置完整的工具链
cargo build --workspace --release
cargo test --workspace --all-targets

# 本机 GNU 工具链；不修改全局环境
.\tools\Invoke-Cargo.ps1 build --workspace --release
.\tools\Invoke-Cargo.ps1 test --workspace --all-targets
```

本机脚本优先使用 `LITETRAY_MINGW_BIN`，否则读取用户 `tools\mingw64\bin`。GNU 构建产物位于用户 `.cargo-target\litetray\release\LiteTray.exe`。MSVC 默认位于 `target\release\LiteTray.exe`。

运行参数：

```text
LiteTray.exe --settings
LiteTray.exe --calendar
LiteTray.exe --clock-preview
LiteTray.exe --background
LiteTray.exe --diagnose [output.json]
LiteTray.exe --weather-check [output.json]
LiteTray.exe --check-updates
LiteTray.exe --probe-taskbar [output.json]
LiteTray.exe --prepare-symbols [existing-symbol-cache]
LiteTray.exe --inspect-taskbar [output.json]
LiteTray.exe --self-test-clock
LiteTray.exe --preview-windhawk <profile.json> [new-report.json]
```

默认打开设置；关闭窗口收至托盘，右键托盘菜单可以退出。开机启动是可选项，默认关闭。配置保存在 `%LOCALAPPDATA%\LiteTray`；开发检查可用 `LITETRAY_DATA_DIR` 指向隔离目录。

显式指定不同配置目录时可以并行运行独立实例；同一目录仍保持单实例。隔离的是配置和缓存，凭据管理器中的服务凭据仍按 Windows 用户共享。

“时钟显示”页的“打开原生时钟预览”或 `--clock-preview` 可打开独立 XAML 时钟窗口，使用当前两行模板、字体和宽度。鼠标点击或聚焦后按回车可打开 LiteTray 日历天气面板，面板的 Esc 关闭已实测。空格处理已编写，仍待可靠的界面验收。预览始终使用 LiteTray 面板；系统日历选项留待任务栏接管验证。此窗口不接管任务栏，也不代表已能停用 Windhawk。

面板按触发控件的物理屏幕矩形定位，托盘入口优先使用 Shell 返回的图标矩形；布局考虑工作区边界、负坐标和 DPI。不同任务栏方向与小屏缩放已做几何测试，真实混合 DPI 多屏组合仍待验收。

`--probe-taskbar` 通过同目录 Rust DLL 在 Explorer 任务栏线程执行一次受限只读探测，返回窗口树与模块身份后移除 hook，不修改托盘。`--prepare-symbols` 校验当前 taskbar.dll 的 PE/PDB 身份，优先使用独立缓存，也可显式导入匹配的已有 PDB；缺失时从 Microsoft 符号服务器下载，使用 DbgHelp 验证 GUID/age 后保存按模块 SHA-256 命名的符号映射及完整接口布局。`--inspect-taskbar` 的代码进一步核对加载模块和函数入口，读取受限 XAML 树；当前未完成实机验收，不会启用写入。

`--self-test-clock` 在 **LiteTray 自己进程** 的新 XAML 控件中检查时钟文本和样式写入、原值/原绑定恢复、部分失败回滚、会话更新、外部修改保留与失联恢复。另启动同一程序的隔离测试子进程，验证真实跨进程更新和子进程结束后的自动恢复，并创建隐藏的 XAML Island 验证输入订阅与释放。检查约 18 秒，包含真实 Windows 定时器的 15 秒心跳超时验证。它不访问 Explorer，也不修改任务栏。适合开发检查，不代表已经实现任务栏接管：

```powershell
.\tools\Test-XamlClock.ps1 -BinaryPath <release目录>\LiteTray.exe
```

接口来源和隔离边界见 [ABI 说明](crates/taskbar-adapter/ABI.md)，上游参考与许可见 [第三方声明](THIRD-PARTY-NOTICES.md)。

## 天气来源

| 来源 | 认证 | 用途 |
| --- | --- | --- |
| Open-Meteo | 常规非商业免费接口无需 Key | 当前预估与三日预报 |
| MET Norway | 无需 Key，携带可识别 User-Agent | 逐小时预报托底 |
| 和风天气 | 控制台 API Host + API Key | 当前观测 |
| OpenWeather | API Key | 当前观测 |
| wttr.in | 无需 Key | 最后备用 |

支持添加、移除和调整来源顺序，按超时/错误/数据过期自动切换。缓存与城市坐标绑定；全部失败才使用未超过保留期限的离线缓存，并明确标记“非实时”。免费服务没有稳定性保证，商业发布需核对各服务条款与额度。

API Key 与 GitHub Token 使用 Windows 凭据管理器，不进入 JSON 配置、日志或 Git 仓库。更新检查读取 GitHub Releases，正确区分没有 Release、私有仓库无权限、限流与新版本；不静默执行下载内容。

## 实现方向与兼容范围

按用户决定采用 Rust：`windows-sys` 驱动 Win32 原生窗口和控件，`windows` 调用系统农历，网络使用系统 TLS。网络、配置、日历计算放在独立宿主；后续 Rust DLL 中只保留显示、输入转发和原生托盘适配。

开发目标为 Windows 11 build 26300.9711 x64。架构保留 Windows 10 经典任务栏适配分支；当前尚未完成 Windows 10 或未来版本的运行验证，不宣称所有 Windows 版本兼容。现代 Rust 工具链下不把 Windows 7/8 列为当前支持目标。

首先验证当前 Windows build 上的任务栏接入和退出恢复，再完成多屏托盘、时钟与面板的纵向闭环。完整接管通过之前，不能宣称已经支持替代 Windhawk。
