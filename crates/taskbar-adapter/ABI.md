# Windows XAML 接口与调用边界

## 开发版 3 的实验任务栏连接

当前增加 `taskbar` Prepare/Reply/Status 协议与 `clock::Change::Start`。宿主明确请求连接，适配器先保存原值和输入绑定，返回已验证目标与端点；随后 Start 才修改时钟。停止不授予新租期，在过期及重复停止时仍允许恢复，Closed 是恢复确认。准备和状态验证拒绝其他 owner、接收窗口及会话 nonce。

`taskbar_source.rs` 使用当前两个系统模块的完整 hash、加载身份及已核对私有字段；仅实验原位路径启用。Session 持有其私有宿主引用，在输入、属性和窗口绑定都释放后清理。DLL 在 Explorer 内固定加载到进程结束，以覆盖晚到回调及恢复重试。与此不同，独立预览继续使用其原有可释放运行环境。

原位时钟的 Text 由 Shell 持续写入，不能把每次系统刷新当作一般外部冲突。`clock_text.rs` 使用准确的 IDependencyObject2 接口和线程绑定 DependencyPropertyChangedCallback：只观察两行 Text，捕获系统刚写入的原值或绑定，在同步回调中重新设置自定义文本；后续停止恢复最新快照。自身写入有重入门闩，委托对上下文持有 Weak，不构成控件引用环。属性/绑定/解除失败会通过会话检查恢复，其余字体、颜色、宽度等仍遵循冲突保护。相同普通值的再次写入不会触发属性事件，这不是跨插件排他锁。

隔离检查先证明直接写 Text 和 held Storyboard 都无法抵抗系统刷新，再在相同真实 TextBlock 用例验证同步回调：120 次系统写入、自定义 Tick、最新值恢复、绑定恢复、重复 Stop、Drop 与回调释放均通过。另通过真实 WM_COPYDATA 验证 Prepared→Start→Active→Stop→Closed 和 sender/nonce 拒绝。通用检查现为 138 项，版本限定自有 Shell 宿主检查另计 6 项。

以上自检结论与生产验证分别记录。2026-11-06 后续已在真实 Explorer 验证主副屏原位时钟与副屏应用图标生成，见 [实机记录](../../docs/live-clock-acceptance.md)；完整多托盘菜单及系统图标仍未完成。下文保留普通属性、预览、模型事务的详细机制；普通属性的冲突语义不适用于上述专用原位 Text 观察器。

Prepare 新增可缺省的 `source`、`clocks` 和 `mirror_icons`。来源只能是同一线程的主任务栏；只读来源不会因此成为时钟修改目标。原生复制包含 NotificationAreaIcons、NotifyIconStack、MainStack、NonActivatableStack、ControlCenterButton、ShowDesktopStack。Reply 只返回实际启用的时钟输入目标；独立托盘可返回空目标集合，使用 `StartTray` 启动，随后仅发送心跳。副屏依赖属性、模型和来源引用仍受同一恢复事务保护。

### 托盘输入及菜单边界

SystemTray.dll 仅接受 SHA256 `c5f5260a2a56c4d7b0ee25978aa06c387b5ddffc57c5fe412be429d1289913ce`，PDB `2007F3890B9D4D16AF78BAFE771227281`；同时验证内存身份和每个入口的 16 字节前缀。Microsoft PDB 得到 GetInvocationPointRelativeToScreen RVA772912、GetIconBoundsRelativeToScreen RVA772612、GetScreenRectFromXamlElement RVA773136。离线反汇编另核对隐藏返回指针和参数寄存器；不把 C++ 按值 FrameworkElement 临时对象当借用参数释放两次。

DispatchMessageW 作用域保存真实任务栏根，涵盖鼠标、pointer、键盘、WM_TIMER 延迟悬停及鼠标离开；嵌套消息恢复外层上下文。坐标使用 verified source HWND 的 client origin 和 XamlRoot scale。Shell_NotifyIconGetRect 通过 runtime class + DataContext IUnknown 身份寻找对应屏幕的图标。隐藏图标 popup 的 SetWindowPos 仅对明确点击入口后的已知窗口类调整位置，限制在目标屏工作区。

