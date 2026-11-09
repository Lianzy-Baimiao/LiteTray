# Windows 11 22H2 / 23H2 兼容记录

记录日期：2026-11-09。**5.14.2 默认支持 Windows 11 Pro 22H2 `22621.1702` 和 Pro 23H2 `22631.2861` 的精确 x64 模块，内置目录扩展至七组。** 两套实际安装映像先经静态核验，再以冻结实验包在独立客体完成六项检查，独立复核通过后移入默认目录；同一份默认包随后也在两台客体完成六项检查及独立验收。22H2 默认首轮失败和后续通过分别记录，不混用实验包、默认包或不同轮次的身份。版本文字仅描述来源；选择资料仍要求三模块完整 PE/PDB 身份和 SHA-256 一致。

## 官方介质和实际模块

两份中文 x64 ESD 来自微软产品目录，并与目录内的完整 SHA-1 一致。微软目录原始下载地址使用 HTTP；本次未绕过 TLS 证书检查。SHA-256 为本地完整文件计算结果，独立官方校验依据是产品目录的 SHA-1。

| 来源 | 文件字节数 | 官方及实际 SHA-1 | 本地 SHA-256 |
| --- | ---: | --- | --- |
| 22H2 `22621.1702.230505-1222` | 4,111,968,778 | `463e5912b10f22ffd608abafaeb79e7e2ef8c116` | `3243ba736bf35ad7249d32cb021961eccba4dc81f71e29260fc7df3fea9761ea` |
| 23H2 `22631.2861.231204-0538` | 4,936,374,662 | `6d0c727c4c83f8095e6d3cecc7afae120d0553cc` | `3f38f5ccb73f15bca7ccdb7a8914441953382860ef53ae0151d496c9ae45e41a` |

两份 ESD 的 Professional 映像均为索引 7。23H2 介质的 WIM 元数据标记公共核心 `22621.2861`；独立安装后的客体实际报告 `22631.2861 / 23H2`。另一个客体实际报告 `22621.1702 / 22H2`，不能仅用介质标签代替这项核对。提取、分段长度、ETag、校验结果及后续实验保存在 `local/compatibility-22-23/`，不随安装包分发系统介质、虚拟磁盘或微软 DLL/PDB。

| 精确资料 | taskbar.dll | twinui.pcshell.dll | Core/Taskbar.View.dll |
| --- | --- | --- | --- |
| [22H2 1702](../crates/adapter-protocol/profiles/windows11-22621.1702-x64.json) | `d39046dd6d75f693e770cfb14c50aebce7bfbe704d362735c42e0ab499b19472` | `14a91ac45b016694988f68f17eef84fab2d46196d11c1ee5bde95763ae008a03` | `1e5fe48b6418b4587544c4dbcbb526c8d0a1a9a2fbd81d2917b8c69e6b81d4be` |
| [23H2 2861](../crates/adapter-protocol/profiles/windows11-22631.2861-x64.json) | `11f795ade465af0658e2168add113e48f439e412dc869fc0d5c5758fd3c339ef` | `2def9450cf84247eb31a802de9c72fbec58a2b1a7318ec200cb68ee8816393df` | `40b885be70c8ea0212bd2f995607753f86be8464588e2fb695be4d27ee364014` |

## 共用方案和实际差异

两套实际映像都具有可验证的 `FrameHeight`、六个宿主符号、`TaskbarHost` 的 `+0x40` 元素字段及现有 `XamlExplorerHostChildV1` source 链。实际 22H2 DLL 及其 `resources.pri` 均包含时钟模板名字；`DateTimeIconContent::OnApplyTemplate` 明确以 `TimeInnerTextBlock` 和 `DateInnerTextBlock` 查询 TextBlock。先前 BASE 文件的局部差异不能代表这些实际安装映像。

两者的屏幕矩形入口为 `DragDropManager::ScreenRectForElement(FrameworkElement const&, HWND)`，x64 上依次接收不透明成员指针、隐藏 RECT 返回指针、借用的元素包装指针和 HWND。调用者保留元素所有权。新 `system_tray_drag_drop_borrowed_coordinates_x64_v1` 方案使用独立四参数 detour，在调用原函数前取得自己的接口引用，返回后只释放该引用，不清空或消费调用者包装。原有三参数按值方案保持独立。

22H2 的 96 条指令函数体与 23H2 的 114 条指令函数体分别核验。22H2 另需核验 `TransformBounds` 辅助函数、三个通过辅助函数返回包装指针的调用点，以及精确 IUnknown 析构清理；23H2 的四个调用点直接证明借用包装由调用者释放。解析器拒绝不符合完整模板、调用目标或所有权证据的文件。Rust 调用测试覆盖四参数转发、借用/按值两种释放顺序、QueryInterface 失败和原函数返回指针。

