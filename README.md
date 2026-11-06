# LiteTray · 轻托盘

Rust 原生 Windows 任务栏工具，目标是替代 Windhawk 的 Taskbar Clock Customization 和 Taskbar multi-tray，提供可配置时钟、日历与天气。私有开发仓库，当前版本为 **开发版 3**。

## 当前能做什么

| 功能 | 当前状态 |
| --- | --- |
| 日历与天气同屏面板 | 已编写原生界面；浅深色、月份切换、键盘日期选择、农历、三日天气与缓存状态 |
| 时钟配置 | 按内容自动宽度、两行模板、秒、各行字体/字号/颜色/对齐、悬停提示、Windhawk 设置迁移 |
| 天气 | 五种来源、顺序切换、失败托底、城市绑定的离线缓存、Windows 凭据管理器 |
| 任务栏原位时钟 | **已在真实 Explorer 副屏验证写入、持续走时、停止恢复与进程意外退出恢复**；仅接受当前核对的系统模块 |
| 多屏托盘 | 应用图标、系统指示区、网络/音量及显示桌面区域已复制；主副屏实际顺序、尺寸与右侧位置对齐通过；悬停与菜单仍需应用级交互验收 |
| 关于与更新 | GitHub Releases 检查、版本与权限错误提示；不自动执行下载文件 |

**当前仍不能认定为两个 Windhawk 模块的完整替代品。** 独立时钟预览和离屏界面图都不证明任务栏替换成功。

## 本轮实际改动

- 接通设置页、任务栏线程入口、原生 source 解析、会话握手、Start、逐秒更新、点击事件和显式恢复。
- 时钟文本通过同步属性回调处理系统刷新，记录最新系统值，停止后恢复该值或原绑定。样式保持原有冲突检测。
- 时钟常驻服务独立于设置窗口刷新；保存开启状态，重启软件后继续连接。样式更新原位生效，范围/点击方式变化先恢复再重连。
- 显示器、Explorer 与睡眠唤醒通知触发重新连接；失败有退避与三次上限，恢复未确认时禁止建立新连接。实际 Explorer 重启和睡眠仍待实机验收。
- 面板按选定的日历天气同屏方案重新排版，660×452 逻辑像素；真实控件与离屏绘制共用布局代码。取消巨大的重复时间及宣传文案，补足天气空状态。

以下是同一原生绘制代码生成的**离屏检查图，天气为示例**，不是桌面接管截图：

![浅色日历天气面板](docs/images/panel-light.png)

## 使用与设置

默认打开设置，关闭设置窗口会收至托盘；从托盘菜单或常规设置退出程序。配置保存在 `%LOCALAPPDATA%\LiteTray`。开机启动默认关闭。

先在“多屏托盘”选择显示范围，再到“时钟显示”点击“启用并保存（实验）”。连接流程核对系统模块、任务栏窗口、屏幕、原生时钟元素与会话端点，收到 Active 后才显示运行中。“停用并恢复”需原生端确认 Closed，并保存停用状态。启用状态会保存，`--background` 在不显示设置窗口的情况下保持时钟与天气运行。“测试独立时钟预览”用于调样式，不影响真实任务栏。

时钟宽度留空时按两行实际内容自动扩展和收缩（配置中 `width: 0`）；需要固定宽度时可填 80–600。自动模式直接使用 XAML 的 Auto 布局，不按字符数估算。多屏页勾选“复制主屏托盘图标和隐藏图标入口”并保存，副屏使用主屏的原生数据模型、显隐及内容宽度；时钟和图标复制可以独立开启。源屏保留原位图标。已增加原生输入坐标修正、隐藏图标弹层定位和系统菜单的跨 XAML 根代理。QQNT 延迟悬停消息遗漏已补回并通过原生窗口回归检查；QQNT 实际悬停弹层、双击、右键、控制中心左键弹层和拖放仍需应用级交互验收。

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

`Test-XamlClock` 与 `Test-ShellSource` 只创建自身控件，不访问 Explorer。当前 83 项单元测试通过；通用 XAML 检查包含模型集合增删通知、替换、恢复、Auto 宽度与绑定恢复。UI 离屏检查另有 6 张浅深色、缩放与空状态图。

2026-11-06 用户重新授权后，实际 GUI 启动与 Explorer 接入均成功。新增显式命令 `--test-secondary-clock <report.json>`：不打开窗口、不发送鼠标键盘输入，只向已核对的副屏原生时钟写入 6 次测试内容，检查系统刷新后仍保持，随后停止并检查恢复。没有副屏时拒绝修改。该命令确实会暂时更改副屏任务栏，不属于隔离自检；测试期间应关闭其他 LiteTray 原位连接。

`--test-secondary-tray <report.json>` 在同一后台流程中额外检查副屏生成的原生应用图标数量、独立复制模式和停止恢复；用长短天气文字检查原生宽度确实自动变化。此命令不模拟鼠标点击，不能代替菜单和回调位置验收。

实机后台实例持续显示秒和真实天气；主动结束该测试进程后，原生 watchdog 在 2 秒后的检查中已恢复系统时间。宿主工作集约 21.3 MiB、私有内存约 3.3 MiB，运行两分多钟累计 CPU 时间约 0.06 秒；这仅是一次宿主观察，不包括 Explorer 内的适配器，也不是与 Windhawk 的同口径对比。由于用户正在主屏游戏，后续测试没有发送输入；真实鼠标点击、跨屏菜单和 Explorer 重启仍不计为验收通过。

## 配置迁移与命令行

`tools/Export-WindhawkProfile.ps1` 只读导出两个模块的设置，原设置不变。设置中的“配置迁移”页先预览逐字段转换报告，采用草稿并保存后写入 LiteTray。无法转换的格式保留原行并报告原因。迁移不会自动停用 Windhawk 或启用任务栏连接。

常用参数：`--settings`、`--calendar`、`--background`、`--clock-preview`、`--diagnose [file]`、`--weather-check [file]`、`--check-updates`、`--preview-windhawk <profile.json> [new-report.json]`。原生开发检查与只读探测参数见 `--help`。

- [完整产品规格](docs/implementation-plan.md)
- [开发与验证记录](docs/progress.md)
- [任务栏 source 依据与限制](docs/taskbar-source-binding.md)
- [原生 ABI 说明](crates/taskbar-adapter/ABI.md)
- [第三方声明](THIRD-PARTY-NOTICES.md)
