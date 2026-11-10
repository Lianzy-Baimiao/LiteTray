# Windows 11 后续覆盖研究

核对日期：2026-11-10；官方版本来源采集于 10:10–10:19（UTC+08:00），后续模块取证时间见单独收据。这是 5.15.2 发布后的覆盖盘点，不新增产品支持声明。本研究读取缓存、微软公开页面和文件清单，并按后续协作范围取得五个小型 x64 模块；没有下载系统 ISO/MSU、启动虚拟机或修改宿主环境。以下“最新”指取证时微软页面已列出的条目，不预判美国时间 11 月 10 日稍后可能发布的更新。

## 官方版本、架构与维护范围

微软当前将 **26H2 / 26300** 和 **26H1 / 28000** 都列为 General Availability Channel。26H2 于 2026-09-29 发布；26H1 于 2026-02-10 发布，但仅面向部分新硬件预装。不能根据构建号大小推断更新顺序，也不能因为 28000 曾在 Canary 出现就把当前整个分支都标为预览版。[Windows 11 release information](https://learn.microsoft.com/en-us/windows/release-health/windows11-release-information)、[26H1 发布状态](https://learn.microsoft.com/en-us/windows/release-health/status-windows-11-26h1)、[最初的 28000 Canary 公告](https://blogs.windows.com/windows-insider/2025/11/07/announcing-windows-11-insider-preview-build-28000-canary-channel/)。

| 版本 / OS 分支 | 本次核实的架构证据 | Home / Pro 更新结束日 | Enterprise / Education 更新结束日 | 取证时发布表最新构建 |
| --- | --- | --- | --- | --- |
| 21H2 / 22000 | x64、ARM64 累计更新 | 2023-10-10 | 2024-10-08 | 22000.3260，2024-10-08，KB5044280 |
| 22H2 / 22621 | x64、ARM64 累计更新 | 2024-10-08 | 2025-10-14 | 22621.6060，2025-10-14，KB5066793 |
| 23H2 / 22631 | x64、ARM64 累计更新 | 2025-11-11 | **2026-11-10（今日）** | 22631.7654，2026-10-26，KB5134130，OOB |
| 24H2 / 26100 | x64、ARM64 累计更新 | 2026-10-13 | 2027-10-12 | 26100.9725，2026-10-26，KB5134133，OOB |
| 25H2 / 26200 | x64、ARM64 累计更新 | 2027-10-12 | 2028-10-10 | 26200.9818，2026-10-28，KB5127936，非安全预览 |
| 26H1 / 28000 | ARM64 新硬件预装；另确有 x64 更新包，见下文渠道区别 | 2028-03-14 | 2029-03-13 | 28000.3365，2026-10-28，KB5127653，非安全预览 |
| 26H2 / 26300 | x64、ARM64 公开 ISO 与累计更新 | 2028-10-10 | 2029-10-09 | 26300.9818，2026-10-28，KB5127936，非安全预览 |

版本、最新构建和维护日期来自[发布信息表](https://learn.microsoft.com/en-us/windows/release-health/windows11-release-information)、[Home / Pro 生命周期](https://learn.microsoft.com/en-us/lifecycle/products/windows-11-home-and-pro)及 [Enterprise / Education 生命周期](https://learn.microsoft.com/en-us/lifecycle/products/windows-11-enterprise-and-education)。生命周期页 HTML 使用带时区的日期控件，表中采用微软面向用户的维护结束日，不直接照抄 UTC 时间戳的次日日期。架构逐项以微软目录中的实际条目交叉确认：[21H2](https://www.catalog.update.microsoft.com/Search.aspx?q=KB5044280)、[22H2](https://www.catalog.update.microsoft.com/Search.aspx?q=KB5066793)、[23H2](https://www.catalog.update.microsoft.com/Search.aspx?q=KB5134130)、[24H2](https://www.catalog.update.microsoft.com/Search.aspx?q=KB5134133)、[25H2 / 26H2](https://www.catalog.update.microsoft.com/Search.aspx?q=KB5127936)。存在更新包不能代替每个 SKU 的授权、安装介质或设备适用性核对。

LTSC 另算维护范围：Windows 11 Enterprise LTSC 2024 在 2029-10-09 结束更新；只有 **IoT Enterprise LTSC 2024** 延长至 2034-10-10，不能把十年期限写给普通 Enterprise LTSC。26H1 不支持 IoT Enterprise；因此不存在需要用普通 26H1 结果代替验证的“26H1 IoT Enterprise”条目。[发布信息表及 LTSC 注释](https://learn.microsoft.com/en-us/windows/release-health/windows11-release-information)。

26H2 使用适用于合格 24H2 / 25H2 设备的启用包，并与这两个版本共享维护分支。26H1 则使用不同 Windows 核心，不提供从已有 24H2 / 25H2 的就地更新，也不能更新至 2026 年下半年的年度版本；微软承诺未来版本会提供后续路径。共享维护分支只是解释模块可能相同的背景，不能代替逐文件身份、ABI 和运行时核验。[26H2 IT 专业人员指南](https://techcommunity.microsoft.com/blog/windows-itpro-blog/an-it-pro%E2%80%99s-guide-to-windows-11-version-26h2/4559760)、[26H1 官方说明](https://techcommunity.microsoft.com/blog/windows-itpro-blog/what-to-know-about-windows-11-version-26h1/4491941)。

## 已有介质及 21H2 的直接推进路径

本地已存在 `D:\IDMDown\Win11_Chinese(Simplified)_x64v1.iso`，大小 **5,587,222,528 字节**。本轮操作员只读检查 WIM XML，找到 Pro x64 zh-CN，索引 4，构建 **22000.318**；完整顺序哈希记录如下。这是本地身份收据，**尚未与微软发布的整张 ISO 哈希匹配**。

```text
SHA256 f476e2c4fa66aef5b1aca9be40c03315dd3cfff5c8eca167cf711c5b4b823b96
SHA1   b7472802105507ad5706634f104976a109d6fae7
```

收据与限制保存在 `local/compatibility-completion-next/cached-21h2-iso-identity.json` 和 `cached-21h2-metadata/image-metadata.json`。初次检查器在保存 XML 后有一次解释器退出警告；操作员随后用 wimlib 独立取得相同 XML 字节哈希并提取关键模块，记录为 `cached-21h2-module-extraction.json` 和 `cached-21h2-module-pe.json`。模块根目录为 `D:\LiteTrayLab\compatibility-next-21h2\modules`。该交叉检查解决了 XML 读取结果是否一致的问题，仍不是已安装系统或整张介质来源认证。缓存目录中的 `Win10_21H2_Chinese(Simplified)_x64.iso` 是 Windows 10，不能用于填补 Windows 11 21H2。

微软确认 22000.318 对应 **2021-11-09 KB5007215**。它的[官方文件清单](https://download.microsoft.com/download/9/e/8/9e83553b-c50c-4b0f-a18a-19b0bb67b8a4/5007215.csv)提供 x64 `twinui.pcshell.dll` 22000.282、`Taskbar.dll` 22000.132、`ExplorerExtensions.dll` 421.22500.115.0 等版本和文件大小，可供初步交叉检查；该旧 CSV **没有文件 SHA 列**，不能据此完成密码学来源认证。应继续对实际关键 PE 和匹配符号做微软来源校验，保留整张 ISO 来源未认证的事实，不必再次下载旧介质。[KB5007215](https://support.microsoft.com/help/5007215)。

已有模块树还包含 22H2、23H2、24H2、25H2 的若干旧维护版本，以及 LTSC 2024 / 普通 Pro 24H2 的已比较模块。研究开始时的库存未发现 ARM64 或 28000 系统模块；该库存结论只覆盖记录中的搜索根目录，不代表用户所有磁盘均不存在。清单为 `media-inventory-workspace.json`、`media-inventory-download-caches.json` 和 `module-cache-inventory.json`，均位于上述本地证据目录。

## 26H1 能取得什么，仍缺什么

微软明确说明 26H1 是特定新硬件预装版本，已公开点名 Qualcomm Snapdragon X2 系列设备。本次访问 [x64 下载页](https://www.microsoft.com/en-gb/software-download/windows11)与 [ARM64 下载页](https://www.microsoft.com/en-gb/software-download/windows11arm64)，两者均显示当前公开 ISO 为 **Windows 11 2026 Update / 26H2**，不能把它们当作 26H1 安装介质。[26H1 官方说明](https://techcommunity.microsoft.com/blog/windows-itpro-blog/what-to-know-about-windows-11-version-26h1/4491941)。

本次没有取得公开通用 26H1 基础 ISO 的有效链接、大小和官方整文件哈希；这是“未找到并核实”，不是断言所有 OEM 或订阅渠道都不存在。公开 [Insider ISO 页面](https://www.microsoft.com/en-us/software-download/windowsinsiderpreviewiso)要求 Insider 成员登录，未登录响应不提供可核验的版本选项。因此没有臆造 28000 ISO 文件名或把历史 Canary 公告当作当前可下载介质。

但是 **26H1 x64 二进制确实存在**。微软目录提供以下包，DownloadDialog 返回官方 CDN URL、SHA1 与 SHA256。下列大小为目录原始字节字段，哈希为目录给出的值；本轮未下载 MSU，未重新计算包哈希，也没有测试安装。

| 包 / 架构 | 目录 ID | 字节数 | 目录 SHA256 |
| --- | --- | ---: | --- |
| KB5127653 / 28000.3365 ARM64 预览 | `c6af3bb9-0f80-41ad-a03d-5bda9d49a5d5` | 4,199,375,904 | `b480020d50fc6a31a0cb390c6fc1b4051d0ff8d0f7e818786d9de43ee6df88ef` |
| KB5127653 / 28000.3365 x64 预览 | `ad7c42de-bc71-4e1c-a5d5-2c87dfa219f9` | 4,353,046,426 | `85c1acd0ebb35a60f090333fc1ccbf1e4b6c6271839d7f203ce8baff446ff3b6` |
| KB5127646 / 28000.3231 ARM64 安全更新 | `fa3c0ca5-5a7a-475f-8baf-b5d7c311d87a` | 3,601,319,990 | `40432891a1d3c08c5713006b523c41a42cfb0837942117bea0f532e957b5ab9c` |
| KB5127646 / 28000.3231 x64 安全更新 | `44e18c3a-ec6a-480d-897a-e3b26b5e47c9` | 4,139,524,203 | `2094adc4e6ffccabf3a38998bd2849e3a2851c249ab5e8d58191123e23d098c6` |

取证时两个 KB5127653 条目都标为产品 **Windows 11**；KB5127646 的 ARM64 条目标为 Windows 11，而其 x64 条目标为 **Windows Insider Pre-Release**。应保留这一实际目录差异，不统一改写为同一个渠道。对应官方目录：[26H1 查询](https://www.catalog.update.microsoft.com/Search.aspx?q=Windows%2011%2026H1)、[ARM64 预览详情](https://www.catalog.update.microsoft.com/ScopedViewInline.aspx?updateid=c6af3bb9-0f80-41ad-a03d-5bda9d49a5d5)、[x64 预览详情](https://www.catalog.update.microsoft.com/ScopedViewInline.aspx?updateid=ad7c42de-bc71-4e1c-a5d5-2c87dfa219f9)。微软 KB 当前仅列 ARM64 安装说明；x64 包存在不足以证明通用 x64 零售系统获得 26H1 设备支持。[KB5127653](https://support.microsoft.com/help/5127653)、[KB5127646](https://support.microsoft.com/help/5127646)。

确切包名与地址：

- [windows11.0-kb5127653-arm64_9f0969ea9e0fd06494842816cd1d5337a3868e42.msu](https://catalog.sf.dl.delivery.mp.microsoft.com/filestreamingservice/files/276b79f5-4371-4aec-b96f-10077e99bc3d/public/windows11.0-kb5127653-arm64_9f0969ea9e0fd06494842816cd1d5337a3868e42.msu)
- [windows11.0-kb5127653-x64_319cd0de593780c2d6b663ce0c8eab06beb1a22b.msu](https://catalog.sf.dl.delivery.mp.microsoft.com/filestreamingservice/files/e9115a85-b733-45c8-9e9c-b56983f05ff4/public/windows11.0-kb5127653-x64_319cd0de593780c2d6b663ce0c8eab06beb1a22b.msu)
- [windows11.0-kb5127646-arm64_9495674eba343f8241770ebcb479bc4325de9cb7.msu](https://catalog.sf.dl.delivery.mp.microsoft.com/filestreamingservice/files/dab0b147-7e03-4102-aa20-9d1a64e8a3df/public/windows11.0-kb5127646-arm64_9495674eba343f8241770ebcb479bc4325de9cb7.msu)
- [windows11.0-kb5127646-x64_1581b4e504578c7885f2ce34a86c37f785bef551.msu](https://catalog.sf.dl.delivery.mp.microsoft.com/filestreamingservice/files/23a416b1-3cfa-451b-8ca3-186075b34a87/public/windows11.0-kb5127646-x64_1581b4e504578c7885f2ce34a86c37f785bef551.msu)

这些是更新包，不能直接当作可启动 ISO。是否能从包中独立重建全部目标文件，仍要检查实际包结构与基线要求；本次没有做提取试验。KB5127653 的 ARM64 安装表说明无需前置 checkpoint，但这也不等于无需基础操作系统映像。[KB5127653 安装说明](https://support.microsoft.com/help/5127653)。

### 无需下载 MSU 就能取得的模块身份

KB5127653 附带[微软 CSV 文件清单](https://download.microsoft.com/download/108995d1-e4cc-455a-9c50-c7b06db33e0d/5127653.csv)，已完整保存：**31,429,741 字节**，本地 SHA256 `58986b3f7f9d2fcd939b408cce4533497f12f1ba5c111621adf23398f5494d72`。它分别列出 ARM64 / x64 包内文件的大小、版本、SHA1 和 SHA256，是后续取得模块时可直接使用的独立校验依据。

| 清单所属架构 | 文件 | 版本 | 字节数 | 微软清单 SHA256 |
| --- | --- | --- | ---: | --- |
| ARM64 | `Core\Taskbar.View.dll` | 2609.10000.200.0 | 13,860,864 | `7aed658864c1822d52abde52fabfaac534cafd537e77b26a8b67224fe7f9464d` |
| ARM64 | `Core\SystemTray.dll` | 2609.4001.100.0 | 2,642,432 | `699353faf43ac9ffc20d35aaf043995c3afe8c3f9ee11304469431f1401655c7` |
| ARM64 | `twinui.pcshell.dll` | 10.0.28000.3365 | 21,696,512 | `e98472618d9c5d8e1e372bea86abf28b08cf40a9faf006433708307234d2639b` |
| x64 | `Core\Taskbar.View.dll` | 2609.10000.200.0 | 9,935,360 | `1a9eff11c64f515cf938939bad4d52ed98a3aba7ca05866b8d5807d47da93430` |
| x64 | `Core\SystemTray.dll` | 2609.4001.100.0 | 2,080,256 | `aa258b28f6c7afa5d74b2baf46f2d9d6a13777bdfa608d51043a4e696724fcb2` |
| x64 | `twinui.pcshell.dll` | 10.0.28000.3365 | 10,117,120 | `7cda1bce4543969ab2d626b28ead3e6ce66db73d2582e1505ae780887aa0b7bc` |

完整的 12 条关键记录还包含 `Taskbar.dll` 和 `explorer.exe`，保存于 `research-sources/26h1-preview-critical-file-hashes.json`。CSV 同一架构段有多个同名 Explorer 文件，且路径不足以区分全部副本；仍须核对实际映像路径、PE machine / ARM64X 信息、签名与 CodeView 身份。文件存在和哈希正确只证明样本身份，不证明 LiteTray 已解析结构或运行成功。

### 已取得并核验的 26H1 x64 实际样本

后续仅从微软符号服务器取得五个文件：[Taskbar.dll](https://msdl.microsoft.com/download/symbols/Taskbar.dll/56801589311000/Taskbar.dll)、[twinui.pcshell.dll](https://msdl.microsoft.com/download/symbols/twinui.pcshell.dll/BC6D4ADB9ad000/twinui.pcshell.dll)、[Taskbar.View.dll](https://msdl.microsoft.com/download/symbols/Taskbar.View.dll/6AB5C671981000/Taskbar.View.dll)、[SystemTray.dll](https://msdl.microsoft.com/download/symbols/SystemTray.dll/6AA9E54B202000/SystemTray.dll) 和 [explorer.exe](https://msdl.microsoft.com/download/symbols/explorer.exe/9C5BF8FC30e000/explorer.exe)。总下载 **28,642,424 字节（约 27.3 MiB）**，最大单个文件 10,117,120 字节。五个文件的完整 SHA256 和大小均与微软 KB5127653 CSV 对应 x64 记录一致，PE 均为 `0x8664` / PE32+，各有一个 CodeView RSDS 身份；下载后再次读取确认哈希未变。

文件保存在 `local/compatibility-completion-next/26h1-targeted-modules/verified-files/`，逐文件 URL、哈希、PE 时间戳、SizeOfImage、PDB GUID / age 及合并收据在同级 `acquisition-index.json`。实际载荷低于本轮允许的总计 256 MiB、单文件 128 MiB 上限。没有执行这些模块或运行静态 DbgHelp，也没有新增生产资料。

取得地址时，[Winbindex 索引](https://winbindex.m417z.com/)仅提供不受信任的 PE 定位线索，最终文件身份权威仍是微软自己的 CSV 和服务器字节。Taskbar.dll 的索引缺少 SizeOfImage，使用其最后节 RVA 和相邻记录的节长度形成一个候选；下载文件自身的 PE SizeOfImage 与候选完全一致，完整文件哈希也通过微软清单核验。twinui 的定位候选同样由自身 PE 和完整哈希确认。文件暂按名称平铺保存，**没有把第三方索引中的安装路径宣称为已核实的实际系统路径**。ARM64 实际模块不属于这次下载范围。

随后在单独组装的离线目录，用新取得且经过 GUID / age 校验的四份微软 PDB 分别检查 SystemTray.dll 和 Taskbar.View.dll。两个独立输出为 `26h1-static-systemtray-r1/`、`26h1-static-taskbarview-r1/`，摘要为 `26h1-static-comparison.json`，均位于 `local/compatibility-completion-next/`。宿主与 source 两次结果相同：保留六个现代宿主符号，元素偏移 `+0x10`、子宿主 `+0x38`、source `+0x10`，匹配已有现代解析规则；模块字节和各自 RVA 仍与现有七组资料不同。

SystemTray.dll 组合得到 `static_candidate`，其中通知区域三参数传值矩形入口及两个直接调用者通过已有规则。它的独立四参数借用拖放入口仍为 `unreviewed`：105 条指令不匹配已审查的完整模板；输出保留 `implementation_selection=requires_consumer_choice` 和顶层 `abi_candidate=null`。通知区域传值规则自身也标注 `complete_instruction_template=false`，只提供既有参数、结果、释放和用途证明，不能写成整函数指令等价。备用 Taskbar.View.dll 组合因缺少已审查的屏幕矩形签名被拒绝。四份 PDB 共 101,933,056 字节；检查器未访问 Explorer，也未验证真实安装路径、系统一致性或运行兼容性。

后续独立消费者审查确认：通知区域入口 `0xb86ec` 的隐藏 RECT 结果指针、传值 FrameworkElement 临时引用和 HWND 三参数约定，与现有 `SystemTrayCoordinatesX64V1` 运行时一致。实际调用者为 `OnScreenBoundsRequested` 和 `TryGetChevronScreenRect`；原生函数消费临时引用，现有 detour 在调用前保留自己的引用，所有权顺序相符。最小接入需要显式选择该已审查 trait，并绑定其真实模块身份及三个入口，不需要新增原生调用约定。拖放入口 `0xa8320` 的四个直接调用点虽仍体现借用约定，但完整函数体、TransformBounds 辅助函数、R13 零值比较和局部清理偏移均有独立变化，现有严格模板继续拒绝。详细本地证据为 `coord-26h1-consumer.md`、`26h1-consumer-review/consumer-evidence.json` 与 `coord-26h1-owned-consumer-review.md`；这项结论不包含实际安装系统或运行验收。

## ARM64 的验证条件

Windows 11 能在 ARM 设备上模拟运行 x86 / x64 应用，但 DLL 装载规则仍区分架构：ARM64 进程不能直接加载普通 x64 或 ARM64EC DLL；x64 / ARM64EC 进程也不能直接加载普通 ARM64 DLL，ARM64X 有其特殊兼容用途。因而“LiteTray 的 x64 EXE 能启动”不能证明适配 DLL 能进入原生 Explorer，也不能证明已有 x64 指令解析和 ABI 可用。[Windows on Arm](https://learn.microsoft.com/en-us/windows/arm/overview)、[ARM64EC 互操作表](https://learn.microsoft.com/en-us/windows/arm/arm64ec)。这是对本项目注入及模块解析机制的推论，必须用实际 Explorer 进程和模块架构验证。

微软支持在 Windows ARM 主机用 Hyper-V 创建 ARM64 客体；明确说明 **x64 主机的 Hyper-V 不支持 ARM64 客体**。微软给 x64 开发者的路径是 Azure 上的 ARM64 VM。[ARM ISO 概览](https://learn.microsoft.com/en-us/windows/arm/iso)、[Azure Windows on Arm VM 指南](https://learn.microsoft.com/en-us/windows/arm/create-arm-vm)。这不等于断言其他软件模拟器不可能运行 ARM64，但现有 x64 客体验收不能充当 ARM64 验收。

Azure 文档证明存在 ARM64 开发测试环境路线，没有证明账户当前可取得 **26H1** 映像，也不能代替 Snapdragon X2 设备、物理多屏及混合 DPI 的验证。应分别核对可用映像的 SKU、构建、架构、驱动及交互桌面。本轮没有登录账户、创建付费资源或宣称已拥有可用 ARM64 实验机。

## 与 LiteTray 现有覆盖的区别

以下是研究开始时的仓库事实，以 [compatibility.md](compatibility.md)、[模块识别规则](module-recognition.md)和[5.15.2 补充验收](compatibility-completion-5.15.1.md)为准：

| 版本范围 | 已有实现样本 / 证据 | 本轮仍缺的覆盖 |
| --- | --- | --- |
| 21H2 x64 | 找到 22000.318 缓存介质；XML 已交叉核对，关键 PE 已提取 | 模块的独立来源核验、实现解析和真实运行证据 |
| 22H2 / 23H2 x64 | 22621.1702 / 22631.2861 七组目录中的两组；有绑定旧测试包的有限实测 | 其他月更、当前发布包完整回归及完整交互矩阵 |
| 24H2 x64 | 26100.1742；普通 Pro 与已有 LTSC 三个模块逐字节相同；5.15.2 在普通 Pro 复测 | 其他模块实现、其他 SKU 和场景的独立证据 |
| 25H2 x64 | 26200.8037 / 8457；5.15.2 在 8037 复测 | 其他月更及未完成场景 |
| 26H2 x64 | 26300.9711 / 9818；前者历史、后者有明确限定范围的证据 | 当前包在独立系统中的完整功能验收 |
| 26H1 x64 | 五个微软模块身份通过；宿主/source 及 SystemTray 通知区域静态规则通过，拖放规则仍未审查 | 实际安装路径、基础系统、生产实现和运行验收 |
| 26H1 ARM64 | 本轮取得官方包与模块哈希元数据 | 实际样本、基础系统、实现和运行验收 |
| 其他 Windows 11 ARM64 分支 | 本轮确认官方介质 / 更新路径 | ARM64 适配构建、模块实现和运行验收 |

5.15.0 起按模块身份及严格实现等价规则协商功能，并非以系统版本区间放行。七组 x64 参考资料不覆盖全部累计更新、全部 SKU 或 ARM64；物理插拔、混合 DPI、完整托盘复制、菜单与输入交互仍不能写为全部验收。26H2 的系统名称得到官方核实，也不改变这些产品证据边界。

## 可直接执行的优先顺序

1. **先完成缓存 21H2 x64。** 复用已完成的 XML 交叉核对和提取，继续核验关键 PE 的微软来源，再建立 22000.318 的独立实现；无需重新下载 ISO。保留整张 ISO 官方哈希未匹配的限制，并在进入默认目录前完成独立客体的当前候选验收。
2. **按已审查用途接入 26H1 候选并取得真实系统。** 宿主/source 静态规则已通过，通知区域 ABI 的独立评审已完成；可以显式选择通知区域三参数规则制作受限候选，拖放证据仍不得启用。基础系统与真实运行路径另行取得。两个架构的更新包和官方模块哈希已经备齐，只有确实需要额外文件时再由介质操作员检查包结构、基线依赖和存储位置，避免为已有小模块重复下载数 GB 更新包。本轮尚未下载这些更新包。
3. **为 ARM64 建立真实运行环境和独立实现。** 优先确认已有 ARM 主机 / 合格设备，或当前可获得的 Azure ARM64 映像；核对该环境是否真正覆盖所需分支。模拟执行 x64 GUI、成功交叉编译或离线哈希匹配都不是完成条件。
4. **沿已知分支补月更和场景。** 按上表最新 / 最终维护构建与已有样本比较模块身份；只有实现确实相同时复用解析方案，否则补适配。分别记录单屏、双屏拓扑恢复、物理插拔、混合 DPI、时钟输入及完整托盘交互，避免把“一组模块可用”写成“全部 Windows 11 完成”。

## 可复核材料

原始响应、最终 URL、HTTP 状态、取证时间及 SHA256 保存在 `local/compatibility-completion-next/research-sources/`，索引为 `evidence-index.json`。关键机器可读材料为 `26h1-selected-artifacts.json`、`26h1-preview-critical-file-hashes.json`、`21h2-318-critical-file-metadata.json`；后续实际模块的独立收据为 `26h1-targeted-modules/acquisition-index.json`。这些目录是本地证据，不随本研究文档自动分发。

搜索工具本轮没有配置可用提供方，因此改为直接 HTTPS 读取微软页面。最初 `en-us` 下载页返回 403，随后微软公开 `en-gb` / 中性语言链接返回 200；没有关闭 TLS 验证。两个大型 CSV 初读采用 3 MB 上限并留下 `truncated=true` 记录；26H1 **预览**文件清单随后完整获取并单独记录，截断的 **安全更新**清单未用于逐文件结论。没有使用第三方下载站、镜像站或用户评论作为事实权威。

## 26H1 实验配置接入

已将 `28000.3365` x64 的已核验模块整理为 `compatibility-lab` 专用配置 `windows11-28000.3365-x64-notification`。重新读取三份微软模块及准确 PDB，复核模块身份、六个宿主入口、三个通知区域入口及 28 个 source 虚表槽；显式选择已审查的 `notification_area_owned` / `notification_area_bounds` 三参数传值约定。未审查的四参数拖放入口 `0xa8320` 没有启用。该配置暂只接受精确映像，不启用实现等价回退，也没有继承其他版本的运行验收记录。

冻结实验程序对保留的离线目录执行 `--compatibility-report --system-root`，三个模块匹配、配置选择正确。报告仍为 `scope=disk_only`，`loaded_modules_checked=false`、`runtime_topology_checked=false`。这完成了实验程序的配置接入，不证明真实安装路径、系统一致性或 Explorer 中的运行兼容性。默认构建仍排除此配置。没有新增网络下载。

可复核生成器、原始输入哈希和独立检查位于 `local/compatibility-completion-next/26h1-lab-proposal/`；程序输出为 `26h1-lab-disk-r1.json`。生产支持仍需真实 26H1 客体验证，ARM64 继续需要独立架构实现和对应运行环境。

## 5.16.0 推进结果

早期 21H2 `22000.318` x64 已完成独立通知区域 source 与单文本时钟适配，真实客体通过三次主屏时钟文字更新、宽度变化、停止恢复及原 Explorer 连续性验证，纳入默认目录。默认资料由七组增至八组。它目前只支持主屏时钟；副屏、托盘和完整输入仍未验证。包、实际报告与退出确认哈希见 [21H2 记录](compatibility-21h2.md)。本文前面调查表记录的是当时阶段，不应覆盖这项后续结果。

26H1 `28000.3365` x64 已接入实验配置并通过磁盘匹配，仍需要真实系统的加载与运行验证。ARM64 需要独立实现与运行环境。当前仍不能将八组参考模块或相同版本名称当作完整 Windows 11 覆盖。