独立复核曾发现，22H2 的元素转换辅助函数把待检查的直接调用地址当成 QueryInterface 目标，未证明其语义；将该目标替换为 IUnknown 析构函数的内存突变仍被接受。修复后必须匹配确切 QueryInterface 符号、完整 27 条指令中的虚表槽 0、CFG 和内部调用，另检查 30 条指令的元素转换函数及完整 IFrameworkElement IID。原始文件未修改，失败证明保留；修复后的新候选通过，所有生产资料字段与已冻结实验包一致。相关否定测试已加入工具套件。

这些静态证据为进入隔离实验提供依据，实际拓扑、显示和停止恢复以独立客体结果为准。日历点击、滚轮、真实悬停、菜单、托盘复制、多屏及混合 DPI 分别记录，不由单屏准备成功推断。

## 实验包和安装准备

5.14.2 的 lab 包于 08:28:40 冻结，包含七组资料。清单列出 78 个文件，SHA-256 为 `038338a00ee5b3ec4182870535dbcd0aeb1a227803ea7f54902b4e3bb016c780`。两套只读实验介质的清单和全部文件、六个执行脚本均另行核对一致。

| 程序 | SHA-256 |
| --- | --- |
| LiteTray.exe | `05ef0820ba14ff2d9c2018eef47394777cce411bd544231f8e286a0f7f79cb01` |
| litetray_taskbar_adapter.dll | `f3760a76e5f43d53e7a076f357523942150b1ad739527d36c865936146b56e1a` |
| litetray_popup_hook.dll | `d24b1c961b9d35229bcef92872c248502cec4344385dae326e7d2faeced89019` |

lab 构建的 workspace 测试为 257 项通过、9 项按现有条件忽略；两个实际映像的磁盘导出都选择正确资料并匹配三模块，报告仍标记 `disk_only`，没有据此声称运行验收。Python 46 项测试通过；新规则重放 LTSC1742、8037、8457、9818 四个既有默认原始模块集、新增 22H2/23H2 及一个仅作 discovery 的 24H2 BASE 集，生产字段未变。9711 没有保留的原始 PE，本轮没有重放。结果见 `python-evidence/final-parser-verification.json`。

两个客体各使用独立的新动态 VHD，无网卡、无宿主显示窗口。原始 Pro 映像无修改导出后，由其自带 WinPE 安装；自定义启动只负责找到受约束的客体部署脚本。首次 WinPE 部署因 BIOS 注册表不提供 `SystemSerialNumber` 而在分区前停止，原脚本、ISO 和画面保留。第二版改用已经存在的 WinPE、制造商/产品、唯一介质标记、空盘、QEMU 磁盘和映像版本检查，通过后才开始写入客体新盘。此阶段的安装画面不算任务栏运行测试。

执行前预审另发现四项隔离检查在超时终止后没有回收等待上限。`Invoke-SandboxGuestCheck.ps1` 已加 10 秒回收限期，失败则写入结果并停止剩余检查，原有 60 秒检查期限不变。新版脚本 SHA-256 为 `f736f7c88aac61b60980c07fdbd1f24866052aa9e7a1e2e24ed8d8b893c75c87`，两套 v2 测试介质及其固定脚本哈希已复核，Windows PowerShell 5.1 语法检查通过。旧测试介质未执行并保留；修改没有改变冻结程序包。

独立验收核对精确 run/nonce/实例、Medium 进程、包清单、真实 OS 和加载模块、显示保持状态恢复、六项检查，以及完整时间/日期/容器对象在时钟停止和托盘停止后的恢复。原生报告里的 `passed=true` 并不覆盖全部原始字段；复核保留 NUL 和方向控制字符，不把缺失字段或两端 null 视为恢复成功。复核器先用旧 8037/LTSC 原始结果及错误样本演练，再单独检查以下新客体结果。

## 实验包真实运行与默认收录

