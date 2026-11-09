# Windows 11 下一批版本：官方介质与验证路线

查证时间：2026-11-09 02:27，Asia/Shanghai。本文记录取得的微软第一方文档、官方文件响应和可执行的后续路线；它不是 24H2、23H2 或 22H2 已通过 LiteTray 接管验收的声明。原始网页、响应头、小型产品清单和提取结果保存在 Git 忽略目录 `local/windows11-next-versions-research/`。研究查询没有执行外部脚本，也没有下载 GB 级介质；主任务另行完成了已确认来源的 LTSC 镜像下载，其结果在下文注明。

后续实施状态见 [8037 / LTSC 运行记录](compatibility-next-versions.md) 与 [22H2 / 23H2 适配记录](compatibility-22h2-23h2.md)。后者已下载并校验本文两份中文 ESD，核对实际 Pro 映像模块，并实现独立四参数托盘 ABI；本文以下 BASE 资料及首轮拒绝属于当时研究记录，不能据此排除实际安装映像。

推荐先使用微软仍公开提供的 **Windows 11 Enterprise LTSC 2024、26100.1742 x64**，新增一套独立的 24H2 代系统与模块证据。下一步可使用仍可获取的 **23H2 22631.2861** 官方 ESD，之后是 **22H2 22621.1702**。同时按精确 PE 身份从微软符号服务取少量 DLL/PDB，提前检查 source、字段和 ABI 差异。LTSC 的结果须明确写成 LTSC，不能泛化为所有普通 24H2 设备。

## 24H2 与 25H2 的共性，以及回退边界

