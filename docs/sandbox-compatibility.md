# 在隔离系统中检查 LiteTray

Windows Sandbox 适合验证安装包在全新用户环境中的启动、磁盘兼容检查和本进程 XAML 自检，也可通过独立的显式流程验证客体主屏时钟接管与停止恢复。**它不能选择另一个 Windows 版本或指定旧 build**：沙盒环境由宿主提供，实际版本以客体采集结果为准。要比较 23H2、24H2、25H2 或同一分支的不同累计更新，应使用分别安装、保留快照的虚拟机。

这里提供生成器与客体脚本，不会在宿主启动 Windows Sandbox、创建虚拟机、启用系统功能或下载镜像。当前产品支持范围仍以 [兼容检查说明](compatibility.md) 为准，生成测试文件不代表新增版本已经适配。

2026-11-08 更新后的宿主为 Windows 11 Pro 26H2 `26300.9818`，Windows Sandbox 功能已启用；物理资源为 8 核 16 线程、约 47.1 GiB 内存。完整 Hyper-V 功能当前未启用，如选择用 Hyper-V 建立跨版本 VM，需要另行启用并可能重启。宿主已有 25H2 Pro `26200.8457`（映像 index 3）安装镜像 `E:\iso\zh-cn_windows_11_business_editions_version_25h2_updated_may_2026_x64_dvd_4e896038.iso`，约 8.6 GB；尚未核对微软官方哈希，不能将其作为已验证镜像。此记录是环境清单，不是对客体的测试结果。

更新前的 `26300.9711` 上，vGPU 开启、关闭和无映射最简配置均在进入客体桌面前返回 `0x80370106`；管理员日志记录来宾 OS 启动后约 68 秒重置关闭。DISM/SFC 组件修复成功后仍能复现。用户随后通过 Windows Update 安装 KB5127936 并重启，最简沙盒成功进入桌面并越过原失败时段。Windows Update 日志记录本次更新任务下载内容为 985,569,837 字节，约 940 MiB（0.99 GB）；5.06 GiB 是两个完整离线包的合计。完整过程见 [本机沙盒启动记录](sandbox-host-check-2026-11-08.md)。

历史记录：21:55 在 vGPU 关闭的全新沙盒中，LiteTray 5.13.1 的四项隔离检查全部完成，退出码均为 0、无超时；XAML 自检包含 164 项，包内 64 个文件通过哈希校验。此前首轮运行暴露了 Windows PowerShell 5.1 未保留进程句柄时退出码为空的问题；修复脚本后重新生成输入并完成复测，原失败记录保留。

客体实际报告为 Windows 11 Enterprise 24H2 `26100.9818`，与宿主的版本标签不同；但采集的 `taskbar.dll`、`twinui.pcshell.dll`、`SystemTray.dll` SHA-256 均与宿主相同。它仍是宿主提供的 Sandbox 映像，不能算作独立安装的 24H2 VM 或指定旧 build 的覆盖。5.13.1 当时只有旧模块的兼容资料，四项功能的磁盘状态均为 `unsupported_modules`；该轮没有执行真实接管，不能用隔离检查通过代替任务栏适配结论。这一历史结果没有被后续测试覆盖。

22:41:41–22:42:06（UTC+08:00），LiteTray 5.13.2 候选版在同一客体 build、禁网、vGPU 关闭的全新沙盒中完成显式接管流程。测试快照的 66 个文件通过哈希校验，三个系统模块完整匹配 `windows11-26300.9818-x64`；普通四项检查、私有 Shell 自检和主屏 live 检查共六个子进程均取得真实退出码 0，无超时。

| 5.13.2 本次检查 | 实际结果 |
| --- | --- |
| 隔离 XAML | 164 项通过；没有接入 Explorer |
| 私有 Shell source | 6 项通过；在应用自己的私有宿主检查 source、root、接管、恢复与失效边界，没有接入 Explorer |
| 主屏真实时钟 | 依次观察到 `LT LIVE 01/02/03` 和对应日期行；容器实际宽度为 `240 → 320 → 320` DIP |
| Stop 与恢复 | 收到停止确认，原生时间、日期、可见性、布局高度及约 55.2 DIP 的原容器宽度恢复；Explorer PID `4796`、线程 `5528`、任务栏窗口 `65704` 前后一致，原进程仍存活 |
| 托盘准备 | 五个指定区域和原生接口准备、停止通过；没有激活托盘复制，单屏模型复制为 `not_applicable_single_monitor` |
| 人工交互与多屏 | 日历点击、滚轮音量、中键静音、悬浮提示、托盘菜单及多屏仍未验证；真实进程异常退出恢复也不由本次 Stop 检查证明 |

