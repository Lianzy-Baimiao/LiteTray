# Windows 11 25H2 26200.8457 x64 虚拟机兼容证据

2026-11-09，独立 Windows 11 Pro 25H2 `26200.8457` x64 客体先通过冻结 lab 包检查，随后完成默认生产构建复测：三模块精确匹配、164 项隔离 XAML 检查、6 项私有 Shell 宿主检查，以及真实主屏的三次时钟文字更新、宽度变化、停止恢复和托盘入口准备/停止均取得成功结果。Explorer PID/TID 和主任务栏 HWND 保持不变。[8457 精确资料](../crates/adapter-protocol/profiles/windows11-26200.8457-x64.json) 已纳入默认内置目录，**验收范围限于上述单屏场景，实际点击、滚轮、菜单、悬停、托盘复制及多显示器/DPI 尚未验收。**

第三轮使用与第二轮相同的冻结 lab 二进制；第四轮使用不启用 `compatibility-lab` 的独立默认构建，两轮的运行 helper 均已核验为与 Explorer 相同的 Medium 完整性。两种包的哈希及原始结果分别记录。前两轮因任务栏窗口发现失败、未收到适配器响应而未请求写入，下面保留失败与诊断结果作为修复对照。

离线符号、字段与 ABI 证据见 [三组模块的共性与差异](windows11-commonality.md)。本文记录真实客体的运行结果，并保留失败现场，供修复后复测对照。

## 客体、输入与模块身份

客体从已有安装介质 `zh-cn_windows_11_business_editions_version_25h2_updated_may_2026_x64_dvd_4e896038.iso` 的 WIM 索引 3 独立安装。系统报告 Windows 11 专业版、`26200.8457`、`25H2`、x64；它不是复用宿主组件的 Windows Sandbox。本次没有取得独立官方 ISO 哈希完成介质来源认证，模块/PDB 身份匹配不能代替这一来源证明。

运行环境为 QEMU 11.1.0、TCG 软件模拟、2 个 vCPU、4 GiB 内存、一块 1024×768 显示器。QEMU 使用 `-display none -nic none`，实验包通过只读 ISO 提供，结果通过客体 COM1 回传。客体窗口和截图操作均经本机回环 QMP 进行，没有对宿主桌面或宿主 Explorer 执行测试。该环境没有证明硬件加速、多显示器或混合 DPI 场景。

客体磁盘三模块与离线候选完整匹配，选中资料 ID 为 `windows11-26200.8457-x64`：

| 角色与相对路径 | SHA-256 |
| --- | --- |
| taskbar：`System32/taskbar.dll` | `41326fffd57a2c6e08d1884a698bd9e9dfe90de0614de98733edaebacb5f5403` |
| shell：`System32/twinui.pcshell.dll` | `bc5e4daaace00d2eaff630f0f27078c29a0a56ae77b5cf51d3cfd5a766581f59` |
| tray：`SystemApps/MicrosoftWindows.Client.Core_cw5n1h2txyewy/Taskbar.View.dll` | `a920c97ace9f336855ebfea221101eb03739398c8b75ac6dcf28d82ed645aed5` |

磁盘报告的 `loaded_modules_checked` 和 `runtime_topology_checked` 仍为 `false`。私有宿主测试只验证应用自建宿主；第三轮真实 Explorer 的检查结果单独记录，不改写这些磁盘报告字段。

## 首轮冻结实验包与检查结果

本轮使用显式 `compatibility-lab` 构建，run ID 为 `sandbox-20261108-234354-833ee83c`。客体于本地时间 00:37:25 开始执行 harness，00:43:01 完成。67 个包文件的 SHA-256 全部与冻结清单一致；清单 SHA-256 为 `3b226f04f16bb6f77d1fe45cbe72b95be6169e30f393e77bbc43ed1e828dc8e8`。

| 冻结文件 | SHA-256 |
| --- | --- |
| `LiteTray.exe` | `a36e17f5047bcc2f5281c5304dc60701fd26e0d2dffa50e6c5496ebee3c06477` |
| `litetray_taskbar_adapter.dll` | `e76b217a230b183e9b4e445268eb1977507e897ae2259579a2c9e757f3f09293` |
| `litetray_popup_hook.dll` | `88e0a1665b45743511eae7687b2fc1a77a9c47cac32c919be3bc1134c56ccd1b` |