微软 [KB5054156：通过启用包升级到 25H2](https://support.microsoft.com/en-us/servicing/os/windows/docs/2025/02/kb5054156-feature-update-to-windows-11-version-25h2-by-using-an-enablement-package) 明确说明：

> Windows 11, versions 24H2 and 25H2 share a common core operating system with an identical set of system files.

该 KB 说明，25H2 的新功能已存在于 24H2 最新月度质量更新中，启用包充当开启这些休眠功能的开关；应用后重启一次。前置条件是运行 24H2，并安装 **KB5064081、26100.5074 或后续累计更新**。本次读取的页面将 Windows Update 和 WSUS 列为分发渠道，Update Catalog 一栏写明通过其他发布渠道提供，未给出 Catalog 独立下载。

这解释了已测 25H2 客体 OS build 为 26200，而某些组件文件版本仍为 26100 的现象，也支持优先研究共同 source/ABI 实现。它没有证明不同累计更新、功能开关、版本或架构中的所有任务栏对象完全一致。模块 SHA-256、CodeView 身份、真实窗口/XamlRoot 及实际场景仍须分别验证。

**未查到该 KB 对“从 25H2 全新安装客体卸载启用包，得到受支持 24H2”的承诺。** 页面没有启用包卸载或逆向版本迁移步骤；它陈述的是 24H2 → 25H2 的升级路径。现有 8457 客体由 25H2 ISO 全新安装，不能假设保留了升级前的 24H2 系统。

微软 [Recovery options in Windows](https://support.microsoft.com/en-us/windows/experience/backup-recovery/recovery-options-in-windows) 将返回先前版本描述为升级后通常 10 天内可用的恢复选项，并说明有些更新不能卸载。[DISM package servicing](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/dism-operating-system-package-servicing-command-line-options?view=windows-11) 提供 `/Get-Packages`、`/Get-PackageInfo` 和 `/Remove-Package`，但通用删除命令不等于 KB5054156 的逆向版本迁移受到支持；文档还说明 `/StartComponentCleanup /ResetBase` 后已安装更新不能卸载。

因此，可在 25H2 客体的副本中只读采集实际包信息，作为研究线索；本次没有验证包是否存在、是否可卸载，也没有执行删除。**不要把改写 MUM 的 permanence、删除组件清单或强制改版本标签作为正式 24H2 证据。** 即使实验副本能改变显示版本，仍需记录其来源、完整模块及功能开关，并与独立安装的 24H2 区别对待。当前已有直接获取独立 LTSC 介质的路线，无须依赖这条未经证实的回退方式。

同类共性也存在于 22H2/23H2：微软 [KB5027397](https://support.microsoft.com/en-us/servicing/os/windows-11/2023/05/kb5027397-feature-update-to-windows-11-version-23h2-by-using-an-enablement-package) 使用相同的 “common core operating system” 和 “identical set of system files” 表述。该事实有助于组织解析方案，但不把某个 22621 模块自动标成所有 22631 设备兼容。

## 可直接取得的官方 24H2 代 LTSC 镜像

入口为微软 [Windows 11 Enterprise Evaluation 下载页](https://www.microsoft.com/en-us/evalcenter/download-windows-11-enterprise)，其中独立列出 “ISO – Enterprise LTSC download”。以下两条链接均从该页取得，本次仅用 HEAD 确认：

| 介质 | 官方入口 | 本次实际响应 |
| --- | --- | --- |
| 中文 x64 LTSC 2024，26100.1742 | [fwlink 2288085](https://go.microsoft.com/fwlink/?linkid=2288085&clcid=0x409&culture=en-us&country=us) | HTTP 200；5,274,607,616 字节，约 4.91 GiB；支持字节范围请求 |
| 英文 x64 LTSC 2024，26100.1742 | [fwlink 2289029](https://go.microsoft.com/fwlink/?linkid=2289029&clcid=0x409&culture=en-us&country=us) | HTTP 200；5,112,850,432 字节，约 4.76 GiB；支持字节范围请求 |

实际固定文件地址：

- [中文 ISO](https://software-static.download.prss.microsoft.com/dbazure/888969d5-f34g-4e03-ac9d-1f9786c66749/26100.1742.240906-0331.ge_release_svc_refresh_CLIENT_LTSC_EVAL_x64FRE_zh-cn.iso)
- [英文 ISO](https://software-static.download.prss.microsoft.com/dbazure/888969d5-f34g-4e03-ac9d-1f9786c66749/26100.1742.240906-0331.ge_release_svc_refresh_CLIENT_LTSC_EVAL_x64FRE_en-us.iso)

微软 [Windows 11 Enterprise LTSC 2024 新增功能](https://learn.microsoft.com/en-us/windows/whats-new/ltsc/whats-new-windows-11-2024) 说明其功能类似 Windows 11 24H2，首次提供于 2024-10-01，同时提醒 LTSC 面向特定用途设备，部分面向普通通道的应用与工具支持可能有限。因此它是有价值的独立 26100 代测试环境，不能替代 Home/Pro/普通 Enterprise 24H2 的全部功能形态。

官方 [Evaluation 说明](https://www.microsoft.com/en-us/evalcenter/evaluate-windows-11-enterprise) 提供 90 天评估，安装不需要产品密钥；未激活或评估到期时会显示提示并每小时关机。应按实际评估状态记录 VM 生命周期，不能将评估到期关机解释为适配器故障。

本次**没有取得适用于上述 LTSC 文件的独立官方 SHA-256 表**。下载页的 [真实性校验入口 2382606](https://go.microsoft.com/fwlink/?LinkId=2382606) 当前跳转到 [26H2 Evaluation ISO 哈希页](https://support.microsoft.com/en-us/servicing/os/windows/docs/2026/09/verify-the-authenticity-of-a-windows-11-enterprise-evaluation-iso-file)，不能套用其哈希验证 LTSC。IoT LTSC 页面引用的 [哈希入口 2269593](https://go.microsoft.com/fwlink/?linkid=2269593) 本次落到 Evaluation Center 首页，也没有取得哈希表。IoT 与普通 Enterprise LTSC 亦不能互换哈希。

响应提供了 `Content-Length`、`Last-Modified` 和 ETag；这些值应保留用于传输核对，但没有查到可以把 ETag 当作 ISO SHA-256 的官方约定。完整下载后应计算本地 SHA-256、核对长度、WIM 元数据及所提取模块身份，并保留 `independent_official_hash_verified=false`，直到获得适用的独立官方哈希。HEAD 200 本身不证明完整文件已经下载或校验。

主任务随后于本地时间 02:22 完成中文 LTSC ISO 下载，实际长度 **5,274,607,616** 字节，本地 SHA-256 为 `fc323a7d0ef02919fc9150d0c61978ce3a2f32b1519bedf8e510e392f37cde57`。文件位于 `local/compatibility-next/media/windows11-enterprise-ltsc-2024-26100.1742-zh-cn.iso`，分段长度、哈希和不变 ETag 记录在同目录 `ltsc-2024-range-download.json`；`independent_official_hash_verified` 仍为 `false`。这项传输结果不等于客体已经安装或任务栏已验收。

## 仍可访问的 23H2、22H2 官方 ESD

当前 [普通 Windows 11 下载页](https://www.microsoft.com/en-us/software-download/windows11) 显示 26H2，没有在本次页面中提供旧版本选择。不过，微软仍托管旧 Media Creation Tool 产品清单。清单地址最初从 [AveYo 的 MCT 包装脚本文本](https://github.com/AveYo/MediaCreationTool.bat/blob/main/MediaCreationTool.bat) 发现；仅将其当作地址线索，没有执行该脚本，也没有使用它改写的产品数据。

随后独立从 `download.microsoft.com` 通过 HTTPS 取回两份原始 CAB，用系统 `expand.exe` 解出原始 `products.xml`。URL、文件大小及下表 SHA-1 均来自这些微软托管 XML，而非第三方哈希列表：

| 分支 | 官方 CAB | 取得字节数 | 本地 CAB SHA-256 |
| --- | --- | --- | --- |
| 23H2 | [products_win11_20231208.cab](https://download.microsoft.com/download/6/2/b/62b47bc5-1b28-4bfa-9422-e7a098d326d4/products_win11_20231208.cab) | 28,789 | `6c250a83c976c7e47c5187045d3701917b1638069b0492e8d0b1fc546c23cb7e` |
| 22H2 | [products_win11_20230510.cab.cab](https://download.microsoft.com/download/b/1/9/b19bd7fd-78c4-4f88-8c40-3e52aee143c2/products_win11_20230510.cab.cab) | 29,481 | `03e013d0a23bc0285742e3fb07ba278195554b1d19fa2ea26929e9aee8419541` |

筛选 XML 中 `Architecture=x64`、`Edition=Professional` 的中文与英文记录，得到：

| 分支与语言 | 官方 ESD URL | XML Size / 本次 HEAD Content-Length | XML SHA-1 |
| --- | --- | --- | --- |
| 23H2 22631.2861，中文 | [CLIENTCONSUMER x64 zh-cn](http://dl.delivery.mp.microsoft.com/filestreamingservice/files/d867ac31-3ea9-4249-a867-3bd213a6ce44/22631.2861.231204-0538.23H2_NI_RELEASE_SVC_REFRESH_CLIENTCONSUMER_RET_x64FRE_zh-cn.esd) | 4,936,374,662 字节，两者一致 | `6d0c727c4c83f8095e6d3cecc7afae120d0553cc` |
| 23H2 22631.2861，英文 | [CLIENTCONSUMER x64 en-us](http://dl.delivery.mp.microsoft.com/filestreamingservice/files/5c5cedfc-2821-4395-a497-c9c45ec24194/22631.2861.231204-0538.23H2_NI_RELEASE_SVC_REFRESH_CLIENTCONSUMER_RET_x64FRE_en-us.esd) | 4,870,804,855 字节，两者一致 | `8d8b38a9f97b71446301f3a6170078aa445f4958` |
| 22H2 22621.1702，中文 | [CLIENTCONSUMER x64 zh-cn](http://dl.delivery.mp.microsoft.com/filestreamingservice/files/69e12f53-2f77-4cf9-aeb2-8f95ee0a446b/22621.1702.230505-1222.ni_release_svc_refresh_CLIENTCONSUMER_RET_x64FRE_zh-cn.esd) | 4,111,968,778 字节，两者一致 | `463e5912b10f22ffd608abafaeb79e7e2ef8c116` |
| 22H2 22621.1702，英文 | [CLIENTCONSUMER x64 en-us](http://dl.delivery.mp.microsoft.com/filestreamingservice/files/ec26d435-5d20-4563-9c75-b7726e55f93b/22621.1702.230505-1222.ni_release_svc_refresh_CLIENTCONSUMER_RET_x64FRE_en-us.esd) | 4,059,863,632 字节，两者一致 | `4232d743be089cc3141164ef79b66549795d3dbf` |

四个原始 HTTP 地址均返回 200。本环境直接将相同主机地址改为 HTTPS 时，证书主机名检查失败；没有忽略 TLS 检查，也没有将这个失败解释成官方文件不存在。若使用 XML 原始地址下载，必须以经 HTTPS 获取的原始 XML 核对完整文件的 Size 与 SHA-1，并另外记录本地 SHA-256。此处 XML 只提供 SHA-1，不能称为取得独立官方 SHA-256。HEAD 成功还不证明完整 ESD、内部索引或安装流程已经验证。

这是一条无需获取整套第三方安装脚本的官方内容路线。取得 ESD 后先枚举内部映像和版本，确认所选索引确为目标版本与 edition，再按实际压缩格式准备部署；不要猜索引、将 ESD 改名为 ISO 或混用不同系统的 `install` 文件。微软的 [DISM 映像管理说明](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/dism-image-management-command-line-options-s14?view=windows-11) 记录映像信息、导出与 recovery/ESD 格式，[应用 Windows 映像部署说明](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/capture-and-apply-windows-using-a-single-wim?view=windows-11) 记录在 WinPE 中应用映像及用 BCDBoot 准备启动。能否直接处理当前 ESD、所需导出格式和安装索引须在实际文件上确认；本次没有把通用部署文档当作已安装成功的证据。

## 小体积旧模块与精确符号路线

微软 [公共符号服务](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/microsoft-public-symbols) 提供按需下载与缓存；[符号存储格式](https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/symbol-storage-format) 和 [SymFindFileInPath](https://learn.microsoft.com/en-us/windows/win32/api/dbghelp/nf-dbghelp-symfindfileinpath) 说明映像使用时间戳与映像大小识别，PDB 使用其对应签名/age。微软服务可以返回部分 Windows PE 映像，不代表每个历史映像都永远可用。

并行调查使用第三方 [Winbindex taskbar 元数据](https://winbindex.m417z.com/data/by_filename_compressed/taskbar.dll.json.gz) 作为发现线索，为 24H2、23H2、22H2 的 `BASE` 记录各选出 taskbar、shell、tray 三个角色，共九条微软 PE 地址。完整计划在本地 `local/compatibility-next/older-module-downloads.json`，九条 HEAD 均为 200，长度与该元数据一致。三个模块组的预计 PE 总量分别约 **20.6 MB、21.3 MB、16.8 MB**，不含 PDB，适合优先静态筛查。

两条可复现例子如下；期望 SHA-256 的来源仍是第三方索引，须与实际微软返回文件另行核对：

| 线索记录 | 微软 PE 地址 | 期望文件大小与身份 | 第三方索引期望 SHA-256 |
| --- | --- | --- | --- |
| 24H2 BASE taskbar | [taskbar.dll / 1F014E7B2df000](https://msdl.microsoft.com/download/symbols/taskbar.dll/1F014E7B2df000/taskbar.dll) | 3,016,400 字节；x64；timestamp `520179323`；SizeOfImage `3010560`；资源版本 `10.0.26100.1` | `541736f4bb4ced40420f42bd1f6a6a5f74d77e868d0b5e2184ef3e4073a92135` |
| 23H2 BASE taskbar | [taskbar.dll / E27B4A5438c000](https://msdl.microsoft.com/download/symbols/taskbar.dll/E27B4A5438c000/taskbar.dll) | 3,723,488 字节；x64；timestamp `3799730772`；SizeOfImage `3719168`；资源版本 `10.0.22621.2428` | `80e92bfc21a6707d0c28a0e679b84af141321c086de9c13f6bae44cad4c9f7eb` |

这里的 24H2 BASE 索引对应其第三方记录中的 2024-09-30 介质，**不等于上文 LTSC 26100.1742 镜像**。23H2 的模块资源版本为 22621 也符合共同基础的可能性，不能仅凭该标签重新分组。三组 tray 路径线索均是 Core 下的真实 `Taskbar.View.dll`，不得改名为 `SystemTray.dll` 套用现有资料。

模块获取流程须记录微软响应跳转来源与实际字节，读取真实 PE 的 machine、timestamp、SizeOfImage、RSDS GUID/age 和 SHA-256，再请求对应 PDB；核对 PDB GUID、DBI age、符号/指令及必要的签名或目录证据。PE 下载键只包含 timestamp 与 SizeOfImage，不能代替 SHA、架构和 CodeView 的完整身份。元数据中的 “Unsigned” 也不能直接解释为非微软文件，因为有些系统文件依赖目录签名。

单独取得九个有效微软文件仍不能证明它们曾在同一安装中共同加载；此路线只产生 `production_ready=false` 的静态候选，不激活产品支持。正式资料仍需从独立安装客体复核三模块集合、实际 loaded image、source/拓扑和停止恢复，随后逐项执行交互及多屏矩阵。

截至首轮研究封存（02:27），主任务已经取得这九个实际 PE，并按预期长度、SHA-256 和 PE 身份检查后运行当时的离线解析器。下列首轮真实结果均为 `status=blocked`、`production_ready=false`；它们表示当时解析规则不能接收该组模块，保留这些原始结果用于后续对照：

| 组别 | 本地原始结果 | 解析器拒绝原因 |
| --- | --- | --- |
| 24H2 BASE | `local/compatibility-next/candidate-base-24H2/candidate.json` | source 实现字段未知或不唯一；托盘清理调用形状不符合既有直接调用规则 |
| 23H2 BASE | `local/compatibility-next/candidate-base-23H2/candidate.json` | 找不到当前精确签名的 `get_XamlContent`；找不到当前签名的 `GetScreenRectFromXamlElement` |
| 22H2 BASE | `local/compatibility-next/candidate-base-22H2/candidate.json` | 找不到当前精确签名的 `FrameHeight`、factory `CreateInstance` 和 `GetScreenRectFromXamlElement` |

缺少某个精确修饰名不等于该系统完全没有相应能力；应对实际符号和代码重新建模，不能删除检查、猜偏移或用新版函数地址放行。这三组尚无新增生产支持结论，也不因 LTSC ISO 下载成功而自动获得运行时证据。

后续更新：在对 RSI 内联 Content 调用与独立 transform 临时对象清理增加窄规则、并修复错误调用目标可被接受的两个问题后，同一 24H2 BASE 模块组已在 `local/compatibility-next/candidate-base-24H2-reviewed/` 取得 `static_candidate`、空 `analysis_blockers` 结果，仍为 `production_ready=false`，没有加入运行注册表。独立 LTSC 1742 映像在首轮新规则加入后才首次分析，首次即为 `static_candidate`、空 `analysis_blockers`，审查收紧后复测仍通过；它没有上述 BASE 24H2 的首轮拒绝历史。8037 与 LTSC 1742 目前仅加入显式 lab 资料，默认仍为原三组。22H2/23H2 的原始拒绝记录继续保留，后续已找到 source 名称/字段与不同托盘 ABI 的证据，但没有完整运行通过结论。具体字段、规则、候选和待验范围见 [本轮兼容证据](compatibility-next-versions.md)。

## 推荐执行顺序与验收记录

1. **先独立安装 LTSC 2024。** 下载上文中文或英文官方 ISO，保留来源/长度/本地哈希与独立官方哈希是否取得的标记；使用新的可丢弃 VM 和虚拟盘，不修改宿主系统。记录 edition 为 Enterprise LTSC Evaluation、实际 OS/UBR、三模块及真实托盘载体。此轮只为对应 LTSC 模块建立证据。
2. **并行静态比较 24H2、23H2、22H2。** 使用九条微软 PE 链接和精确 PDB，先判断现有 source 关系、对象字段、虚表/入口和托盘 ABI 是否能复用。遇到未知指令或生命周期差异保留拒绝原因，不按相同构建家族放行。
3. **随后建立 23H2 独立客体。** 从原始微软 XML 中的 ESD 获取 22631.2861，完整校验后确认映像索引和安装方式；完成同样六项基础/主屏检查，再补实际输入、托盘复制、多屏/DPI。22H2 22621.1702 使用已确认的下一条来源重复该流程。
4. **分别记录版本与测试场景。** 已有 8457 的单屏通过、未来 LTSC 的结果、其他累计更新和未来普通 24H2/23H2/22H2 结果各自绑定实际模块与程序哈希。共享 servicing 基础和相同解析方案有助于复用代码，不能合并不同版本的实测结论。

本次可直接使用的证据文件包括 `selected-esd-media.json`、四份 `esd-*-http-head.meta.json`、两份 `products-*.meta.json` 与解出的 XML、`ltsc-*-head.meta.json`，以及对应官方网页快照。小型 CAB 的地址发现来源、官方取回内容与本地哈希均已分开记录；第三方线索没有取得指挥执行或修改兼容资料的权限。
