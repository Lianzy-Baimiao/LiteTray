# Explorer 生命周期与未完成功能审计（2026-11-14）

本轮先发布 [v5.16.4](https://github.com/Lianzy-Baimiao/LiteTray/releases/tag/v5.16.4)，再从发布提交 `af641d978a4a8c286cddb7700ee87a21eaa9c165` 建立独立工作目录调查和修复，最终代码归入 `main`。**下述安全修复属于未发布的后续代码，不在已发布的 5.16.4 安装包内。**

5.16.4 的安装包和便携包已公开，上传后的大小和 GitHub SHA-256 均与本地产物一致。发布提交的 Windows、candidate-tool CI 最终均通过；Windows 首轮有一次设置冷导航耗时失败，同一提交重跑后通过，失败日志保留，没有修改耗时标准。

上一轮定位菜单重入等五处可复现的缺陷，本次继续修复迁移多重失败、显示参数丢失、根分类与跨重启保护。其中菜单恢复回调能够使承载进程以 `0xC0000409` 终止；其他回归证明命令丢失、状态污染、原始调用被吞、旧树被保留以及恢复预算被重置。测试均使用私有进程、COM 夹具或自建 XAML 窗口；没有安装新版、启动正常后台实例、注入当前 Explorer 或重启 Explorer。

## 已复现并修复的缺陷

| 问题 | 复现证据与影响 | 本分支的修复 |
| --- | --- | --- |
| 菜单恢复期间原生回调重入 | `restore → IFlyoutBase::Hide → Closing/Show → prepare` 再次借用同一个 `RefCell<Menus>`，出现 `RefCell already borrowed`。私有原生回调进程连续两次以 `3221226505 / 0xC0000409` 终止。适配器运行在 Explorer 内时，此类不可展开 ABI 边界的 panic 可以终止 Explorer。 | 菜单准备和停止恢复共用 RAII 重入门；门在查询根等 COM 调用之前取得，嵌套调用返回错误，防御性使用 `try_borrow_mut`。正常返回和错误返回都会释放门。 |
| 停止后的常驻菜单钩子吞掉系统调用 | 菜单 detour 存活至进程结束。停止会话后，共享目标仍走准备逻辑，失败直接返回 0。真实 detour 配合私有 XAML 目标连续两次得到 `result=0, called=false`，而原入口应返回 `S_FALSE`。 | 无活跃会话时立即转发原入口；普通与 options 两种重载保留全部参数和 HRESULT。 |
| 首次副屏使用改写缓存菜单的原生根 | 未使用的系统缓存菜单接受 `SetXamlRoot`，旧逻辑因此将它直接绑定到副屏。两个真实私有 XAML island 的回归连续两次读回被改写的根；失败发生在停止之前。这是确定的状态污染，尚不是一次真实 Explorer 崩溃复现。 | 原生菜单已有同一个根时继续原始显示。未使用的缓存仅在原生/未管理的目标上允许原始首次显示；复制根从第一次使用就借用根专属代理。只修改代理的根，停止后原生菜单仍保留原来的主屏根或 null。 |
| 退出后代理缓存持有废弃 XAML 根 | 原生项目已归还，但线程局部代理缓存一直保留 `root` 和 `menu`。引用计数夹具在停止后读到 2，预期为 1。可证明延长旧树的生命周期，不能据此估计现场内存增长或归因整机卡死。 | 停止使用 `Menus::retire`：先成功恢复项目，再释放代理缓存。原生 Hide 失败时保留命令归属和根，等待恢复重试。普通菜单切换仍可复用同一会话的代理。 |
| Explorer/环境通知清空失败与暂停保护 | `TaskbarCreated` 等环境通知调用重新初始化恢复状态的路径。三次短时间原生进程退出后仍是 `Waiting`，预期为 `Suspended`；额外通知也能重新打开已经暂停的连接。 | 环境变化与显式用户重试分开。只用原先保留的进程句柄做零等待退出观察，保留失败次数、等待期限、恢复失败预算和暂停状态。三次短暂退出后暂停；普通显示变化不增加退出失败计数。 |
| 菜单迁移与回滚连续失败丢失原生命令 | 真实迁移入口的 COM 向量故障注入：目标 Append 失败、回滚 Clear 与原集合 Append 再失败，三个命令中两个被释放；源 Clear 已执行后报告失败则释放全部三个。归还部分插入后报告失败、回滚再失败，重试只剩两个命令。 | 在第一次修改前把集合、原菜单与全部命令存入 `Loan`。所有 Clear/Append 检查 HRESULT；归还失败保持所有权。每次重试核对实际前缀，继续剩余命令；系统重建的外部命令优先，无法确认的自有命令乱序保持待恢复状态。身份查询和根缓存有数量上限。 |
| 活跃代理吞掉 ShowAt options 重载 | 生产代理显示入口传入 options 时仍调用基本 ShowAt，夹具收到基本重载的 `S_FALSE`，而 options 重载应收到原目标/options 并返回 `E_ACCESSDENIED`。 | 使用由 Windows metadata 生成的 `IFlyoutBase5` 三参数 ABI，保持原目标接口指针、options（包括传入 null）和 HRESULT；无 options 时保留基本重载。 |
| 原生普通 Flyout 首次使用与缓存代理根判断不完整 | 未设置根的原生主屏普通 Flyout 被当作需要借用项目的 MenuFlyout 而拒绝。已知代理则对另一个根直接返回“原生可显示”，放行了潜在危险的跨根 ShowAt。 | 在重入门内按已复制的副屏根分类；原生首次显示由原入口建立缓存。已知代理只允许自己的根，身份查询失败也不能成为放行依据。普通 Flyout 的不同根内容代理仍保守拒绝。 |
| 菜单祖先遍历超限绕过跨根保护 | 私有真实 XAML 树在共享 MainStack 下建立 35 层目标，32 步遍历未到达共享祖先却返回“普通目标”，使后续根保护不被执行。独立宿主得到明确失败结果，`explorer_accessed=false`。 | 超限与 COM 读取错误均视为无法确认，而不是普通目标。未执行 ShowAt 的拒绝路径保持原有 S_OK 抑制语义并记录有界内存诊断；真正执行 ShowAt 时继续保留实际 HRESULT。 |
| 连续失败保护不跨 LiteTray 重启 | 无持久许可的入口在三个私有宿主进程直接退出后仍允许下一次接入；故障后的正常恢复也无法保留跨宿主失败预算。 | 后台 preflight 在 native hook 接入前完成保护记录的写入、同步和原子替换，许可随 Prepared/Connection 持有。三次失败或未确认结束后立即暂停；恢复成功不抹掉已发生的失败，稳定运行 60 秒清除旧失败并保留当前标记。显式重试/重新启用可重置当前身份，环境通知不能重置。 |

实现与回归入口：

- [菜单准备、停止和原始调用转发](../crates/taskbar-adapter/src/tray_interaction.rs)、[菜单代理所有权](../crates/taskbar-adapter/src/tray_menu.rs)。
- [原生回调重入与对象释放回归](../crates/taskbar-adapter/src/tray_menu_reentry_checks.rs)、[停止后真实 detour 回归](../crates/taskbar-adapter/src/tray_menu_lifecycle_checks.rs)、[两岛缓存菜单根与命令身份回归](../crates/taskbar-adapter/src/tray_menu_root_checks.rs)。
- [恢复状态机](../src/recovery.rs)、[环境通知与失败预算回归](../src/taskbar_service.rs)、[只读原生进程句柄观察](../src/platform/taskbar_clock.rs)。
- [多重迁移故障与命令身份回归](../crates/taskbar-adapter/src/tray_menu_transfer_checks.rs)、[代理显示重载与参数回归](../crates/taskbar-adapter/src/tray_flyout_options_checks.rs)。
- [持久许可与后台写入](../src/taskbar_safety.rs)、[异常/正常宿主退出、满额及写盘阻塞回归](../src/taskbar_safety_checks.rs)、[后台 DLL 准备与锁定](../src/platform/dll.rs)。

原生进程退出或持久标记未清除只是保守的暂停信号，不能识别退出原因，也不等于适配器崩溃证据。保护按软件版本、已验证适配器 SHA-256、宿主模块/布局和启用能力生成身份；不同身份不继承旧身份的暂停。文件为数据目录中的独立 `taskbar-safety.json`，不修改 config/agenda。

日志最多 16 KiB、8 个身份，消息队列最多 8 项。正常确认停止清除当前标记，未接入的旧 Prepared 释放许可时取消标记；迟到的取消/故障消息不能修改新许可。记录损坏、无法写入、更新队列丢失或接入前落盘超时均暂停接入。满额时自动流程仅淘汰已正常结束且没有失败预算的旧身份；显式重试可以淘汰最旧身份，损坏的日志也需显式重试后重建。

接入前的磁盘操作和 DLL 哈希/复制/校验只在后台执行。Native 故障、健康及恢复确认仅非阻塞地排队；全队列和被阻塞的写入器回归要求这些入口在 100 毫秒内返回。消息循环结束、COM 清理完成后，主程序给保护日志合计最多 200 毫秒排空；超时继续退出，并保留未确认标记。突然关机、终止进程或严重 I/O 延迟仍可能留下保守标记，因此暂停提示不宣称磁盘损坏或软件崩溃。

## 验证结果与复现入口

修复前后的原始结果保存在本机 `E:\AI开发\LiteTray\local\explorer-audit-20261114`。失败证据保留，没有用后续成功覆盖：

| 回归 | 修复前 | 修复后 |
| --- | --- | --- |
| Hide 回调借用重入与承载进程存活 | `menu-reentry-red.log`、`menu-reentry-red-repeat.log` | 同一测试通过，私有子进程正常结束 |
| 停止后两种原始菜单调用 | `inactive-menu-red.stderr.log`、`inactive-menu-red-repeat.stderr.log` | 私有原生自检确认原入口、参数和 `S_FALSE` |
| 首次副屏准备不改写缓存根 | `native-root-red-1.stderr.log`、`native-root-red-2.stderr.log` | 两种初始根、代理命令身份、停止归还及主屏再准备均通过 |
| 停止释放旧根；失败保留命令归属 | `retired-menu-root-red.log` | 引用计数、Hide 失败后的保留与恢复重试均通过 |
| 环境通知不抹掉连续失败 | `process-exit-budget-red.log`、`process-exit-budget-red-repeat.log` | 三次退出后暂停，额外通知维持暂停；显式重试可恢复 |
| 迁移及回滚连续失败 | `menu-transfer-red.log`、`menu-transfer-red-repeat.log` | 同一入口保留三项命令、顺序与归还责任，部分返回重试不重复 |
| options 参数与原生首次使用 | `flyout-options-red.log`、`native-first-use-red.log`、`proxy-root-mismatch-red.log` | 真实代理显示入口及私有 XAML 根回归通过 |
| 祖先超限与无操作 HRESULT | `native-ancestor-connected-red.log`、`menu-suppressed-hresult-red.log` | 深层目标不能绕过保护，拒绝的代理不调用 ShowAt 并保持原有无操作返回值 |
| 持久许可与异常退出 | `restart-journal-red.log` | 三个私有进程退出后暂停；五次确认停止的私有进程退出不消耗预算；故障恢复、显式重试、迟到消息和阻塞写入均覆盖 |
| 历史身份满额 | `journal-full-history-red.log` | 自动接入暂停，显式重试可接入新身份，数量仍限制为八项 |

本次完整 workspace all-targets 共 **381 项通过、0 项失败**：宿主库 185 项、原生界面等二进制测试 82 项、协议 69 项、事件日志 4 项、popup hook 1 项、任务栏适配器 40 项。所有进程级参与者由父测试实际启动，原有显式联网检查继续保留 ignored。`cargo fmt --all -- --check`、workspace all-targets Clippy（`-D warnings`）及 workspace Release 构建通过，原始输出分别为 `final-all-targets.log`、`final-clippy.log` 和 `final-release-build.log`。所有本机构建串行使用 `--locked --offline -j 1`。

祖先超限的初版夹具未连接 XAML island，不能证明真实父链边界；它的日志保留用于说明夹具修正，不作为该缺陷的生产路径证据。最终复现由 ModelFixture 的真实 source、完成布局的 35 层目标进入原函数，并先验证 XamlRoot 非空。`native-ancestor-connected-red.log` 得到 `Ok(false)`；相同已连接场景在修复后通过。

```powershell
& .\tools\Invoke-Cargo.ps1 -CargoArguments @(
  'test', '--workspace', '--all-targets', '--locked', '--offline', '-j', '1',
  '--', '--test-threads=1'
)

& .\tools\Invoke-Cargo.ps1 -CargoArguments @(
  'test', '-p', 'litetray-taskbar-adapter',
  'tray_menu::reentry_checks::native_hide_reentry',
  '--locked', '--offline', '-j', '1', '--', '--nocapture', '--test-threads=1'
)

& .\tools\Invoke-Cargo.ps1 -CargoArguments @(
  'test', '--lib',
  'explorer_restart_notifications_cannot_erase_short_lived_process_exit_failures',
  '--locked', '--offline', '-j', '1', '--', '--nocapture', '--test-threads=1'
)
```

私有原生菜单检查通过 `--self-test-clock` 或 `tools/Test-XamlClock.ps1 -BinaryPath <private-directory>\LiteTrayAuditHost.exe` 调用真实实现。本次调试版 **193 项通过**；最终 Release 在两个独立进程中连续两次各通过 **193 项**，响应均明确包含 `explorer_accessed=false`。根所有权和祖先边界使用真实 XAML；显示重载通过生产代理显示函数进入受控 COM ABI。根测试只准备、读取和归还菜单，不调用系统 ShowAt；“主屏再准备”通过不等于实际系统菜单位置、点击或完整通知中心验收。最终结果为 `native-final-release-1.stdout.json`、`native-final-release-2.stdout.json` 与 `native-final-release-receipts.json`。上一轮各 185 项的记录也保留，未被覆盖。

## 当前代码中仍未完成或受限的功能

调查按现有生产调用链确认状态，历史进度文档中的“尚未实现”不直接作为当前结论。模块能力协商、小时和多日天气、农历显示等已有实现，不列为未完成功能。

| 优先级 | 项目与当前状态 | 代码/规格依据及下一步 |
| --- | --- | --- |
| P1 | 不同根的共享普通 Flyout 完整内容代理尚未覆盖 | 原生同根与主屏首次显示、菜单代理 options 已修复。普通 Flyout 的 Content 可能持有 UIElement、binding、style 和布局；MenuFlyoutItem 借用无法提供通用内容代理。不同根场景继续拒绝，后续须单独设计并在真实客体测试。 |
| P1 | “有系统通知时点击可能崩溃”的现场反馈仍未结案 | [兼容页面](../src/ui/compatibility.rs)仍保留 `notification_click_crash_report_unresolved`；本轮菜单回调失败不能自动替代该现场复现。补齐有/无通知、铃铛/时钟独立点击、快切屏和正常/异常退出的真实栈与菜单行为。 |
| P1 | 跨版本热升级时驻留钩子的并存尚未验收 | [钩子模块](../crates/taskbar-adapter/src/hooks.rs)有意保留进程终身的跳板和菜单 detour，加载的适配器会固定至承载进程结束。干净私有进程中的停止转发检查不覆盖已经加载旧 DLL 的 Explorer；需要独立测试旧/新版本钩子链、缓存与激活身份。 |
| P2 | 控制中心选择与组件作用范围没有完整接入生产通道 | [配置](../src/config.rs)与[迁移](../src/migration.rs)保存 `show_control_center`、`copied_taskbars_only`；[一般路由](../src/routing.rs)也支持独立参数。但 [生产 preflight](../src/platform/taskbar_clock.rs)固定控制中心偏好与副屏规则，[Prepare 协议](../crates/adapter-protocol/src/taskbar.rs)仅运送 `mirror_icons`，把控制中心与图标复制绑定，并固定 `copied_taskbars_only=true`。控制中心不是完全没有复制实现，欠缺的是独立设置和端到端一致的规则。 |
| P2 | ARM64 原生 Explorer 适配没有实现 | 现有 DLL、指令检查和调用约定面向 x64。见 [ARM64 环境记录](windows11-arm64-test-environment.md)。x64 程序能启动不能证明可以把 x64 DLL 加载到 ARM64 Explorer。 |
| P2 | 独立签名的在线兼容资料包尚未实现 | [兼容计划](windows11-compatibility-plan.md)描述独立签名资料包；当前内置目录与实现识别已经存在，但没有该在线校验、分发和激活通道。新 ABI 仍需更新软件。 |
| P3 | 空气质量、网速/硬件/媒体扩展与农历纪念日重复 | [实现规格](implementation-plan.md)列出扩展内容；当前天气模型没有 AQI，未见这些指标的采集/界面通道。[日程模型](../src/organizer.rs)支持公历 yearly 等重复，[使用文档](usage.md)明确农历生日/纪念日重复尚未实现。 |

实际 Windows 场景的验收仍需覆盖主副屏、混合 DPI、自动隐藏、主题变化、物理热插拔、睡眠恢复、触摸/笔、第三方悬停、原生菜单和通知点击。不同系统版本的既有单屏准备/恢复证据不代表这些场景完成；现有兼容标记没有因此扩大。

## 原卡顿事件的归因边界

此前现场证据为 14:09–14:10 间的五次 `stornvme` 129 控制器重置、小写入耗时 28–36 秒以及 LiteTray 通信超时/恢复记录，没有同步的 Explorer 崩溃栈、GPU TDR 或逐进程 I/O 跟踪。它们不能证明物理硬盘损坏，也不能把整机卡死全部归给本轮发现的菜单缺陷。

本轮补上了确定的软件失败路径、持久暂停和菜单所有权/参数回归，不将缺少现场数据的问题包装为已经归因。下一步是在可恢复的真实 Windows 测试环境补齐通知点击、普通 Flyout 内容代理、驻留旧版钩子与完整交互矩阵。

预防这些缺陷的具体接口原则是：可能回调的菜单准备与恢复共用一个入口门；原生命令和系统缓存根有明确的归还责任；环境通知只改变路由有效性，不能隐式重置用户可见的安全暂停。