[客体 harness](../tools/Invoke-SandboxCompatibilityCheck.ps1) 按隔离前置检查、私有宿主、真实主屏检查的顺序运行。所有阶段均取得实际退出码，没有超时：

| 检查 | 退出码 | 记录耗时 | 本轮结果 |
| --- | --- | --- | --- |
| 磁盘兼容性 | 0 | 5,984 ms | 三模块匹配；四项功能均为磁盘匹配、仍需运行时验证 |
| 诊断报告 | 0 | 1,023 ms | 报告生成成功；`taskbars=[]`，不表示发现任务栏成功 |
| 应用图标 | 0 | 504 ms | 图标检查通过 |
| 隔离 XAML 时钟 | 0 | 34,024 ms | 164 项通过；未访问 Explorer |
| 自有私有 Shell 宿主 | 0 | 6,870 ms | 6 项通过；source/root、路由和自有宿主中的恢复检查通过，`production_taskbar_verified=false` |
| 真实主屏任务栏 | 1 | 3,001 ms | `未找到唯一的原生主任务栏，未接管` |

最后一项的原始 `taskbar-live.json` 记录 `explorer_accessed=false`、`writes_requested=false`、`input_injected=false`，`checks` 和 `samples` 均为空；时钟未通过，托盘未准备、未激活。`cleanup_acknowledged=true` 只说明没有待清理连接，不能当作“修改后恢复通过”。同理，`explorer_identity_preserved=false` 表示本轮未建立该验收结论，不能据此推断 Explorer 曾崩溃或重启。

首轮没有证明真实时钟文字或宽度修改、停止恢复、托盘 Prepare/Stop、模型复制、鼠标/键盘交互、跨屏菜单或混合 DPI 可用。私有宿主的输入委托检查不替代真实日历点击、滚轮音量、菜单和悬停测试。

## 窗口发现失败的只读对照

原失败结果保留后，分别从 bootstrap 继承环境和显式 `WinSta0\Default` 启动同一个冻结 `--diagnose` 程序。两个进程都在 session 2、窗口站 `WinSta0`、线程桌面和输入桌面 `Default`；Explorer PID 均为 4104，启动时间相同。两次 `EnumWindows` 都成功且 `GetLastError=0`，各返回 65 个窗口，但没有 `Shell_TrayWnd`。因此“首次登录命令继承了另一张桌面”不能解释这次失败。

进一步对比了同一客体的查询结果：

| 查询 | 实际观察 |
| --- | --- |
| `FindWindowW("Shell_TrayWnd")` | 找到 HWND 131252、PID 4104、TID 944 |
| `FindWindowExW(NULL, NULL, "Shell_TrayWnd", NULL)` | 找到同一个窗口；可见，矩形 `[0,720,1024,768]`，父为桌面 HWND 65548，根为自身 |
| `EnumDesktopWindows` | 成功、错误码 0，67 个窗口，遗漏该任务栏 |
| 从桌面开始的 `GW_CHILD/GW_HWNDNEXT` 链 | 67 个窗口、未截断，遗漏该任务栏 |
| `EnumChildWindows(desktop)` | 可见 `TrayDummySearchControl`、`TrayNotifyWnd` 子窗口，父和根均指向 HWND 131252 |
| `EnumThreadWindows(944)` | 成功、错误码 0，只返回 `MiracastConnectionWindow` 和 `WorkerW`，同样遗漏 HWND 131252 |

这组结果将故障定位到窗口发现路径。宿主诊断和 Explorer 内部 inspect 分别使用的桌面/线程枚举都受影响；只改变其中一处仍可能在另一处得到空结果。精确类名查询可以用于发现候选，但后续仍必须核对唯一主任务栏、顶层窗口关系、PID/TID、模块身份、入口、source、XamlRoot 以及恢复语义。该观察不证明所有 25H2 更新都有同样枚举行为，也不证明类名查询本身足以安全接管。

## 第二轮窗口发现修复与响应阻断

第二轮 run ID 为 `sandbox-20261109-010801-912eccaa`，于本地时间 01:09:18 至 01:10:02 执行。宿主诊断与 DLL 内部发现均改用两种精确任务栏类名的 `FindWindowExW` 查询；查询有数量和循环边界，失败不返回部分结果，保留唯一主任务栏检查，DLL 在私有宿主读取前继续核对当前 PID/TID 及顶层窗口归属。模块身份、入口、source 和 XamlRoot 校验没有放宽。该源码的默认与 lab workspace 测试分别为 **251 项通过、9 项忽略**，两种构建的 Clippy 均通过。

