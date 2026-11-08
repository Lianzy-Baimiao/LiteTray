# 新一批 Windows 11 候选与验证证据

记录日期：2026-11-09。**5.14.1 默认构建收录 26300.9711、26300.9818、26200.8457、25H2 26200.8037 和 Enterprise LTSC 2024 26100.1742 五组精确资料**。新增的 8037 与 LTSC 最初仅放入 `compatibility-lab`；两者分别在独立客体中通过冻结 lab 包的模块/布局、单屏时钟更新与停止恢复、托盘准备/停止三项限定验收后，才移入默认目录。原有 5.14.0 默认三组、lab 五组的包和记录保持原样。当前默认与显式 lab 构建均为五组；**同一份 5.14.1 默认二进制已在 8037 和 LTSC 两个独立客体完成六项限定复测**，显示保持与恢复均通过。日历/滚轮/菜单、实际托盘复制、多屏及混合 DPI 仍未完成验收。

官方来源、旧版介质与启用包边界见 [下一批版本来源研究](windows11-next-versions-research.md)。已完成的 8457 默认构建实测仍见 [25H2 8457 记录](compatibility-8457.md)，不能作为本轮 8037 或 LTSC 的实测替代。

## 候选来源与精确模块

8037 来自已有 `Win11_25H2_Chinese_Simplified_x64_v2.iso`，8,543,608,832 字节。只读 WIM XML 显示索引 4 为 Windows 11 Pro、x64、`10.0.26200.8037`；本轮从同一索引提取所需模块。来源文件名和 WIM 元数据不等于独立官方 ISO 认证，当前没有针对该本地介质的独立官方哈希核验记录。元数据与提取记录分别位于 `local/compatibility-next/vm-infrastructure/iso-v2-metadata/` 和 `iso-v2-modules/extraction-result.json`。

