# 本机 Windows Sandbox 启动检查

2026-11-08 检查开始时，宿主为 Windows 11 Pro 26H2 `26300.9711`。CIM 确认 Windows Sandbox 可选功能已启用，固件虚拟化与 hypervisor 正在运行；完整 Hyper-V 管理角色未启用。以下先保留更新前的失败记录，再记录当晚更新到 `26300.9818` 后的复测结果。

| 尝试 | 配置 | 结果 |
| --- | --- | --- |
| 16:46 | 程序包只读输入、独立结果目录、禁网、vGPU 开启 | 客体初始化失败，`0x80370106` |
| 16:49 | 相同检查，vGPU 关闭 | 客体初始化失败，`0x80370106` |
| 16:52 | 最简配置，vGPU/网络/剪贴板关闭；无目录映射、无启动命令 | 客体初始化失败，`0x80370106` |

错误均由 Windows Sandbox 原生对话框报告：“虚拟机或容器意外退出”。三次均未进入 LiteTray 客体检查；不能把它记为 LiteTray 兼容失败或通过。没有提交系统反馈，错误窗口已关闭。

微软 [HCS 错误码](https://learn.microsoft.com/en-us/virtualization/api/hcs/reference/hcshresult) 将 `0x80370106` 定义为 `HCS_E_UNEXPECTED_EXIT`。它不同于表示缺少所需功能的 `0x80370102`，因此不能直接归因于完整 Hyper-V 未启用。最简配置同样失败，也将 LiteTray、文件映射与客体启动脚本排除出本次失败路径。

## 管理员日志已取得

19:03 后通过提权只读采集取得全部 7 个通道的日志，原始 JSON 与 EVTX 保存在被 Git 忽略的 `local/` 诊断目录。

三次均有 Worker 18500（虚拟机启动成功）和 18601（操作系统启动成功），随后出现 18515（由于来宾操作系统发起重置而关闭）。以下时间为 UTC+08:00：

| 配置 | 来宾 OS 启动 | 来宾重置关闭 | 间隔 |
| --- | --- | --- | --- |
| vGPU 开启 | 16:46:22.045 | 16:47:30.017 | 67.972 秒 |
| vGPU 关闭 | 16:49:57.998 | 16:51:05.484 | 67.486 秒 |
| 无映射、无启动命令 | 16:52:24.045 | 16:53:31.492 | 67.447 秒 |

因此故障发生在来宾初始化过程中、桌面和测试脚本运行之前。Compute 的 `0xC0370103` 是 `ERROR_VMCOMPUTE_OPERATION_PENDING`，表示异步操作已开始，不能当作创建失败；关闭路径中的 `0xC0370106` 才是 `ERROR_VMCOMPUTE_UNEXPECTED_EXIT`。没有取得客体崩溃转储，也没有证据将“来宾重置”进一步断言为蓝屏或某个驱动崩溃。

`vmcompute`、`hns`、`HvHost` 均正常运行。Sandbox 功能已启用且三次来宾 OS 都已启动，当前证据不支持通过启用完整 Hyper-V 管理角色来修复这个错误。此诊断阶段没有修改 Windows 功能、安全设置、服务配置或执行重启。

## 组件存储扫描结果

19:04 的 `DISM /Online /Cleanup-Image /CheckHealth /English` 返回组件存储可修复。归档 CBS 日志表明，2026-10-28 的旧组件清理已记录载荷哈希不匹配和腐坏标记；单凭 CheckHealth 不能区分历史标记与当前损坏。

随后实际执行 `DISM /Online /Cleanup-Image /ScanHealth /English`，19:08:28 开始，19:14:00 完成，退出码 0，结果仍为 `The component store is repairable`。本次 CBS 摘要明确为：

```text
Operation: Detect only
Operation result: 0x0
Total Detected Corruption: 416
CSI Payload Corruption: 416
Total Repaired Corruption: 0
Reboot required: no, RepairNeeded: yes
```

清单和元数据损坏计数均为 0。载荷记录涉及组件存储中的 UserExperience、CloudExperienceHost、WinAppSDK 等基线组件；这是当前扫描检出的 416 条损坏记录，不等于 416 个正在运行的系统文件损坏。现有证据仍未建立这些记录与沙盒退出之间的直接因果关系。这次扫描仅检测，随后经授权执行了下述修复。

## 组件修复与沙盒复测

19:21:13 执行 `DISM /Online /Cleanup-Image /RestoreHealth /NoRestart /English`，通过 Windows Update 获取匹配修复源，19:39:15 成功结束，退出码 0。CBS 记录：

```text
Operation: Detect and Repair
Operation result: 0x0
Total Detected Corruption: 555
Total Repaired Corruption: 555
CSI Payload Corruption: 416
CSI Payload Repaired: 416
CSI FileFlags Repaired: 139
All WCP store corruptions were fixed
```

随后运行 `sfc /scannow`，19:43:54 结束，退出码 0，明确报告“Windows 资源保护找到了损坏文件并成功修复了它们”。本次 CBS 的修复对象为 AutoSuperRes 注册数据 `updater_AutoSuperRes.json`，事务报告全部修复成功。

紧接的 `DISM /Online /Cleanup-Image /CheckHealth /NoRestart /English` 退出码 0，结果变为 `No component store corruption detected`。DISM 正式结束日志报告无需重启；CBS RebootPending、PackagesPending、Windows Update RebootRequired 和 `pending.xml` 均不存在。组件修复阶段未执行重启。原始日志和检查结果保存在被 Git 忽略的 `local/windows-component-repair-20261108-192112/`。

19:44:55 重复打开相同最简 Sandbox 配置，仍出现原生 `0x80370106` 错误，未进入客体桌面。管理员日志确认 19:44:57.294 来宾 OS 成功启动，19:46:05.160 因来宾重置关闭，间隔 67.866 秒。没有运行完整 LiteTray 客体检查，没有提交系统反馈，错误窗口已关闭。

因此，本轮系统组件修复成功，但没有消除沙盒初始化故障，不能把“组件健康”当作“沙盒恢复”的证据。组件修复结束时，系统仍为 `26300.9711`；当时的 LiteTray 磁盘兼容报告继续全部匹配，Explorer 和 LiteTray 均未重启。

## 已核实的微软修复与处理边界

微软 [KB5127936（2026-10-28）](https://support.microsoft.com/help/5127936) 明确写明修复 Windows Sandbox 意外关闭并显示 `0x80370106` 的问题。对 26H2 分支，对应 build 为 `26300.9818`；核对修复说明时，本机为已安装 KB5127648 的 `26300.9711`。官方说明未描述本机约 68 秒后重置的细节，因此需结合下述实际更新和复测结果判断，不能仅凭错误码确认内部根因。

该补丁属于正式发布渠道的**可选非安全 Preview 累积更新**。安装前，本机 Windows Update 主页显示 KB5127936 的单独“下载并安装”入口；“可选更新”子页面没有显示它。21:04 使用明确指定 `DeploymentAction='OptionalInstallation'` 的在线 COM 查询成功返回该更新，UpdateID 为 `c3777685-2dc4-4554-93ba-bd9964d07bca`，修订号 1，差分内容可用且优先使用，当次查询的 `IsDownloaded=false`。

此前只返回 .NET 的查询隐含 `DeploymentAction='Installation'`，漏掉了该更新的 `OptionalInstallation` 类型，不能据此判断本机未获得推送。该项 `BrowseOnly=false`，再添加 `BrowseOnly=1` 也会排除它。查询默认条件的依据见 [WUA Search 文档](https://learn.microsoft.com/en-us/windows/win32/api/wuapi/nf-wuapi-iupdatesearcher-search)。

[Microsoft Update Catalog](https://www.catalog.update.microsoft.com/Search.aspx?q=KB5127936) 的 x64 目标离线包为 4,898,703,157 字节，KB5043080 检查点包为 533,761,740 字节，两包共 5.43 GB（5.06 GiB）。这只是两个完整离线文件的合计，不能作为本机 Windows Update 的实际下载量。DISM 确认本机 KB5043080 已暂存，尚不能据此断言必须重新下载其完整文件。

用户随后通过 Windows Update 完成该更新，由系统按设备状态处理差分和检查点；微软说明它[只下载设备缺少的内容](https://learn.microsoft.com/en-us/windows/deployment/update/catalog-checkpoint-cumulative-updates)。安装前的元数据不足以确定实际下载字节，`MaxDownloadSize` 也[不等于实际下载量](https://learn.microsoft.com/en-us/windows/win32/api/wuapi/nf-wuapi-iupdate-get_maxdownloadsize)。安装后只读解码已有 Windows Update 日志，目标更新任务最终记录 `totalBytes=985569837`，并报告 `DownloadSucceeded`，即约 **940 MiB（0.99 GB）**；主 MSU 按内容范围下载，最后一轮记录为 1078 个 subranges。该数字是本次更新任务的内容字节计数，不是网卡全部流量；没有混入 .NET 更新、DO 缓存累计计数或两个完整离线包的大小。原始日志、摘要和摘录保存在 `local/kb5127936-installed-evidence-20261108/`。

官方 KB 还列出部分 USB Audio Class 1.0 设备可能出现 Code 10 或无声音的已知问题，源于 2026-09-08 更新，尚未解决；不能据此推断本机设备受影响，也不能声称该 Preview 没有已知问题。

## 更新后的宿主与最简沙盒复测

用户完成更新及重启后，21:34 的检查确认宿主为 Windows 11 Pro 26H2 `26300.9818`，已安装 KB5127936 和 SSU KB5132402；最近启动时间为 21:23:56，CBS RebootPending、PackagesPending、Windows Update RebootRequired 和 `pending.xml` 均不存在。WUA 安装历史为成功、HRESULT 为 0，更新日志的重启后状态也为成功，且无需后续提交或重启。已有的 25H2 ISO 未用于本次宿主修复或更新。

21:34:30 启动与此前相同的最简 Sandbox 配置，21:42:47 已观察到客体桌面和任务栏，运行超过此前约 68 秒的失败点。此前的最简沙盒初始化故障在这次更新及重启后未再复现。宿主状态、最简复测和兼容报告保存在 `local/post-update-sandbox-20261108-213429/`。

更新后的 LiteTray 宿主磁盘报告为 `disk_only`，现有 `windows11-26300.9711-x64` 资料与新的 `taskbar.dll`、`twinui.pcshell.dll`、`SystemTray.dll` 身份不匹配；四项功能均为 `unsupported_modules`。报告没有接入 Explorer，也没有检查运行中的模块和拓扑。沙盒恢复不能当作 LiteTray 已适配 `26300.9818` 的证据；未知模块继续拒绝接管，不能通过替换哈希或偏移绕过校验。

## 项目客体检查与退出码采集修正

21:50 在禁网、vGPU 关闭的隔离模式下完成首轮项目客体检查。客体实采为 Windows 11 Enterprise 24H2 `26100.9818`、Windows PowerShell `5.1.26100.9818`，宿主为 26H2 `26300.9818`。客体三项关键模块 `taskbar.dll`、`twinui.pcshell.dll`、`SystemTray.dll` 的 SHA-256 均与更新后的宿主相同，原始兼容报告继续将它们标记为 `unknown_image`。Sandbox 由宿主提供；客体显示为 24H2 不构成独立安装的 24H2 镜像覆盖，也不能据此扩大跨版本适配结论。

首轮 `xaml-clock.stdout.json` 包含 164 项检查，整体 `passed=true`，`explorer_accessed=false`。但客体脚本在 Windows PowerShell 5.1 下未取得四个子进程的退出码，`summary.json` 中各项 `exit_code` 均为 `null`，汇总结果为 `checks_failed`。保留这一真实首轮结果，不能将 XAML 自检通过直接改写为整轮检查通过。首轮证据保存在上述目录的 `isolated-vgpu-disabled/results/`。

客体工具随后在 `Start-Process` 后缓存进程 `Handle`，并增加退出码为 `null` 时的防护。21:55:12 至 21:55:31 使用同一程序包重新执行隔离检查，仍禁网且 `vGPU=Disable`。四个子进程均取得真实退出码 0，无超时，汇总为 `checks_passed`：

| 检查 | 退出码 | 超时 | 检查结果 | 关键证据与边界 |
| --- | --- | --- | --- | --- |
| `--compatibility-report` | 0 | 否 | 通过 | 磁盘检查完成；四项功能仍为 `unsupported_modules` |
| `--diagnose` | 0 | 否 | 通过 | 诊断报告成功生成；未执行 Explorer 接管 |
| `--self-test-app-icon` | 0 | 否 | 通过 | 包内图标自检通过 |
| `--self-test-clock` | 0 | 否 | 通过 | 164 项 XAML 检查，`passed=true`、`explorer_accessed=false` |

客体版本仍为 Windows 11 Enterprise 24H2 `26100.9818`。客体核验了包清单中的 64 个文件，清单 SHA-256 为 `bb24583a1a8eb28cce12fb58208533cffb2d6a8659a158d554f2c3c0b6cfcaf1`。复测原始报告保存在 `local/post-update-sandbox-20261108-213429/isolated-vgpu-disabled-fixed/results/sandbox-20261108-215424-c4200612-guest-6433665f/`；首轮 `checks_failed` 报告继续保留，没有覆盖。

此次 `checks_passed` 表示隔离检查按预期完成，不能将四项功能的 `unsupported_modules` 记为当前系统适配通过。自动 Explorer 接管未执行，时钟接管、托盘交互、退出恢复和多屏验证仍为 `not_run`；vGPU 关闭下的结果也不代表物理显卡或多屏验收。

跨版本测试基础已准备好，见 [Sandbox / VM 使用说明](sandbox-compatibility.md)。真正的第二个 Windows 分支仍计划使用已有 25H2 Pro 26200.8457 镜像建立独立 VM；尚未创建或验收。

## 后续 5.13.2 候选版客体接管验证

22:41:41–22:42:06，加入精确 `windows11-26300.9818-x64` 档案后的 5.13.2 候选版，在禁网、vGPU 关闭的全新 Sandbox 中完成四项隔离检查、6 项私有 Shell 自检和真实主屏时钟接管检查；六个子进程实际退出码均为 0，无超时。真实时钟文字及 `240 → 320` DIP 宽度更新、Stop 后原生文字/布局/可见性恢复和托盘五区准备通过，Explorer 原进程 PID `4796` 保持不变。日历、滚轮、悬浮、托盘菜单、模型复制及多屏仍未验收。

结果及后台自动流程见 [Sandbox / VM 使用说明](sandbox-compatibility.md)，模块与 ABI 依据见 [9818 兼容证据](compatibility-9818.md)。这是新候选版的后续验证，不改变上文 5.13.1 的退出码采集失败记录、模块不支持结论或宿主修复阶段的测量。