MinHook 的 fallible C API 建立跳板；会话停止时恢复输入和 SystemTray 入口，但跳板不释放，避免已进入的回调执行释放后代码。IFlyoutBase.ShowAt（slot14）及 IFlyoutBase5.ShowAt(options)（slot12）从自建 MenuFlyout/Flyout 的公开 ABI 读取，常驻 guard 只处理共享系统图标子树。锁定其他 XamlRoot 的缓存菜单把原生 command items 借给目标 island 的代理，下次交互或停止时归还；失败时抑制已知会崩溃的跨根调用。为保护此前显示过的缓存菜单，guard 随固定加载的 DLL 保留。鼠标按住期间不重新附着 ItemsSource，以免吞掉按下或拖动。

只读检查现在同时返回物理矩形、模型身份、HitTest/Enabled/Opacity 和最多 32 项最近坐标转换；不记录键盘字符或应用消息正文。这些诊断和独立窗口回归不等于真实 QQNT 悬停、系统左键弹层或拖放已经验收。

宽度 0 表示 Auto，在原生端设置 Width=NaN；所有权比较已有 NaN 判等支持。固定 80–600 宽度保持兼容，Tick 不计算或重设宽度。watchdog 窗口类按所属线程与回调地址区分，防止 Explorer 中尚被固定加载的旧 DLL 与新 DLL 混用窗口过程；主机仍验证端点进程、线程和类型前缀。

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

隔离检查目前覆盖 101 个场景，包含两组真实 XAML 控件、配置更新、乱序/其他会话消息、超时、退出、窗口消失、外部写入冲突，以及实际 WM_COPYDATA 的包边界、跨线程样式更新、跨进程文字更新和宿主进程退出。隐藏 XAML Island 检查输入事件订阅/解除、事件发布失败保留默认行为、坐标和委托释放；独立 island 再验证输入与会话统一停止、回滚、外部冲突、回调重入和失联路径，并验证下述托盘模型与范围可见性事务。实等 15 秒验证原生计时器自动恢复；12 次完整启停通过原生消息停止，再比较进程句柄数与 USER 对象数，确认该检查场景没有累积原生资源。此结果不代表长期运行或 Explorer 的内存增量测量。

跨进程场景启动同一 LiteTray.exe 的内部测试子进程，创建独立 HWND 并发送一次更新。子进程核对目标为同路径 LiteTray 的 watchdog 窗口，父进程验证实际 XAML 文本后只结束自己创建的测试子进程，再等待原生计时器恢复。测试有有界输入、响应和进程存活时限，不读取用户配置、不发现任务栏、不安装 hook。

上述会话和原生命令端点目前由隔离检查创建，尚未连接真实任务栏。长期接管仍需：原生节点识别与实际显示协调、点击转发、生产会话加载/退出协议、Explorer 重启恢复，以及多屏托盘复制。不能将隔离验证结果作为这些能力的完成证据。

## 时钟输入与独立预览

`input.rs` 在 XAML 所属 STA 上订阅普通 Tapped/KeyDown，停止时按注册 token 解除。委托只暴露 IUnknown 和对应 delegate IID，不声明 Agile；上下文使用线程绑定引用，输入回调只计算控件矩形和投递有界事件。键盘分支限制为 Enter/Space，过滤 WasKeyDown 重复；两类事件均跳过已 handled 的输入。发送成功才标记事件 handled，失败不改变原 handled 状态。解除订阅先停用上下文再移除事件 token；若原生移除失败，模块保持映射至进程退出，避免残留委托调用已卸载代码。