| 项目 | 22H2 | 23H2 |
| --- | --- | --- |
| 实际客体版本 | `22621.1702 / 22H2 / Professional` | `22631.2861 / 23H2 / Professional` |
| run | `sandbox-20261109-084727-e86004a4` | `sandbox-20261109-084727-d187d2f1` |
| nonce | `d179e9ee3a38442a8ffc23f1a93f4fb3` | `2227b04fc16b406ca12a2a44fe8d40d2` |
| 隔离 / live 实例 | `guest-202e9c31` / `live-b39fc49b` | `guest-46c5cf01` / `live-7dd2040b` |
| Medium worker / session | `7116 / 2` | `5176 / 2` |
| Explorer PID / 任务栏 TID / HWND | `1976 / 6104 / 65798` | `6136 / 5796 / 65802` |
| 原生宽度 | 56 DIP | 56 DIP |
| 结果归档字节数 | 108,379 | 108,930 |
| 结果归档 SHA-256 | `f96e90ef312c80a5cd971e9dfc3ef89a6cc68c71fe3f6a5bc99c64601fbe4373` | `07e8b9b4309f75615bebd8efb053f7f8951385ad988b15594ac45f82690e69c0` |
| live 报告 SHA-256 | `0de19057f2c7673ae0e7cad8804c109a9efc9872cac7e563a2e697b1dc9a3aab` | `5ab5c327c5bb8cb53d0760cb6b0cb56cc7c1038d69cf5cacaf475f9aa37447d0` |

两者首轮实际运行均完成 compatibility、diagnostics、app-icon、XAML、private-shell-source 和 taskbar-live 六项检查，退出码全部为 0、无超时；XAML 为 164 项、私有宿主为 6 项。全部 78 项输入/客体文件及三程序哈希与冻结包一致，三个系统模块的实际加载路径、内存 PE/PDB 身份及映射路径文件 SHA-256 与对应资料一致。非提升的 Medium worker 与 Explorer 属于同一用户、会话和 WinSta0/Default；握手绑定本次 nonce、PID 与进程启动时间。

每个客体均读回三次真实时钟文字，宽度依次为 **240 → 320 → 320**。时间、日期、容器的完整原始对象满足 **before == after_clock == final after**，并与完整树节点交叉核对；两者原日期长度均为 255，包含的 243 个尾部 NUL 和方向标记未经裁剪。停止恢复至原宽度 56 DIP，clock Stop、tray Prepare/Stop 和 cleanup 均确认。Explorer 的 PID、session、启动时间以及任务栏 HWND/TID 持续一致；执行状态保持及同线程恢复成功，无错误。

独立复核七个部分均通过，两个 ZIP 的长度、SHA-256、CRC 及全部 43 个成员与解包目录逐字节一致。复核器 SHA-256 为 `80fb16b36de279ff57a5204667a38c908d1cd8e375d40ac37ed7947b556e81bf`；结果分别为 `vm-22h2/independent-lab-v2-review.json` 和 `vm-23h2/independent-lab-v2-review.json`，归档复核为 `independent-lab-v2-archive-review.json`。原始结果及部署失败记录均保留。

据此，两套精确资料从实验注册表移入默认注册表，默认与显式 `compatibility-lab` 构建均包含七组。兼容性目录只把模块/布局、单屏时钟更新/恢复和托盘准备/停止标为 `tested`，绑定资料 ID 与三个完整模块哈希，日期为 2026-11-09。

托盘本轮 `activated=false`。日历点击、滚轮、中键、tooltip hover、托盘菜单/复制、真实多屏、混合 DPI、像素绘制和独立 native refresh 计数仍未验证；本轮也未独立验收真实任务栏字体、颜色或绑定。上述三项通过不能提升其余四个场景。

## 默认包冻结与复测

默认七组构建的 workspace 测试 **259 项通过、9 项按现有条件忽略**；显式 `compatibility-lab` 协议测试 38 项、UI 后台检测/导出专项 1 项、独立 EXE 导出 5 项、隔离 XAML 164 项和异常回调生命周期检查通过。Rustfmt、Clippy 和默认 release 构建通过。首次英文布局测试发现预设提示和兼容检测按钮文案裁切，缩短英文文案并同步翻译断言后完整复测通过；原失败日志保留。

5.14.2 默认测试包于 **09:47:36** 冻结在 `default-package/`，未启用 `compatibility-lab` feature。清单列出 78 个文件，含清单自身共 79 个文件；清单 SHA-256 为 **`0e36ec4e6c02debe48a01473ee94bd2989ec300094948d0edef2fa2b00c75f6f`**。

| 程序 | 字节数 | SHA-256 |
| --- | ---: | --- |
| LiteTray.exe | 2,521,088 | `c598c0b218b406a35506c700c6f6266a3f69a37b0f1c04183465415025667a26` |
| litetray_taskbar_adapter.dll | 1,285,120 | `595d1d3c718b79b656655c2cbcb6ec3481039d3e9d1ff421a11548feb9069f51` |
| litetray_popup_hook.dll | 282,112 | `a7f3b23f4f97965347118e9aaa45bf0aec120f3087b2d66ccf905109b771d6e7` |

