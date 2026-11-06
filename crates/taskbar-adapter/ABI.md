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

输入目标通过 `Watchdog::prepare_inputs` 在 Prepared 阶段配置；时钟 ID 必须属于该会话，原生输入控件不能重复。每个 Subscription 先注册但保持停用，所有属性写入成功后再统一启用。停止首先停用整组输入，随后解除 native token、恢复属性；部分失败保持 Restoring 并保留待重试对象。接收窗口、端点或宿主失联、超时和属性冲突都经过相同停止路径。

`watchdog.rs` 先建立原生消息窗口和 1 秒检查定时器，再允许启用时钟；持有宿主进程同步句柄，避免仅依赖可能重用的 PID。接收窗口消失、宿主退出或心跳超时会在 XAML 所属线程恢复。COM 调用前暂时取出会话，避免重入定时器重复借用；计时器执行期间保留 Context 引用。关闭时撤销计时器、窗口类与进程句柄。

已建立会话通过 watchdog 的独立消息窗口接收 `WM_COPYDATA`，tag 为 `clock::COMMAND_TAG`。先检查发送窗口、保留的进程句柄和数据大小，再复制消息并验证 owner/nonce/sequence。接收窗口不会转发任意消息或执行任意命令。返回 1 表示命令成功，0 表示拒绝；发送超时是结果未知，不能自动重发或假定没有生效。HWND 本身不是针对同用户恶意进程的身份认证，生产启动握手与模块加载边界仍待实现。

意外 `WM_NCDESTROY` 会清除窗口上下文并恢复会话；若在原生 setter 内发生重入销毁，当前操作返回时再完成恢复。对象销毁时核对窗口归属，避免碰到已被系统重用的 HWND。调用方仍需保持模块加载，直至会话和所有窗口都释放。

每个已写属性保存实际 local value，后续更新和恢复先核对属性归属；其他组件新写的值、尺寸或绑定会被保留。检测到冲突会停止会话。相同可见文本上的新绑定也视为外部写入。无法区分外部组件再次写入完全相同的普通值，这不构成通用跨插件锁。

隔离检查目前覆盖 69 个场景，包含两组真实 XAML 控件、配置更新、乱序/其他会话消息、超时、退出、窗口消失、外部写入冲突，以及实际 WM_COPYDATA 的包边界、跨线程样式更新、跨进程文字更新和宿主进程退出。隐藏 XAML Island 检查输入事件订阅/解除、事件发布失败保留默认行为、坐标和委托释放；两个独立 island 再验证输入与会话统一停止、回滚、外部冲突、回调重入和失联路径，并验证下述托盘模型事务。实等 15 秒验证原生计时器自动恢复；12 次完整启停通过原生消息停止，再比较进程句柄数与 USER 对象数，确认该检查场景没有累积原生资源。此结果不代表长期运行或 Explorer 的内存增量测量。

跨进程场景启动同一 LiteTray.exe 的内部测试子进程，创建独立 HWND 并发送一次更新。子进程核对目标为同路径 LiteTray 的 watchdog 窗口，父进程验证实际 XAML 文本后只结束自己创建的测试子进程，再等待原生计时器恢复。测试有有界输入、响应和进程存活时限，不读取用户配置、不发现任务栏、不安装 hook。

上述会话和原生命令端点目前由隔离检查创建，尚未连接真实任务栏。长期接管仍需：原生节点识别与实际显示协调、点击转发、生产会话加载/退出协议、Explorer 重启恢复，以及多屏托盘复制。不能将隔离验证结果作为这些能力的完成证据。

## 时钟输入与独立预览

`input.rs` 在 XAML 所属 STA 上订阅 Tapped/KeyDown。委托只暴露 IUnknown 和对应 delegate IID，不声明 Agile；上下文使用线程绑定引用，输入回调只计算控件矩形和投递有界事件。键盘分支限制为 Enter/Space，并过滤 WasKeyDown 重复；已经 handled 的输入保持原处理。发送成功才标记事件 handled，发送失败保持默认行为。解除订阅先停用上下文再移除两个 token；若原生移除失败，模块保持映射至进程退出，避免残留委托调用已卸载代码。

同一会话的控件共用 `Sequence`，同步重入时不再次消费输入。watchdog 发布闭包只持有 Context 的 Weak，检查持有的进程句柄、接收窗口、租期与 Active 状态后释放 Session 借用再调用发送方。若发送方同步停止会话，原生委托返回前重新检查停用标志，保留默认输入。会话被取出进行 COM 更新时，重入点击不发布事件。

几何计算使用 TransformToVisual(XamlRoot.Content)、TransformBounds 和 RasterizationScale，再加已验证的 island HWND 客户区原点。调用者必须证明 HWND 对应同一个 XAML root，不能把任务栏外层 HWND 直接代入。协议保留有符号物理坐标，宿主校验已建立会话、时钟 ID、源窗口和递增序号，后续异步处理弹窗，避免在输入调用栈里创建 UI。