5.2 保持输入边界与格式化边界分离：文字和宽度仍只改 DateTimeStackPanel；找到包含 SystemTray.DateTimeIconContent 且子树仅包含两行文字与已知布局的 SystemTray.IconView 后，选取 DateTimeIconContent 内部最靠近文字、且与整个按钮同尺寸的布局子节点。禁止在 IconView 本身订阅 handledEventsToo：其类处理器可能已经打开系统日历，事后设置 Handled 无法撤销该动作。布局区未设置背景时，input_surface 加入透明画刷用于命中留白，结束时经 Property 租约恢复原始值/绑定，并保留外部写入。出现未知按钮/其他动作或尺寸不匹配时不扩展。整个扩展输入子树由会话守卫核对；通知铃铛、显示桌面等兄弟区域不属于此输入目标。

回归分两层：捕获控件层级上的冒泡顺序回放验证 LiteTray=1、原生=0；独立 XAML 岛验证真实订阅、透明画刷、已处理事件跳过与背景恢复。这些检查不等同于向 Explorer 注入真实鼠标点击；实机自然点击结果单独记录。

同一会话的控件共用 `Sequence`，同步重入时不再次消费输入。watchdog 发布闭包只持有 Context 的 Weak，检查持有的进程句柄、接收窗口、租期与 Active 状态后释放 Session 借用再调用发送方。若发送方同步停止会话，原生委托返回前重新检查停用标志，保留默认输入。会话被取出进行 COM 更新时，重入点击不发布事件。

几何计算使用 TransformToVisual(XamlRoot.Content)、TransformBounds 和 RasterizationScale，再加已验证的 island HWND 客户区原点。调用者必须证明 HWND 对应同一个 XAML root，不能把任务栏外层 HWND 直接代入。协议保留有符号物理坐标，宿主校验已建立会话、时钟 ID、源窗口和递增序号，后续异步处理弹窗，避免在输入调用栈里创建 UI。

`preview.rs` 通过 DesktopWindowXamlSource 在本进程的独立窗口承载真实两行时钟，使用同一 Session、watchdog 和输入生命周期。宿主通过有序命令更新和续租，原生回调不再与更新中的 Preview 存在可变引用别名。原生 IDesktopWindowXamlSourceNative/Native2 的 IID 与三个方法槽来自 Microsoft SDK 头文件：
https://github.com/microsoft/win32metadata/blob/main/generation/WinSDK/RecompiledIdlHeaders/um/windows.ui.xaml.hosting.desktopwindowxamlsource.h

预览模块必须留在创建线程，按“解除输入、恢复属性、关闭 XAML source、销毁窗口、关闭 manager”的顺序退出。焦点使用原生 Control.Focus，且在首次布局与切回窗口后投递焦点请求。宿主只对消息所属窗口使用 IsDialogMessage，防止设置页抢走预览的 Tab/Enter；面板处理 IDCANCEL，避免 Esc 被原生对话框预处理吞掉。

宿主对每次 native 调用持有独立 Rc，直到调用返回才允许销毁预览和卸载模块。续租入口在设置对象借用之外，由主窗口计时器驱动，最长 5 秒一次；文件对话框的嵌套消息循环不会因 App 正在借用而漏掉心跳。真实对话框保持超过 58 秒后仍可显示当前时间和点击展开面板已验证。暂停消息泵或睡眠恢复属于另一个场景，仍需验收。

预览新增本进程 ABI：`LiteTrayPreviewState` 返回 Busy=0、Prepared=1、Active=2、Restoring=3、Closed=4，空句柄返回 -1；未知状态和 Busy 都不证明恢复完成。`LiteTrayPreviewStop` 的成功返回仅表示调用成功，宿主仍须单独观察 Closed。宿主只在此后释放旧连接并允许新建，恢复失败三次则保留连接等待明确重试。有效原生句柄必须仍由创建 STA 持有；接口不校验任意外来指针，也不是跨进程协议。

`LiteTrayPreviewRuntimeCreate/Destroy` 建立覆盖所有预览代次的线程 XAML 生命周期。每个连接额外持有该环境的 Rc，运行环境最终释放前必须销毁所有预览句柄并结束在途调用。自动恢复不卸载 DLL 或关闭最后一个 WindowsXamlManager；本机实测最后一个 manager 关闭后，同线程再次 InitializeForCurrentThread 返回 0x8000FFFF，须等整个服务退出才关闭运行环境。首次窗口创建失败也保留环境供重试。已有原生套件与宿主恢复套件在各自的 STA 中执行，不能在已经结束 XAML 生命周期的线程上重启套件。

