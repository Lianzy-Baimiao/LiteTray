# 任务栏窗口与 XAML source 的关联证据

2026-11-06。离线读取磁盘模块、PDB 和 Microsoft 符号服务器，并在自有进程中创建独立私有宿主；未访问 Explorer 对象或安装 hook。以下证据不代表实时任务栏验证通过。

## 当前实验接入（开发版 3）

`taskbar_source.rs` 已把本文已核对版本的对象路径接入实验连接：TaskbarHost +0x38 → 已知 IXamlExplorerHostChildWindow 具体 vtable → 子接口 +0x10 的 IDesktopWindowXamlSource。磁盘完整 SHA-256、加载路径、PE/PDB 身份及内存区域检查全部通过后才调用接口；其他模块版本不启用此路径。

`taskbar.rs` 在请求的任务栏线程核对 HWND/PID/TID/物理屏幕，解析 source，发现时钟，再用 Binding::capture 证明 source 的真实窗口、直接父窗口、Content 与 XamlRoot 归属。准备阶段不写时钟。独立宿主验证返回的端点和目标后发送 Start，原生端确认 Active 才显示运行状态。点击事件通过已建立的 owner、源窗口和序号核对。

Session 保留私有宿主与子接口直到属性和输入恢复成功；DLL 固定加载到 Explorer 退出，停止后仍保留小型会话端点供恢复状态查询。此选择需要后续实测内存和版本升级行为。

**这个真实对象解析分支没有在 Explorer 上运行。** 以下证据来自离线分析和自有进程宿主；不能证明真实 SystemTray 结构、其他修改工具共存、系统默认点击行为和多屏接管通过。

## 当前对象类型

`taskbar.dll` 的 `TaskbarHost::HasFocus` 使用 `Windows.Internal.Shell.XamlExplorerHost.IXamlExplorerHostChildWindow`。PDB 名称、调用关系与参数签名能够互相核对。这个对象不能直接当作 `Windows.UI.Xaml.Hosting.IDesktopWindowXamlSource`。

本次在 `twinui.pcshell.dll` PDB 找到的接口实现包含 Bounds、XamlContent、IsVisible、HasFocus、ActivationOptions、焦点事件及消息过滤；找到的方法没有直接返回 HWND 的 getter。另有独立的 `_WASDK` 类型与扩展接口，不能假定与系统 XAML 具有相同 ABI。

当前 HasFocus 指令访问 TaskbarHost 的 `+0x38` 字段，已有 FrameHeight 路径导出的元素字段为 `+0x10`。前者已进入上述版本限定的实验解析器；原只读 inspect 入口仍使用 FrameHeight 路径，不能单独证明 source 与输入 HWND 的对应。

## 模块与符号证据

| 模块 | PDB key | SHA-256 |
| --- | --- | --- |
| taskbar.dll | DAB69A79FEE033F0D81E023FBF172B511 | ca1e335ac1fae5ff57519035302a2f365c4620d3a230f31f1231af7331760741 |
| twinui.pcshell.dll | 9507F3DAD47F5F026569FC50273C40D71 | 8ba440eded430a2417692a98b34695a12ad0ee000333dfed07e0c2723e17111c |

| 模块 | PDB 方法 | RVA |
| --- | --- | --- |
| taskbar.dll | TaskbarHost::HasFocus | 1187100 |
| taskbar.dll | consume_…IXamlExplorerHostChildWindow::HasFocus | 1019368 |
| taskbar.dll | consume_…IXamlExplorerHostChildWindow::XamlContent getter | 110040 |
| twinui.pcshell.dll | produce<…IXamlExplorerHostChildWindow>::get_XamlContent | 310832 |
| twinui.pcshell.dll | implementation::XamlExplorerHostChildWindow::XamlContent getter | 310920 |

DbgHelp 核对模块/PDB GUID 与 age，离线反汇编核对调用关系。额外符号枚举没有保留到产品路径。原始映射为本地忽略文件 `local/taskbar-host-symbols.json`、`local/xaml-host-symbols.json`；辅助工具和缓存也只在 local 中。

