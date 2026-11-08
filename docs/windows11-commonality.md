# Windows 11 三组模块的共性与差异

2026-11-08 完成的离线结果表明：9711 已审阅资料、9818 当前映像和 25H2 安装映像中的三组 x64 模块共享现有宿主/source 字段关系及三项托盘坐标调用约定，但模块身份、入口地址、虚表地址和托盘代码载体均不能跨版本复用。2026-11-09，独立 25H2 `26200.8457` 客体进一步通过真实主屏 source/拓扑检查、时钟文字与宽度更新、停止恢复，以及托盘准备/停止。**8457 精确资料因此纳入默认目录，已测范围仍限单屏；实际输入、复制、菜单、多屏和混合 DPI 尚未验收。** 运行证据及失败对照见 [25H2 虚拟机记录](compatibility-8457.md)。

| 对象 | 本次证据来源 | 托盘代码所在模块 | 可得结论 |
| --- | --- | --- | --- |
| 26300.9711 | 既有已审阅 JSON 资料；本次未重新取得原映像 | Core/SystemTray.dll | 用于历史对照，不能把对照称为本次重新运行 |
| 26300.9818 | 当前磁盘文件、精确缓存 PDB、通用工具重新反汇编；与既有资料逐项对照 | Core/SystemTray.dll | 全部静态结果与原资料相符；既有单屏 Sandbox 的实际范围见 [9818 证据](compatibility-9818.md) |
| 25H2 26200.8457 | 安装映像索引 3 提取的文件、微软精确 PDB、通用工具反汇编及独立安装客体 | Core/Taskbar.View.dll | 静态规则通过；真实单屏时钟更新/恢复及托盘准备/停止通过 |

表中 Core 是 `SystemApps/MicrosoftWindows.Client.Core_cw5n1h2txyewy`。资料以完整模块身份匹配，OS 标签只供显示；例如本次 25H2 的 taskbar/shell 文件资源版本是 `10.0.26100.8457`。各组件可以有自己的版本号，不能要求所有文件资源版本相同来替代映像核对。

## 25H2 输入与精确身份

输入介质为本地 `E:\iso\zh-cn_windows_11_business_editions_version_25h2_updated_may_2026_x64_dvd_4e896038.iso`，索引 3 的映像元数据为 Windows 11 Pro `10.0.26200.8457`。本次没有取得独立官方 ISO 哈希来完成介质来源认证；以下 PE/PDB 匹配证明分析的是哪些二进制，不代替介质来源证明。提取记录为本地忽略目录 `local/windows11-25h2/extraction-result.json`，文件根为 `local/windows11-25h2/Windows`。

| 角色与相对路径 | SHA-256 | PDB key（GUID + age） |
| --- | --- | --- |
| taskbar：`System32/taskbar.dll` | `41326fffd57a2c6e08d1884a698bd9e9dfe90de0614de98733edaebacb5f5403` | `1C6067D3BC7AABA7CDE214B3AE66C6D31` |
| shell：`System32/twinui.pcshell.dll` | `bc5e4daaace00d2eaff630f0f27078c29a0a56ae77b5cf51d3cfd5a766581f59` | `8314175B206200416271631D88BB22741` |
| tray：`SystemApps/MicrosoftWindows.Client.Core_cw5n1h2txyewy/Taskbar.View.dll` | `a920c97ace9f336855ebfea221101eb03739398c8b75ac6dcf28d82ed645aed5` | `0AE0E302E8E34BE29CC303A7A62ABCA71` |

`explorer.exe` 也已提取，SHA-256 为 `a91761acf6de6d350c72ebe8025da961dc3902e5fe5a47d7a56303444a124a81`，不参与这三项私有角色选择。提取器对 Core/SystemTray.dll、Core/ExplorerExtensions.dll、CBS/Taskbar.View.dll、CBS/ExplorerExtensions.dll 返回路径不存在；Core/Taskbar.View.dll 提取成功。通用工具在提取树中检查全部已知位置，只发现这个托盘候选，并保留它的真实名称。