Create 输入新增可选 `visible`，省略时维持原先立即显示的行为。宿主始终隐藏创建，再通过 `LiteTrayPreviewPresent(handle, activate)` 显示；自动重建使用 SW_SHOWNOACTIVATE，只有明确打开预览才请求前台与焦点。宿主原生调用门闩将同步重入识别为 Busy，并保留连接。更新失败、原生停止和窗口失效触发恢复状态机；排队点击在真正展开面板前再次核对当前代次及 Active 状态。恢复策略仅整合进本进程预览，生产任务栏重连和模块生命周期仍未完成。

自检直接调用新建 WinRT TappedRoutedEventArgs 的委托，验证几何、handled 和引用释放，不能称为模拟了操作系统输入。当前实机鼠标点击、回车打开面板和 Esc 关闭另经界面测试通过；空格和跨屏拖动仍需可靠实测。预览路径没有任务栏发现或 hook 安装；真实时钟节点、XamlRoot 与 HWND 的对应验证，以及生产会话加载和退出仍待完成。

## 原生托盘模型

`tray.rs` 接收同一 XAML STA 内的明确 source/target 对象和字段，最多 64 组目标。字段限定 DataContext、ItemsSource、Content；不会移动 UIElement 或重新实现 Shell 图标管理器。所有来源必须与所有目标不重叠，目标及字段不能重复；接口或初始来源缺失则拒绝启动。`Session::prepare_tray` 再检查托盘对象与时钟对象不重叠。

`ModelLease` 保存每个目标的 binding/local/unset 原值，以引用共享模型。每批只读取、校验一次相同 source/field；先校验全部来源和目标归属，再依次写入 DataContext 和其余字段。首次 null 表示尚未就绪，运行中 null 则主动清除旧引用；引用不变跳过写入。ItemsSource 使用 `windows-collections` 的 `IIterable<IInspectable>` 精确通用接口，最多检查 512 项。UIElement Content、DataContext 以及集合中的 UIElement 都被拒绝；接口检查只将 E_NOINTERFACE 解释为不支持，其他 COM 错误向上传递。

这不是任意第三方集合的安全包装：检查之后插入 UIElement 会先触发 XAML 原生通知，随后刷新才能发现。因此真实任务栏发现层必须确认所选 ItemsSource 是系统数据模型集合，不能把未知集合交给此接口。控件对象及 COM 引用依旧线程绑定，调用方必须保留两棵树；对象替换/销毁后的重新发现尚未实现。

`property.rs` 为时钟和托盘共用的恢复逻辑。首次写入前比较原 local value 或 ParentBinding 身份；写入后校验预期值，避免将 setter 内同步重入的其他写入认领为自己的。相同原生对象、已支持的盒装字符串或 Double 可以判等，不提供全类型序列化比较，也无法区分外部再次写入完全相同的值。

Session 先启动时钟与模型，全部成功才启用输入。watchdog 原有 1 秒 poll 刷新模型引用；刷新前停用整组输入，成功后重新启用，保证模型回滚中的同步重入不消费点击。模型异常进入统一 stop 解除输入、恢复模型和时钟，保留竞争写入。失败恢复保留 Restoring，后续重试；关闭后的 ModelLease 不接受重启/更新。18 个新增隔离场景在真实 XAML island 验证数据传播和恢复，watchdog 场景向本进程的既有端点发送 WM_TIMER 执行同一处理路径，没有另等一轮超时。

这层尚未连接 Explorer。任务栏原生菜单、单例 flyout 的 island 归属、坐标、overflow、控制中心以及图标回调都需要真实适配；数据模型共享通过不构成这些交互的完成证据。

