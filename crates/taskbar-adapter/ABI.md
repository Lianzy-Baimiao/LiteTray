# Windows XAML 接口与调用边界

`src/xaml_abi.rs` 由 `tools/xaml-bindgen` 从 Windows 自带的 `Windows.UI.Xaml.winmd` 生成。构建使用已签入的定义，不需要开发机的 winmd，CI 也不从运行环境重新生成接口。

生成命令：

```powershell
.\tools\Invoke-Cargo.ps1 -CargoArguments @('run','--manifest-path','tools/xaml-bindgen/Cargo.toml','--','crates/taskbar-adapter/src/xaml_abi.rs')
```

生成器锁定 windows-bindgen 0.66.0。只请求所需接口，其余依赖保留为 usize 占位槽。生成器为少量 WinRT 引用类型参数补足 `void*` ABI 签名，严格检查每个替换恰好出现一次；不插入或删除槽位。单元测试验证 Name、XamlRoot、Content、GetChild、Text 等槽位。

`xaml.rs` 封装引用计数及 HSTRING 生命周期，COM 对象通过 Rc phantom 标记为线程绑定；接口和对象不会在线程之间传递。检查时所有持有引用在同步检查结束前释放。检查只读名称、尺寸和托盘范围内的文本，不设置属性。

`inspect.rs` 依次核对协议、x64 模块身份、PDB GUID/age、映像大小、时间戳、入口所在内存区域及代码字节。发现模块替换或已有 hook 改写入口即返回不可用；不猜测布局，不使用固定备用偏移。任务栏宿主指针仅来自已校验的 GetTaskbarHost 接口，std::shared_ptr 在同一线程由模块自身的 _Decref 释放。

在确认六个内部符号、对象布局和只读 XAML 检查通过之前，不启用任何时钟/托盘写入。仅 `cargo test` 通过不代表 Explorer 实机兼容性验证完成。

## 时钟属性的隔离验证

`clock.rs` 的 `ClockLease` 保存两行 Text、FontSize、FontFamily，以及指定容器 Width 的原始依赖属性。保存阶段不写入任何属性；普通值、UnsetValue 与 Binding 分别恢复为 SetValue、ClearValue 与 SetBinding，保留动态绑定。修改前先构造全部 WinRT 值；中途 setter 失败时尝试恢复所有已接触属性，返回恢复错误并保留待重试状态。Drop 再尝试恢复，不替代生产会话的显式退出协议。

`LiteTray.exe --self-test-clock` 只在调用进程创建 WindowsXamlManager 和新 TextBlock/Grid，完全不查找或接入 Explorer。真实 XAML 检查覆盖值与绑定捕获、文本/字体/宽度修改、tick、非法参数、正常/重复/Drop 恢复，以及故意传入错误属性类型后的部分写入回滚。恢复后改变 DataContext，再断言原绑定继续更新。

Win32 进程承载此 XAML 需要 manifest 中的 `maxversiontested`。使用 Microsoft 文档中的最低 XAML Islands 声明 `10.0.18362.0`，不会据此声称已验证该系统：
https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/host-standard-control-with-xaml-islands-cpp

## 会话与宿主失联

`adapter-protocol::clock` 传递已经格式化的两行文本和样式，限制消息尺寸、字符串长度、协议版本、会话身份及递增序号。固定 15 秒租期，宿主预期每 5 秒发心跳；重复、乱序和其他会话命令不能续租。使用单调时钟而不是系统日期，过期会话不能被延迟消息重新启动。

`session.rs` 在同一 UI 线程统一持有各个时钟的属性快照，拒绝重复 ID 和重叠 COM 对象。状态为 Prepared → Active → Restoring → Closed。先保存全部原值再启动；配置更新区分文本和样式，文本未变化时跳过 setter。任一目标更新失败时停止整组接管，恢复仍由 LiteTray 管理的属性；恢复失败保留 Restoring 状态并由后续 poll 重试。生产调用者必须在显式 stop 成功后才移除接入和卸载模块，Drop 只作为最后一次恢复尝试。

`watchdog.rs` 先建立原生消息窗口和 1 秒检查定时器，再允许启用时钟；持有宿主进程同步句柄，避免仅依赖可能重用的 PID。接收窗口消失、宿主退出或心跳超时会在 XAML 所属线程恢复。COM 调用前暂时取出会话，避免重入定时器重复借用；计时器执行期间保留 Context 引用。关闭时撤销计时器、窗口类与进程句柄。

每个已写属性保存实际 local value，后续更新和恢复先核对属性归属；其他组件新写的值、尺寸或绑定会被保留。检测到冲突会停止会话。相同可见文本上的新绑定也视为外部写入。无法区分外部组件再次写入完全相同的普通值，这不构成通用跨插件锁。

隔离检查目前覆盖 29 个场景，包含两组真实 XAML 控件、配置更新、乱序/其他会话消息、超时、退出、窗口消失、外部写入冲突；实等 15 秒验证原生计时器自动恢复。12 次完整启停后比较进程句柄数与 USER 对象数，确认该检查场景没有累积原生资源。此结果不代表长期运行或 Explorer 的内存增量测量。

上述会话目前由隔离检查调用，尚未连接任务栏写入 IPC。长期接管仍需：原生节点识别与实际显示协调、点击转发、生产会话加载/退出协议、Explorer 重启恢复，以及多屏托盘复制。不能将隔离验证结果作为这些能力的完成证据。