精确 PDB 来自 [Taskbar.pdb](https://msdl.microsoft.com/download/symbols/Taskbar.pdb/1C6067D3BC7AABA7CDE214B3AE66C6D31/Taskbar.pdb)、[twinui.pcshell.pdb](https://msdl.microsoft.com/download/symbols/twinui.pcshell.pdb/8314175B206200416271631D88BB22741/twinui.pcshell.pdb)、[Taskbar.View.pdb](https://msdl.microsoft.com/download/symbols/Taskbar.View.pdb/0AE0E302E8E34BE29CC303A7A62ABCA71/Taskbar.View.pdb)。三者 SHA-256 分别为 `6fa8959b6a345f83902fb9e834a699d8aa47ec97c8a5759cf8cb5cdfa1004486`、`f9e58fbaa0997760d37e1aad2023e0738a266f2830f6e4b644654d50e3a01a0c`、`c0af75a441c16559f3c96748bdfd2a20533eb08bb59762f0e25a9b8acafac4a8`。

工具核对 PE CodeView 与 PDB 信息流 GUID、DBI age，再通过 DbgHelp 的 `PdbSig70`、`PdbAge`、`PdbUnmatched=false`、`DbgUnmatched=false`、`SymType=3` 及注册映像的 timestamp/size 交叉核对。公开 PDB 信息流 age 分别为 3/2/2，DBI age 均为 1；应以 DBI age 与 PE 的 age 匹配。枚举只加载调试元数据，没有加载目标 DLL 或访问宿主 Explorer。

## 可复用的字段关系与不能复用的地址

| 角色或字段 | 26300.9711 | 26300.9818 | 26200.8457 |
| --- | --- | --- | --- |
| primary_site RVA | `0x1CF0C8` | `0x1CFA30` | `0x1D2348` |
| secondary_site RVA | `0x1C14C8` | `0x1D2A48` | `0x1CAF38` |
| primary_host RVA | `0x10B540` | `0x1237B0` | `0x1128B0` |
| secondary_host RVA | `0x16BE10` | `0x17DB20` | `0x176A30` |
| FrameHeight RVA | `0x8A11C` | `0xB10A4` | `0x7DCC8` |
| shared_ptr release RVA | `0x27BA0` | `0x290B0` | `0x4A330` |
| FrameHeight element 字段 | `+0x10` | `+0x10` | `+0x10` |
| host → child 字段 | `+0x38` | `+0x38` | `+0x38` |
| child → source 字段 | `+0x10` | `+0x10` | `+0x10` |
| child 虚表 RVA | `0x7616A0` | `0x787CD8` | `0x7226A8` |
| factory 虚表 RVA | `0x780850` | `0x7A7CD0` | `0x741878` |

25H2 六宿主符号的完整修饰名与既有角色一致，主副 `GetTaskbarHost` 仍以 RDX 为隐藏结果指针，`_Decref` 以 RCX 为引用计数对象。`FrameHeight` 的 `add rcx,0x10` 与调整后字段检查重新从该映像确认。入口位于不可写可执行节，站点虚表位于不可写非执行节。

25H2 的 `TaskbarHost::HasFocus` 位于 `0x12748C`，将 RCX 调整 `+0x38` 后调用子接口消费方法。非 WASDK 的 `IXamlExplorerHostChildWindow` 虚表槽 8 指向 `get_XamlContent`（`0xC5E90`）；该 ABI 方法将 this 调整 `-0x10`，再调用 `0xC5EE8` 的实现。实现以对象 `+0x20` 为 source 字段，因此从 child 接口的相对偏移是 `-0x10+0x20=+0x10`。工厂槽 6 指向同一完整签名的 `CreateInstance(unsigned __int64, void*, void**)`（`0x16D420`）。每个地址都来自本次精确符号，未按统一差值迁移。

这些数值首先支持复用 `xaml_explorer_host_child_v1` 作为 lab 待验方案，随后由独立 25H2 客体的私有宿主和真实主屏检查取得运行证据。它们不能推导将来其他映像也有同一布局。每次接管仍需准确查询 source 接口，核对 child/factory 虚表和内存区权限，并验证线程、HWND、XamlRoot 与生命周期归属。

## 托盘 ABI 的共性与清理差异

| 方法角色 | 26300.9711 RVA | 26300.9818 RVA | 26200.8457 RVA | 三组资料中的 x64 参数角色 |
| --- | --- | --- | --- | --- |
| invocation_point | `0xBCB30` | `0xB860C` | `0x12870` | RCX=this；RDX=Point 隐藏结果；R8=const Point* |
| icon_bounds | `0xBCA04` | `0xB84E0` | `0x11D98` | RCX=this；RDX=Rect 隐藏结果；R8=const Rect* |
| screen_rect | `0xBCC10` | `0xB86EC` | `0xF28FC` | RCX=RECT 隐藏结果；RDX=按值 FrameworkElement 临时包装对象；R8=HWND |

25H2 前两项保存 `R8/RDX/RCX → RSI/RBX/RDI`，通过 RBX 写结果并返回 RBX。第三项保存 `R8/RDX/RCX → R15/RSI/R14`，通过 R14 写回 16 字节 RECT 并返回 R14。三个完整修饰名、函数边界及实际指令共同支持复用 `system_tray_coordinates_x64_v1`；真实客体随后通过入口准备/停止。该范围未包括副屏菜单坐标行为，不能仅凭同名符号推断 ABI，也不能把 Prepare 当作交互验收。

首轮工具将 25H2 拒绝为 `Missing owned FrameworkElement cleanup check`，原始拒绝结果保留。检查后确认差异是编译形状：9818 在 screen_rect 内检查 `[rsi]` 后直接调用 `IUnknown::unconditional_release_ref`；25H2 在 `0xF2A73` 将 RSI 传入 `0xEFE0` 的精确 `IUnknown` 析构函数，由它完成空值检查，再调用 `0x1E770` 的 release 叶函数。叶函数保存原指针、清零包装字段、取虚表 `+0x10` 的 Release，经精确 `__guard_dispatch_icall_fptr` 尾调用。工具现用两条独立的窄规则检查这两种形状，也检查清理发生在结果写入之后、返回结果指针之前。

该叶函数没有 `.pdata` 条目，因此仅为它提供六条指令的固定形状例外，同时验证精确符号和 CFG 指针。其余函数仍要求 `.pdata` 边界；没有允许任意无边界函数、猜测偏移或跳过清理检查。调用方仍须保留自己需要的接口引用，并允许原函数消费按值临时对象，避免双重释放。

25H2 二进制包含 `SystemTray.SystemTrayFrame`、`SystemTrayFrameGrid`、`TimeInnerTextBlock`、`DateInnerTextBlock`、`NotificationAreaIcons`、`SecondaryClockStack` 等名称，且存在相关类型符号。离线阶段这些只构成 `system_tray_frame_v1` 的拓扑线索，不能证明控件实际存在、父子层级、模板、可见性、线程或根窗口归属。第三轮真实客体检查确认了主屏需要的控件关系、source/XamlRoot 及归属，并在这些控件上完成更新与恢复；它没有创建第二个 XamlRoot 或验证副屏树。

## 候选工具、拒绝边界与复现

通用入口为 [`tools/compatibility/inspect_modules.py`](../tools/compatibility/inspect_modules.py)，参数与输出约定见 [README](../tools/compatibility/README.md)。输出采用独立的 `litetray.compatibility.candidate.v1`，始终标记 `production_ready=false`、`registry_modified=false`；生产 Profile 解码器不能直接读取这个封套。工具没有注册、提升、激活或 `--force` 路径。

```powershell
python tools/compatibility/inspect_modules.py `
  --system-root local/windows11-25h2/Windows `
  --output local/compat-26200.8457-repeat `
  --symbol-cache local/compat-26200.8457-evidence/symbols `
  --offline --label "Windows 11 Pro 25H2 26200.8457, image index 3"
```

输出目录必须是新目录。已知模块位置缺失或有歧义时会拒绝，应以 `--module ROLE=relative/path.dll` 明确真实路径。角色重复、同一文件/同一映像冒充多个角色、逃逸路径、PDB 错配、同一完整符号对应不同地址、不可接受节权限、缺失边界、未知指令及超范围字段均不会通过。模块来自同一个输入树仍不证明它们属于同一次安装，所以 `coherent_installation_verified=false`；静态候选不得用来拼装跨版本生产配置。

本地证据目录如下，均受 Git 忽略；Windows DLL 和 PDB 不随源码发布：

- `local/compat-26200.8457-evidence/`：第一轮未知清理形状的拒绝结果及精确 PDB 缓存。
- `local/compat-26200.8457-candidate/`：全部静态规则通过的候选、三组符号与反汇编，及手工 lab 转换审计记录。
- `local/compat-tool-9818-regression-v2/`：新清理规则的 9818 回归。所有完整身份、哈希、宿主布局、source 偏移/虚表及托盘入口与既有 9818 JSON 逐项相同；路径大小写按 Windows 语义比较。

本次 Python 测试 14 项通过，包括两种正确清理调用图，以及错误参数寄存器、错误对象清理、错误虚表槽、未清零包装对象、错误 CFG 指针、错误条件跳转、PDB 截断/身份错配、歧义符号/模块、路径越界和节权限拒绝。测试不读系统文件、不联网、不打开 Explorer。另以真实 9818 和 25H2 映像执行完整工具，二者均返回静态成功。

## 从 lab 候选到限定范围的内置资料

8457 首先人工审阅并转写为显式 lab 候选，用冻结实验包完成独立客体检查。真实主屏验收通过后，该精确资料移入默认注册表，现为 [`profiles/windows11-26200.8457-x64.json`](../crates/adapter-protocol/profiles/windows11-26200.8457-x64.json)。默认构建包括 9711、9818、8457 三组资料；保留的 `compatibility-lab` feature 当前不增加额外候选。这次提升以人工复核和客体证据为依据，通用取证工具仍只输出 `production_ready=false` 的候选，不自动注册，也不能以环境变量或任意外部 JSON 覆盖模块身份。

该模块变体保持真实 `Taskbar.View.dll` 文件名和 Core 路径，名称、路径、source recipe、topology 和 ABI 组合均精确限定。9711/9818 仍使用各自已审阅的 `SystemTray.dll` 组合。适配器继续检查磁盘 SHA-256、已加载的实际路径和完整 PE/CodeView 身份、不可写可执行入口区域与 16 字节入口前缀；不得加载替代系统 DLL 来绕过这些条件。

第三轮使用与第二轮相同的 69 文件冻结 lab 包，从已核验的普通用户 Medium 完整性 helper 运行，三模块匹配、164 项隔离 XAML 和 6 项私有宿主检查通过。真实主屏时钟连续读回三帧文字和 55→240→320→55 的容器宽度变化，停止后恢复原文字、尺寸和可见性；Explorer PID 4104、TID 944、任务栏 HWND 131252 保持不变。五类托盘必需区域和入口 Prepare/Stop 通过，复制未激活。此前两轮的发现与响应失败均保留，不能只摘取成功结果。

第四轮以不启用 lab feature 的默认 release 构建重复全部六项检查，均 exit 0、无超时；三帧更新、宽度恢复、托盘准备/停止及 Explorer 连续性再次通过。71 文件冻结包与三份正式程序哈希单独记录；最终分发仅补入验收文档时，整体清单可变化，三份二进制须与已测程序相同。

对应资料仅登记 `module_layout`、`clock_update_restore`、`tray_prepare_stop` 三个实测场景。托盘复制、时钟输入、日历/通知、第三方悬停、菜单和多显示器/DPI 需分别补验；仅准备三个坐标入口或发现五个系统区域不等于激活复制。lab 与默认构建各自的二进制、清单和原始报告哈希见 [运行记录](compatibility-8457.md)。