新的冻结 lab 包有 69 个记录，客体核验全部与冻结设置一致。清单 SHA-256 为 `80d656bbfd1d0bee407feb1e8f071cbe49384f728b079e69ed07507128a18393`，二进制身份为：

| 第二轮冻结文件 | SHA-256 |
| --- | --- |
| `LiteTray.exe` | `79909e5d79ff2dff0eab8f197c0cdbfae1388b53e119d774fdaa4eac330fdf72` |
| `litetray_taskbar_adapter.dll` | `ec1eaa8c586a38778da7b68d4df1388a98fb970b8480032cbbaad7ec572b72fe` |
| `litetray_popup_hook.dll` | `6e5232db030998a16b15677822aaafe9617c0bed660d880f287bb73721cb0ad4` |

| 第二轮检查 | 退出码 | 记录耗时 | 实际结果 |
| --- | --- | --- | --- |
| 磁盘兼容性 | 0 | 1,524 ms | 三模块匹配同一 8457 候选 |
| 诊断报告 | 0 | 932 ms | 发现主任务栏 HWND 131252、PID 4104、TID 944，并映射到 `DISPLAY1` |
| 应用图标 | 0 | 274 ms | 通过 |
| 隔离 XAML 时钟 | 0 | 25,421 ms | 164 项通过 |
| 自有私有 Shell 宿主 | 0 | 2,951 ms | 6 项通过；仍限自有进程宿主 |
| 真实主屏任务栏 | 1 | 8,567 ms | `任务栏未返回兼容响应，未启用接管` |

本轮均未超时。live 报告已包含正确主屏路由，`explorer_accessed=true` 表示已经尝试只读适配器检查；初始检查和最终检查均未取得兼容响应。`writes_requested=false`，`checks/samples` 为空，时钟未通过，托盘未准备、未激活。宿主发现层的改动已得到真实报告验证，但没有响应就不能证明 DLL 内部发现代码已经执行成功，也不能声称时钟恢复或托盘 Prepare/Stop 通过。

只读令牌诊断 run `sandbox-20261109-011221-75a28183` 随后确认：

| 进程 | Session | 完整性级别 | RID | Elevated / UIAccess |
| --- | --- | --- | --- | --- |
| bootstrap PowerShell，PID 4936 | 2 | High | 12288 | 1 / 0 |
| Explorer，PID 4104 | 2 | Medium | 8192 | 0 / 0 |

这些是诊断时两个仍存活进程的直接令牌观察；当时第二轮测试子进程已结束，不能把该记录冒充子进程令牌采样。该结果促使第三轮从已核验的 Medium 完整性 helper 运行同一冻结程序。相同 session 和桌面不能证明完整性级别相同；仅观察到这项差异，也不能独立证明 UIPI 就是故障根因。令牌诊断还列出了 Explorer 中三份目标模块的实际路径与对应磁盘哈希，不能替代适配器内部的 loaded-image、source 或拓扑检查。

## 第三轮普通用户环境中的实际通过

第三轮 run ID 为 `sandbox-20261109-012629-e342827c`，于本地时间 01:28:45 至 01:29:35 完成。运行 helper 是已核验的 64 位 PowerShell，PID 7044、session 2、Medium 完整性 RID 8192、`elevated=0`。其完整性级别与 Explorer 相同；69 个包文件重新通过 SHA-256 核验，清单与第二轮完全相同，仍为 `80d656bbfd1d0bee407feb1e8f071cbe49384f728b079e69ed07507128a18393`，三份程序哈希沿用上表。

| 第三轮检查 | 退出码 | 记录耗时 | 实际结果 |
| --- | --- | --- | --- |
| 磁盘兼容性 | 0 | 1,676 ms | 三模块匹配 `windows11-26200.8457-x64` |
| 诊断报告 | 0 | 712 ms | 发现同一主任务栏并建立主屏路由 |
| 应用图标 | 0 | 255 ms | 通过 |
| 隔离 XAML 时钟 | 0 | 26,896 ms | 164 项通过 |
| 自有私有 Shell 宿主 | 0 | 3,207 ms | 6 项通过 |
| 真实主屏任务栏 | 0 | 12,807 ms | 文字与宽度更新、停止恢复、托盘准备/停止通过 |