集合夹具通过明确的 DataContext 建立 ItemsSource 绑定，不依赖 XamlReader 的 ElementName 解析作用域。隐藏输入夹具使用 64 DIP 最小高度并执行布局，避免尚未创建默认模板时出现零高度；生产预览没有此测试高度设置。输入检查仍先调用真实坐标计算，再验证刷新前后发布成功及冲突后的停用。字符串归属比较使用完整 HSTRING，检查包含相同 9000 字符前缀、不同末尾的竞争值，不使用展示文本的截断结果。

属性更新失败时，若原生 setter 保留上次成功写入的值，事务继续持有该值的归属并将其恢复；如果目标被写成别的值则保留外部写入。新增回归场景先完整启动时钟，再让第五个 setter 以真实类型错误失败，核对文字、字号、字体绑定快照和宽度均退出接管，没有遗留已写属性。

## 托盘容器与角色识别

`discovery::View::discover` 从已校验 TaskbarHost 的 XamlRoot.Content 查找唯一直接子类 `SystemTray.SystemTrayFrame`，再找唯一直接子名 `SystemTrayFrameGrid`。严格入口没有对类名或层级的备用猜测。VisualTreeHelper 的入参先查询为 IDependencyObject，不把 COM identity 指针当作依赖对象接口传入。

`topology::analyze_panel` 处理最多 320 节点、40 层的前序快照，拒绝截断、深度跳跃、多根、未知容器类型和重复角色。识别八个直接子区域；内部列表只接受唯一的 SystemTray.StackListView，深度最多 6 层。角色失败与时钟失败分开记录，缺少时钟不会删除其余区域的诊断。协议新增可选 visible/topology 字段；缺少 visible 的旧快照不能获得可见时钟结论，报告内的索引只引用同一报告的 panel 快照。

时钟必须在 NotificationCenterButton 或 SecondaryClockStack 中存在唯一可见的 TimeInnerTextBlock / DateInnerTextBlock，且接口类为标准 TextBlock、布局有正尺寸。最小共同祖先必须是只含两行文字和 Grid/StackPanel 的独立子树，不能等于整个系统按钮。panel 外层祖先可见性合并到 panel 根的 visible 字段。报告本身不包含或创建输入订阅，不授权拦截通知按钮。

View 保留节点引用和到 XamlRoot 的父级链，准备时钟与模型时重新遍历并比较 COM 身份、类名、名称、深度和 root 归属。对象即使同名替换也不能复用旧原值。该准备快照有意严格，图标增删在准备期间改变整棵树也要求重新识别；当前没有主动的结构变更订阅或自动重建会话，不能把此准备检查称为 Explorer 热重启支持。

模型配对由调用方显式指定来源 View、目标 View 和区域，不依据枚举顺序猜测主屏。要求两棵不同 XamlRoot、所选角色齐全；对每个区域及匹配的内层列表，检查来源实际接口和非空模型，再核对目标接口，生成此前 ModelLease 接受的 Pair。跳过 UIElement Content；缺少所需接口、内部列表不对称或没有可用模型均拒绝整批。该步骤不设置可见性、不移动真实托盘所有者、不处理原生菜单。

9 个新增隔离场景覆盖共享 panel 读取、分离时钟文字/通知按钮、从识别到 Session 实际更新和恢复、区域选择拒绝、外层隐藏、重复/重命名/接口错配、同名原生对象替换、可视 Content 排除及 root 脱离。夹具从自己创建的标准 Grid/StackPanel 进入同一 panel 读取器；它不复制 SystemTray 私有类。严格生产入口在夹具上必须失败，其在当前 Explorer 上的成功路径仍未实测。此前自动审批拒绝后未再次执行任务栏接入。

## 范围与可见性

协议 `routing::Plan::build` 将已解析的桌面设备列表和功能选项转换为单次窗口路由计划。路由需唯一，最多 16 个选定区域；跨进程/跨线程的模型共享被拒绝。模型来源是唯一主任务栏，可位于选定范围之外；时钟与可见性只作用于计划内目标。该计划是本地生成的输出结构，不是新增的远程启动命令。