## 已实现与待完成的边界

`island::Binding::capture` 查询准确的 IDesktopWindowXamlSource 接口，再通过 IDesktopWindowXamlSourceNative::get_WindowHandle 取得子窗口，核对当前 PID/TID、直接父窗口、source.Content 和实际 XamlRoot 身份。它不枚举同名窗口或按相同坐标猜测匹配。Binding 保留 source、内容及根，并为父子窗口建立各自的生命周期标记；Session 在写入与输入发布时持续检查。

接口来源是 Microsoft SDK 的 [windows.ui.xaml.hosting.desktopwindowxamlsource.h](https://github.com/microsoft/win32metadata/blob/main/generation/WinSDK/RecompiledIdlHeaders/um/windows.ui.xaml.hosting.desktopwindowxamlsource.h)。它适用于已取得的系统 XAML DesktopWindowXamlSource，不代表私有 Shell 或 WinAppSDK 类型已经适配。

真实 TaskbarHost 仍须提供可验证的 source，或为其内部宿主建立经过验证的等价绑定；还要证明窗口与当前物理显示器选择一致。本进程夹具不能替代这项生产验证。实验启动已编写，Explorer 重启后重新识别、托盘菜单与回调和多屏弹窗仍待完成。

## 自有进程的私有宿主验证

`--self-test-shell-source` 在新进程、新 STA 中调用内部宿主工厂，只传入自己创建的隐藏父窗口。`Host` 类型不接受外部 HWND、进程或借用的私有宿主。模块限制为上表中已离线核对的 x64 `twinui.pcshell.dll`：先校验磁盘 SHA-256，再核对已加载路径、PE 时间戳/大小、PDB GUID/age、工厂和子接口的具体 vtable。其他版本直接拒绝，不套用原偏移。

工厂 IID 为 `ff119559-d636-5e6b-b169-390efeb2cf6e`，子接口 IID 为 `ec97afd0-5c0d-581b-9508-074773c6f1e0`。具体工厂 vtable RVA 为 7866448，具体子接口 vtable RVA 为 7739040。XamlContent getter 的离线调用链先将子接口指针减 `0x10`，再访问对象的 `+0x20` 成员，因此自检在刚创建且仍持有引用的自有对象中读取接口指针 `+0x10` 处的 source。早期该路径只用于 `shell_source_checks.rs`；现在实验 TaskbarHost 解析器复用上述版本限定关系，真实任务栏分支仍未执行。

取得的对象实际支持公开 `IDesktopWindowXamlSource` 及 SDK 原生接口；返回 HWND 的 PID/TID、直接父窗口和 `IsChild` 与夹具一致。私有 XamlContent getter/setter 与公开 source 的根一致。两个自有宿主通过 6 项场景：source/root 交换拒绝、选定时钟和不同模型更新、原生委托点击、停止恢复、内容替换后拒绝旧输入并恢复、关闭宿主后绑定失效。测试使用标准 Windows 控件，未模拟 SystemTray 私有类；点击通过调用保留的真实委托验证，不是操作系统鼠标输入。

初始化有两个实测条件：先创建普通 WindowsXamlManager 再创建该私有宿主，会在本版本记录 `0x8000FFFF` 后异常退出；去掉重复初始化、由内部宿主自行建立 XAML 环境后成功。初次附加内容还需要所属 STA 处理 Windows 消息才能建立可视父级；单独调用 Measure/Arrange/UpdateLayout 不足。Release 构建还复现父级先建立、时钟尚未完成布局的时序，所以自检同时等待真实父级和可识别时钟，限时 2 秒且限制每批消息数量，不放宽原有树归属检查。修复后的 Release 连续 5 个独立进程检查均通过。

这证明当前版本的桥接在自有宿主中可行；真实任务栏对象的获取、物理屏幕关联和生产生命周期仍未验证。`production_taskbar_verified` 明确为 `false`。该专用检查不加入通用 CI 的成功计数，CI 机器使用其他系统版本时不能当作兼容通过；通过 `tools/Test-ShellSource.ps1` 单独运行并保留结果与错误日志。