本轮没有超时。live 摘要为 `checks_passed`、`primary_clock_passed`，且 `prerequisite_checks_passed=true`。初始、活动与最终检查取得实际响应；真实 Explorer 中的 source、XamlRoot、窗口及线程归属均通过现有运行时校验。

三帧时间文字 `LT LIVE 01/02/03` 与日期文字 `Sandbox check A/B/C` 均按请求读回。时钟容器宽度由原生 55 变为 240、320，第三帧维持 320；Stop 收到确认后，原时间、原日期、容器宽度 55、文字尺寸及可见性均与接管前一致。全过程使用 Explorer PID 4104、TID 944、任务栏 HWND 131252，未以重启 Explorer 代替恢复。这只证明报告中观察到的文字、尺寸和可见性恢复，不扩大为所有字体、颜色或绑定场景已经回归。

托盘 Prepare/Stop 在真实任务栏上通过，验证 `hidden_icons`、`notification_icons`、`system_icons`、`privacy_indicators`、`control_center` 五类必需区域及坐标入口。`activated=false`，单屏报告明确 `replication=not_applicable_single_monitor`；没有创建副屏复制目标，也没有执行实际鼠标或键盘输入。日历点击、滚轮音量、托盘菜单、第三方悬停、多显示器和混合 DPI 仍为 `not_run`。

相同冻结二进制在已核验 Medium helper 下取得响应并通过，说明第二轮的进程权限环境是实际排查因素。未捕获具体 UIPI 拒绝码，不能把该对照表述为已经证明某条 Windows 底层拦截路径。

这项记录仅为资料目录中的 `module_layout`、`clock_update_restore`、`tray_prepare_stop` 三个场景提供限定范围的实测证据。其他验收场景继续保留 `not_run`。资料精确绑定三个模块身份与 SHA-256，不覆盖其他 25H2 累计更新或全部 Windows 11。

## 第四轮默认生产构建复测

8457 资料进入默认注册表后，使用未启用 `compatibility-lab` 的 release 构建重新冻结 71 个包文件。清单 SHA-256 为 `f8ab40d3f64f23f1d2006a2cc977b6bd7080106ce8dc5c5b8945b923ec69094e`，客体逐文件核验通过。程序身份与第三轮不同：

| 默认构建冻结文件 | SHA-256 |
| --- | --- |
| `LiteTray.exe` | `06240bb3bf5933990bf304c8efb49f5913d7a3eda607a0344a267d604dcb9729` |
| `litetray_taskbar_adapter.dll` | `bddbd2a7c238ed941174fa540cfe0425a1086de2f5edd727912807cc14492ec3` |
| `litetray_popup_hook.dll` | `b042fc40dec401f10c8376aec439a691de18570e98e65c7637b0e81e285f92ac` |

run ID 为 `sandbox-20261109-013933-36962106`，本地时间 01:43:16 至 01:44:12。已核验 helper PID 2620、session 2、Medium RID 8192、`elevated=0`，令牌读取无错误。隔离基线与真实任务栏检查各有本轮独立实例，均报告 `checks_passed`：

| 默认构建检查 | 退出码 | 记录耗时 | 实际结果 |
| --- | --- | --- | --- |
| 磁盘兼容性 | 0 | 2,019 ms | 三模块匹配默认内置 `windows11-26200.8457-x64` |
| 诊断报告 | 0 | 648 ms | 主屏发现与路由通过 |
| 应用图标 | 0 | 288 ms | 通过 |
| 隔离 XAML 时钟 | 0 | 25,175 ms | 164 项通过 |
| 自有私有 Shell 宿主 | 0 | 4,104 ms | 6 项通过 |
| 真实主屏任务栏 | 0 | 18,717 ms | 三帧文字/宽度更新、停止恢复、托盘准备/停止通过 |

六项均未超时。`LT LIVE 01/02/03` 与 `Sandbox check A/B/C` 的文字逐帧读回匹配，容器宽度为 240、320、320；Stop 后原时间、日期、尺寸、可见性及宽度 55 恢复。`clock.passed`、两项 Stop 确认、`explorer_identity_preserved` 和 `cleanup_acknowledged` 均为 `true`；Explorer 仍为 PID 4104、TID 944，主任务栏仍为 HWND 131252。托盘复制未激活，`input_injected=false`，仍只登记前三个场景通过，其余四项场景保持 `not_run`。