适配层 `routing::prepare` 从显式 BoundView 中捕获时钟、模型和可见性，核对当前 PID/TID、原生窗口和完整路由记录。BoundView 字段不再允许调用方直接填充，必须通过 `capture(route, view, source)` 验证实际 XAML source；当前物理显示器映射仍由宿主负责。函数不会根据 HWND 推测 root，也不发现或接入 Explorer。捕获本身不写 XAML 显示属性；所持窗口、root 及模块必须覆盖整个会话生命周期。

`island::Binding` 规范化到 IDesktopWindowXamlSource，使用 SDK IDesktopWindowXamlSourceNative 取得 HWND，再检查同进程/线程、直接父窗口、Content 和 XamlRoot 身份。父子窗口分别持有 WindowGuard，COM 读取前后重新核对窗口关系。BoundView 与 Session 通过 Rc 保留同一绑定；停止的 Session 释放自身引用，若 BoundView 仍在，绑定仍保留供后续校验。整个绑定释放后才回收窗口标记。

`routing::attach_interactive` 从该绑定产生 InputTarget，独立核对点击控件属于同一 root，窗口由 source 返回；不会让调用方另选输入 HWND。只为计划中的时钟准备订阅，返回尚未启动的 watchdog，成功应用整组组件后才启用输入。无自定义点击行为的路径仍可只使用 prepare/Watchdog::attach。完整准备快照在布局后可能变化，重建必须重新识别，不能复用旧 View。

当前真实 Shell 使用 IXamlExplorerHostChildWindow。新增独立 `LiteTraySelfTestShellSource` 仅在本进程新建私有宿主中验证其到 DesktopWindowXamlSource 的桥，并将 Binding、范围会话与恢复串联；它不接受生产 TaskbarHost。这个导出要求新 STA、之前没有初始化 XAML，且调用方不得是 Explorer。模块 SHA-256、加载路径、PE/PDB 与具体接口 vtable 全部匹配才允许访问该自有对象的已核对字段；不匹配则返回错误。工厂与 source 引用留在所属线程，模块持有到所有对象释放以后。单独 CLI 在读取配置前运行，20 秒外部超时；通用 XAML 自检不调用此版本特定实验。已验证行为和边界见 [任务栏 source 调查](../../docs/taskbar-source-binding.md)。

`VisibilityLease` 仅修改直接托盘模型区域的 Visibility，禁止通知按钮和时钟区域。依赖属性入口从 winmd 生成 IUIElementStatics.VisibilityProperty。枚举值由新建的临时 XAML Grid 读取原生装箱对象；判等仅接受标准参数化 `IReference<Windows.UI.Xaml.Visibility>` 的精确签名，不把任意 OtherType 当成整数。所有目标先捕获、全部原值核对后写入；Session 负责部分失败整组回滚与 poll 中的归属冲突停用。

Object::clone 使用 AddRef 保留当前接口指针，不借由 QueryInterface(IUnknown) 改变接口。Property::capture 另将传入属性规范化为 IDependencyProperty，再调用 GetBindingExpression/ReadLocalValue。回归用单控件验证从不同 COM 接口身份传入的依赖属性仍能正确捕获、写入和恢复。

Session 的 Prepared 状态可以暂时没有时钟，`start_tray` 要求至少有模型或可见性事务；普通 `start(frame)` 仍要求时钟目标。独立托盘拒绝时钟更新，拒绝的命令不推进序号或续租。Watchdog 的宿主句柄和定时器保持同一生命周期；仅可见性会话也会在过期时恢复原绑定。真实系统区域的动态可见性、菜单和交互仍需 Explorer 实机适配验收。

## 活跃会话身份检查

`View::lease_guard` 在准备时重新校验快照，捕获所用直接区域、内部列表及其至 root 的父级关系；时钟另保存布局子树。LeaseGuard 保留真正的 IXamlRoot 身份，不只比较 Content 指针；每次检查核对缩放、区域的唯一性、节点名称/类型及父级。条目内部生成的图标、无关通知内容不纳入整树身份比较；时钟输入容器内的节点数量和顺序仍严格核对，最多 80 节点/40 层。

