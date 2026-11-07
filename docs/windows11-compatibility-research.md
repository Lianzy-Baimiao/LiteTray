# Windows 11 跨版本接管：一手资料核对

核对日期：2026-11-08。本文记录官方文档和上游源代码能证明的事实，不代表 LiteTray 已完成这些版本的实机适配。落地设计见 [兼容计划](windows11-compatibility-plan.md)。

## 系统版本不等于任务栏实现版本

微软 [Windows 11 release information](https://learn.microsoft.com/en-us/windows/release-health/windows11-release-information) 在本次取得的页面中列出以下系列。该页元数据更新时间为 `2026-10-28T17:55:00Z`。

| Windows 11 系列 | OS build 家族 | 官方页面在核对日列出的维护状态 |
| --- | --- | --- |
| 21H2 | 22000 | 在历史表中，未列于当前维护版本表 |
| 22H2 | 22621 | 在历史表中，未列于当前维护版本表 |
| 23H2 | 22631 | 家庭/专业版已结束更新；企业/教育版到 2026-11-10 |
| 24H2 | 26100 | 家庭/专业版已结束更新；企业/教育版到 2027-10-12；LTSC 2024 另有生命周期 |
| 25H2 | 26200 | 家庭/专业版到 2027-10-12；企业/教育版到 2028-10-10 |
| 26H2 | 26300 | 2026-09-29 正式发布；家庭/专业版到 2028-10-10；企业/教育版到 2029-10-09 |
| 26H1 | 28000 | 面向 2026 年初开始上市的新设备；不是现有 24H2/25H2 设备的原地功能升级 |

26H1 的定位来自页面原文：

> Windows 11, version 26H1 is scoped to support new devices that started coming to market in early 2026 and is not designed as a feature update for existing devices. This version is not offered as an in-place update from 24H2 or 25H2 on existing devices.

26H2 的核对原文与相邻记录：

```text
Version 26H2 (OS build 26300)
General Availability Channel | 2026-10 D   | 2026-10-28 | 26300.9818 | KB5127936
General Availability Channel | 2026-10 OOB | 2026-10-26 | 26300.9725 | KB5134133
General Availability Channel | 2026-10 B   | 2026-10-13 | 26300.9711 | KB5127648
```

因此不能把当前已验证的 `26300.9711` 仅因 build 数较大就标为 Insider；本次官方资料将它归入 26H2 正式通道。页面内容会更新，本地取回原文存于忽略目录 `local/compat-research/release.html`，SHA-256 为 `c2b93ffe789dc193372c6ca434ecee04cfe178a627d1cdf3a1fd08a98af731fa`。

LTSC 也不能只写一个截止日期：Windows 11 Enterprise LTSC 2024 到 2029-10-09，只有 IoT Enterprise LTSC 2024 延长支持到 2034-10-10。[同一官方页面的 LTSC 注释](https://learn.microsoft.com/en-us/windows/release-health/windows11-release-information#enterprise-and-iot-enterprise-ltsc-editions)

### 早期托盘与功能分批启用

微软 [Build 25197 公告（2022-09-08）](https://blogs.windows.com/windows-insider/2022/09/08/announcing-windows-11-insider-preview-build-25197/) 单独描述 `System Tray Updates`，并明确说：

> We are beginning to roll this feature out, so it isn’t available to all Insiders just yet...

随后 [22621.1343 发布预览公告（2023-02-21）](https://blogs.windows.com/windows-insider/2023/02/21/releasing-windows-11-build-22621-1343-to-the-release-preview-channel/) 将增强后的托盘作为新功能列出，包括所有图标的圆角焦点/悬停和重新排列隐藏图标。两项资料证明 22621 家族内不能忽略累计更新与分批启用状态；它们**没有**承诺某个 build 起所有机器一定使用同一 XAML/Win32 托盘树。

实现层的补充证据是下文 clock 源码同时保留 `ClockSystemTrayIconDataModel` 和可选 `ClockSystemTrayIconDataModel2`，注释后者随 feature flag `38762814` 加入。把 22000、22621 简单归为“全是旧 Win32”或“全是新 XAML”都不严谨：任务栏宿主、时钟和图标区域应分别现场识别。公开资料不足以替代早期镜像的窗口树、模块与视觉树采集。

## 微软符号：可用于精确解析，不是兼容担保

- [Microsoft public symbol server](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/microsoft-public-symbols) 免费提供 Windows 调试符号，支持本地缓存与按需下载，官方示例为 `srv*DownstreamStore*https://msdl.microsoft.com/download/symbols`。符号获取可放在辅助进程中，避免 Explorer UI 线程等待网络。
- [SymFindFileInPath](https://learn.microsoft.com/en-us/windows/win32/api/dbghelp/nf-dbghelp-symfindfileinpath) 明确：查找 PDB 时使用原模块 CodeView 目录的 PDB signature 和 age；查找映像时使用 PE `TimeDateStamp` 和 `SizeOfImage`。不能仅以系统显示版本挑选另一份 PDB。
- [SymSetOptions](https://learn.microsoft.com/en-us/windows/win32/api/dbghelp/nf-dbghelp-symsetoptions) 的 `SYMOPT_EXACT_SYMBOLS` 明确拒绝不匹配 PDB，`SYMOPT_LOAD_ANYTHING` 则关闭文件匹配检查。产品应保持精确匹配，不能为了覆盖更多系统启用后者。
- [SymInitialize](https://learn.microsoft.com/en-us/windows/win32/api/dbghelp/nf-dbghelp-syminitialize) 提醒所有 DbgHelp 函数都是单线程接口，并说明会话标识须唯一；辅助进程内部也要串行化符号调用。
- 公共符号用于调试与测试，不承诺存在全部私有类型、字段、符号或长期 ABI。微软符号服务器的[许可条款](https://learn.microsoft.com/en-us/legal/windows-sdk/microsoft-symbol-server-license-terms)独立适用；LiteTray 的 Apache 2.0 不会把微软 PDB 或系统 DLL 变成可自由再分发内容。

设计推论：保留精确模块身份，增加经过验证的解析规则；找到同名符号后仍检查节区、函数形状、对象归属与恢复能力。符号解析成功只证明取得候选入口，不能算真实接管验收通过。

## XAML diagnostics 是候选发现渠道，有明确边界

[InitializeXamlDiagnosticsEx](https://learn.microsoft.com/en-us/windows/win32/api/xamlom/nf-xamlom-initializexamldiagnosticsex) 被微软定义为调试工具使用 XAML Diagnostic APIs 的入口；参数包含目标 PID、`XamlDiagnostics.dll` 路径及“要注入该进程的 DLL”。最低客户端为 Windows 10 1703，桌面应用适用。

[IVisualTreeService](https://learn.microsoft.com/en-us/windows/win32/api/xamlom/nn-xamlom-ivisualtreeservice) 提供视觉树变化监听、属性查询/修改和子项操作。因此它值得作为减少 `TaskbarHost` 私有字段依赖的单独验证方向。但从官方文档不能推出：

- 任意 Explorer 版本都有同样可连接的诊断端点；
- 能直接把任意任务栏 HWND 映射为所需 `XamlRoot` / `DesktopWindowXamlSource`；
- 可长期、无冲突地修改所有托盘私有模型及原生输入逻辑；
- 可以免除进程内 DLL、跨岛菜单规则或真实多屏验证。

尤其不能把“有公开调试入口”写成“存在微软支持的整套第三方任务栏替换扩展协议”。是否采用应由隔离原型验证决定，而不是立即取代已工作的路径。

## Windhawk 上游的实际兼容策略与许可

核对仓库固定提交：`ramensoftware/windhawk-mods@6a2eda932f27a788ca5627e4e817b0a6f9f9c568`。

### Taskbar multi-tray 1.3.0（MIT）

[文件头与功能说明](https://github.com/ramensoftware/windhawk-mods/blob/6a2eda932f27a788ca5627e4e817b0a6f9f9c568/mods/taskbar-multi-tray.wh.cpp#L1-L107) 声明 MIT，架构元数据为 `x86-64`。源文件包含 ARM64 条件分支，不足以证明该版本已对 ARM64 发布或通过测试。

核心做法：

1. [按符号获取 `CTaskBand` / `CSecondaryTaskBand` vtable、`GetTaskbarHost`、`FrameHeight` 与释放入口](https://github.com/ramensoftware/windhawk-mods/blob/6a2eda932f27a788ca5627e4e817b0a6f9f9c568/mods/taskbar-multi-tray.wh.cpp#L6107-L6172)。必须符号统一解析失败就拒绝装载，不是按每个 Windows build 硬写全部函数 RVA。
2. [从 `FrameHeight` 指令推导字段，再获取任务栏 XamlRoot](https://github.com/ramensoftware/windhawk-mods/blob/6a2eda932f27a788ca5627e4e817b0a6f9f9c568/mods/taskbar-multi-tray.wh.cpp#L5058-L5168)，包含主、副任务栏各自路径。但此实现中不识别指令时记录错误后仍保留初始 `0x10` 偏移；这不是 LiteTray 应照搬的安全行为，未知布局应拒绝继续。
3. [根据实际加载模块选择 `SystemTray.dll` 或早于 2604 系列的 `Taskbar.View.dll`](https://github.com/ramensoftware/windhawk-mods/blob/6a2eda932f27a788ca5627e4e817b0a6f9f9c568/mods/taskbar-multi-tray.wh.cpp#L6004-L6030)，并监听延迟加载。坐标修复符号是可选项，缺一个只损失对应修复；LiteTray 是否允许该降级应根据自己的功能承诺决定，不能宣称图标已完整可用。
4. 通过 `SystemTrayFrame` 等实际树形特征选择 Grid/StackPanel 布局，共享 `DataContext` / `ItemsSource` / 非 UIElement 内容，保持每个岛的菜单归属。这里既有模板变化，也有输入生命周期变化，仅扩大哈希白名单不能覆盖它们。[源码中的技术说明](https://github.com/ramensoftware/windhawk-mods/blob/6a2eda932f27a788ca5627e4e817b0a6f9f9c568/mods/taskbar-multi-tray.wh.cpp#L64-L107)

在所核对的文件中没有找到一条可当作完整实测矩阵的“最低 Windows 11 build”声明；它以符号和运行时树存在性为实际前提。不能据其名称承诺 21H2 至 26H2 全覆盖，也不能把对 ExplorerPatcher/旧任务栏的解释当作该 mod 对旧实现的支持。

### Taskbar Clock Customization 1.8.1（GPL v3）

[文件头](https://github.com/ramensoftware/windhawk-mods/blob/6a2eda932f27a788ca5627e4e817b0a6f9f9c568/mods/taskbar-clock-customization.wh.cpp#L1-L31) 明确代码按 GPL v3 发布，与 multi-tray 的 MIT 不同。LiteTray 当前 Apache 2.0 项目不能直接复制这部分实现并继续当作纯 Apache 代码发布；本文只记录行为与兼容事实，LiteTray 应独立设计实现。

- [Explorer 版本分组](https://github.com/ramensoftware/windhawk-mods/blob/6a2eda932f27a788ca5627e4e817b0a6f9f9c568/mods/taskbar-clock-customization.wh.cpp#L5500-L5528) 使用 22000、26100 分界，但这只是选择大类行为。
- [SystemTray 符号表](https://github.com/ramensoftware/windhawk-mods/blob/6a2eda932f27a788ca5627e4e817b0a6f9f9c568/mods/taskbar-clock-customization.wh.cpp#L5689-L5779) 有多个可选方法签名，包含 feature flag 引入的 `ClockSystemTrayIconDataModel2`，及 21H2 专用签名。
- [模块选择](https://github.com/ramensoftware/windhawk-mods/blob/6a2eda932f27a788ca5627e4e817b0a6f9f9c568/mods/taskbar-clock-customization.wh.cpp#L5784-L5818) 依次处理 `SystemTray.dll`、`Taskbar.View.dll` 和 `ExplorerExtensions.dll`，并通过模块文件版本识别 SystemTray 类型迁移，而不是只看 OS build。

这说明可靠覆盖需要“模块身份 + 方法变体 + 运行时能力”，但上游支持也不能代替 LiteTray 自己的接管与恢复测试。

## 公开托盘 API 能做什么

微软 [Notification Area](https://learn.microsoft.com/en-us/windows/win32/shell/notification-area) 和 [Shell_NotifyIconW](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyiconw) 文档描述添加、修改、删除图标，设置版本及焦点；[Shell_NotifyIconGetRect](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyicongetrect) 要求传入已知 `NOTIFYICONIDENTIFIER`，返回该图标的矩形。

这些文档中未提供“枚举所有第三方图标及回调并克隆完整通知区到另一个任务栏”的公开接口，也没有把自己添加的图标变成系统原生时钟的接口。因此从此组公开 API 无法实现用户要求的完整替换。对所有 Windows API 的不存在性无法用单个文档严格证明；这里的结论限于已核对的官方通知区接口，不能夸大成数学意义上的穷尽证明。

## CI 证据的范围

GitHub 官方 [runner-images README](https://github.com/actions/runner-images/blob/fad015e47059f40cf0786dcb4593384f6a0720d3/README.md#L36) 在核对日把 `windows-latest`、`windows-2025`、`windows-2025-vs2026` 映射为 **Windows Server 2025 x64**，并说明 `-latest` 会迁移。

所以本库现有 Windows CI 可以证明编译、逻辑测试及隔离 XAML 检查通过，不能证明 Windows 11 多个任务栏家族接管成功。跨版本结论需要分别保留实际系统/模块身份、探测结果、主副屏交互与停止恢复的实测报告；ARM64、LTSC、早期 Win32 托盘、第三方替代任务栏应有独立结果。