本次 live 检查独立观察的是文字、有效布局尺寸与可见性；没有独立导出字体、颜色、tooltip 或原始绑定的恢复证据。结果中的 `interactive_settings_opened=true` 仅表示自动检查后曾打开设置，不计入交互验收。原始报告保存在 `local/compatibility-9818-20261108-222728/sandbox-live-01/results/`，包括 `sandbox-20261108-224109-25313c7b-guest-e6b9a8bd/` 的隔离结果和 `sandbox-20261108-224109-25313c7b-live-617b11a2/` 的接管结果。新档案的模块、PDB 与 ABI 依据见 [9818 兼容证据](compatibility-9818.md)。

## 生成一次可审阅的测试

先使用项目构建流程准备带 `SHA256SUMS.txt` 的解压包，再运行：

```powershell
./tools/New-SandboxCheck.ps1 -PackageDirectory './dist/LiteTray-5.13.2'
```

命令仅生成独立目录，返回 `configuration` 指向的 `.wsb`。默认输出在 `local/sandbox-checks/sandbox-日期-时间-随机值/`；也可用 `-OutputDirectory` 指定**不存在的新目录**。不会覆盖旧的运行目录。

生成器校验清单中每个文件的 SHA-256，只复制清单所列内容到 `input/package/`。路径穿越、绝对路径、ADS、重解析点和重复文件名会拒绝；复制完成后重新核验哈希。这个检查证明包内容与所选清单一致，不是发布者签名验证。

生成目录包含：

```text
LiteTray-Check.wsb                 可审阅的 Sandbox 配置
README.txt                        本次模式与映射说明
input/                            映射到客体 C:\LiteTrayInput，只读
  run.json                        本次测试配置、包哈希与固定检查清单
  Invoke-SandboxGuestCheck.ps1     客体脚本
  package/                        校验过的包快照
results/                          仅此目录向客体开放写入
```

检查 `.wsb` 与 `input/run.json` 后，由用户打开 `.wsb` 开始测试。每次客体执行都会创建新的结果子目录，关闭沙盒后这些报告仍留在宿主 `results/`；客体自己的应用、配置与临时运行文件随沙盒丢弃。

## 默认边界

下表适用于普通 `New-SandboxCheck.ps1`；后面的显式主屏接管流程使用另一入口，默认关闭 vGPU。

| 项目 | 默认值及用途 |
| --- | --- |
| 网络 | 禁用；不下载符号、天气数据或更新 |
| 剪贴板、麦克风、摄像头、打印机重定向 | 禁用 |
| vGPU | 启用，让 XAML 接近通常的图形运行条件；也可显式 `-VGpu Disable` 比较软件渲染，结果须单独记录 |
| 内存 | 4096 MiB，可用 `-MemoryInMB` 调整至 2048–16384 MiB |
| 宿主映射 | `input` 只读；只将此次新建的 `results` 目录设为可写 |
| 客体程序路径 | 先复制到客体 `%LOCALAPPDATA%\LiteTraySandboxChecks\<实例>\app` 并校验，再执行 |
| 配置 | 全新客体目录；时钟接管、托盘复制、QQ 兼容、天气、自动更新和开机启动均关闭 |
| Explorer 接管 | 默认不执行；不会调用 `--inspect-taskbar`、副屏实测或安装钩子的命令 |

vGPU 依赖宿主图形环境；关闭 vGPU 后 XAML 失败应记录为该渲染条件的失败，不直接归因于 Windows 版本。启用 vGPU 的沙盒也不等于验证了物理显卡、混合 DPI、HDR 或多屏驱动。

## 默认检查与结果

客体只运行固定的四个命令，每个进程独立记录 stdout、stderr、退出码和最长 60 秒的等待结果：

5.14.2 收紧超时后的回收：终止自己启动的隔离子进程后，最多再等 10 秒；无法回收或终止失败时，保存失败记录并停止后续检查。超时始终不算通过。这一等待上限不代表所有文件读取、输出流或客体启动步骤都有统一的总截止时间。

| 命令 | 实际覆盖范围 |
| --- | --- |
| `--compatibility-report` | 读取客体系统模块的 SHA-256、PE 与 PDB 身份，按功能给出磁盘检查结果；不加载这些系统 DLL、不接入 Explorer |
| `--diagnose` | 读取屏幕与任务栏窗口信息、路由计划和磁盘兼容报告；枚举窗口不等于进程内接管 |
| `--self-test-app-icon` | 验证包内图标资源加载 |
| `--self-test-clock` | 在 LiteTray 自己的隔离进程中创建真实 XAML 控件，检查属性、输入、会话与恢复；要求输出 `explorer_accessed=false` |