默认源码验证为 workspace **251 项通过、9 项忽略**，Clippy 通过；另有兼容性页后台检测/导出检查通过。日志保存在 `promoted-default-tests.log`、`promoted-default-clippy.log`、`promoted-default-compatibility-e2e.log`。这些源码检查与本轮虚拟机结果分开记录，不能互相替代。

该冻结包先于本节验收文档生成。最终分发包补入这些文档后，整体清单哈希会变化；发布核验须保证 EXE、适配器 DLL 和 popup DLL 仍与上表完全一致，并把新的分发清单与本次测试清单分开记录。只有文档变化不能被写成重新运行了另一套二进制；程序内容发生变化则需另行复测。

## 原始材料与复测边界

以下原始文件均位于 Git 忽略目录 `local/compatibility-5140/`，不随源码打包发布；Windows DLL、PDB、安装介质、虚拟磁盘和客体凭据也不随本记录发布。

| 记录 | 相对该目录的位置 |
| --- | --- |
| 首轮隔离结果 | `guest-results/sandbox-20261108-234354-833ee83c/sandbox-20261108-234354-833ee83c-guest-127bc539/` |
| 首轮私有宿主和真实任务栏结果 | `guest-results/sandbox-20261108-234354-833ee83c/sandbox-20261108-234354-833ee83c-live-f25f9eb0/` |
| 两种桌面启动方式对照 | `guest-results/sandbox-20261109-004838-873fc214/desktop-diagnostics-sandbox-20261109-004838-873fc214/` |
| 窗口查询对照 | `guest-results/sandbox-20261109-005157-45d1bd40/window-discovery-sandbox-20261109-005157-45d1bd40/windows.json` |
| 线程窗口查询对照 | `guest-results/sandbox-20261109-005615-08e040f1/thread-window-discovery-sandbox-20261109-005615-08e040f1/thread-windows.json` |
| 第二轮隔离结果 | `guest-results/sandbox-20261109-010801-912eccaa/sandbox-20261109-010801-912eccaa-guest-b1ae2a7e/` |
| 第二轮私有宿主和真实任务栏结果 | `guest-results/sandbox-20261109-010801-912eccaa/sandbox-20261109-010801-912eccaa-live-c7af0e2b/` |
| 完整性与模块路径诊断 | `guest-results/sandbox-20261109-011221-75a28183/integrity-diagnostics-sandbox-20261109-011221-75a28183/integrity.json` |
| 第三轮 Medium helper 核验与结果根 | `guest-results/sandbox-20261109-012629-e342827c/medium-launch-cadc49e53c7c47d4a28ba4be189a4aff/` |
| 第三轮隔离结果，相对该结果根 | `runner-results/sandbox-20261109-012629-e342827c-guest-82560de3/` |
| 第三轮私有宿主和真实任务栏结果，相对该结果根 | `runner-results/sandbox-20261109-012629-e342827c-live-fe09ed4e/` |
| 第四轮默认构建 Medium helper 核验与结果根 | `guest-results/sandbox-20261109-013933-36962106/medium-launch-9de8a06fe67742a2b59b1fb3896c1c49/` |
| 第四轮隔离结果，相对该结果根 | `runner-results/sandbox-20261109-013933-36962106-guest-775aa93b/` |
| 第四轮私有宿主和真实任务栏结果，相对该结果根 | `runner-results/sandbox-20261109-013933-36962106-live-05893e00/` |

`vm-lab-5140-first-run-review.json`、`vm-taskbar-discovery-diagnosis.json`、`vm-thread-taskbar-discovery-diagnosis.json` 和 `vm-discovery-fix-run-review.json` 保存前两轮提炼后的观察。`vm-medium-live-readback-review.json` 与 `vm-medium-live-run-review.json` 保存第三轮逐帧读回、恢复、Explorer 连续性及随后 loaded-image 身份探测的复核。`vm-final-default-run-review.json` 保存第四轮默认构建的完整复核。`vm-validation-evidence-sha256.json`、`vm-thread-discovery-evidence-sha256.json`、`vm-discovery-fix-run-evidence-sha256.json`、`vm-medium-live-evidence-sha256.json`、`vm-final-default-evidence-sha256.json` 清单保存这些原始文件完整性信息。关键摘要如下：

