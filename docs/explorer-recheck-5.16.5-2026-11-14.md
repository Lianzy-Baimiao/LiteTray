# 5.16.5 发布后的资源管理器复查（2026-11-14）

复查发现一处确定的停止顺序缺陷：`Session::stop` 在恢复输入、可见性和托盘模型后，才停用 `Runtime`。真实 XAML 恢复 setter 会同步进入回调；此时菜单与坐标转换仍可观察到活跃交互，旧 overflow 锚点也仍可改写定位参数。修复归入 [5.16.6](https://github.com/Lianzy-Baimiao/LiteTray/releases/tag/v5.16.6)。

本轮先公开 [5.16.5](https://github.com/Lianzy-Baimiao/LiteTray/releases/tag/v5.16.5)，其提交为 `7d2ca6fa1bbc74afec6db83bf97a27e815d84f53`，BuildId 为 `5.16.5-4420815850d4`；五个上传文件的大小与 GitHub SHA-256 均与本地一致，发布提交的两项 CI 通过。5.16.5 包含[前一轮菜单及持久保护修复](explorer-safety-audit-2026-11-14.md)，上述新的停止顺序修复从 5.16.6 开始提供。

## 复现及修复

最小场景通过真实 `Watchdog::stop → Session::stop → ModelLease::stop → Property::restore` 调用恢复 Grid 的 Opacity。在启动完成后，使用 `IDependencyObject2::RegisterPropertyChangedCallback` 订阅该属性；回调既观察当前 Runtime，又调用生产 `position` detour。定位目标是当前私有进程自己创建的隐藏 overflow 窗口，保存的原入口只记录参数，未移动系统或用户弹窗。

| 场景 | 恢复回调 | 回调观察到活跃 Runtime | 定位参数被改写 |
| --- | ---: | ---: | ---: |
| 显式先停止 Runtime 的对照 | 1 | 0 | 0 |
| 5.16.5 原停止顺序，连续两次 | 每次 1 | 每次 1 | 每次 1 |
| 5.16.6 修复后的停止顺序 | 1 | 0 | 0 |

原顺序即使最终正确恢复属性并进入 Closed，恢复期间仍会误改定位参数，因此单独检查最终状态不足以捕获这一缺陷。夹具在启动后才订阅，排除启动事件；空的 transient 注册不修改任何函数，显式 Runtime 停止对照排除底层钩子关闭失败。第一次仅观察 Runtime 的复现也连续两次失败，其原始日志保留。

修复由 [Session](../crates/taskbar-adapter/src/session.rs) 在所有时钟输入停用后、首个原生恢复调用前，执行 [Runtime::deactivate](../crates/taskbar-adapter/src/tray_interaction.rs)：停用交互，清除悬停选择及 overflow 锚点，移除属于当前实例的线程运行状态。它保留菜单 Loan、模型及原生宿主引用，随后仍按原顺序完成解除、模型恢复、钩子停用、菜单归还与主屏重新附着。缓存的旧 Runtime 也不能在停用后重新设置锚点。失败时仍维持 Restoring，继续保留恢复对象和失败预算。

[原生回归](../crates/taskbar-adapter/src/tray_stop_checks.rs) 接入已有私有自检，覆盖恢复回调、原始定位参数、实际原值恢复和 Closed 状态；成功的 native 检查共 195 项。两次独立 Release 进程通过，均记录 `explorer_accessed=false`。

这条复现证明恢复期间发生了不应发生的交互与定位改写。它没有产生一次真实 Explorer 崩溃，也不能代替历史通知内容点击或清除的故障栈。

## 本次复查的其他结果

| 路径 | 本轮证据与结论 | 仍需补齐 |
| --- | --- | --- |
| 菜单恢复重入、迁移失败、停止后转发和跨根拒绝 | 最终 private native 检查继续覆盖前一轮的命令归属、调用参数、根身份、祖先超限及停止恢复回归。 | 原生系统菜单位置、点击和完整多屏交互。 |
| 旧版与新版异常回调同时驻留 | 同一私有进程加载发布包中内容命名的 5.16.4 与 5.16.5 DLL，各注册两次回调；两份模块不同，释放调用方引用后两份回调映像均仍被保留。 | 旧版与新版完整菜单、坐标和分派钩子链的热升级验收。此检查没有安装任务栏钩子，也没有触发异常。 |
| 历史通知内容点击或清除崩溃 | 核对历史记录、已有符号化结果及近三天 Application 1000/1001/1002 事件；事件查询成功，未找到新的 Explorer、LiteTray 或 ShellExperienceHost 相关记录。 | 实际通知动作与同时刻的异常栈、线程上下文和客体身份。现有 `Taskbar.dll+0x16b2f8` 空读只能条件性对应 `CTaskListWnd::_HandleJumpViewDismissed+0x108`，不能建立通知归因。 |
| 普通 Flyout 不同根内容共享 | 保留原生同根/首次显示；无法确认的跨根内容仍按现有规则拒绝，未新增放行或扩大兼容标记。 | 普通 Content 的控件、绑定和样式代理，以及真实交互验收。 |
| 日志与退出记录 | native 故障、恢复及健康入口继续使用有界队列；持久许可、预算和退出限时机制沿用前一轮修复。 | 严重存储阻塞或突然终止后，保守暂停仍可能发生；其提示不等于磁盘损坏或已确认的软件崩溃。 |

发布包仍使用默认兼容目录。物理多屏、混合 DPI、热插拔、完整通知动作以及旧/新驻留钩子交互未因这些私有检查而成为已验收场景。

## 界面耗时检查的波动

5.16.6 本机完整 workspace 检查中，时钟设置页耗时检查先后失败两次：首轮冷导航为 230 毫秒，其中同步绘制约 184 毫秒；次轮冷导航 53 毫秒，但重复切换 P95 为 64 毫秒。原日志保留于 `local/release-5.16.6/all-targets-first-failed.log` 和 `all-targets-second-failed.log`。

使用完全相同的 workspace 测试二进制，单独连续三次检查均通过：冷导航 53–64 毫秒，P95 11–16 毫秒。阈值仍为冷导航低于 100 毫秒、P95 低于 50 毫秒；没有修改界面生产代码或放宽标准。这排除了测试构建差异，完整运行中的耗时波动尚未归因。此场景仅创建自己的非输入桌面窗口，App 没有任务栏连接，不能将其当作 Explorer 故障复现。

本机将该检查与其余 380 项分开验证，发布 CI 仍按原阈值完整执行全部 workspace 检查。最终 CI 及发布二进制自检结果随发布记录保留；两次本机完整运行的失败没有用后续成功覆盖。

## 证据与重跑

本机证据保存在 `E:\AI开发\LiteTray\local\explorer-recheck-5.16.5-20261114`：

- `stop-red-1/2.stderr.log` 与 `stop-red-receipts.json`：原生恢复回调看到活跃 Runtime。
- `stop-redirect-red-1/2.stderr.log` 与 `stop-redirect-red-receipts.json`：显式停止对照通过，正常停止仍改写真实 detour 的原入口参数。
- `stop-green-1/2.stdout.json` 与 `stop-green-receipts.json`：修复后两次各 195 项通过。
- `coexisting-beacons.stdout.json` 与 `coexisting-beacons-receipt.json`：两份实际发布 DLL 的身份和回调映像保留结果。
- `application-events.json`：事件查询结果和查询错误字段。

发布 5.16.5 的构建、包内 100 个文件校验、native 193 项双进程检查、CI 和公开资产摘要另存于 `local/release-5.16.5`。后续 5.16.6 的最终发布验证保存于 `local/release-5.16.6`，旧版资产保持原样。

使用构建完成的私有发布目录重跑回调与恢复检查：

```powershell
& .\tools\Test-XamlClock.ps1 -BinaryPath '<private-directory>\LiteTray.exe'
```

它运行自己的 XAML 窗口和受控原入口，不接入当前 Explorer。整个复查未安装或启动正常后台实例，未点击、生成或清除用户通知，未重启 Explorer；用户 config/agenda 的 SHA-256 按发布前基线核对。

预防原则是把“禁止新的交互”与“完成所有恢复”分成明确的两个阶段：前者必须先于任何可能同步回调的原生恢复调用，后者保留可重试的所有权和原有恢复顺序。
