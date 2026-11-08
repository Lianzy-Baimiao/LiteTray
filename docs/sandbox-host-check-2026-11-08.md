# 本机 Windows Sandbox 启动检查

2026-11-08，宿主 Windows 11 Pro 26H2 26300.9711。CIM 确认 Windows Sandbox 可选功能已启用，固件虚拟化与 hypervisor 正在运行；完整 Hyper-V 管理角色未启用。

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

`vmcompute`、`hns`、`HvHost` 均正常运行。Sandbox 功能已启用且三次来宾 OS 都已启动，当前证据不支持通过启用完整 Hyper-V 管理角色来修复这个错误。没有修改 Windows 功能、安全设置、服务配置或执行重启。

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

紧接的 `DISM /Online /Cleanup-Image /CheckHealth /NoRestart /English` 退出码 0，结果变为 `No component store corruption detected`。DISM 正式结束日志报告无需重启；CBS RebootPending、PackagesPending、Windows Update RebootRequired 和 `pending.xml` 均不存在。未执行重启。原始日志和检查结果保存在被 Git 忽略的 `local/windows-component-repair-20261108-192112/`。

19:44:55 重复打开相同最简 Sandbox 配置，仍出现原生 `0x80370106` 错误，未进入客体桌面。管理员日志确认 19:44:57.294 来宾 OS 成功启动，19:46:05.160 因来宾重置关闭，间隔 67.866 秒。没有运行完整 LiteTray 客体检查，没有提交系统反馈，错误窗口已关闭。

因此，本轮系统组件修复成功，但没有消除沙盒初始化故障，不能把“组件健康”当作“沙盒恢复”的证据。系统仍为 `26300.9711`。修复后的 LiteTray 磁盘兼容报告继续全部匹配；Explorer 和 LiteTray 均未重启。

## 已核实的微软修复与处理边界

微软 [KB5127936（2026-10-28）](https://support.microsoft.com/help/5127936) 明确写明修复 Windows Sandbox 意外关闭并显示 `0x80370106` 的问题。对当前 26H2 分支，对应 build 为 `26300.9818`；本机仍为 `26300.9711`，已安装 KB5127648。官方说明未描述本机约 68 秒后重置的细节，仍须安装后复测才能确认是否是同一缺陷。

该补丁属于正式发布渠道的**可选非安全 Preview 累积更新**。19:09 通过 Windows Update COM 接口在线查询未安装、未隐藏的软件更新，查询成功，但只返回 .NET 更新，没有返回 KB5127936；不能把目录存在等同于本机 Windows Update 已提供它。[Microsoft Update Catalog](https://www.catalog.update.microsoft.com/Search.aspx?q=KB5127936) 已核实存在 Windows 11 26H2 x64 条目。当前没有下载或安装系统补丁。

官方 KB 还列出部分 USB Audio Class 1.0 设备可能出现 Code 10 或无声音的已知问题，源于 2026-09-08 更新，尚未解决；不能据此推断本机设备受影响，也不能声称该 Preview 没有已知问题。

组件修复后的最简复测仍失败，下一步候选为安装此可选修复，或采用后续已确认包含修复的安全累积更新。已有的 25H2 ISO 未用于 26H2 宿主修复。累计更新可能要求重启，需安排在桌面工作保存之后。

最简 Sandbox 成功后再运行项目客体检查。系统修复或更新可能改变 Shell 模块身份，LiteTray 也必须重新生成兼容报告；未知模块继续拒绝接管，不能绕过校验。组件修复已完成；系统补丁安装和重启尚未执行。

跨版本测试基础已准备好，见 [Sandbox / VM 使用说明](sandbox-compatibility.md)。真正的第二个 Windows 分支仍计划使用已有 25H2 Pro 26200.8457 镜像建立独立 VM；尚未创建或验收。
