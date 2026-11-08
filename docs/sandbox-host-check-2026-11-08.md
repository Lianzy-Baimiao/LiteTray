# 本机 Windows Sandbox 启动检查

2026-11-08，宿主 Windows 11 Pro 26H2 26300.9711。CIM 确认 Windows Sandbox 可选功能已启用，固件虚拟化与 hypervisor 正在运行；完整 Hyper-V 管理角色未启用。

| 尝试 | 配置 | 结果 |
| --- | --- | --- |
| 16:46 | 程序包只读输入、独立结果目录、禁网、vGPU 开启 | 客体初始化失败，`0x80370106` |
| 16:49 | 相同检查，vGPU 关闭 | 客体初始化失败，`0x80370106` |
| 16:52 | 最简配置，vGPU/网络/剪贴板关闭；无目录映射、无启动命令 | 客体初始化失败，`0x80370106` |

错误均由 Windows Sandbox 原生对话框报告：“虚拟机或容器意外退出”。三次均未进入 LiteTray 客体检查；不能把它记为 LiteTray 兼容失败或通过。没有提交系统反馈，错误窗口已关闭。

微软 [HCS 错误码](https://learn.microsoft.com/en-us/virtualization/api/hcs/reference/hcshresult) 将 `0x80370106` 定义为 `HCS_E_UNEXPECTED_EXIT`。它不同于表示缺少所需功能的 `0x80370102`，因此不能直接归因于完整 Hyper-V 未启用。最简配置同样失败，也将 LiteTray、文件映射与客体启动脚本排除出本次失败路径。

Compute、Worker、VID、Hypervisor 等事件通道存在并启用，但当前非管理员会话直接查询返回 `Access is denied`。某些 `Get-WinEvent` 调用的“无匹配事件”不构成日志为空的证据。容器 Dumps 目录没有可分析文件；Application/System 查询没有取得对应错误。根因仍待管理员日志。

下一步在管理员 PowerShell 中执行只读导出：

```powershell
./tools/Export-SandboxHostDiagnostics.ps1 `
  -OutputDirectory './local/sandbox-admin-events-20261108' `
  -StartTime '2026-11-08T16:43:00+08:00' `
  -EndTime '2026-11-08T17:05:00+08:00'
```

输出目录必须尚不存在。脚本仅读取限定时间的系统日志并保存 JSON，不启用功能、不修复系统、不重启服务或电脑。根据具体子 HRESULT 再决定后续动作，当前未修改 Windows 功能或安全设置。

跨版本测试基础已准备好，见 [Sandbox / VM 使用说明](sandbox-compatibility.md)。真正的第二个 Windows 分支仍计划使用已有 25H2 Pro 26200.8457 镜像建立独立 VM；尚未创建或验收。