`preview.rs` 通过 DesktopWindowXamlSource 在本进程的独立窗口承载真实两行时钟，使用同一 Session、watchdog 和输入生命周期。宿主通过有序命令更新和续租，原生回调不再与更新中的 Preview 存在可变引用别名。原生 IDesktopWindowXamlSourceNative/Native2 的 IID 与三个方法槽来自 Microsoft SDK 头文件：
https://github.com/microsoft/win32metadata/blob/main/generation/WinSDK/RecompiledIdlHeaders/um/windows.ui.xaml.hosting.desktopwindowxamlsource.h

预览模块必须留在创建线程，按“解除输入、恢复属性、关闭 XAML source、销毁窗口、关闭 manager”的顺序退出。焦点使用原生 Control.Focus，且在首次布局与切回窗口后投递焦点请求。宿主只对消息所属窗口使用 IsDialogMessage，防止设置页抢走预览的 Tab/Enter；面板处理 IDCANCEL，避免 Esc 被原生对话框预处理吞掉。

宿主对每次 native 调用持有独立 Rc，直到调用返回才允许销毁预览和卸载模块。续租入口在设置对象借用之外，由主窗口计时器驱动，最长 5 秒一次；文件对话框的嵌套消息循环不会因 App 正在借用而漏掉心跳。真实对话框保持超过 58 秒后仍可显示当前时间和点击展开面板已验证。暂停消息泵或睡眠恢复属于另一个场景，仍需验收。

自检直接调用新建 WinRT TappedRoutedEventArgs 的委托，验证几何、handled 和引用释放，不能称为模拟了操作系统输入。当前实机鼠标点击、回车打开面板和 Esc 关闭另经界面测试通过；空格和跨屏拖动仍需可靠实测。预览路径没有任务栏发现或 hook 安装；真实时钟节点、XamlRoot 与 HWND 的对应验证，以及生产会话加载和退出仍待完成。

## 原生托盘模型

`tray.rs` 接收同一 XAML STA 内的明确 source/target 对象和字段，最多 64 组目标。字段限定 DataContext、ItemsSource、Content；不会移动 UIElement 或重新实现 Shell 图标管理器。所有来源必须与所有目标不重叠，目标及字段不能重复；接口或初始来源缺失则拒绝启动。`Session::prepare_tray` 再检查托盘对象与时钟对象不重叠。

`ModelLease` 保存每个目标的 binding/local/unset 原值，以引用共享模型。每批只读取、校验一次相同 source/field；先校验全部来源和目标归属，再依次写入 DataContext 和其余字段。首次 null 表示尚未就绪，运行中 null 则主动清除旧引用；引用不变跳过写入。ItemsSource 使用 `windows-collections` 的 `IIterable<IInspectable>` 精确通用接口，最多检查 512 项。UIElement Content、DataContext 以及集合中的 UIElement 都被拒绝；接口检查只将 E_NOINTERFACE 解释为不支持，其他 COM 错误向上传递。

这不是任意第三方集合的安全包装：检查之后插入 UIElement 会先触发 XAML 原生通知，随后刷新才能发现。因此真实任务栏发现层必须确认所选 ItemsSource 是系统数据模型集合，不能把未知集合交给此接口。控件对象及 COM 引用依旧线程绑定，调用方必须保留两棵树；对象替换/销毁后的重新发现尚未实现。

`property.rs` 为时钟和托盘共用的恢复逻辑。首次写入前比较原 local value 或 ParentBinding 身份；写入后校验预期值，避免将 setter 内同步重入的其他写入认领为自己的。相同原生对象、已支持的盒装字符串或 Double 可以判等，不提供全类型序列化比较，也无法区分外部再次写入完全相同的值。

Session 先启动时钟与模型，全部成功才启用输入。watchdog 原有 1 秒 poll 刷新模型引用；刷新前停用整组输入，成功后重新启用，保证模型回滚中的同步重入不消费点击。模型异常进入统一 stop 解除输入、恢复模型和时钟，保留竞争写入。失败恢复保留 Restoring，后续重试；关闭后的 ModelLease 不接受重启/更新。18 个新增隔离场景在真实 XAML island 验证数据传播和恢复，watchdog 场景向本进程的既有端点发送 WM_TIMER 执行同一处理路径，没有另等一轮超时。

这层尚未连接 Explorer。任务栏原生菜单、单例 flyout 的 island 归属、坐标、overflow、控制中心以及图标回调都需要真实适配；数据模型共享通过不构成这些交互的完成证据。

集合夹具通过明确的 DataContext 建立 ItemsSource 绑定，不依赖 XamlReader 的 ElementName 解析作用域。隐藏输入夹具使用 64 DIP 最小高度并执行布局，避免尚未创建默认模板时出现零高度；生产预览没有此测试高度设置。输入检查仍先调用真实坐标计算，再验证刷新前后发布成功及冲突后的停用。字符串归属比较使用完整 HSTRING，检查包含相同 9000 字符前缀、不同末尾的竞争值，不使用展示文本的截断结果。