LTSC 来自微软 Evaluation Center 的官方 [中文 LTSC 下载入口](https://go.microsoft.com/fwlink/?linkid=2288085)。完整 ISO 为 5,274,607,616 字节，本地 SHA-256 为 `fc323a7d0ef02919fc9150d0c61978ce3a2f32b1519bedf8e510e392f37cde57`；本次没有取得适用的独立官方 SHA-256 表，保留该来源核验限制。WIM 只有一个索引，索引 1 为 Windows 11 Enterprise LTSC 2024 Evaluation、`EnterpriseSEval`、x64、`10.0.26100.1742`。元数据与提取记录位于 `vm-infrastructure/ltsc2024-metadata/` 和 `ltsc2024-modules/extraction-result.json`。它属于 24H2 代 LTSC，不能把它的结果扩大到普通 Home/Pro/Enterprise 24H2。

两组均只提取所需成员，没有把整个 WIM 展开或把系统 DLL 加载到宿主进程。以下 Core 表示 `SystemApps/MicrosoftWindows.Client.Core_cw5n1h2txyewy`：

| 候选 | 角色/相对路径 | 文件资源版本 | SHA-256 |
| --- | --- | --- | --- |
| 8037 | taskbar：`System32/taskbar.dll` | `10.0.26100.7920` | `9312c39df6be503f54a3e9bee1a7b32a249375daba0e0a8d5941f941d6e09481` |
| 8037 | shell：`System32/twinui.pcshell.dll` | `10.0.26100.8036` | `d56742e631ab34b07b0b908eddbdfd1811e8f0ad760641ba57125922f1ce1dc0` |
| 8037 | tray：`Core/Taskbar.View.dll` | `2126.2400.20.0` | `662d4cbfcbe2547ee2c66f68245acaaceb10c4280e6568b35c95aff295e20030` |
| LTSC 1742 | taskbar：`System32/taskbar.dll` | `10.0.26100.1591` | `b3be279bcb5a7fb63dbdc24cc1c55442a3965b68702a98fbdb6496d91cb40de5` |
| LTSC 1742 | shell：`System32/twinui.pcshell.dll` | `10.0.26100.1591` | `3ef119746b60885b8923c49de95860caca0639aec1e2e7aa0a4113356b2cedb9` |
| LTSC 1742 | tray：`Core/Taskbar.View.dll` | `2124.20800.0.0` | `734e19a7e749f4112acc1e072ab35a11fdd3ffc04d8fcf54f603593758702d5b` |

不同组件的资源版本不必等于 OS build 或彼此相同。两份实验资料保存完整 machine、timestamp、SizeOfImage、CodeView GUID/age、磁盘哈希、入口字节和布局，而非按 `26200` 或 `26100` 范围选择。程序仍按 `taskbar.dll` 完整身份唯一选资料，其余模块必须属于该资料；tray 保留真实 `Taskbar.View.dll` 路径，不改名为 `SystemTray.dll`。

当前内置资料为 [8037](../crates/adapter-protocol/profiles/windows11-26200.8037-x64.json) 与 [LTSC 1742](../crates/adapter-protocol/profiles/windows11-26100.1742-x64.json)。它们最初均以 VM validation pending 加入 lab；后续逐轮实测以本文记录为准。生产启动不读取外部 JSON，修改安装目录文件不能激活候选。

## 共性与实际变化的字段

| 项目 | 已登记 8457 | 已登记 8037 | 已登记 LTSC 1742 | 24H2 BASE 静态候选 |
| --- | --- | --- | --- | --- |
| FrameHeight 元素字段 | `+0x10` | `+0x10` | `+0x40` | `+0x40` |
| host → child 字段 | `+0x38` | `+0x38` | `+0x18` | `+0x18` |
| child → source 字段 | `+0x10` | `+0x10` | `+0x10` | `+0x10` |
| source 字段实现形状 | RDI 经 Content helper | `rdi_content_helper_v1` | `rsi_inline_content_v1` | `rsi_inline_content_v1` |
| screen_rect 所有权清理 | 独立析构函数 | `delegated_null_check_release_v1` | `delegated_with_transform_temporary_v1` | `delegated_with_transform_temporary_v1` |

8037 沿用已实现的 source 与托盘调用约定，但其三个模块身份、符号地址和虚表地址独立解析。LTSC 1742 和 24H2 BASE 显示字段确有变化，不能把 25H2 的 `+0x38` 或 `+0x10` FrameHeight 字段固定到所有 Windows 11。

LTSC 的 `HasFocus` 将宿主指针调整 `+0x18`。child 接口 getter 保留 `this -0x10` 的调整，实际实现从对象 `+0x20` 读取 source，所以接口相对字段仍为 `-0x10+0x20=+0x10`。当前 Rust source 方案已有有界字段参数，因此这些精确变化可以放入单独资料，运行时继续检查具体虚表、内存权限、线程、窗口、XamlRoot 与对象归属。表中共性只对已取证映像成立。

LTSC 与 24H2 BASE 的 RSI 实现会内联调用 source 的 Content ABI 槽 6。新静态规则核对同一字段的直接使用、空值分支、有界指令序列，以及最终调用必须通过精确 5 字节跳板到匹配符号 `_guard_dispatch_icall`。任意 `call` 或其他寄存器目标不会作为证明。

它们的 screen_rect 还会持有并释放独立 transform 临时包装对象。新规则分别核对输入 wrapper 的唯一析构和栈上临时对象的析构；初始化必须来自完整修饰名匹配的 `TransformToVisual`，核对 `R8=[RBP-0x60]`、`RDX=[RBP-0x50]`、`RCX=RSI` 及返回对象写入隐藏结果的位置。最终输入 wrapper 的析构仍须位于 RECT 写回之后、返回结果指针之前。

审查中曾发现两个过宽条件：将 Content 调用改成 `call rbx`，或将 transform 初始化改成提前调用 Release，初版规则仍可通过。收紧后这两个反例均被拒绝，并加入针对性拒绝测试。该修复不引入猜测偏移、宽泛字节扫描或任意无 `.pdata` 边界函数；固定跳板和既有 Release 叶函数各使用明确的短指令规则。

随后又发现第三个反例：保留两次预期析构，但在 `TransformToVisual` 之后、RECT 写回之前额外执行 `mov rcx,rsi; call UNKNOWN_RELEASE`，上一版规则仍会接受。它会提前结束输入 wrapper 的生命周期；这是合成代码揭示的解析规则漏洞，未在所审查的微软映像中观察到此错误路径。

修复后，transform 方案只允许一次指向精确 Release 叶函数的直接调用，且必须匹配已核验的临时参数释放序列：局部 `[rbp-0x60]` 先置空并作为 R8 传给精确 `TransformToVisual`，紧接着为 `nop; cmp [rbp-0x60],0; je after-release; lea rcx,[rbp-0x60]; call release`，发生在 RECT 写回之前。空值分支必须恰好跳过该次释放。额外释放输入 RSI、重复释放临时对象、换栈局部、换分支目标或缺少这次释放均被拒绝；后续独立 transform 对象析构和最终输入 wrapper 析构继续单独核对。

## 首次分析、规则修复与回归

所有候选封套保持 `production_ready=false`、`registry_modified=false`，静态成功不会自动提升资料：

| 对象 | 初始结果 | 后续已有结果 | 尚未证明 |
| --- | --- | --- | --- |
| 8037，同 ISO Pro 索引 4 | 现有静态规则通过 | 新旧编译形状规则加入后回归仍为 `static_candidate`、无 analysis blockers；后续 lab 与默认客体验收见下文 | 实际输入、托盘复制及多屏/DPI |
| LTSC 1742，同 ISO 索引 1 | 在首轮窄规则后首次分析通过，审查收紧后复测仍通过 | reviewed 与最新 `older-abi-analysis/cleanup-regression-ltsc1742` 均为 `static_candidate`、无 analysis blockers，完整布局不变；后续 lab 与默认客体验收见下文 | 实际输入、托盘复制及多屏/DPI |
| 24H2 BASE，微软符号服务单独获取模块 | source 字段未知、cleanup 规则拒绝 | reviewed 与最新 `older-abi-analysis/cleanup-regression-24h2-base` 在收紧规则下静态通过，完整布局不变 | 同一安装来源、独立运行验证；没有加入默认或 lab 注册表 |
| 23H2 BASE | source 精确名称、当前 screen_rect 符号不匹配 | source 名称/调用流和另一托盘 ABI 已有后续只读证据 | 新 ABI 实现、完整候选及真实运行 |
| 22H2 BASE | FrameHeight、factory、当前 screen_rect 符号不匹配 | source 名称与替代 host 字段证据已有后续分析 | 当前 HostLayout 以外的宿主表示、托盘入口及真实运行 |

LTSC 的首次 `candidate-ltsc2024-1742` 分析发生在两条新规则加入之后，结果已经是 `static_candidate`、空 `analysis_blockers`。本轮 source/cleanup 旧规则的首轮拒绝来自 24H2 BASE，不属于 LTSC 的分析历史。

8037 与 LTSC 的三个角色分别来自同一安装映像索引，这是输入来源记录；离线工具本身仍报告 `coherent_installation_verified=false`，实际安装与加载集合须由客体另行核对。两者的后续实际加载证据见逐轮记录，不能反向改写旧静态封套。BASE 三组仅按第三方索引线索取回微软 PE，不把九份分别有效的文件当作已经共同安装或加载。

资料提升前的源码验证日志显示：lab workspace **251 项通过、9 项忽略**；默认三组协议注册表 **37 项通过**；lab Clippy 与 release 构建通过。收紧直接 Release 规则后的 Python 检查为 **25 项通过**，包括上述错误调用目标、清理对象、额外释放及精确空值分支拒绝。日志与静态结果不替代真实任务栏测试，默认五组构建另行验证。

最后一次静态复测为 LTSC 和 24H2 BASE 各生成新的候选目录，均完整运行离线 inspector、返回 `static_candidate` 且无 blockers，原 reviewed 结果保留为历史快照。另用同一版解析器对 **24H2 BASE、LTSC 1742、8037、8457、9818 五组**磁盘字节及保存的精确符号映射重放，重新核对映像身份和哈希；全部完整布局与原候选一致，`layout_differences=[]`。工具身份、两份新候选哈希和五组字段比较见 `older-abi-analysis/cleanup-regression-verification.json`，没有加载目标 DLL 或增加真实客体验收结论。

提升前的实验注册表测试覆盖两份 lab 的门控、完整身份五项变造、磁盘/loaded 身份分离、错误哈希、两份 lab 互混、与三份正式资料互混及双向宿主布局串用。当时默认测试读取 lab fixture 做拒绝检查，默认运行注册表仍为三组；提升后的注册表测试按默认五组调整。

已冻结的 `lab-two-versions-package/` 的 `SHA256SUMS.txt` 列出 71 个文件，含清单自身共 72 个文件。使用其中 EXE 的 `--compatibility-report --system-root` 分别检测两份真实提取树，`lab-disk-8037.json` 与 `lab-disk-ltsc1742.json` 均选择各自精确 profile ID，三个模块全部匹配；报告保留 `scope=disk_only`、`explorer_accessed=false`、`loaded_modules_checked=false` 和 `runtime_topology_checked=false`。这证明冻结程序能读到所编入的资料，未证明其已在客体 Explorer 中运行。新解析器对 9818、8457 与 8037 的离线回归还与原资料逐字段一致。

## 22H2 与 23H2 的更早形态

最初的“找不到精确修饰名”没有证明接口功能消失。对匹配 PDB 扩大只读枚举后，23H2 的 getter、factory 和实现采用具体的未修饰原始名称；22H2 只有 factory `CreateInstance` 需要这种名称处理。23H2 的 getter 与 setter 还共用同一原始名称、对应不同 RVA，须以 child 虚表槽 8、factory 槽 6、唯一直接调用和有界指令流作为锚点消歧。两者都给出 **host→child `+0x18`、child→source `+0x10`**。使用冻结 `b049a0a` 基线解析器加本地显式绑定仍可复核这些 source 字段，避免后续工具改动被误当作旧结论的依据。这些本地绑定尚未变成通用生产解析入口。

23H2 发现的是 `DragDropManager::ScreenRectForElement(FrameworkElement const&, HWND)`，该映像 RVA 为 `0x2CBB3C`。完整函数和四个调用点显示其 x64 参数与已实现 screen_rect 不同：

| 入口约定 | 已实现的按值 screen_rect | 23H2 静态候选成员函数 |
| --- | --- | --- |
| RCX | RECT 隐藏结果指针 | 输入 this；本函数体覆盖前未使用 |
| RDX | 按值 FrameworkElement 临时包装对象 | RECT 隐藏结果指针 |
| R8 | HWND | 借用的 FrameworkElement wrapper 指针 |
| R9 | 无此参数 | HWND |
| 输入 wrapper 所有权 | 被调用方消费临时包装对象 | 调用方在返回后释放自己持有的 wrapper |

这需要独立 ABI 类型和相应四参数处理，不能把地址填入现有三参数入口。只读证据还核对 XamlRoot、Content、Transform、尺寸/缩放调用及坐标换算；没有据此声明菜单、跨屏或实际回调行为已可用。

22H2 的公共符号中缺少当前强制的 `FrameHeight`，但独立 `TaskbarHost::Show`、Width/Height 消费及 controller 写入链支持元素字段 `+0x40`。当前 `HostLayout` 验证仍要求精确 FrameHeight 入口，不能把该替代字段证据直接伪装成已存在的入口。22H2 此轮没有找到已核验的 screen_rect 方法；这不证明系统没有该能力，也不足以断言它必然被内联。

详细材料位于 `local/compatibility-next/older-abi-analysis/`，以其中的 `README.md` 正式报告及 `source-baseline/source-proof-22H2.json`、`source-baseline/source-proof-23H2.json` 为 source 结论依据；冻结解析器身份另见 `source-baseline/tool-provenance.json`。其他证据为 `screen-proof-23H2.json`、`call-sites-23H2-screen.json` 和 `host-field-proof-22H2.json`。所有报告保持静态研究边界，没有加载目标 DLL、进入 Explorer 或修改兼容资料。

## 证据位置与后续逐轮记录

以下位置均相对 `local/compatibility-next/`，Windows DLL、PDB、ISO、虚拟盘与客体凭据不随源码发布：

| 记录 | 位置 |
| --- | --- |
| 8037 只读介质元数据/提取 | `vm-infrastructure/iso-v2-metadata/`、`vm-infrastructure/iso-v2-modules/extraction-result.json` |
| LTSC 传输与介质元数据/提取 | `media/ltsc-2024-range-download.json`、`vm-infrastructure/ltsc2024-metadata/`、`vm-infrastructure/ltsc2024-modules/extraction-result.json` |
| 8037 静态原始与回归 | `candidate-26200.8037/`、`candidate-26200.8037-regression/` |
| LTSC 初次分析、中间验证与收紧后结果 | `candidate-ltsc2024-1742/`、`candidate-ltsc2024-1742-v2/`、`candidate-ltsc2024-1742-reviewed/` |
| 24H2 BASE 初次拒绝与收紧后结果 | `candidate-base-24H2/`、`candidate-base-24H2-v2/`、`candidate-base-24H2-reviewed/` |
| 直接释放规则收紧后的完整候选与五组布局回放 | `older-abi-analysis/cleanup-regression-24h2-base/`、`older-abi-analysis/cleanup-regression-ltsc1742/`、`older-abi-analysis/cleanup-regression-verification.json` |
| 额外释放反例、修复验证与最终审查 | `older-abi-analysis/review-negative-probe-owned-release.json`、`older-abi-analysis/review-fix-negative-probes.json`、`older-abi-analysis/CODE_REVIEW.md` |
| LTSC 人工转写审计 | `ltsc-1742-profile-transcription-review.json` |
| Rust 源码验证 | `ltsc-lab-workspace-tests.log`、`ltsc-default-registry-tests.log`、`ltsc-lab-clippy.log`、`ltsc-lab-build.log` |
| 冻结 lab 包及真实提取树离线检测 | `lab-two-versions-package/`、`lab-disk-8037.json`、`lab-disk-ltsc1742.json` |
| 提升前精确 lab 资料快照与来源 | `recorded-lab-profiles/`，从 Git `2e0d74c` 原始 blob 保存并匹配下表旧哈希 |
| 已有资料的新解析器回归 | `9818-tool-regression-explicit/`、`8457-tool-regression/`、`candidate-26200.8037-regression/` |
| 8037 / LTSC 客体准备材料 | `vm-8037/`、`vm-ltsc2024/` |
| LTSC 首轮 helper 超时及迟到 worker 补充复核 | `vm-ltsc2024/independent-lab-attempt-025411-review.json`、`vm-ltsc2024/independent-lab-attempt-025411-late-worker-review.json` |
| LTSC 第二轮真实检查失败独立复核 | `vm-ltsc2024/independent-lab-attempt-033636-review.json` |
| LTSC 只读诊断入口修正复核 | `vm-ltsc2024/independent-readonly-route-review.json` |
| LTSC 只读诊断与实际 loaded 身份独立复核 | `vm-ltsc2024/independent-readonly-035139-review.json` |
| LTSC lab 完整六项限定验收独立复核 | `vm-ltsc2024/independent-lab-034939-review.json` |
| LTSC lab 显示范围补充 | `vm-ltsc2024/independent-lab-034939-display-scope.json`、`vm-ltsc2024/observer-0039.png` |
| 8037 首轮 helper 超时及迟到 worker 独立复核 | `vm-8037/independent-lab-attempt-025705-review.json` |
| 8037 第二轮外层等待超时的部分证据 | `vm-8037/independent-lab-attempt-035454-partial-review.json` |
| 8037 同 nonce 迟到完整结果独立验收 | `vm-8037/independent-lab-035454-late-review.json` |

关键静态文件哈希：

| 文件 | SHA-256 |
| --- | --- |
| 8037 回归 `candidate.json` | `ed4bd0d5e54e461cfcab49c00b102d08ce07dd9202875d97484e2d1859bdfdc6` |
| LTSC reviewed `candidate.json`，保留的历史快照 | `93eb8eb29ab25002e6d3bfb26c9877bb3fab0dccc16d1fd8d693418e0104f0c5` |
| 24H2 BASE reviewed `candidate.json`，保留的历史快照 | `25685cddf05357a249e0bd18509aee1dbd50581b2a3e772b6963440b2303d359` |
| LTSC `cleanup-regression-ltsc1742/candidate.json` | `2ae766500d0aadb9c7de836729e6b805b8e6d1881621f5c2e6906faa21a93d9f` |
| 24H2 BASE `cleanup-regression-24h2-base/candidate.json` | `64deb67b9fd1e6642adf7346a5b199fc7bb4ffff92b1fb6385b951c03794fd12` |
| 五组 `cleanup-regression-verification.json` | `acfc6bff287af9f81c9b228c636d9a19f905803c70cbece0464f1a67247ec074` |
| 8037 最初 lab JSON 历史快照 | `342be9161df6fc7635138976ab5efc5dcb45a44bbee17abb34275f463e1b3149` |
| LTSC 1742 最初 lab JSON 历史快照 | `ec3cc7134f9891095ab074ddd3d7c479d4391f5b5fdccd9ac01f1345df39e960` |

后续每个真实客体轮次将在本节追加实际 run ID、隔离/live 实例目录、包清单与三程序哈希、客体版本及 Medium helper 身份、六项命令退出码/超时、逐帧读回和停止恢复、Explorer 连续性。部署或启动失败与接管失败分开记录；尚未取得报告的阶段不预填成功。

| 资料 | 当前真实任务栏结论 | 后续验证 |
| --- | --- | --- |
| 8037 | lab 及 5.14.1 默认包单屏三项限定验收通过 | lab 原 `035454-a5ef1897` 迟到完整结果通过，外层超时保持失败记录；默认 `042201-8d605056` 六项及显示保持/恢复通过，其余四场景待验 |
| LTSC 1742 | lab 及 5.14.1 默认包单屏三项限定验收通过 | lab `034939-43e55159` 和默认 `045031-4d58fae7` 六项通过；首次读取失败、许可关机和默认启动前失败均保留；其余四场景待验 |

8037 首轮 `sandbox-20261109-025705-b1b0fb31` 使用 nonce `27b2168581f04ce8bf31b24d47bf46aa`。父启动器在 03:51:05 等待 ready 超时，未取得完整测试结果；后续只读诊断轮 `sandbox-20261109-035637-416f1056` 回收原 nonce 的 ready/done，确认迟到 worker PID 8008 为 **Medium 8192、非提升、同一 Explorer SID、会话 2、WinSta0/Default**。时间线均为 2026-11-09 北京时间：

| 时间 | 8037 首轮已取得的证据 |
| --- | --- |
| 03:48:50 | worker 进程启动 |
| 03:51:05 | 父启动器的 90 秒 ready 等待超时 |
| 03:54:11 | 同一 worker 的 PowerShell 引擎进入 Available |
| 03:54:28 | 同一引擎记录 `Add-Type` 初始化 |
| 03:56:11 | worker 等待不到 grant 自行退出，`runner_started=false`、`runner_returned=false` |
| 03:57:37 | 只读进程采样开始时 worker 已退出；grant 不存在，任务已删除 |

此轮是启动握手未完成，不能算产品兼容通过或失败。诊断在 worker 退出后才开始，实际取得的 worker CPU 样本数为 0，不能把启动延迟归因于已测得的 CPU 忙碌或某个后台进程。原始补充材料位于 `vm-8037/guest-results/sandbox-20261109-035637-416f1056/launcher-diagnostic-sandbox-20261109-035637-416f1056/`；归档共 13 个文件、43,873 字节，SHA-256 `80172363981b75e4f922c2fae4e6a0c189be873d433f6b29929b9cb992dadd98`。之后的新轮次保留精确身份、grant 和包校验，将 helper 握手等待延长为 600 秒，六项产品命令自身的超时不变。

LTSC 首轮 `sandbox-20261109-025411-29c17ce9` 已取得 24H2 `26100.1742` 的登录环境。启动器最终报告为等待 `ready.json` 超时，`runner_completion_observed=false`、`tests_evaluated=false`，并确认删除了该轮计划任务。保留的归档只有 `job.json`、`parent-and-explorer.json` 和 `launcher-final.json`；没有取得通过身份核验的 worker 报告及六项检查结果。原始材料位于 `vm-ltsc2024/guest-results/sandbox-20261109-025411-29c17ce9/medium-launch-b3a367fef755487eb37252285e51b2a7/`。

该轮已确认 bootstrap 为 High 完整性 12288，Explorer 为 Medium 8192，二者属于同一用户 SID 和会话 2；首份归档尚不能证明测试 worker 已按要求执行。此记录归为启动验证未完成，不能算作产品接管失败、通过或已完成恢复。首份归档中的等待超时也不足以断言延迟 worker 此后绝不会启动，因此继续进行了只读诊断。

后续诊断轮 `sandbox-20261109-033159-67a7fd93` 取回同一 nonce 的 `original-worker-state/ready.json` 与 `done.json`，确认迟到 worker PID 3140 为 **Medium 8192、非提升、同一 SID/会话 2、WinSta0/Default**。时间线如下，均为 2026-11-09 北京时间：

| 时间 | 已取得的证据 |
| --- | --- |
| 03:26:32 | worker 进程启动 |
| 03:27:34 | 同一 worker 的 PowerShell 引擎进入 Available |
| 03:28:24 | 同一引擎记录 `Add-Type` 初始化 |
| 03:28:32 | 父启动器的 90 秒 ready 等待超时 |
| 03:30:23 | worker 等待不到 `grant.json` 自行退出，`runner_started=false`、`runner_returned=false` |
| 03:32:20 | 只读诊断确认 grant 不存在、PID 3140 已不在进程列表、计划任务已删除 |

这些新增证据将首轮问题定位为启动握手时间不足，并确认 LiteTray 测试 runner 没有开始。原始补充材料位于 `vm-ltsc2024/guest-results/sandbox-20261109-033159-67a7fd93/launcher-diagnostic-sandbox-20261109-033159-67a7fd93/`，保留各文件哈希。重试使用新 nonce 和结果目录，仅将 helper 双方的 ready/grant 等待从 90 秒延长到 600 秒；继续要求 Medium 身份、SID/会话/桌面与包哈希验证，六项产品命令各自的超时保持不变。延长握手不是兼容测试通过的证据。

LTSC 第二轮 `sandbox-20261109-033636-ddd5a318` 使用新 nonce `92eebe879daf423a9354a5ca8e28eaf7`，实际完成六项命令。独立复核确认 worker PID 5368 为 Medium 8192、非提升、会话 2、WinSta0/Default，并核对 grant 的 nonce、PID 和进程启动时间。客体为 `24H2 / 26100.1742 / EnterpriseSEval`，三个磁盘模块的完整身份与哈希均匹配 LTSC 资料；冻结包清单及客体复制记录的 71 项哈希全部一致，清单 SHA-256 为 `042c95d8f2f71c95eb5f3374a5337ad72c02f238a5eaa24592d8acf2a631ea7c`。

| 第二轮命令 | 退出码 | 超时 | 结果 |
| --- | --- | --- | --- |
| compatibility | 0 | 否 | 三个磁盘模块匹配，仍为 disk-only |
| diagnostics | 0 | 否 | 报告导出通过 |
| app-icon | 0 | 否 | 通过 |
| xaml-clock | 0 | 否 | 164 项通过 |
| shell-source | 0 | 否 | 私有宿主 6 项通过 |
| taskbar-live | 1 | 否 | 首次真实任务栏读取未返回兼容响应，52,028 ms 后结束 |

第六项报告没有 `before` 基线，`samples=[]`、`writes_requested=false`；按检查顺序，它在 `Connection::prepare` 和 `Start` 之前的首次 `inspect_primary` 失败。末次只读检查随后成功读取 149 个元素和时钟节点，时间、日期及原生容器 `55×32` 均可见；Explorer PID 4136、TID 3656、主任务栏 HWND 196862 和进程启动时间在检查前后不变。由于没有取得写入前基线和三帧更新，这不构成时钟恢复验收。`cleanup_acknowledged=true` 仅说明尚未建立接管连接时的清理过程返回成功，不能扩大为有效会话的停止恢复通过。

同轮模块审计按预设条件拒绝执行，原因是六项检查未全部成功，因此本轮没有独立 loaded-module 审计结论。真实任务栏结果保留 `checks_failed`，catalog 未提升。完整归档为 `vm-ltsc2024/guest-results/sandbox-20261109-033636-ddd5a318.zip`，74,039 字节，SHA-256 `ad26bd17159a138b210d13c6c2ab47f35c4454cfe5b20e87f9fecddaea564a61`；归档包含前序材料，本轮报告必须在 `medium-launch-92eebe879daf423a9354a5ca8e28eaf7/runner-results/` 下读取，不能混用旧轮次。

为定位首次响应差异，计划追加两次 `--inspect-taskbar` 和一次 `--probe-taskbar`。最初诊断轮 `sandbox-20261109-034852-5e16d152` 的 `live=false` 仍让 bootstrap 选择原 `Invoke-SandboxGuestCheck.ps1`，实际只执行了四项 isolated 检查，均退出 0；没有执行预定的两次 inspect 或 probe，也没有该轮的 Medium 诊断结果。其归档 SHA-256 为 `833f0da2b519fa9b4cdb5acce85e1d22ee1b65444d71b188861c333633811019`，其中此前轮次的目录不能当作本轮诊断证据。

新的只读诊断包 `sandbox-20261109-035139-b0b569a8` 将 false 分支入口绑定到已核验的 Medium wrapper，原 isolated 脚本另存为 `Invoke-FrozenGuestHarness.ps1`。独立检查确认执行链为 bootstrap → wrapper → Medium 600 秒握手 → 限定只读的 runner → inspect-1、inspect-2、probe；保留的 live/isolated harness 只被校验哈希，没有被调用。71 项冻结包哈希、8 份脚本及 ISO 内 81 个文件均与保存的 payload 一致，脚本语法检查通过。该路由复核发生在 VM 操作方已附加包之后，只审查保存的文件，不能当作已取得实际诊断结果或接管验收。

随后该诊断轮的实际结果已回传。只读取本轮 nonce `a05bf2a6bef94823ab7dc56db46888e1`：worker PID 380 为 Medium 8192、非提升、会话 2、WinSta0/Default，71 项包记录与冻结哈希相符。两次 inspect 和一次 probe 均退出 0、未超时，耗时依次为 **4,978 ms、992 ms、312 ms**；均指向同一 Explorer PID 4136、任务栏 TID 3656 和 HWND 196862，报告 `changes_applied=false`。前后 Explorer 进程启动时间及三份模块的映射路径、基址和映像大小保持不变。

本轮 `probe.json` 直接读取到三份实际 loaded PE 的 machine、timestamp、SizeOfImage、PDB GUID/age，逐字段与 LTSC 资料一致；这些基址与进程模块枚举相同，映射路径对应文件的 SHA-256 也与三份资料完全匹配。这里将内存身份与路径文件哈希分开记录，不把磁盘哈希称为整份已加载内存的哈希。原始报告位于该轮 `medium-launch-a05bf2a6bef94823ab7dc56db46888e1/runner-results/readonly-diagnostic-sandbox-20261109-035139-b0b569a8/`；完整归档 129,133 字节，SHA-256 为 `f420c6074a3dc987566c727c6ef79a09928d40ae5e15ae2fc4d1e23ba99d6100`。

这证明该只读轮次能正常取得响应和精确 loaded 身份，**仍未执行时钟 Start、三帧更新、Stop 或托盘准备验收**。此前首次响应失败的具体原因尚未证实，不能仅凭后续读取成功就标为已修复或归因于某一种初始化延迟；必须另用完整六项新轮次确认。

随后完整复测轮 **`sandbox-20261109-034939-43e55159`** 已完成限定验收。该 run ID 是生成时间，实际完整检查在上述诊断后执行；只选取 nonce **`d52dadc913844f84ba91acbff1c14dfe`** 下的 `guest-e950fda8` 与 `live-de428f69`。同一冻结 lab 包的六项命令均退出 0、未超时，XAML 164 项、私有宿主 6 项通过；实际 OS 仍为 Windows 11 Enterprise LTSC 2024 Evaluation、`24H2 / 26100.1742 / EnterpriseSEval`。worker PID 4772 为 Medium 8192、非提升、会话 2、WinSta0/Default，grant 与该进程的 nonce、PID、启动时间相符，任务已删除。

| 完整复测命令 | 退出码 | 超时 | 耗时 |
| --- | --- | --- | --- |
| compatibility | 0 | 否 | 4,191 ms |
| diagnostics | 0 | 否 | 783 ms |
| app-icon | 0 | 否 | 375 ms |
| xaml-clock | 0 | 否 | 25,998 ms |
| shell-source | 0 | 否 | 6,405 ms |
| taskbar-live | 0 | 否 | 25,064 ms |

独立复核逐帧按完整树和 topology 选定同一时钟节点，确认以下请求均实际读回，三个节点的 XAML `visible` 属性均为 true：

| 帧 | 时间文字 | 日期文字 | 请求/读回容器宽度 |
| --- | --- | --- | --- |
| 1 | `LT LIVE 01` | `Sandbox check A` | 240 / 240 |
| 2 | `LT LIVE 02` | `Sandbox check B` | 320 / 320 |
| 3 | `LT LIVE 03` | `Sandbox check C` | 320 / 320 |

停止前的原始读回为时间 `3:31`、日期 `2026/11/9`、容器 **55×32**；停止后各节点的文字、尺寸和可见性属性逐项恢复，时间/日期文本中的方向标记也与基线一致。`clock.stop_acknowledged`、`cleanup_acknowledged` 和五类必需托盘区域的准备/停止均为 true，托盘 `activated=false`、复制仍为 `not_applicable_single_monitor`。全程 Explorer PID 4136、TID 3656、HWND 196862 及进程启动时间保持不变。同轮后置 module audit 退出 0，直接 probe 的三模块 loaded PE/PDB 身份与映射路径文件 SHA-256 全部匹配该 LTSC 资料。

该轮实际执行时间为 03:55–03:56，原始时钟节点仍读为 `3:31`；此前的客体观测 `observer-0039.png` 为全黑，VM 操作方报告虚拟显示器进入休眠，本轮未记录显示器唤醒。因此这里验收的是实际任务栏对象的字段更新和恢复；`visible=true` 不是当时屏幕像素已经绘出的证明，也不能据此证明原生时钟持续刷新正常。屏幕绘制、独立原生刷新与恢复刷新仍未验收，后续完整复测需先在客体保持显示器唤醒并记录状态。

验收使用的包清单仍是 `042c95d8f2f71c95eb5f3374a5337ad72c02f238a5eaa24592d8acf2a631ea7c`，71 个成员在输入和客体复制记录中逐项核对：

| 程序 | SHA-256 |
| --- | --- |
| `LiteTray.exe` | `868a7c973aff6f6fb74d98aada0875f76638d8335b5e2fec6dbbef73718ae8fe` |
| `litetray_taskbar_adapter.dll` | `d844f224772260443510118fe9f3f22c03b7a8d425c1470e1278c8f706975c08` |
| `litetray_popup_hook.dll` | `c3581c629d1b39d3a5752da86a685574e257a114be129699f3d599c6b3d9804c` |

完整归档 `vm-ltsc2024/guest-results/sandbox-20261109-034939-43e55159.zip` 为 232,237 字节，SHA-256 `9ab81c96bf47a6424b8ef63232752d7276069a239e64ba5470fc2528062af96e`。本轮关键文件哈希为：

| 证据文件 | SHA-256 |
| --- | --- |
| isolated `summary.json` | `1792d705c486b39aaf8c2ae63e2d0a24187d8a36e3308e6dc97303aeacdd7255` |
| live `summary.json` | `67b204da02d85d22b2e0d416698e9dc059a938c4fc17334a4f8e2dc500432f29` |
| `taskbar-live.json` | `734679caac8019e89624dd19d4a2ad9d9b75cba4a3bf2fdebf946fa57297ad43` |
| `runner-context` | `be817270c5c05af39a017fbfca0d4399f7486c4a6ea5832541f49590521e18a4` |
| module audit `summary.json` | `3027a7117e116009cbd090c4af94f19be76d692de96d4199ae848198cfe7c8a2` |
| `probe-taskbar.json` | `cb4298cee4211901ab36a8ee70284c1ecc6ede4a76f8d546cece2d0a8a3a9462` |

因此 catalog 仅为这一精确 LTSC ID 与三份模块 SHA-256 登记 **模块/布局、时钟更新与停止恢复、托盘准备/停止三项 `tested`**，日期 2026-11-09；指针交互、实际托盘复制、多屏与混合 DPI 四项仍为 `not_run`。这次通过使用与失败轮相同的产品包，且发生在先前只读检查之后；保留首次响应失败的未解原因，不能写成代码修复或首次启动稳定性已经证明。字体、颜色、原始绑定导出和独立原生刷新回调计数仍未验收。

8037 第二轮 **`sandbox-20261109-035454-a5ef1897`** 使用 nonce **`741330f4e8ad4390a10b57798605bc09`**，600 秒 helper 握手取得并核验 Medium worker PID 7984。它为非提升、会话 2、WinSta0/Default，与 Explorer 属于同一 SID；grant 的 nonce、PID 和启动时间均相符。04:00:41 的 runner context 核对了同一冻结 lab 包的 71 个文件，04:04:40 建立 live summary，04:05:28 建立 isolated summary。父启动器于 **04:05:53** 等待 done 超时，原始 300 秒外层预算不足，保存的两份 summary 当时仍为 `running`、`tests=[]`、`os=null`。没有终止 worker，也没有更换其原输入盘；这份初始归档不能判定产品成败。

初始归档 `vm-8037/guest-results/sandbox-20261109-035454-a5ef1897.zip` 为 61,417 字节，SHA-256 `0f0e2a1a976a77131e7ecc41dd9a6e3404bae8fa0f570d5e62305e6352663255`。独立复核封存这份未完成快照。六项产品命令各自的既定最大等待合计 450 秒，且外层还包含运行准备与报告处理；后续完整轮将外层等待增至 1,800 秒，产品命令本身的 60/90/120 秒边界不变。

只读诊断轮 **`sandbox-20261109-040822-04e629f4`** 随后在独立 install0 输入盘上取回原 nonce 的最终文件，保留了 worker 原输入盘。该诊断只采样进程与事件、复制状态，不写 grant、不终止进程、不重新启动产品。最终验收读取的是 `launcher-diagnostic-sandbox-20261109-040822-04e629f4/original-worker-state/runner-results/` 下的原 `guest-15cb2196` 和 `live-c44b4759`，不是同一累积归档内先前导出的未完成目录。

原 worker 的 isolated 检查于 04:09:01 完成，live 检查于 04:09:39 完成，同轮模块审计于 04:09:57 完成；done 记录 **04:09:59.732** 正常返回，`runner_started=true`、`runner_returned=true`、错误为空。04:12:41 的进程采样确认该 worker 和 LiteTray 测试进程均已退出，诊断报告于 04:12:52 完成。父启动器原来的 `runner_completion_observed=false` 与超时错误保持原值；迟到的真实结果通过不等于父启动器已经成功观察到完成。

实际客体为 **Windows 11 Pro / 25H2 / 26200.8037 / Professional**，三模块磁盘和实际 loaded 身份均与该资料匹配。六项命令结果如下：

| 8037 原轮次命令 | 退出码 | 超时 | 耗时 |
| --- | --- | --- | --- |
| compatibility | 0 | 否 | 8,014 ms |
| diagnostics | 0 | 否 | 1,714 ms |
| app-icon | 0 | 否 | 909 ms |
| xaml-clock | 0 | 否 | 57,349 ms |
| shell-source | 0 | 否 | 16,397 ms |
| taskbar-live | 0 | 否 | 18,979 ms |

XAML 164 项、私有宿主 6 项通过。逐帧完整树与 topology 均读回 `LT LIVE 01/02/03`、`Sandbox check A/B/C`，请求和实际宽度依次为 **240、320、320**，三个时钟节点的 `visible` 属性均为 true。前后原生时间为 `3:55`、日期为带方向标记的 `2026/11/9`、容器 **55×32**；停止后文字、尺寸、可见性逐项相等。时间/日期 TextBlock 宽度分别为 22.0078125 和 54.6328125，高度均为 15.9609375。`clock.stop_acknowledged`、`cleanup_acknowledged` 及五类托盘区域准备/停止均为 true，托盘未激活，实际复制仍为单屏不适用。

Explorer **PID 4576、TID 2564、主任务栏 HWND 262352** 和进程启动时间 `/Date(1794166951042)/` 始终相同。同轮模块审计退出 0、未超时，直接 probe 的三份 loaded PE machine、timestamp、SizeOfImage、PDB GUID/age 逐字段匹配；其基址和映射路径与进程模块枚举一致，对应路径文件 SHA-256 亦匹配。内存身份与路径文件哈希分别核验，不把磁盘哈希称为整个已加载内存的哈希。

本轮仍使用清单 `042c95d8f2f71c95eb5f3374a5337ad72c02f238a5eaa24592d8acf2a631ea7c`，EXE、adapter 和 popup 的三份哈希与上文 LTSC 冻结 lab 包相同。取回完整结果的诊断归档 `vm-8037/guest-results/sandbox-20261109-040822-04e629f4.zip` 共 77 个文件、177,730 字节，SHA-256 为 `1f3b066fd13ac17665501b2c7287f8715c51ce7cd6a3a8eaeb289b0ee3ce4bb9`。关键迟到文件哈希为：

| 8037 证据文件 | SHA-256 |
| --- | --- |
| isolated `summary.json` | `f600e6871c93c55ee41e8d648488d5a0b5216739ef56d6e284e66fbd78f6bbdb` |
| live `summary.json` | `4b26391e69e5b06b0b9feaa17893812258adbf9071159ed96ebd670340e8b964` |
| `taskbar-live.json` | `0b6f17a87c87c272ab2fa60779e0ab7673b0cd114b27afdf81b179b10289093a` |
| `runner-context` | `74f84008781d6e8b1ff8a57927479d724581fb770ebc93a2bf604dfeb48367fe` |
| module audit `summary.json` | `1aafaae9a30dd33ba6be0cf3432f0d86446fbf952910ffb6522de7925ed9bf07` |
| `probe-taskbar.json` | `bb60aeabf2f2d193666ab959467d2c41fc57d683cbae3e61602f0c20eacbcd82` |
| 原 worker `done.json` | `6f72a3b29f58714be93923c3571938fe1cf320da2606919136df9a8860c2f5bd` |

8037 的 catalog 同样仅对精确 ID 和三份模块 SHA-256 登记三项限定 `tested`，其余四项为 `not_run`，日期为 2026-11-09。实际 live 在 04:09 执行，原生节点仍读 `3:55`，本轮没有显示器唤醒状态记录；因此这份证据验收对象属性更新与恢复，未独立验收屏幕绘制或原生时钟持续刷新。默认构建将另用显示器唤醒及执行状态保持记录复测，不能以本轮对象 `visible=true` 代替像素证据。

## 5.14.1 默认包冻结与正式复测

默认五组源码的 workspace 测试为 **251 项通过、9 项忽略**，显式 feature 协议测试 37 项、独立 CLI 导出 5 项和 UI 后台检测/导出专项 1 项通过，Clippy 和 release 构建通过；日志为 `promoted-default-workspace-tests.log`、`promoted-feature-registry-tests.log`、`promoted-default-cli-export.log`、`promoted-default-ui-worker.log`、`promoted-default-clippy.log`、`promoted-default-build.log`。这些检查包含兼容目录与 UI 关联规则，仍不替代默认二进制在客体的运行结果。

默认包于 04:20:52 冻结在 `default-two-versions-package/`，`default-package-freeze.json` 记录 **5.14.1 / default**；清单列出 75 个文件，含清单自身共 76 个文件。测试包清单 SHA-256 为 **`a1c565cd9c8a2d77b371f06e929a6e698aaceadc3744bb23deb02be742167a9b`**：

| 默认程序 | SHA-256 |
| --- | --- |
| `LiteTray.exe` | `97a323d0e5ce6d6921897d35d0698e74dde15bcca343606289dcf72e8d232bd5` |
| `litetray_taskbar_adapter.dll` | `1cdb1d91434726a9f2d98fd36c6bdbd1c103b5ab07154f9abd8d4611c404cc5c` |
| `litetray_popup_hook.dll` | `dd367d6aabf931fce2557c33f67586202156077da66e71ec61d11039f2e9e03d` |

新载荷分别为 8037 的 `sandbox-20261109-042201-8d605056` 与 LTSC 的 `sandbox-20261109-042202-794f43a9`。执行前独立核对两份输入的全部 75 项与冻结包一致，各自 ISO 的 83 个文件逐字节匹配保存的 input；Windows PowerShell **5.1.26100.9818** 对两组共 12 份脚本解析均无错误。实际生成文件的 helper ready/grant 等待均为 600 秒，wrapper 完成等待为 1,800 秒，六项 CLI 本身与其超时不变。载荷审查记录为 `independent-default-payload-review.json` 与 `independent-default-payload-ps5-parse.json`。

此次 runner 在客体身份、Medium 完整性与全部包哈希验证后请求 `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED)`，在 finally 恢复先前执行状态，并保存请求/恢复线程、时间、返回值与错误。完整验收须额外检查请求非零、同线程恢复成功且错误为空；仅六项退出 0 不足以证明这项清理成功。客体唤醒前后画面另行保存，唤醒操作本身不算鼠标/键盘产品交互验收。

8037 默认轮 **`sandbox-20261109-042201-8d605056`** 已完成六项限定验收。独立复核只选取 nonce **`fb3157459faf48f68c6d933b38914754`** 下的 `guest-70fb4c7b` 与 `live-6ed101cf`。worker PID **6356** 为 Medium 8192、非提升、会话 2、WinSta0/Default，ready、grant、verified worker 与进程启动时间一致；实际 OS 为 `25H2 / 26200.8037 / Professional`。全部 75 项输入及客体复制哈希、清单和三程序与上述默认冻结包匹配，不能混入归档内较早的 lab 目录。

| 8037 默认命令 | 退出码 | 超时 | 耗时 |
| --- | --- | --- | --- |
| compatibility | 0 | 否 | 5,225 ms |
| diagnostics | 0 | 否 | 907 ms |
| app-icon | 0 | 否 | 390 ms |
| xaml-clock | 0 | 否 | 25,247 ms |
| shell-source | 0 | 否 | 5,781 ms |
| taskbar-live | 0 | 否 | 13,326 ms |

XAML 164 项、私有宿主 6 项通过；三帧完整树和 topology 分别读回 `LT LIVE 01/02/03`、`Sandbox check A/B/C` 及宽度 **240、320、320**。本轮前后原生时钟读为 **`4:29`**、日期可见部分为 `2026/11/9`、容器为 **55×32**，文本、尺寸和可见性严格恢复。日期原始字符串共 255 个字符，包括方向标记和末尾 243 个 NUL；完整原串前后逐字符相同，验收没有仅比较去掉 NUL 的显示前缀。Explorer PID 4576、TID 2564、HWND 262352 与启动时间保持不变；clock stop、cleanup、五类托盘区域准备/停止均通过，未激活复制。同轮只读 module audit 退出 0、未超时，直接 loaded PE/PDB 身份、基址/映射与对应路径文件 SHA-256 全部匹配。

显示保持在 **04:28:57.165** 由线程 **7420** 请求，返回先前状态 `0x80000000`；**04:30:03.962** 同线程恢复原状态，返回 `0x80000003`，`restored=true`、错误为空。请求和恢复均非零，时间顺序覆盖 harness 与后置模块审计。done 正常返回，父启动器本轮 `runner_completion_observed=true`、错误为空，计划任务已删除，未再发生外层超时。

独立审查已查看 `vm-8037/display-wake-default-before.png` 为全黑，`display-wake-default-after.png` 为可见客体桌面、任务栏时间 **4:25**；`display-wake-default.json` 记录 04:25:32 对精确客体虚拟盘的 Shift 按下/释放。live 时读回的 4:29 与实际运行时段一致，补充了此前显示休眠下原生时间陈旧的观测。停止后的 `observer-0050.png` 还显示可见的原生任务栏时间 **4:34**，对应 `vm-infrastructure/observation-20261109-043433.jsonl` 的 **04:34:33.796**，截图过程未注入客体输入。这是恢复后继续走时的一次视觉观察；没有逐帧拍摄三次文字写入，也没有原生刷新回调计数，不能扩大为完整屏幕绘制、持续原生刷新或完整输入矩阵已验收。独立画面范围记录为 `vm-8037/independent-default-042201-display-review.json`。

本轮归档 `vm-8037/guest-results/sandbox-20261109-042201-8d605056.zip` 为 286,521 字节、121 个文件，SHA-256 **`f7c4c84a01dc2898d49e84e5948ad64af6ddda065c1679faed41bd450ab25124`**。独立结论保存在 `vm-8037/independent-default-042201-review.json`，关键证据哈希为：

| 8037 默认证据 | SHA-256 |
| --- | --- |
| isolated `summary.json` | `a2e4309fb91c4309671945bca13ae36d7f79e1b85231c28f14f05dacdb703e5c` |
| live `summary.json` | `98dd66ad0695fd77e06db6d60f9a6a310ab5332a33ead3e0d6788a98191140ce` |
| `taskbar-live.json` | `3352fec9760f641a7cb1621e8823b671333d329bcf2aea0fa0580cb272022ff4` |
| `runner-context` | `1a41cb29262f3ca60d240622c6b9d2f789813bd029220aad00dd66298c454d94` |
| module audit `summary.json` | `52fa0038d5bb436a683eb9d02cc7cfbb6b64902aa420751150c0134eb92be818` |
| `probe-taskbar.json` | `6e05a9b2897303fc3931fbc2b4e1e27d901d6a797953e44e3420bf8d5306ff53` |
| `launcher-final.json` | `d2210b5f15412ffc80c004a0982c513803610f891eae6213a234ca3493a7756f` |

LTSC 在默认轮启动前已自行关机。保留原盘和第一轮全部材料后，第二次启动的只读诊断 **`sandbox-20261109-043433-d2fb5f06`** 取回系统与许可事件，确认本次关机为评估许可到期触发：

| 时间 | 客体事件证据 |
| --- | --- |
| 04:16:53.216 | Application / WLMS 101：许可证期限已过期，即将关闭操作系统 |
| 04:16:53.278 | System / User32 1074：`C:\Windows\system32\wlms\wlms.exe` 代表 SYSTEM 发起计划内关机，原因代码 `0x80000000`，注释明确为许可证期限到期 |
| 04:17:06.490 | EventLog 6006：事件日志服务停止 |
| 04:17:10.701 | Kernel-Power 109：执行 Shutdown 转换 |
| 04:17:13.707 | Kernel-General 13：操作系统关闭 |

WLMS 100 还明确记录“操作系统将每小时关闭一次”；只读许可查询为 `EnterpriseSEval / TIMEBASED_EVAL`、`LicenseStatus=5`、`GracePeriodRemaining=0`。这些事件支持本次计划关机的原因，不构成更早 `033636` 首次任务栏响应失败的根因证据。后者继续保持未证实。

第二次启动使用同一 `windows.vhd`，仅从已安装系统盘启动，保留无网络、隐藏客体显示的配置；没有修改系统时间、许可或电源策略。`boot1-preservation.json` 中原 `launch.json`、`guest-com1.log`、stdout/stderr 四份文件的长度和 SHA-256 经独立复核仍相同，新的启动日志另存。第二次启动时间为 04:37:28.500；此次默认验收需要按本轮新 Explorer 身份检查内部连续性，不能要求它与关机前的 lab Explorer ID 一致。

诊断原始文件为 `vm-ltsc2024/guest-results/sandbox-20261109-043433-d2fb5f06/shutdown-event-diagnostic-sandbox-20261109-043433-d2fb5f06/shutdown-events.json`，归档 252,941 字节、136 个文件，SHA-256 **`afb30e89b89e6ce6fd3ad9922868fac2d23874c6a12c85c3ccd36dd283ee3545`**；独立结论保存在 `vm-ltsc2024/independent-shutdown-043433-review.json`。04:41:58 的客体唤醒记录和 `display-wake-default-boot2-after.png` 显示可见桌面、时间 4:41 与评估许可已过期水印。

默认首次尝试 `sandbox-20261109-042202-794f43a9` 于 04:42:46.892 开始，04:42:55.455 在 helper 前置条件失败，错误为 `Expected a high bootstrap and medium Explorer for the same interactive user/session.`。实际代码的这项检查位于 nonce、worker 目录和任务创建之前，因此本轮没有启动产品 runner。它是包括身份、完整性、会话与桌面的复合检查，仅凭报错不能确定具体哪个字段不满足，另需读取实际进程身份。

该 run ID 导出的 ZIP 与前一 `043433` 诊断逐字节相同，仍为 252,941 字节、SHA-256 `afb30e89b89e6ce6fd3ad9922868fac2d23874c6a12c85c3ccd36dd283ee3545`，没有本轮新增的结果。不能将其中累计的旧 lab 通过记录当作默认包通过；独立失败记录为 `vm-ltsc2024/independent-default-attempt-042202-review.json`，依据第二次启动的串口状态及该轮实际 helper 代码。后续修正测试启动流程需使用新的 run ID 和 nonce，默认程序包继续保持原冻结哈希。

新的测试启动入口限定 **Medium bootstrap 与 Medium Explorer 同 SID、同会话、WinSta0/Default**，记录实际 preflight 后才隐藏启动新的 worker；原 worker 分支仍独立验证 PID、启动时间、argv、token、nonce、grant 与 runner 哈希。它不创建计划任务，报告明确记为直接子进程启动，任务删除为不适用。只有测试启动流程改变，产品程序、75 项包内容、六项命令及其超时、显示保持/恢复和 loaded 审计均保持冻结版本。

最初生成的 `sandbox-20261109-044808-bbcf6abd` 未运行；审查发现生成脚本把 LF 换成 CRLF，逻辑无变化但不能称为 worker 字节不变，因此保留它并另外生成 **`sandbox-20261109-045031-4d58fae7`** 的 LF 载荷。后者独立核对原 worker 前缀逐字节相同，FullRunner 原字节仅替换 64 字节的 helper 哈希，75 项包和 ISO 的 83 个文件全部一致，PowerShell 5.1 对六脚本解析无错。其 helper SHA-256 为 `ef6a93c352fa113e3472631b45b933a6b4cac538c3377efcd842b8714d730f12`，FullRunner 为 `705c74d2bde53f771bc6976d50623ac5ae4b5dc34cc08d7881088833184cd9da`，ISO 为 `0c544882827d7f2444a54f5525bfcc9dc79770117964862c91f81b51dc4edb14`。载荷复核见 `vm-ltsc2024/independent-default-boot2-lf-payload-review.json`。

LTSC 默认轮 **`045031-4d58fae7`** 随后已通过独立验收，精确 nonce 为 **`a14d1d065a854972bb18d43d44edfdca`**，只读取其 `guest-c9daf07f`、`live-245ed125` 和同轮后置审计。preflight 实际记录 bootstrap PID **5568** 与 Explorer 均为 Medium 8192、非提升、同 SID 和会话 **1**、WinSta0/Default；隐藏子进程 **5620** 的创建 PID 与 ready、独立 worker 证据、grant、启动时间和 argv 一致。未创建计划任务，`task_created=false`、`task_deleted=null`，父启动器正常观察到 done 完成，所有启动/runner 错误为空。

实际 OS 仍为 `24H2 / 26100.1742 / EnterpriseSEval`。全部 75 项包、清单及三程序哈希与 8037 默认轮使用的冻结包一致：

| LTSC 默认命令 | 退出码 | 超时 | 耗时 |
| --- | --- | --- | --- |
| compatibility | 0 | 否 | 4,527 ms |
| diagnostics | 0 | 否 | 727 ms |
| app-icon | 0 | 否 | 376 ms |
| xaml-clock | 0 | 否 | 27,310 ms |
| shell-source | 0 | 否 | 4,545 ms |
| taskbar-live | 0 | 否 | 13,335 ms |

XAML 164 项、私有宿主 6 项通过；完整树与 topology 三帧实际读回 `LT LIVE 01/02/03`、`Sandbox check A/B/C`，宽度为 **240、320、320**。原生基线与停止后时间均为 **`4:53`**、容器 **55×32**，三节点的原始文字、尺寸、可见性和类名完全恢复。日期原始字符串含方向标记及末尾 243 个 NUL，共 255 字符，整串前后相等；其 UTF-8 SHA-256 为 `2d38344d72330f4250adfe11f738adb0e7d22faea033bf41f3b7e0d5c1c0d4fa`。独立比较覆盖完整 `clock.before/after` 以及完整树和 topology 的原始节点，不以仅供展示的清理后文字代替恢复判据。

本轮新 Explorer **PID 1092、TID 4112、主任务栏 HWND 65690**，启动时间 `/Date(1794170333045)/` 在前后和审计中均不变。clock stop、cleanup、五类托盘区域的准备/停止均通过，复制未激活。同轮后置 audit 退出 0，直接 loaded PE/PDB 身份、模块基址、映射路径及对应路径文件 SHA-256 全部匹配 LTSC 精确资料。

显示保持在 **04:52:23.837** 由线程 **5320** 请求，返回 `0x80000000`；**04:53:33.228** 同线程恢复先前状态，返回 `0x80000003`，`restored=true`、错误为空。检查与后置审计均处于保持区间，原生时间与实际运行时间一致。归档共 182 个文件、363,895 字节，SHA-256 **`29e3c23aae9b3c0b82411f1e1b4bb2d72177296e593c607afebbe788a50fcbaa`**，独立验收文件为 `vm-ltsc2024/independent-default-045031-review.json`。

这轮还从只读客体截图中逐张确认三次文字确实绘出、停止恢复和稍后走时，时间来自 `vm-infrastructure/observation-20261109-045208.jsonl`：

| 原图 | 北京时间 | 实际可见内容 |
| --- | --- | --- |
| `observer-0130.png` | 04:53:11.313 | `LT LIVE 01` / `Sandbox check A` |
| `observer-0131.png` | 04:53:13.416 | `LT LIVE 02` / `Sandbox check B` |
| `observer-0132.png` | 04:53:15.520 | `LT LIVE 03` / `Sandbox check C` |
| `observer-0134.png` | 04:53:19.727 | 原生 `4:53` / `2026/11/9` |
| `observer-0154.png` | 04:54:01.807 | 原生 `4:54` / `2026/11/9` |

这是五个实际静帧的视觉证据，截图未注入客体输入；不能外推未采样的每个时刻，也没有独立原生刷新回调计数。原图与哈希保存在 `vm-ltsc2024/independent-default-045031-display-review.json`。冻结 catalog 的 LTSC 描述仍保留先前 lab 的显示验收边界，本文补充此次默认包新增的有限视觉证据；不因此提升另外四个尚未完成的场景。

| LTSC 默认证据 | SHA-256 |
| --- | --- |
| isolated `summary.json` | `03844f6f93812b3290ed4871c199eb147575bb0277309e1b2dc37a79afb469a8` |
| live `summary.json` | `bce5d8e0c6ad93b8f54dc7c3caa389a8993db23bfb28ed22a49e96c14f83729e` |
| `taskbar-live.json` | `8e071d2817e8d6348620fae58e91d67baa8e53e520437c3afab077695763b4cf` |
| `runner-context` | `77122420a8fa2abb4b2c0dbba66784c7511d5ef50ce5de2c73bff7ca3b286642` |
| module audit `summary.json` | `30995277a49c3cfa60880b92117d0426d9632c20b0d0ee68ebf48106dfd19b7b` |
| `probe-taskbar.json` | `db2df64c9f654457b7a855a47bdbafa22e2065a761e074bd454805d60d2beac5` |
| Medium parent preflight | `09cbe3c20d3e6ab695d2485918eb5a9133d81a3b77330741194a893c2ad51fca` |
| `launcher-final.json` | `762997aa6d6d321868e9814aef0ff3c26722475079da8c9fd7d6f6b9e58e56bf` |

最终分发包补入这些验收文档时，清单哈希可以变化；程序本身应保持上述已测默认 EXE、adapter 和 popup 三份哈希。测试包的 `a1c565…67a9b` 清单始终指向本节两次实际运行的冻结内容。

上述单屏文字与恢复通过后，实际日历点击、滚轮/中键、托盘复制、菜单/悬停、多显示器和混合 DPI 仍须逐项记录。资料加入 lab、编译通过或安装程序完成均不能自动提升这些状态。