该默认 EXE 对两个实际映像提取树的磁盘导出均选择正确资料并匹配三模块，仍为 `disk_only`，未访问 Explorer 或网络。复测器保留原 lab 版快照，另增加显式期望构建模式（仅 `default` 或 `compatibility-lab`），必须与冻结记录一致；身份、原始恢复与进程检查全部保留。新复核器 SHA-256 为 `e27c68ed3b8260d39e8c488860eccdc25343e92667e42d566f4714afa6b24262`；两套 lab 记录回归仍通过，9 项模式匹配及拒绝检查符合预期。此处的离线匹配和复核器演练不替代后续实际默认包运行结果。

### 22H2 默认包首轮失败

`sandbox-20261109-094802-972f7054`，nonce `6f8485c475f2414ea5177800ac2ab316`，隔离实例 `guest-8adb010d`、真实检查实例 `live-1012980e`。前五项检查退出 0；`taskbar-live` 实际退出 1、未超时，返回“任务栏拒绝时钟命令，原状态可能正在恢复”。其报告开始于 `01:54:57.704722Z`、结束于 `01:55:01.8757209Z`，跨过整分钟；尚无 Start 确认或样本，不能判断是否曾有瞬时写入。

最终完整树中，原生时间由 `9:54` 变为 `9:55`，时间节点其余字段、日期和容器原始字段均相等，宽度仍为 56 DIP，未残留 `LT LIVE` 文字。`cleanup_acknowledged=true`，但最终文字严格比较失败，所以报告仍为失败；不会裁剪或忽略差异来放行。实际 Explorer 的 PID 1976、session、启动时间、任务栏 TID 6104 和 HWND 65798 在上下文与独立审计中一致。报告的 `explorer_identity_preserved=false` 同时受最终恢复检查失败影响，不能仅据该字段断言 Explorer 已重启。

整分钟变化能够解释最终文字比较差异，尚不能证明 Start 命令拒绝的原因。此时仅准备时钟，尚未进入托盘坐标入口安装。首轮没有收集到 adapter 事件日志；后续诊断另行记录。模块审计正确拒绝将未通过的 live 报告作为加载模块验收。结果 ZIP 共 168,046 字节、82 个成员（本轮失败 39 个及旧 lab 43 个），SHA-256 为 `6e766a9f35b90251c990ed09c9bdf0b384b80495a3ce2bb75cdf1b21a39ee0eb`。独立失败复核及原始归档保留，后续通过也不覆盖本轮失败。

### 23H2 默认包通过

`sandbox-20261109-094802-35dd3a69`，nonce `747a6edf01f340ca838fc2f66afa30f8`，隔离实例 `guest-4813b8fb`、真实检查实例 `live-ba1ebd69`。六项均退出 0、无超时，XAML 164 项、私有宿主 6 项通过；78 项文件和三个程序与上述默认冻结包一致。Medium worker 为 PID 2200、session 2，Explorer 仍为 PID 6136、TID 5796、HWND 65802。实际系统、模块身份/加载路径及映射文件哈希匹配，显示保持与恢复在同一线程 5392 完成。

三次真实文字与 240→320→320 DIP 宽度读回通过；时间、日期及容器完整原始对象满足 before == after_clock == final，日期中的 243 个 NUL 保留，原宽度 56 DIP 恢复。clock Stop、tray Prepare/Stop、cleanup 及 Explorer 连续性通过，复制未激活。独立七部分全部通过，报告 `vm-23h2/independent-default-review.json` SHA-256 为 `5689da39dda272ddbbd8763e53361fd18c6035f44b28755fe450edcebab019b0`。

结果 ZIP 为 217,844 字节、86 个成员（新默认 43 个及原 lab 43 个），SHA-256 为 `52bf55e8c127125ee33e0ea0486842f27b1feeb11d6bcbde82720f2f0c14e452`。归档长度、CRC、SHA-256 和各成员逐字节核验通过，默认验收严格选取新 nonce 下的实例，没有混用旧 lab 结果。

### 22H2 同一默认包的后续复测

只读诊断 `sandbox-20261109-100530-32e94bec` 确认，同一用户的 `%LOCALAPPDATA%\LiteTray` 目录及两个 adapter 日志文件均不存在；该诊断没有调用产品或创建目录。后续在已核验的 Medium/SID 与冻结包条件下，仅创建该日志目录，让程序已有的事件记录可落盘，并等待客体秒数 10–20 放行，在启动 live 子进程前再复核秒数 10–25；本轮实际于第 10 秒启动。窗口等待有独立 90 秒上限，真实检查仍使用原 120 秒期限，六项命令和全部成功/恢复断言保持不变，没有改系统时间、电源策略或重启 Explorer。

