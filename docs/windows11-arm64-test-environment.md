# ARM64 与 26H1 测试环境

最初核查于 2026-11-10，接续 [Windows 11 覆盖研究](windows11-coverage-next-research.md)。最初这轮查询没有下载安装镜像、安装编译器、创建云资源或修改宿主显示设置。下文保留当时条件；当日晚间已取得并安装 26H1 x64 基础系统，见末节更新。ARM64 条件没有因此补齐。

## 已有条件

本机为 x64，已有 `qemu-system-aarch64.exe`、`edk2-aarch64-code.fd` 和 `edk2-arm-vars.fd`，位于 `local/compatibility-5140/tools/qemu/`。因此无需重新下载模拟器即可准备 ARM UEFI 启动实验，但尚未完成这个启动实验，也没有 ARM64 Windows 完整系统映像。

QEMU 的 ARM `virt` 平台支持 64 位 CPU 与 TCG 系统模拟，需要显式选择 64 位 CPU 和显示设备；这不是 Windows 客体或 Snapdragon 硬件兼容认证。[QEMU virt 文档](https://www.qemu.org/docs/master/system/arm/virt.html)。x64 主机上的 Hyper-V 不支持 ARM64 客体，不能将 QEMU 模拟路径写成 Hyper-V 的跨架构能力。[微软 ARM ISO 与虚拟机说明](https://learn.microsoft.com/en-us/windows/arm/iso)。

当前 Rust 仅配置 `x86_64-pc-windows-gnu`，标准 VS2022 / Windows Kits 路径未找到 ARM64 C++ 编译器及链接库；这个有限路径检查不证明其他目录一定没有工具。已有媒体库存和当前可调用的连接中，也没有已核实可用的 ARM64 Windows 桌面。

## 下一步的实际依赖

| 路径 | 可直接复用 | 仍需取得和验证 |
| --- | --- | --- |
| 本地 QEMU | 已有 ARM 模拟器与 UEFI | 完整 ARM64 ISO/VHD、系统所需驱动、真实桌面启动及交互；先用独立固件变量副本验证 UEFI |
| 已有 ARM64 电脑 | 原生 Explorer 和设备驱动 | 可访问的测试桌面、实际版本和架构；26H1 优先使用实际预装设备 |
| Azure ARM64 VM | 微软有 Windows 11 Pro Arm64 部署流程 | 已授权账户、区域/配额、实际镜像版本和费用授权；本轮未创建资源 |

Azure 官方提供 Windows on Arm 开发环境的部署路线，但这不证明当前账户有可用资源，也不证明有 26H1 镜像。[Azure Windows on Arm 指南](https://learn.microsoft.com/en-us/windows/arm/create-arm-vm)。

LiteTray 还需要相应的原生适配 DLL、ARM64 指令与调用约定检查。普通 ARM64 进程不能加载普通 x64 DLL，x64 EXE 在 ARM 系统上启动不代表任务栏托管已经兼容；还须检查实际 Explorer 的 PE / ARM64X 架构。[ARM64EC 互操作规则](https://learn.microsoft.com/en-us/windows/arm/arm64ec)。因此当前产品继续明确标记 ARM64 未实现。

## 最初核查时的 26H1 x64 阻碍

已经缓存并核验的 `28000.3365` x64 模块与符号可以继续做静态分析，但没有找到并核实可直接部署的完整 x64 基础系统。微软将 26H1 定位为特定新设备预装版本，不能从已有 24H2 / 25H2 就地更新；已有 x64 更新包的存在也不能代替基础介质和设备适用性核验。[26H1 官方说明](https://techcommunity.microsoft.com/blog/windows-itpro-blog/what-to-know-about-windows-11-version-26h1/4491941)。

本日上午已保存的公开下载页显示 26H2；本轮重试公开/Insider ISO 页面得到 HTTP 403，未取得新版本选项或有效的 26H1 基础 ISO 地址。新增 Catalog 查询结果中的 “Upgrade & Servicing Drivers” 是驱动类别，不能当作完整功能更新。历史 Canary 公告也不是当前介质下载凭据。

实际推进条件是：从有访问权限的 Insider、OEM 或订阅目录，核实完整 28000 基础映像的架构、文件名、大小、官方哈希及使用条件，或取得实际预装设备。没有这些条件之前，26H1 保持实验配置。现有约 4 GiB 的累积更新包不是可启动系统，本轮没有再次下载它们。

## 取证范围

本地报告为 `local/compatibility-expansion-5.16/media-arm64-research.md`；环境清单和每次请求的 URL、状态、时间、哈希保存在同目录 `media-receipts/`。成功页面正文共 935,890 字节，约 0.893 MiB；这不包含失败响应和网络协议开销。本次未扩大任何产品运行支持声明。

## 2026-11-10 晚间的 x64 进展

后续依据微软签名的 UUP 元数据取得 `28000.1` amd64 Professional 基础文件，逐文件核验后转换为隔离测试介质，已在独立 WHPX 虚拟机安装。实际加载模块与基础版静态资料一致；必需 HEVC 组件使用随附离线许可证安装并核对注册状态。首次登录脚本阻塞桌面的问题已在测试工具侧处理，未通过重启 Explorer 或更改系统时间取得通过结果。完整来源与失败记录见 [26H1 x64 验证](compatibility-26h1.md)。

冻结实验包已完成单屏时钟更新/恢复、托盘准备/停止、四次主屏日历点击，以及正常退出后 75 秒原生走时检查；5.16.2 默认候选纳入此基础实现，最终默认包仍待独立复测。28000.3365 仍仅有实验配置，不能继承基础版的运行记录。

上述环境是 x64 客体，不是 ARM64 或 Snapdragon 设备验证。当前没有新增 ARM64 原生 DLL、对应调用约定验证或可用 ARM64 Windows 桌面，也没有创建付费云资源；这些仍是独立工作。
