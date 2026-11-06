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

已建立会话通过 watchdog 的独立消息窗口接收 `WM_COPYDATA`，tag 为 `clock::COMMAND_TAG`。先检查发送窗口、保留的进程句柄和数据大小，再复制消息并验证 owner/nonce/sequence。接收窗口不会转发任意消息或执行任意命令。返回 1 表示命令成功，0 表示拒绝；发送超时是结果未知，不能自动重发或假定没有生效。HWND 本身不是针对同用户恶意进程的身份认证，生产启动握手与模块加载边界仍待实现。

意外 `WM_NCDESTROY` 会清除窗口上下文并恢复会话；若在原生 setter 内发生重入销毁，当前操作返回时再完成恢复。对象销毁时核对窗口归属，避免碰到已被系统重用的 HWND。调用方仍需保持模块加载，直至会话和所有窗口都释放。

每个已写属性保存实际 local value，后续更新和恢复先核对属性归属；其他组件新写的值、尺寸或绑定会被保留。检测到冲突会停止会话。相同可见文本上的新绑定也视为外部写入。无法区分外部组件再次写入完全相同的普通值，这不构成通用跨插件锁。

隔离检查目前覆盖 41 个场景，包含两组真实 XAML 控件、配置更新、乱序/其他会话消息、超时、退出、窗口消失、外部写入冲突，以及实际 WM_COPYDATA 的包边界、跨线程样式更新、跨进程文字更新和宿主进程退出。新增隐藏 XAML Island 的输入事件订阅/解除、事件发布失败保留默认行为、坐标和委托释放检查。实等 15 秒验证原生计时器自动恢复；12 次完整启停通过原生消息停止，再比较进程句柄数与 USER 对象数，确认该检查场景没有累积原生资源。此结果不代表长期运行或 Explorer 的内存增量测量。

跨进程场景启动同一 LiteTray.exe 的内部测试子进程，创建独立 HWND 并发送一次更新。子进程核对目标为同路径 LiteTray 的 watchdog 窗口，父进程验证实际 XAML 文本后只结束自己创建的测试子进程，再等待原生计时器恢复。测试有有界输入、响应和进程存活时限，不读取用户配置、不发现任务栏、不安装 hook。

上述会话和原生命令端点目前由隔离检查创建，尚未连接真实任务栏。长期接管仍需：原生节点识别与实际显示协调、点击转发、生产会话加载/退出协议、Explorer 重启恢复，以及多屏托盘复制。不能将隔离验证结果作为这些能力的完成证据。

## 时钟输入与独立预览

`input.rs` 在 XAML 所属 STA 上订阅 Tapped/KeyDown。委托只暴露 IUnknown 和对应 delegate IID，不声明 Agile；上下文使用线程绑定引用，输入回调只计算控件矩形和投递有界事件。键盘分支限制为 Enter/Space，并过滤 WasKeyDown 重复；已经 handled 的输入保持原处理。发送成功才标记事件 handled，发送失败保持默认行为。解除订阅先停用上下文再移除两个 token；若原生移除失败，模块保持映射至进程退出，避免残留委托调用已卸载代码。

几何计算使用 TransformToVisual(XamlRoot.Content)、TransformBounds 和 RasterizationScale，再加已验证的 island HWND 客户区原点。调用者必须证明 HWND 对应同一个 XAML root，不能把任务栏外层 HWND 直接代入。协议保留有符号物理坐标，宿主校验已建立会话、时钟 ID、源窗口和递增序号，后续异步处理弹窗，避免在输入调用栈里创建 UI。

`preview.rs` 通过 DesktopWindowXamlSource 在本进程的独立窗口承载真实两行时钟，使用同一 ClockLease 和 Subscription。原生 IDesktopWindowXamlSourceNative/Native2 的 IID 与三个方法槽来自 Microsoft SDK 头文件：
https://github.com/microsoft/win32metadata/blob/main/generation/WinSDK/RecompiledIdlHeaders/um/windows.ui.xaml.hosting.desktopwindowxamlsource.h

预览模块必须留在创建线程，按“解除输入、恢复属性、关闭 XAML source、销毁窗口、关闭 manager”的顺序退出。焦点使用原生 Control.Focus，且在首次布局与切回窗口后投递焦点请求。宿主只对消息所属窗口使用 IsDialogMessage，防止设置页抢走预览的 Tab/Enter；面板处理 IDCANCEL，避免 Esc 被原生对话框预处理吞掉。

自检直接调用新建 WinRT TappedRoutedEventArgs 的委托，验证几何、handled 和引用释放，不能称为模拟了操作系统输入。当前实机鼠标点击、回车打开面板和 Esc 关闭另经界面测试通过；空格和跨屏拖动仍需可靠实测。预览路径没有任务栏发现或 hook 安装，真实时钟订阅归属与 watchdog 生命周期尚需生产会话整合。