`os.json` 记录客体的实际版本、build、UBR、版本分支、架构与虚拟硬件标识。`compatibility.json` 保留各系统模块的完整身份。`summary.json` / `summary.md` 汇总执行结果，`package-verified.json` 固定所测包身份。客体桌面的 `LiteTray-Test-Results.txt` 提供结果与临时工作目录位置。

`checks_passed` 的含义是**这四个检查按预期完成**。其中磁盘报告返回 `unsupported_modules` 仍可能是正确的检查结果，并不表示兼容性通过。`disk_matched_live_check_required` 也只表示磁盘资料匹配；真实时钟、托盘操作、停止恢复和多屏检查继续保持 `not_run`。

## 后台自动完成主屏接管检查

已经完成精确模块适配、需要在一次性客体中验证真实 Explorer 时，使用独立入口：

```powershell
./tools/New-SandboxCompatibilityCheck.ps1 -PackageDirectory './dist/LiteTray-5.13.2'
```

它仍只生成配置，不启动沙盒或宿主程序。生成器复用普通流程的包快照、只读输入和独立可写结果目录，另复制 `Invoke-SandboxCompatibilityCheck.ps1`，在 `run.json` 写入 `live_taskbar_test_authorized=true`，并将启动命令指向该脚本。默认 `vGPU=Disable`、4096 MiB 内存，网络及重定向仍关闭。普通四项入口的默认行为保持独立。

打开生成的 `.wsb` 后，客体脚本在后台依次完成以下步骤，默认不打开设置，也不产生键盘或鼠标输入：

1. 调用原隔离脚本，在客体复制并核验包，建立接管、复制和 QQ 功能均关闭的新配置，运行原四项检查。每项必须 `passed=true`、实际 `exit_code=0` 且未超时。
2. 复核磁盘报告为 `disk_only`、`explorer_accessed=false`、`all_profile_images_matched=true`，并具有明确的 `profile_id`；复核 XAML 自检的隔离边界。普通流程允许正常报告“不支持”，这里会据此终止，禁止继续接管。
3. 执行 `--self-test-shell-source`，最长 90 秒；要求实际退出码 0、`passed=true`、`explorer_accessed=false`，环境为应用进程内的私有 Shell 宿主，且 `profile_id` 与磁盘报告相同。
4. 前置检查全部通过后，执行 `--test-taskbar-compatibility <新报告.json>`，最长 120 秒。它记录主屏原生基线，接管后更新文字和 `240 → 320` DIP 宽度，Stop 后重新读取原生文字、布局和可见性，并核验原 Explorer 的进程、线程与窗口身份。随后只 Prepare/Stop 托盘接口和五个区域，不创建副屏复制目标。
5. 只有真实退出码 0、报告通过、停止确认、恢复观察及 Explorer 身份保持均满足，才将 live 汇总标记为 `checks_passed`。默认流程到此结束，时钟会话已经恢复；超时只终止 LiteTray 测试子进程，不重启 Explorer，恢复状态保持未确认。

每次运行产生独立的 `*-guest-*` 与 `*-live-*` 结果目录；live `summary.json` 的 `baseline_results` 指向四项前置报告。`shell-source.stdout.json`、`taskbar-live.json` 和各进程 stdout/stderr 保留实际证据，存在的事件日志会一起复制。live 汇总的通过范围是**主屏时钟接管与 Stop 恢复、托盘准备**，不包括托盘模型复制、真实鼠标交互或多屏。

需要人工操作时才额外传入 `-InteractiveAfterChecks`；它只在自动流程全部通过后打开设置，初始接管功能仍关闭。后台检查不需要此选项，打开设置也不会改变未验证项目的状态。两个客体脚本默认都要求 `WDAGUtilityAccount`；将 live 输入复制到一次性 VM 后，可显式选择 `-EnvironmentType VirtualMachine`，不能在宿主执行该接管脚本。

## 显式选择交互阶段

```powershell
./tools/New-SandboxCheck.ps1 -PackageDirectory './dist/LiteTray-5.13.2' -Mode Interactive
```

