# 清理旧版本后的 Explorer 复查（2026-11-14）

复查又确认了两条组件内部恢复早于会话停用的路径：托盘模型刷新失败，以及原生时钟部分写入失败。修复位于 **5.16.6 之后的 main**；已经发布、当前安装运行的 [5.16.6](https://github.com/Lianzy-Baimiao/LiteTray/releases/tag/v5.16.6) 不包含本次后续修复。

5.16.6 已处理正常 `Session::stop` 的顺序，但不能阻止组件在返回错误之前自行恢复属性。真实 XAML setter 会同步回调，回调仍可能进入活跃的菜单、坐标或 overflow 定位逻辑。本轮复现了恢复期间不应发生的定位参数改写，没有制造一次真实 Explorer 崩溃，也没有把它归因到历史通知点击故障。

## 最小复现与修复

| 场景 | 生产入口与故障触发 | 修复前结果 | 修复与验证 |
| --- | --- | --- | --- |
| 托盘定时刷新失败 | 自建原生 Grid 启动两项 Opacity 同步；外部 writer 修改其中一项；向私有 Watchdog 发送实际 `WM_TIMER`，进入 `Session::poll → ModelLease::refresh → update → stop`。 | 连续两次各有 1 次原值恢复回调、1 次活跃 Runtime 观察、1 次实际 `position` detour 参数改写；私有检查退出码 1。显式先停用 Runtime 的对照通过。 | ModelLease 的启动与刷新在内部回滚前调用会话提供的 `before_restore`；随后保留原恢复顺序、错误和重试责任。 |
| 托盘首次启动部分写入失败 | 首项 Opacity setter 的真实回调修改下一项；下一项写入被所有权检查拒绝，第一项必须恢复。只统计回到原值的回调，排除正常初次写入。 | 与刷新路径共用同一个模型内部回滚入口。 | 回归确认已写入值恢复、外部值保留、会话 Closed；恢复期间没有活跃交互或定位参数改写。 |
| 原生时钟部分写入失败 | 使用 `ClockLease::capture_native`；更新字号时，第一行真实 FontSize 回调修改第二行字号，使后续 setter 被拒绝；进入 `ClockLease::write → restore`，包含原生文字层停止。 | 撤销时钟恢复入口的停用调用后，连续两次各有 1 次原值恢复回调、1 次活跃 Runtime 观察、1 次定位参数改写。仅保留模型修复不能使此用例通过。早先普通时钟文字夹具也连续两次复现，日志分别保留。 | Session 在准备时为各 ClockLease 安装恢复回调；`ClockLease::restore` 在原生文字层、装饰和属性恢复之前执行它。回调只弱引用 Runtime，不新增旧 XAML 根的强所有者。 |

定位探针只创建本进程的隐藏 overflow 窗口，调用生产 `position` detour；其原入口只记录参数，不移动系统或用户窗口。所有恢复观察均来自 `IDependencyObject2::RegisterPropertyChangedCallback`。外部 writer 的值在失败后继续保留，其他已写入值恢复，Clock 的原宽度与原生文本也恢复。

本次没有改动函数钩子、菜单 ABI、兼容目录或跨根放行规则。正常、无数据变化的定时刷新回归确认 Runtime 与 overflow 锚点仍保留；停用只发生在整组恢复开始之前。

## 本机验证

安装版原有 195 项原生检查通过，因此原有检查未覆盖本轮两条内部回滚路径。新增 7 项检查后，最终 Release 适配器在两个独立私有进程中各 **202 项通过**，均为 `explorer_accessed=false`，退出码 0。

`cargo fmt --all -- --check`、适配器 all-targets 的 **40 项测试**、适配器 all-targets Clippy（`-D warnings`）以及 Release 构建通过。原有隔离子进程入口仍按原规则 ignored，由父测试调用。所有本机构建串行执行，使用 `tools/Invoke-Cargo.ps1`、`--locked --offline -j 1`；没有改动耗时标准。

可重复运行的隔离自检入口（调用与本轮相同的 `--self-test-clock`）：

```powershell
# 原模型顺序：确定失败，日志保留。
& .\tools\Test-XamlClock.ps1 -BinaryPath .\local\explorer-recheck-5.16.6-20261114-221624\private-red\LiteTray.exe

# 两条修复及完整原生回归：202 项通过。
& .\tools\Test-XamlClock.ps1 -BinaryPath .\local\explorer-recheck-5.16.6-20261114-221624\private-final\LiteTray.exe

& .\tools\Invoke-Cargo.ps1 -CargoArguments @(
  'test', '-p', 'litetray-taskbar-adapter', '--all-targets',
  '--locked', '--offline', '-j', '1', '--', '--test-threads=1'
)
```

证据位于 `E:\AI开发\LiteTray\local\explorer-recheck-5.16.6-20261114-221624`：`refresh-red-1/2.stderr.log`、`clock-red-1/2.stderr.log`、`native-clock-red-1/2.stderr.log` 保留各阶段失败；`refresh-red-decoded.json`、`clock-red-decoded.json` 保存异常输出中的原生 JSON；`final-native-1/2.stdout.json` 与 receipts 保存最终通过结果。各阶段私有二进制、测试源码 diff 和构建日志分别保留。

## 清理后的真实运行状态

22:16:57 至 22:19:58 连续采样 181.0 秒，共 61 次。CPU 按 16 个逻辑处理器归一化。采样期间包含私有原生检查，未重新接入或重启真实 Explorer。

| 指标 | 当前 LiteTray | 主 Explorer |
| --- | ---: | ---: |
| 平均整机 CPU | 0.00054% | 0.06369% |
| 3 秒区间 CPU 峰值 | 0.03233% | 0.68132% |
| 采样末工作集 | 42.0 MiB | 272.1 MiB |
| 私有内存变化 | −0.344 MiB | +0.727 MiB |
| 句柄数首 → 末 | 369 → 370 | 4584 → 4589 |
| 缺失或 Responding=false 样本 | 0 | 0 |

22:40 的核对仍是 LiteTray PID 20116、主 Explorer PID 27312，开始时间与清理后基线相同。八个 Explorer 进程均保留；七个文件夹进程未加载 LiteTray 模块。主 Explorer 只加载已发布 5.16.6 的 `d6793f03…` 适配器，安装目录只保留 `5.16.6-eae63bab1a9b`。

从当前 LiteTray 启动至事件核对，没有新的相关 Application 崩溃/挂起记录、相关 System 事件或应用 fault 日志；事件查询成功。22:04:37 的 Winlogon 外壳停止记录单独保存，与此前获准执行的 Explorer 重启一致，发生在当前 LiteTray 启动之前。保护记录 failures=0；config/agenda 的 SHA-256 与清理后基线一致。

这组短时样本没有观察到持续高占用，不能证明长期运行没有泄漏。七个文件夹进程的工作集之和约 1540.8 MiB，包含共享页，不能当作 LiteTray 的独占内存。

`process-samples.jsonl`、`process-summary.json`、`events-after-current-launch.json`、`authorized-shell-restart-events.json` 和 `final-runtime-receipt.json` 保存现场证据。首次 receipt 的进程时间比较将 JSON 日期转换为显示字符串，产生了错误的 retained=false；原版保留，最终版按 UTC ticks 精确比较，修正说明存于 `receipt-date-comparison-correction.json`。这不是一次进程重启。

## 尚未结案的范围

历史通知内容点击/清除的 Explorer 故障仍缺匹配的异常栈；旧记录 `Taskbar.dll+0x16b2f8` 的空读不能用本轮定位改写自动归因。真实系统图标菜单、通知动作、多屏弹窗与混合 DPI 验收仍未完成，现有兼容标记未扩大。历史整机卡顿的 NVMe 重置与长 I/O 记录也不能据此归因到这两条软件缺陷。

本次预防原则是：**任何组件开始整组原生恢复时，都先停用会话交互，不能只依赖最外层 `Session::stop` 的调用顺序。** 恢复回调保留原生命令、属性和宿主的归还责任；停用本身不执行 COM 或磁盘操作。
