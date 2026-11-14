# Explorer 生命周期与未完成功能审计（2026-11-14）

本轮先发布 [v5.16.4](https://github.com/Lianzy-Baimiao/LiteTray/releases/tag/v5.16.4)，再从发布提交 `af641d978a4a8c286cddb7700ee87a21eaa9c165` 建立 `codex/explorer-safety-audit` 分支调查。**下述安全修复属于后续版本，不在已发布的 5.16.4 安装包内。**

5.16.4 的安装包和便携包已公开，上传后的大小和 GitHub SHA-256 均与本地产物一致。发布提交的 Windows、candidate-tool CI 最终均通过；Windows 首轮有一次设置冷导航耗时失败，同一提交重跑后通过，失败日志保留，没有修改耗时标准。

本轮定位五处可复现的代码缺陷。其中菜单恢复回调能够使承载进程以 `0xC0000409` 终止；其余测试证明菜单状态污染、原始调用被吞、旧树被保留以及恢复预算被重置。测试均使用私有进程、COM 夹具或自建 XAML 窗口；没有安装新版、启动正常后台实例、注入当前 Explorer 或重启 Explorer。

## 已复现并修复的缺陷

| 问题 | 复现证据与影响 | 本分支的修复 |
| --- | --- | --- |
| 菜单恢复期间原生回调重入 | `restore → IFlyoutBase::Hide → Closing/Show → prepare` 再次借用同一个 `RefCell<Menus>`，出现 `RefCell already borrowed`。私有原生回调进程连续两次以 `3221226505 / 0xC0000409` 终止。适配器运行在 Explorer 内时，此类不可展开 ABI 边界的 panic 可以终止 Explorer。 | 菜单准备和停止恢复共用 RAII 重入门；门在查询根等 COM 调用之前取得，嵌套调用返回错误，防御性使用 `try_borrow_mut`。正常返回和错误返回都会释放门。 |
| 停止后的常驻菜单钩子吞掉系统调用 | 菜单 detour 存活至进程结束。停止会话后，共享目标仍走准备逻辑，失败直接返回 0。真实 detour 配合私有 XAML 目标连续两次得到 `result=0, called=false`，而原入口应返回 `S_FALSE`。 | 无活跃会话时立即转发原入口；普通与 options 两种重载保留全部参数和 HRESULT。 |
| 首次副屏使用改写缓存菜单的原生根 | 未使用的系统缓存菜单接受 `SetXamlRoot`，旧逻辑因此将它直接绑定到副屏。两个真实私有 XAML island 的回归连续两次读回被改写的根；失败发生在停止之前。这是确定的状态污染，尚不是一次真实 Explorer 崩溃复现。 | 原生菜单已有同一个根时继续原始显示；其他情况从第一次使用就借用根专属代理。只修改代理的根，停止后原生菜单仍保留原来的主屏根或 null。 |
| 退出后代理缓存持有废弃 XAML 根 | 原生项目已归还，但线程局部代理缓存一直保留 `root` 和 `menu`。引用计数夹具在停止后读到 2，预期为 1。可证明延长旧树的生命周期，不能据此估计现场内存增长或归因整机卡死。 | 停止使用 `Menus::retire`：先成功恢复项目，再释放代理缓存。原生 Hide 失败时保留命令归属和根，等待恢复重试。普通菜单切换仍可复用同一会话的代理。 |
| Explorer/环境通知清空失败与暂停保护 | `TaskbarCreated` 等环境通知调用重新初始化恢复状态的路径。三次短时间原生进程退出后仍是 `Waiting`，预期为 `Suspended`；额外通知也能重新打开已经暂停的连接。 | 环境变化与显式用户重试分开。只用原先保留的进程句柄做零等待退出观察，保留失败次数、等待期限、恢复失败预算和暂停状态。三次短暂退出后暂停；普通显示变化不增加退出失败计数。 |

实现与回归入口：

- [菜单准备、停止和原始调用转发](../crates/taskbar-adapter/src/tray_interaction.rs)、[菜单代理所有权](../crates/taskbar-adapter/src/tray_menu.rs)。
- [原生回调重入与对象释放回归](../crates/taskbar-adapter/src/tray_menu_reentry_checks.rs)、[停止后真实 detour 回归](../crates/taskbar-adapter/src/tray_menu_lifecycle_checks.rs)、[两岛缓存菜单根与命令身份回归](../crates/taskbar-adapter/src/tray_menu_root_checks.rs)。
- [恢复状态机](../src/recovery.rs)、[环境通知与失败预算回归](../src/taskbar_service.rs)、[只读原生进程句柄观察](../src/platform/taskbar_clock.rs)。

原生进程退出只是保守的暂停信号，不能识别退出的真正原因。当前修复仅保留正在运行的 LiteTray 宿主中的预算；显式用户重试仍可以重新连接。

## 验证结果与复现入口

修复前后的原始结果保存在本机 `E:\AI开发\LiteTray\local\explorer-audit-20261114`。失败证据保留，没有用后续成功覆盖：

| 回归 | 修复前 | 修复后 |
| --- | --- | --- |
| Hide 回调借用重入与承载进程存活 | `menu-reentry-red.log`、`menu-reentry-red-repeat.log` | 同一测试通过，私有子进程正常结束 |
| 停止后两种原始菜单调用 | `inactive-menu-red.stderr.log`、`inactive-menu-red-repeat.stderr.log` | 私有原生自检确认原入口、参数和 `S_FALSE` |
| 首次副屏准备不改写缓存根 | `native-root-red-1.stderr.log`、`native-root-red-2.stderr.log` | 两种初始根、代理命令身份、停止归还及主屏再准备均通过 |
| 停止释放旧根；失败保留命令归属 | `retired-menu-root-red.log` | 引用计数、Hide 失败后的保留与恢复重试均通过 |
| 环境通知不抹掉连续失败 | `process-exit-budget-red.log`、`process-exit-budget-red-repeat.log` | 三次退出后暂停，额外通知维持暂停；显式重试可恢复 |

修复后的 Rust 检查：宿主库 170 项、适配器 30 项、事件日志 4 项，共 **204 项通过**。两个进程级测试参与者标为 ignored，由各自父测试实际启动；另外保留了原有显式联网检查的 ignored 状态。`cargo fmt --all -- --check`、workspace all-targets Clippy（`-D warnings`）和宿主/适配器 Release 构建通过。

```powershell
& .\tools\Invoke-Cargo.ps1 -CargoArguments @(
  'test', '--lib', '-p', 'litetray', '-p', 'litetray-taskbar-adapter',
  '-p', 'litetray-event-log', '--locked', '--offline', '-j', '1',
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

私有原生菜单检查通过 `--self-test-clock` 或 `tools/Test-XamlClock.ps1 -BinaryPath <private-directory>\LiteTrayAuditHost.exe` 调用真实实现。调试版 185 项通过；最终 Release 在两个独立进程中连续两次各通过 185 项，响应均明确包含 `explorer_accessed=false`。根所有权测试只准备、读取和归还菜单，不调用系统 ShowAt；“主屏再准备”通过不等于实际系统菜单位置、点击或完整通知中心验收。最终 Release 自检结果保存在 `native-safety-release-*.stdout.json` 与 `native-safety-release-status.json`。

## 当前代码中仍未完成或受限的功能

调查按现有生产调用链确认状态，历史进度文档中的“尚未实现”不直接作为当前结论。模块能力协商、小时和多日天气、农历显示等已有实现，不列为未完成功能。

| 优先级 | 项目与当前状态 | 代码/规格依据及下一步 |
| --- | --- | --- |
| P1 | 跨 LiteTray 重启的连续失败熔断缺失 | [兼容计划](windows11-compatibility-plan.md)要求跨重启另设持久状态。当前 `Recovery` 在宿主创建时新建，配置没有失败记录。需要按模块身份、ABI 和时间保存保守的暂停状态，区分显式用户重试与异常终止证据。 |
| P1 | 共享普通 Flyout 与代理菜单的完整原生显示尚未覆盖 | `show` 同时拦截 MenuFlyout/Flyout，但 `Menus` 的借用要求 `IMenuFlyout`。不同根的普通 Flyout 会被拒绝，无法靠移动 MenuFlyoutItem 完成通用适配。代理分支也只调用基本 `ShowAt`，没有转交传入的 options。应分别设计普通 flyout 内容归属和菜单定位参数的生命周期，并在真实客体测试。 |
| P1 | 菜单项目迁移的多重失败恢复没有完整保证 | `move_items` 的 Append 失败回滚没有检查目标 Clear 和原集合 Append 的 HRESULT。本轮尚未构造这个多重故障；仅登记确切的错误处理缺口，不声称已复现命令丢失或 Explorer 崩溃。需要可保留待归还命令的所有权模型及故障注入回归。 |
| P1 | “有系统通知时点击可能崩溃”的现场反馈仍未结案 | [兼容页面](../src/ui/compatibility.rs)仍保留 `notification_click_crash_report_unresolved`；本轮菜单回调失败不能自动替代该现场复现。补齐有/无通知、铃铛/时钟独立点击、快切屏和正常/异常退出的真实栈与菜单行为。 |
| P1 | 跨版本热升级时驻留钩子的并存尚未验收 | [钩子模块](../crates/taskbar-adapter/src/hooks.rs)有意保留进程终身的跳板和菜单 detour，加载的适配器会固定至承载进程结束。干净私有进程中的停止转发检查不覆盖已经加载旧 DLL 的 Explorer；需要独立测试旧/新版本钩子链、缓存与激活身份。 |
| P2 | 控制中心选择与组件作用范围没有完整接入生产通道 | [配置](../src/config.rs)与[迁移](../src/migration.rs)保存 `show_control_center`、`copied_taskbars_only`；[一般路由](../src/routing.rs)也支持独立参数。但 [生产 preflight](../src/platform/taskbar_clock.rs)固定控制中心偏好与副屏规则，[Prepare 协议](../crates/adapter-protocol/src/taskbar.rs)仅运送 `mirror_icons`，把控制中心与图标复制绑定，并固定 `copied_taskbars_only=true`。控制中心不是完全没有复制实现，欠缺的是独立设置和端到端一致的规则。 |
| P2 | ARM64 原生 Explorer 适配没有实现 | 现有 DLL、指令检查和调用约定面向 x64。见 [ARM64 环境记录](windows11-arm64-test-environment.md)。x64 程序能启动不能证明可以把 x64 DLL 加载到 ARM64 Explorer。 |
| P2 | 独立签名的在线兼容资料包尚未实现 | [兼容计划](windows11-compatibility-plan.md)描述独立签名资料包；当前内置目录与实现识别已经存在，但没有该在线校验、分发和激活通道。新 ABI 仍需更新软件。 |
| P3 | 空气质量、网速/硬件/媒体扩展与农历纪念日重复 | [实现规格](implementation-plan.md)列出扩展内容；当前天气模型没有 AQI，未见这些指标的采集/界面通道。[日程模型](../src/organizer.rs)支持公历 yearly 等重复，[使用文档](usage.md)明确农历生日/纪念日重复尚未实现。 |

实际 Windows 场景的验收仍需覆盖主副屏、混合 DPI、自动隐藏、主题变化、物理热插拔、睡眠恢复、触摸/笔、第三方悬停、原生菜单和通知点击。不同系统版本的既有单屏准备/恢复证据不代表这些场景完成；现有兼容标记没有因此扩大。

## 原卡顿事件的归因边界

此前现场证据为 14:09–14:10 间的五次 `stornvme` 129 控制器重置、小写入耗时 28–36 秒以及 LiteTray 通信超时/恢复记录，没有同步的 Explorer 崩溃栈、GPU TDR 或逐进程 I/O 跟踪。它们不能证明物理硬盘损坏，也不能把整机卡死全部归给本轮发现的菜单缺陷。

本轮补上了确定的软件失败路径和回归，不将缺少现场数据的问题包装为已经归因。下一轮应先处理持久暂停和菜单所有权/定位，再在可恢复的真实 Windows 测试环境补完整交互矩阵。

预防这些缺陷的具体接口原则是：可能回调的菜单准备与恢复共用一个入口门；原生命令和系统缓存根有明确的归还责任；环境通知只改变路由有效性，不能隐式重置用户可见的安全暂停。