| 文件 | SHA-256 |
| --- | --- |
| 首轮回传 ZIP（21,422 字节） | `44db27fbabc6a8bed9fb4ad711abb83cc6e81ac450948c1008ffa199aee45bbc` |
| 首轮隔离 `summary.json` | `3e52aa6f8283f1e9b52bf6c28238bd6f22ef7856a1091c3d924b7d33aaa20f9d` |
| 首轮 live `summary.json` | `ee69efbbd01436f6efed5d7d373e5878a0a521fd41421957fe14f9783fd441a2` |
| 首轮 `taskbar-live.json` | `4900fedb80b4d5c1883d6b17a8e1f544824b148e724dd311eb6558728fad1f70` |
| 窗口对照 `windows.json` | `13d1616efb93cc5ac7f0bcdd8d8707f0cc70ae34918282dbc513bf86ed08d7f1` |
| 线程对照 `thread-windows.json` | `6355a1c593951063c971da6f0d69878c571acb300d57623ea11749707d1e28b9` |
| 第二轮回传 ZIP（64,508 字节，含此前累积结果） | `9ba9fc987db3a66a267ac7861f3e313c911bfbfe0cb93179930807dfe91322ab` |
| 第二轮隔离 `summary.json` | `373047a1c1a8c6ba836c73176ac9880d97dac2692defeed82b6c594ec7b6632b` |
| 第二轮 live `summary.json` | `b234d52e5d88a68da01883a87ba6217e0fb467def23f89087440057d66ebb12c` |
| 第二轮 `taskbar-live.json` | `717d65081a822abd337f41e97d1eeaa7a0da196d375f15efd8b9b38927555e58` |
| 完整性诊断 `integrity.json` | `315d3025dfa2767d3ddc98220dbac77ac3df9711e59191638e0937949dd82201` |
| 第三轮 `verified-worker.json` | `4065c08f09780edb9a6ae23ee49d40a261c4957bf09bf8d4ac10833792b5e561` |
| 第三轮隔离 `summary.json` | `0ffd73ffa9aa04aceea7360616a7246d139cc50bc04d7a3e5776a13f9897824d` |
| 第三轮 live `summary.json` | `dc170f66a8620977969e85771f6e4d0fba97a3e7398df9f518710a1377c63629` |
| 第三轮 `taskbar-live.json` | `bc0c2e143c0e41cd572dbdbb40db65375183a6c4f3e9b6bb108704090236c6f0` |
| 第三轮独立 `vm-medium-live-readback-review.json` | `17a5ceb6aa33254068c98d88e66d3496aa3b5a9ce1cd45de82d507b5bca69b3a` |
| 第三轮汇总 `vm-medium-live-run-review.json` | `0b278f70584e6f432ee7cf23522f7085fb0d181ca0159722952b593f10fccb3f` |
| 第四轮 `verified-worker.json` | `c529adf6366314ddcc450c92d460e46ddec5a613cdabc6ebf8d086a696fdf4f1` |
| 第四轮隔离 `summary.json` | `70af593d69a89ee7d43f01004787c664b892c616f88bb63888130b4b458354d9` |
| 第四轮 live `summary.json` | `012bbbeea87be155eea0b0906ada88718e27c45c1f5873e07f0e20df73072bc2` |
| 第四轮 `taskbar-live.json` | `b4dcb6b7f58e209ee932c75ac0084d9a20ba2037cb57df1c70bf058b6c86c3b4` |
| 第四轮默认构建复核 `vm-final-default-run-review.json` | `24d3971ac6f8e278c4a41c26764b62ce7d729cfb23cd275a8fec7e11994baa81` |
| 第四轮 `runner-context-sandbox-20261109-013933-36962106.json` | `2250ab3032af4221870c6bd9effbb2310a950d970dadb1f9fbc4aa98eb4271eb` |

串口收集器核对归档长度、SHA-256、ZIP CRC 和解包路径后才发布结果；传输完整不等于验收通过。后续回传包可能包含此前累积的结果，复测必须按新的 run ID 和 `guest/live` 实例读取，不能把旧轮次的通过项混入新结果。

后续复测应使用已登录的可丢弃客体，在与 Explorer 相同且已核验的 Medium 完整性用户环境运行 harness，核对 session、桌面、实际令牌及冻结包身份；不能假设继承 FirstLogonCommands 的 helper 具有普通用户权限。每次新构建需要记录自己的包与二进制哈希，并在新 run ID 下执行相应检查。前两轮失败、第三轮 lab 成功和第四轮默认构建成功及原始材料继续保留，新增输入、多屏和 DPI 场景时另记结果。