此模式完成上述检查后打开完整设置界面，初始配置仍关闭时钟接管、托盘复制与 QQ 兼容。用户可以在客体里逐项开启要验证的功能并保存。界面打开不算接管成功；未知系统模块被拒绝时应记录为不支持，不修改哈希、偏移或兼容资料来强行接入。

依次记录时钟显示、空模板悬停、滚轮音量、中键静音、点击日历、托盘左右键与悬停、禁用功能和退出后的原生恢复。升级、Explorer 重启与设备变更测试放在独立快照中进行。发生崩溃时保留客体日志及应用事件记录，结果标记失败。

关闭沙盒前可从设置中的“导出诊断信息”生成交互阶段报告，再从客体工作目录中的 `profile` 将实际存在的 `diagnostics.json`、`taskbar-events.log` 复制到 `C:\LiteTrayResults` 下本次结果目录。Explorer 内的适配器日志单独位于客体 `%LOCALAPPDATA%\LiteTray\adapter-events.log`，其中也包含异常记录；发生接管后再检查该文件。报告中的自动检查结论不会因为手动操作而自动改为通过，需要在矩阵里登记场景、结果和证据。

Sandbox 不能代表主副屏实机验收；不要通过调整沙盒窗口尺寸来冒充多屏。需要多屏时选择支持多个虚拟显示输出的虚拟机，或隔离测试机，并记录实际屏幕枚举、DPI 与任务栏布局。

## 真正的跨版本矩阵

[JSON 矩阵模板](compatibility-matrix.template.json) 中所有结果初始为 `not_run`。每个 VM 分支至少保留“干净系统”和“准备开始接管验证”两个快照；从可信系统镜像独立安装，将镜像、累计更新与测试包哈希一起登记。

首台 VM 建议选择已有的 Windows 11 25H2 镜像，在核验镜像来源与哈希后使用 **Generation 2、4 vCPU、8 GiB 内存、80 GiB 虚拟磁盘、Secure Boot 与 vTPM**。先完成全新安装与更新版本登记，保存干净快照，再复制测试输入。此文档不自动启用 Hyper-V、创建磁盘或安装系统。

| 环境 | 预期 build 家族 | 实际 build.UBR | 隔离检查 | 时钟/提示/音量 | 托盘/系统菜单 | 停止/退出恢复 | 升级/重启 | 多屏 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Sandbox，5.13.1 历史（21:55） | 不指定，以客体为准 | 26100.9818（客体显示 24H2） | 四项完成，vGPU 关闭；该包模块资料不支持 | 该轮未运行 | 该轮未运行 | 该轮未运行 | 未运行 | 未运行 |
| Sandbox，5.13.2 候选（22:42） | 不指定，以客体为准 | 26100.9818（客体显示 24H2） | 四项及私有 Shell 6 项通过；新资料完整匹配，vGPU 关闭 | 主屏文字、宽度通过；提示、音量未验证 | 五区准备通过；复制与菜单未验证 | Stop 恢复通过、PID 保持；异常退出未测 | 未运行 | 未运行 |
| Windows 11 23H2 VM | 22631 | 待采集 | 未运行 | 未运行 | 未运行 | 未运行 | 未运行 | 未运行 |
| Windows 11 24H2 VM | 26100 | 待采集 | 未运行 | 未运行 | 未运行 | 未运行 | 未运行 | 未运行 |
| Windows 11 25H2 VM | 26200 | 待采集 | 未运行 | 未运行 | 未运行 | 未运行 | 未运行 | 未运行 |

可在已经准备好的 VM 中复用同一输入快照。把生成目录里的 `input` 复制到客体 `C:\LiteTrayInput`，在客体创建独立的结果目录，然后**只在 VM 内**执行：

```powershell
New-Item -ItemType Directory -Path C:\LiteTrayResults
powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\LiteTrayInput\Invoke-SandboxGuestCheck.ps1 -EnvironmentType VirtualMachine
```

VM 的网络和共享设置由其虚拟化平台控制，`.wsb` 的禁网配置不会应用到 VM；使用同样的离线环境和最小共享目录。客体脚本默认检查 `WDAGUtilityAccount` 以减少误在宿主执行的情况，`VirtualMachine` 是操作者显式选择，不是自动判断或安全认证。

当前包为 x64。ARM64 Explorer 需要对应原生适配器和独立验证，不能把 x64 仿真进程中的隔离检查记为 ARM64 任务栏兼容。

微软参考资料：[Windows Sandbox 概述](https://learn.microsoft.com/windows/security/application-security/application-isolation/windows-sandbox/) · [.wsb 配置](https://learn.microsoft.com/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-configure-using-wsb-file)。
