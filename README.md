# LiteTray · 轻托盘

Rust 原生 Windows 任务栏工具，目标是替代 Windhawk 的 Taskbar Clock Customization 和 Taskbar multi-tray，提供可配置时钟、日历与天气。私有开发仓库，当前版本为 **开发版 3**。

## 当前能做什么

| 功能 | 当前状态 |
| --- | --- |
| 日历与天气同屏面板 | 已编写原生界面；浅深色、月份切换、键盘日期选择、农历、三日天气与缓存状态 |
| 时钟配置 | 两行模板、秒、各行字体/字号/颜色/对齐、悬停提示、Windhawk 设置迁移 |
| 天气 | 五种来源、顺序切换、失败托底、城市绑定的离线缓存、Windows 凭据管理器 |
| 任务栏原位时钟 | **实验启动链路已实现，尚未通过 Explorer 实机验收**；仅接受当前核对的系统模块 |
| 多屏托盘 | 范围选择与底层模型事务已编写；真实图标交互、回调与多屏接管仍未完成 |
| 关于与更新 | GitHub Releases 检查、版本与权限错误提示；不自动执行下载文件 |

**当前仍不能认定为两个 Windhawk 模块的完整替代品。** 独立时钟预览和离屏界面图都不证明任务栏替换成功。

## 本轮实际改动

- 接通设置页、任务栏线程入口、原生 source 解析、会话握手、Start、逐秒更新、点击事件和显式恢复。
- 时钟文本通过同步属性回调处理系统刷新，记录最新系统值，停止后恢复该值或原绑定。样式保持原有冲突检测。
- 会话失联由原生线程定时器恢复；通信出错后停止反复发送更新。**Explorer 重启后自动重连尚未实现。**
- 面板按选定的日历天气同屏方案重新排版，660×452 逻辑像素；真实控件与离屏绘制共用布局代码。取消巨大的重复时间及宣传文案，补足天气空状态。

以下是同一原生绘制代码生成的**离屏检查图，天气为示例**，不是桌面接管截图：

![浅色日历天气面板](docs/images/panel-light.png)

## 使用与设置

默认打开设置，关闭设置窗口会收至托盘；从托盘菜单或常规设置退出程序。配置保存在 `%LOCALAPPDATA%\LiteTray`。开机启动默认关闭。

“时钟显示”页分开提供“测试独立时钟预览”和“连接任务栏时钟（实验）”。连接流程核对实际系统模块、任务栏窗口、屏幕、原生时钟元素与会话端点，收到 Active 后才显示连接运行中；失败返回错误。“停止并恢复”需要原生端确认 Closed。实验会话不随软件重新启动自动开启；保存设置会停止已有实验连接，需要再次显式连接。

实验原位路径当前只接受 Windows 11 build 26300.9711 x64 已核对的 taskbar.dll 与 twinui.pcshell.dll 哈希。其他系统版本拒绝套用私有偏移。Windows 10、系统升级、多屏热插拔、混合 DPI、自动隐藏与原系统点击行为仍需兼容验证。DLL 为保证回调生命周期，会固定加载到 Explorer 结束；停止成功后释放本次时钟属性、点击及私有宿主引用。

开发版标题包含“开发版 3”。从其他目录运行的旧 LiteTray 不会自动升级；新版检测到旧实例时会明确显示双方路径，避免静默打开旧界面。`LITETRAY_DATA_DIR` 可指定独立开发配置目录，凭据管理器仍按 Windows 用户共享。

## 天气来源

| 来源 | 认证 | 数据用途 |
| --- | --- | --- |
| Open-Meteo | 常规非商业免费接口无需 Key | 当前预估与三日预报 |
| MET Norway | 无需 Key，带可识别 User-Agent | 逐小时预报托底 |
| 和风天气 | API Host + API Key | 当前观测 |
| OpenWeather | API Key | 当前观测 |
| wttr.in | 无需 Key | 最后备用 |

支持调整顺序和刷新间隔。来源失败或过期后依次尝试，全部失败时使用有效缓存，并标记非实时。首次失败显示暂无数据；不会用示例值填充正式界面。Key 和 GitHub Token 存放于 Windows 凭据管理器，不写入配置 JSON。各服务的发布用途和额度须遵守其条款。

## 构建与检查

需要 Rust stable 与 Windows C++ 链接工具。支持 MSVC，以及本机 GNU MinGW 开发工具链。

```powershell
.\tools\Invoke-Cargo.ps1 build --workspace --release --locked
.\tools\Invoke-Cargo.ps1 test --workspace --all-targets --locked
.\tools\Invoke-Cargo.ps1 -CargoArguments @('clippy','--workspace','--all-targets','--locked','--','-D','warnings')
.\tools\Test-XamlClock.ps1 -BinaryPath <release目录>\LiteTray.exe
.\tools\Test-ShellSource.ps1 -BinaryPath <release目录>\LiteTray.exe
.\tools\Render-Panel.ps1
```

本机 GNU 产物在 `%USERPROFILE%\.cargo-target\litetray\release`。MSVC 可直接使用 `cargo`，默认产物在 `target\release`。`tools/Package.ps1` 将 EXE、同版本 DLL、说明及 SHA-256 清单放入指定目录。

原生检查只创建 LiteTray 自身的 XAML 控件与隐藏宿主，不访问 Explorer。当前 75 项单元测试、138 项通用 XAML 场景与 6 项当前系统专用私有宿主场景通过；UI 离屏检查单独产生 6 张浅深色、缩放与空状态图。**这些检查不代替真实任务栏或桌面鼠标交互验收。**

此前自动审批对新版 GUI 启动及 Explorer 接入返回 `blocked by policy`；本轮没有换方式重试相同操作。因此真实系统托盘替换和新版桌面交互仍未验证，尚无同口径性能对比结论。

## 配置迁移与命令行

`tools/Export-WindhawkProfile.ps1` 只读导出两个模块的设置，原设置不变。设置中的“配置迁移”页先预览逐字段转换报告，采用草稿并保存后写入 LiteTray。无法转换的格式保留原行并报告原因。迁移不会自动停用 Windhawk 或启用任务栏连接。

常用参数：`--settings`、`--calendar`、`--background`、`--clock-preview`、`--diagnose [file]`、`--weather-check [file]`、`--check-updates`、`--preview-windhawk <profile.json> [new-report.json]`。原生开发检查与只读探测参数见 `--help`。

- [完整产品规格](docs/implementation-plan.md)
- [开发与验证记录](docs/progress.md)
- [任务栏 source 依据与限制](docs/taskbar-source-binding.md)
- [原生 ABI 说明](crates/taskbar-adapter/ABI.md)
- [第三方声明](THIRD-PARTY-NOTICES.md)
