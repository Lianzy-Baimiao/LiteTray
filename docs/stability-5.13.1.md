# 5.13.1 稳定性与更新验证

日期：2026-11-08。继续处理原生悬停提示、设备变化导致的时钟闪回，以及 DLL 替换相关的 Explorer 稳定性问题。

## 已确认的问题与修复

宿主事件日志在 14:37、14:57、15:39 记录了 `WM_DEVICECHANGE 0x7`（`DBT_DEVNODES_CHANGED`）之后的 `restart → restored → connected`，其间时钟恢复为原生外观约 2–3 秒。原实现将任何通用设备通知视为必须重建任务栏会话。

现在仅在收到通用设备通知时，比较连接时与当前的显示器标识、主屏、坐标，以及任务栏 HWND、PID、TID、所属显示器。枚举顺序、设备友好名称、工作区和自动隐藏导致的任务栏矩形变化不单独触发重建。无法可靠枚举时仍进入恢复流程。预检工作线程执行期间收到的通知会保留，连接建立后再检查；普通设备通知不能重置连续失败暂停状态。`TaskbarCreated`、显示设置变化和电源恢复仍走原有明确恢复路径。

空白悬停模板原来会还原原生 tooltip。现在在受管理的文字区和经过拓扑验证的独立时钟祖先维持可恢复的空覆盖；填写模板仍显示自定义提示。相邻通知铃铛不属于处理范围。停止或丢失宿主后恢复原有绑定、本地值、空值或 unset 状态；其他写入方已接管的属性不会被旧值覆盖。设置页中英文提示同步改为“留空隐藏提示”。

Windows 加载器的隔离实验已证明：加载 DLL 后改名，再在原路径放入不同 DLL，第二次 `LoadLibrary` 仍可能返回原映像。若宿主和目标进程看到不同构建，线程钩子的入口偏移就不再可靠。配套适配器和弹窗 DLL 现在使用 `名称.<SHA256>.dll` 路径，加载前核对内容并持有禁止写入／改名的读句柄，直至完成加载和钩子往返。发布文件采用无覆盖方式，已有同名内容损坏时拒绝加载。安装包预置内容命名文件，因此只读安装目录无需运行时创建它们。

上一轮新增的异常诊断在固定 DLL 前注册异常处理器。若准备失败，最后一个加载引用释放后可能遗留回调地址。现在注册前必须成功固定模块；自检使用独立子进程验证释放引用后的地址仍映射，不向 Explorer 注入测试，也不主动制造异常。

## 可重复的验证入口

```powershell
./tools/Invoke-Cargo.ps1 -CargoArguments @('test','--workspace','--all-targets','--locked','--','--test-threads=1')
./tools/Invoke-Cargo.ps1 -CargoArguments @('clippy','--workspace','--all-targets','--locked','--','-D','warnings')
./tools/Test-XamlClock.ps1 -BinaryPath '<构建目录>\LiteTray.exe'
./tools/Test-FaultBeacon.ps1 -AdapterPath '<构建目录>\litetray_taskbar_adapter.dll'
./tools/Test-Compatibility.ps1 -BinaryPath '<构建目录>\LiteTray.exe'
```

设备通知回归先在服务测试中失败：运行状态由 `Running` 变成 `Waiting`，证明发生了原生恢复。修复后检查没有 stop/reconnect，同时覆盖真正的拓扑变化、Explorer 通知和预检期间的显示器变化。

原生 XAML 回归先复现空模板仍保留原生提示；修复后覆盖自定义提示、时钟祖先、通知铃铛边界、属性与绑定恢复。它使用自建 XAML 宿主，不能单独代替真实 Explorer 悬停验收。

异常回调测试先得到 `callback_module_retained_after_free_library=false`，修复后为 `true`。DLL 加载器回归在隔离子进程内检查两个不同构建的导出结果共存、文件锁、并发发布、只读目录以及损坏文件拒绝。

本机运行新版并确认任务栏已连接后，可以执行以下显式实机检查。脚本只向指定 LiteTray 窗口投递 16 次通用设备通知，并验证日志确认连接保留、宿主与 Explorer 均未重启：

```powershell
./tools/Test-DeviceNotifications.ps1 -ProcessId <运行中的LiteTray进程号>
```

## 诊断与仍待验证的范围

默认数据目录 `%LOCALAPPDATA%\LiteTray` 下新增 `taskbar-events.log` 与 `adapter-events.log`，普通记录限制长度并轮转；诊断导出包含两者最近 40 行。适配器日志在 Explorer 中使用默认目录；隔离测试或自定义 `LITETRAY_DATA_DIR` 的宿主不会把该真实桌面日志混入隔离配置。异常线索仅用于定位，处理器不吞掉异常，也不保证挽救 Explorer。

以上证据能证明具体复现与防护生效，尚无此前所有用户反馈崩溃的完整异常栈，不能将“同路径旧映像复用”认定为所有崩溃的唯一原因。Windows 通知点击、第三方安装刷新图标和新系统分支仍需各自的实机回归。

当前只核验 Windows 11 26300.9711 x64 的指定模块。未知版本继续拒绝接管；第二套真实 Windows 分支未完成验收，不扩大兼容范围。

## 本次执行结果

2026-11-08 本机完成工作区 219 项测试，另通过后加入的日历保留回归 1 项；8 项显式忽略用例没有计为通过。Clippy 严格检查与 release 构建通过。发布构建的隔离 XAML 检查 164 项通过，磁盘兼容导出检查 5 项通过，异常回调模块保留检查通过；便携包与 5.13.1 安装器已生成。

16:42 正常退出诊断版后，旧宿主记录了原生恢复确认，新 5.13.1 宿主成功连接。任务栏 Explorer PID 9908、09:30 的启动时间均未改变。主、副屏的 source、时钟拓扑、几何检查无错误；托盘宽度、相对位置与应用图标顺序一致。16 次 `DBT_DEVNODES_CHANGED` 实机消息均保留会话，没有 restore、restart 或重连。用户配置与日程文件 SHA-256 前后相同。

实际悬停时的视觉表现尚未单独完成验收；当前证据包括原生 XAML 提示属性回归和真实主副屏正常接管，不把它描述为已完成全部鼠标场景验证。下一阶段的隔离环境与跨版本矩阵见 [Sandbox 与虚拟机检查](sandbox-compatibility.md)。