`WindowGuard` 使用 SetPropW/GetPropW 的独立令牌识别当前窗口生命周期，限定当前 PID/TID，并读取 MonitorFromWindow/GetMonitorInfoW/GetDpiForWindow 的快照。仅在标记仍匹配时 RemovePropW；释放与准备失败不覆盖别人后来写入的属性。准备阶段因此会添加自己的窗口标记，但不会改动 XAML 显示属性。这个标记不是针对恶意同用户进程的身份认证，也不能证明窗口与 XAML 根对应。

Session 启动和有效命令批次前后、poll、输入发布前核对所持 guard；来源窗口和来源模型区域也检查。拒绝的旧结构不续租，统一停止并在恢复成功后释放 guard。对于显式 Stop、已处于 Restoring 的重试，不要求旧树可访问后才尝试恢复保留的原属性。Watchdog 输入回调使用 with_session 临时取走状态来执行 COM 检查，重入消息不能再次访问同一 Session；失败保留默认输入，停止后的原生委托不再发布。

当前仅验证本进程的窗口、标准 XAML 控件和隐藏 island。窗口销毁已实测；没有强迫 Windows 重用同一个 HWND，也没有真实硬件插拔/混合 DPI 验证。显示器快照不包含完整 DisplayConfig 物理目标映射，宿主仍须处理拓扑通知并重建范围计划。检查允许正常模型条目变化，不会自动建立新会话；生产重连和模块卸载仍待完成。

## 可选时钟样式与提示

`Style` 增加可缺省的 `top_color`/`bottom_color` 与 `alignment`；省略时保留原有颜色和对齐。`Text` 的可选 `tooltip` 在省略或为 null 时隐藏原生时钟提示，非空模板继续显示用户设置的提示。颜色仅接受 #RRGGBB/#AARRGGBB；提示是最多 4096 字节的普通字符串，格式化在宿主执行。配置的 `tooltip_line` 和两行模板一样最多 1024 字节，未知变量拒绝保存；天气字符串不会再作为模板解释。

`clock_style::Decorations` 管理 Foreground、TextAlignment、ToolTipService.ToolTip 的租约。先构造值和捕获新启用字段的原始状态，再写文字与样式；已有归属在写入前复查。颜色和对齐改回 Native 时恢复 binding/local/unset 并释放该字段的租约，之后外部改动可以正常发生。再次启用时重新捕获最新原值。提示租约在整个时钟接管期间保留：清空模板写入 null，停止接管才还原 binding/local/null/unset。提示每秒变化不会重建未改变的画刷或对齐对象。新增 WinRT 接口及属性槽从同一 Windows metadata 生成；枚举判等仅接受精确 IReference<TextAlignment>，不将任意 OtherType 当作整数。

提示的目标沿已核验的时钟祖先链扩展至独立 `SystemTray.IconView`，前提是该子树仅含时钟两行和已知布局节点。共享通知按钮、铃铛及含未知操作的祖先不会被纳入；无法确认独立边界时仅处理两行和已识别的文字容器。自定义提示放在同一独立时钟的完整点击区域，并清除边界内其他节点的原生提示，避免父级提示与自定义提示竞争。停止及 Drop 恢复原值；如果外部组件替换了提示，则保留外部值并停止时钟接管。

样式冲突在时钟更新时停止会话，恢复仍由 ClockLease/Session 统一执行并保留外部写入。没有声称直接修改同一画刷对象内部颜色也能通过 COM 身份比较检测到；所有权检测针对依赖属性值/绑定的替换。

真实 XAML 场景覆盖独立 ARGB、对齐、提示特殊字符、只更新提示时画刷身份保持、无效颜色写入前拒绝、绑定/未设置值恢复、停止后外部替换再启用的最新原值、会话冲突恢复和 Drop。2026-11-08 增加空模板隐藏、祖先提示抑制、自定义提示保留、binding/local/null/unset 精确恢复与外部提示写入保护；拓扑用例另核对铃铛和未知操作边界。`tools/Test-XamlClock.ps1` 在本进程创建的真实控件上通过，报告 `explorer_accessed=false`；实际 Explorer 悬停和音量浮层仍须单独验收。
