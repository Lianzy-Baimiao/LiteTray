# 任务栏窗口与 XAML source 的关联证据

2026-11-06。只读取磁盘模块、PDB 和 Microsoft 符号服务器；未访问 Explorer 对象或安装 hook。以下离线证据不代表实时任务栏验证通过。

## 当前对象类型

`taskbar.dll` 的 `TaskbarHost::HasFocus` 使用 `Windows.Internal.Shell.XamlExplorerHost.IXamlExplorerHostChildWindow`。PDB 名称、调用关系与参数签名能够互相核对。这个对象不能直接当作 `Windows.UI.Xaml.Hosting.IDesktopWindowXamlSource`。

本次在 `twinui.pcshell.dll` PDB 找到的接口实现包含 Bounds、XamlContent、IsVisible、HasFocus、ActivationOptions、焦点事件及消息过滤；找到的方法没有直接返回 HWND 的 getter。另有独立的 `_WASDK` 类型与扩展接口，不能假定与系统 XAML 具有相同 ABI。

当前 HasFocus 指令访问 TaskbarHost 的 `+0x38` 字段，已有 FrameHeight 路径导出的元素字段为 `+0x10`。这些只为调查指明对象层次，**没有把新字段偏移或私有 vtable 槽写进生产适配器**。现有 TaskbarHost 内容读取仍不能给出实际 source 与输入 HWND 的完整关联证明。

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

真实 TaskbarHost 仍须提供可验证的 source，或为其内部宿主建立经过验证的等价绑定；还要证明窗口与当前物理显示器选择一致。当前访问限制未改变，本进程夹具不能替代这项生产验证。之后才能接入正式启动、Explorer 重启后重新识别、托盘菜单与回调和多屏弹窗位置。
