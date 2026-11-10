# Windows 11 21H2 适配记录

目标样本为 Windows 11 Pro x64 `22000.318`。目前已完成宿主/source 的离线适配和实验构建；是否能接管原生时钟，仍须由实际客体中的控件结构和运行结果确定。实验资料不在默认构建中，不能把 XAML 宿主识别成功当作时钟、托盘已经可用。

## 输入与识别依据

使用已有的简体中文 x64 安装镜像，没有重新下载镜像或累计更新包。镜像 SHA-256 为 `f476e2c4fa66aef5b1aca9be40c03315dd3cfff5c8eca167cf711c5b4b823b96`，Pro 为 WIM 索引 4。两种独立读取方式得到相同 WIM 元数据；整张镜像的官方哈希尚未认证。

`Taskbar.dll`、`twinui.pcshell.dll`、`ExplorerExtensions.dll` 和 `explorer.exe` 的字节已与微软符号服务器独立获取的映像逐一匹配。该检查证明所选模块的来源一致，不代表整个安装镜像均已认证。

| 模块 | 版本 | SHA-256 |
| --- | --- | --- |
| Taskbar.dll | 10.0.22000.132 | `21e122d5561ee827534ae6ff3cf2838336d66fa9e533f13e15f94076342f6b00` |
| twinui.pcshell.dll | 10.0.22000.282 | `dc24a218229828455be50ddd5af8a0c88399b74df81c9b6d563a2f78e3e66eb8` |
| CBS/ExplorerExtensions.dll | 421.22500.115.0 | `12079bced5111d8e24411651644127e68dc33f53940b5efd347842424c6bbc15` |

## 早期宿主的实际差异

早期实现使用五个已核对的宿主角色，缺少较新方案的 FrameHeight 角色和工厂虚表。通过保有引用的宿主读取 `host + 0x20 → child + 0x10 → source`，再通过真实 source 的 Content 接口取得根节点，不猜造元素偏移。

子宿主虚表位于映像的可写数据节区。专用 `xaml_explorer_host_child_xfg_v1` 方案核对完整 13 个入口、各入口字节、映像归属和内存权限，再进行首次接口查询；没有放宽其他方案的数据节区规则。离线取证同时核对 DIR64 重定位、返回值、共享引用保有/释放和 XFG 调用方式。

实验资料仅包含宿主和 shell 两个模块。托盘模块、ABI、用途和入口必须同时存在或同时缺省；不允许用占位地址凑成完整资料。磁盘报告中的时钟匹配状态仍要求后续真实结构检查，托盘明确不可用。

检查和接管现在使用相同的 source 解析路径。检查过程中保有宿主、子宿主和 source 引用，直到控件遍历结束。

## 验证范围

- 默认 Rust 工作区和显式实验构建各有 302 项通过、9 项显式忽略；Python 离线解析器 64 项通过。
- 从保留的真实映像验证 20 个模块角色；20 次仅元数据变化正确识别，148 次代码/数据节修改均拒绝。9711 原始映像缺失，仍仅精确身份识别。
- 独立审查核对 13 个虚表入口、对象所有权、缺省托盘规则和七份历史资料摘要，没有把静态结果写成客体运行通过。
- `--test-taskbar-compatibility <新报告.json> --clock-only` 显式执行主屏时钟三帧更新、宽度变化和停止恢复。报告范围为 `primary_clock_live`，托盘标记为未请求，不能代替完整检查。

所有实际接管测试仅在隔离客体执行。默认完整检查仍要求时钟和托盘，不会因托盘不支持而静默降级成通过。

本地原始材料位于 `local/compatibility-completion-next/`：`21h2-static-r3`、`21h2-microsoft-image-check`、`21h2-child-vtable-independent.json`、`21h2-lab-package-r1-receipt.json`、`fingerprint-verify-r1.log` 及各独立审查目录。实验包仍使用开发基线版本 5.15.2，不是新的公开 Release。