首版定时诊断介质未执行：预审发现启动后记录观测若抛错会跳过子进程回收，以及秒窗口判断顺序可能越过等待上限；修订后先核期限，观测失败也先完成原等待/清理再记录失败。新 run 为 **`sandbox-20261109-101339-e11231b3`**，未运行旧介质保留。全部 78 项包文件及三个程序仍与 09:47:36 的默认冻结包逐字节一致。

本轮 nonce **`c0753afb8da94e36b16dca1b95b7a0ce`**，隔离实例 `guest-a5ee0191`、真实实例 `live-58e104eb`。六项检查全部退出 0、无超时，Medium worker 为 PID **1932**、session **2**。显示保持与恢复均在线程 **6236** 成功完成。live 子进程 PID **6556** 实际开始于客体 `10:23:10.2547505`，报告覆盖 `02:23:10.4721626Z` 至 `02:23:20.0150002Z`，没有跨分钟。

三帧文字与 240→320→320 DIP 宽度均读回，clock Stop、tray Prepare/Stop、cleanup 和 Explorer 连续性通过。新日志只有两个 `prepared: session awaiting start` 事件，没有记录拒绝；成功写入本身不产生常规事件，仍以原始报告验收，不能仅用日志判断是否写入。首轮 Start 拒绝的精确内部错误由于当时无日志仍未证实；这次通过不等于修复了时间竞争，也不消除首轮失败。

独立复核七部分全部通过：时间、日期和容器的完整原始对象在 before、after_clock 和 final 中逐字段相同，日期原串含 243 个 NUL，宽度恢复为 56 DIP。复核报告 `vm-22h2/independent-default-timed-review.json` SHA-256 为 **`40a82fe01cd5c0041deb33ebbccf3c4b5b132a13f1b56eb3fefc9843111b88bf`**。归档为 283,655 字节，SHA-256 **`00be5a5bba32b70dd906e817d7d02f8d6f1e86f89a558790e640243cfc686cae`**，129 个成员逐字节核验及 CRC 均通过。其中本次 nonce 下有 46 个文件，另有旧 lab 43 个、首次默认失败 39 个和只读日志诊断 1 个；验收只读取明确绑定的新 nonce。

两台客体因此都完成同一默认二进制的六项限定复测。兼容目录继续只登记三个已测场景，其他四个场景保持待验收。最终便携包和安装包补入本文等文档后，文档及整体清单会变化；三个程序必须与上述已经实际测试的默认包相同。

## 原始记录索引

- `media/22h2-download-verified.json`、`media/23h2-download-verified.json`：完整介质校验。
- `image-22h2/extraction-result.json`、`image-23h2-pro/extraction-result.json`：实际映像和提取文件。
- `python-evidence/candidate-media-22H2-qi-reviewed/`、`python-evidence/candidate-media-23H2-qi-reviewed/`：完整静态候选、符号及指令证据。22H2 v1 拒绝和 v2 的复核缺口保留作历史对照。
- `profile-22h2-transcription.json`、`profile-23h2-transcription.json`：人工审阅后转写资料的源/目标哈希。
- `profile-22h2-qi-review.json`、`profile-and-callee-review*.json`：辅助调用检查修复及生产资料一致性复核。
- `22h2-topology-research.md`：实际 22H2 模板名字、PRI 和现有拓扑规则的核对。
- `host-baseline.json`：实验前宿主 Explorer 身份。
- `lab-package-freeze.json`、`root-payload-review.json`：实验包冻结、介质逐文件复核。
- `default-package-freeze.json`、`default-runtime-source-freeze.json`：默认测试包、程序和源码哈希。
- `default-workspace-tests-final.log`、`default-feature-registry-tests.log`、`default-ui-worker-test.log`、`default-cli-export.log`、`default-xaml-clock.log`：默认检查结果。
- `default-disk-reports-review.json`、`reviewer-default-mode-regression.json`：默认磁盘导出及复核器构建模式回归。
- `vm-22h2/independent-default-review.json`、`vm-22h2/independent-default-failure-analysis.json`：22H2 默认首轮失败。
- `vm-23h2/independent-default-review.json`、`independent-default-archive-review.json`：23H2 默认通过及两份归档一致性核对。
- `vm-22h2/independent-default-timed-review.json`、`vm-22h2/independent-default-timed-archive-review.json`：22H2 同一默认包后续通过及新归档核对。
- `vm-22h2/independent-default-timed-observation-review.json`：实际启动时间、日志目录与事件时间链补充核验，SHA-256 为 `8abce097036e933be9a98fce4040b13c6bdb4bf755913da2301c372d5c9e8df7`。
