# Windows 11 26300.9818 x64 兼容证据

2026-11-08，安装 KB5127936 并重启后的 Windows 11 Pro 26H2。此节记录磁盘模块、微软公开 PDB 和离线反汇编证据；它本身不是 Explorer 接管或多屏验收结论。新资料为 [`windows11-26300.9818-x64.json`](../crates/adapter-protocol/profiles/windows11-26300.9818-x64.json)，保留原有 9711 资料。

## 模块与符号身份

| 模块 | SHA-256 | PDB key（GUID + age） |
| --- | --- | --- |
| taskbar.dll | `a52f99f7be8cc1ee360025d1c06c1480a489b1d309bef230c05eb21a106ccf34` | `8645A6F8C08FC8A18DACD4B77CF1E2EE1` |
| twinui.pcshell.dll | `1894863a224d22bd8d3ef0e1ba382b4d3d3079e0fa9a1fdeee82f3ebace003b6` | `A4CE45DAC5623E0776DD2D5727F080C31` |
| SystemTray.dll | `9772d19ec44f22ab2fcb976941d6ba6d1e66aabfb9d66367723bb9154ce7769f` | `B3CC4C19DFDC4131BBF8F213369DA32F1` |

PDB 分别来自微软符号服务器的精确路径：

- [Taskbar.pdb](https://msdl.microsoft.com/download/symbols/Taskbar.pdb/8645A6F8C08FC8A18DACD4B77CF1E2EE1/Taskbar.pdb)
- [twinui.pcshell.pdb](https://msdl.microsoft.com/download/symbols/twinui.pcshell.pdb/A4CE45DAC5623E0776DD2D5727F080C31/twinui.pcshell.pdb)
- [SystemTray.pdb](https://msdl.microsoft.com/download/symbols/SystemTray.pdb/B3CC4C19DFDC4131BBF8F213369DA32F1/SystemTray.pdb)

独立辅助进程使用 `SYMOPT_EXACT_SYMBOLS`，且逐模块通过 `SymGetModuleInfo64` 核对 `PdbSig70`、`PdbAge=1`、`PdbUnmatched=false`、`DbgUnmatched=false`。PDB 信息流 GUID、DBI age 也与 PE CodeView 匹配。符号枚举只映射调试信息，没有用 `LoadLibrary` 加载被分析的三个 DLL，没有访问宿主 Explorer。

PDB SHA-256 依次为 `8931a245d372b301976135b6fedad2b6c26f3dd4dba10e9e9bfad3bea45c108d`、`f9f7387bfca47885d63ddb4a7d5c4f539f16144bafb440391eb24ada3949b116`、`79393394667046dd68dc56b8e7b34663d7f2f11a97f2dca618330f51655109b6`。这些系统文件和 PDB 不随产品发布；档案只包含模块元数据、地址和最小入口前缀。

## 宿主入口

六个完整修饰名分别匹配唯一 RVA；原有 `--prepare-symbols` 在独立 `LITETRAY_DATA_DIR` 中生成的 `HostLayout` 与本次独立枚举结果逐项相同。

| 角色 | 精确 PDB 符号 | 新 RVA |
| --- | --- | --- |
| primary_site | `??_7CTaskBand@@6BITaskListWndSite@@@` | `0x1CFA30` / 1899056 |
| secondary_site | `??_7CSecondaryTaskBand@@6BITaskListWndSite@@@` | `0x1D2A48` / 1911368 |
| primary_host | `?GetTaskbarHost@CTaskBand@@UEBA?AV?$shared_ptr@VTaskbarHost@@@std@@XZ` | `0x1237B0` / 1193904 |
| secondary_host | `?GetTaskbarHost@CSecondaryTaskBand@@UEBA?AV?$shared_ptr@VTaskbarHost@@@std@@XZ` | `0x17DB20` / 1563424 |
| frame_height | `?FrameHeight@TaskbarHost@@QEBAHXZ` | `0xB10A4` / 725156 |
| release_host | `?_Decref@_Ref_count_base@std@@QEAAXXZ` | `0x290B0` / 168112 |

主、副 `GetTaskbarHost` 仍将 `RDX` 作为 `shared_ptr` 隐藏结果地址，返回该地址；`_Decref` 的 `RCX` 为引用计数对象。`FrameHeight` 在 `0xB10A8` 执行 `add rcx,0x10`，因此 `host.element_offset=16`。所有函数入口位于不可写可执行节；两个站点虚表位于不可写数据节。档案里的节边界与每个可执行入口的 16 字节前缀均重新从本次精确映像读取。

## XAML source 字段与虚表

现有 `xaml_explorer_host_child_v1` 路径由新版本指令重新证明：

1. `TaskbarHost::HasFocus` 位于 `taskbar.dll+0x13883C`，在 `0x138840` 执行 `add rcx,0x38`，随后调用 `+0x75538` 的 `IXamlExplorerHostChildWindow::HasFocus` 消费接口。故宿主子接口字段仍为 `+0x38`。
2. `twinui.pcshell.dll+0x787CD8` 是精确非 WASDK 的 `produce<XamlExplorerHostChildWindow, IXamlExplorerHostChildWindow>` 虚表。槽 8 指向 `+0x4F7C0` 的 `get_XamlContent`，槽 12 对应 `get_HasFocus`。
3. `get_XamlContent` 在 `0x4F7CD` 执行 `lea rax,[rcx-0x10]`，并将调整后的对象传入 `+0x4F818` 的实现；实现在 `0x4F835` 执行 `lea rdi,[rcx+0x20]`。因此从子接口指针读取 source 的相对偏移仍是 `-0x10+0x20=+0x10`。生产路径仍需对所读对象执行准确的 `IDesktopWindowXamlSource` 查询，再核对窗口和 root 归属。
4. 工厂虚表为 `twinui.pcshell.dll+0x7A7CD0`，精确类型是非 WASDK 的 `produce<factory_implementation::XamlExplorerHostChildWindow, IXamlExplorerHostChildWindowFactory>`。槽 6 指向 `+0x167720` 的 `CreateInstance(unsigned __int64, void*, void**)`，与现有独立私有宿主检查的 ABI 相符。

新 `child_vtable_rva=7896280`、`factory_vtable_rva=8027344` 均来自 PDB，完整所需虚表范围均在不可写、不可执行数据节。没有按旧地址加统一差值推算。字段数值与 9711 相同是重新核对的结果，不能据此宣称未来模块也适用。

## 托盘坐标 ABI

| 角色 | 精确方法 | 新 RVA | 反汇编确认的 x64 ABI |
| --- | --- | --- | --- |
| invocation_point | `NotificationAreaIconsDataModel::GetInvocationPointRelativeToScreen(Point const&)` | `0xB860C` / 755212 | `RCX=this`、`RDX=Point*` 隐藏结果、`R8=const Point*`；返回原结果地址 |
| icon_bounds | `NotificationAreaIconsDataModel::GetIconBoundsRelativeToScreen(Rect const&)` | `0xB84E0` / 754912 | `RCX=this`、`RDX=Rect*` 隐藏结果、`R8=const Rect*`；返回原结果地址 |
| screen_rect | `GetScreenRectFromXamlElement(FrameworkElement, HWND)` | `0xB86EC` / 755436 | `RCX=RECT*` 隐藏结果、`RDX=FrameworkElement` 按值临时对象地址、`R8=HWND`；返回原结果地址 |

前两个函数将 `R8/RDX/RCX` 保存到 `RSI/RBX/RDI`，通过 `RBX` 写回结果，最后 `mov rax,rbx`。第三个函数将 `R8/RDX/RCX` 保存到 `R15/RSI/R14`，通过 `R14` 写回 16 字节 `RECT`，最后 `mov rax,r14`；在 `0xB8842` 起检查并释放 `RSI` 指向的按值临时对象。现有钩子先额外保留自己的接口引用，再让原函数消费原临时对象的处理方式继续适用。

三方法完整修饰名与已有实现对应，`.pdata` 标识的函数范围及实际寄存器读写均已检查，因此继续使用 `system_tray_coordinates_x64_v1`。旧 9711 配置中的七个可执行 RVA 在新 DLL 上全部无法匹配旧前缀，本次没有只替换哈希而复用旧入口。

## 验证边界与可复现材料

本次离线工作已完成：三模块/PDB 精确身份核对、六宿主符号唯一性与独立 CLI 交叉核对、source 字段调用链、两个虚表的角色与槽位、三托盘函数 ABI、全部节区及入口字节检查。生成器再次核对当前磁盘 SHA-256 后才写入新档案。

本地忽略目录 `local/compat-26300.9818/` 保留 `collect_symbols.py`、三个 `*-symbols.json`、`disassemble.py`、`disassembly.txt`、`make_profile.py`、独立 CLI 的 `taskbar-layout.json` 及精确 PDB 缓存。生成器不覆盖已有档案；可在新的证据目录复现。新档案 SHA-256 为 `ec7de41df7f12c92f8c9a6aecd156f72487713325bbd6bc8b3e350568ebb5e12`。

公共符号不能证明线程、窗口、XamlRoot、真实输入或停止恢复正常；以下记录独立完成的 Sandbox 验证。单显示器 Sandbox 不构成主副屏、混合 DPI 和第三方托盘交互的完整验收，原有通知点击与系统区域限制不能因磁盘匹配而删除。

## 5.13.2 沙盒实际接管与恢复

22:41:34 启动新生成的隔离客体，22:42:06 完成全部自动检查。宿主为 `26300.9818`，客体报告 Windows 11 Enterprise 24H2 `26100.9818`；客体三个目标模块的身份与上表一致，因此精确选择 `windows11-26300.9818-x64`，选择过程不依赖系统版本标签。禁用网络和 vGPU，程序输入只读，只有本次结果目录可写。

使用 `tools/New-SandboxCompatibilityCheck.ps1` 生成的独立 live 测试先完成四项隔离前置检查和模块完整匹配，再验证自有私有宿主，最后接入真实 Explorer。所有六个检查命令均取得退出码 0，没有超时：

| 检查 | 本轮实际结果 |
| --- | --- |
| 包与磁盘资料 | 66 个文件 SHA-256 核验通过；三个模块及四项功能的磁盘检查匹配新档案 |
| 图标与普通 XAML | 图标加载通过；164 项隔离 XAML 检查通过 |
| 私有 Shell 宿主 | 6 项通过，验证新 factory/child 虚表、source、所属窗口以及自有宿主中的路由、恢复和关闭 |
| 真实主屏时钟 | `Prepared → Active` 后依次读回 `LT LIVE 01`、`LT LIVE 02`、`LT LIVE 03`，实际宽度分别为 `240`、`320`、`320` |
| 停止恢复 | 收到 `Closed`；原生时间/日期、可见性与布局高度恢复，容器宽度从 `320` 恢复到原值 `55.20000076293945` |
| Explorer 连续性 | 前后 PID `4796`、TID `5528`、任务栏 HWND `65704` 相同；原进程保持存活，未用重启掩盖恢复失败 |
| 托盘准备 | 三个坐标入口、钩子准备及五个目标区域接口通过，随后停止准备会话；本轮没有副屏副本，也没有激活复制会话 |

后台自动检查未生成鼠标或键盘输入。日历点击、滚轮音量、中键静音、真实悬停和菜单定位仍需单独交互验收；字体、颜色、tooltip/binding 的恢复由现有会话代码处理，本轮 live 探针只独立读回文字、有效布局和可见性。不能把托盘接口准备通过写成副屏图标复制或完整菜单验收通过。

所测候选包清单 SHA-256 为 `a0039e6e2c7dc60832b0a54045910860ebdd9faf2939a02d17d0d473b793b963`，二进制身份如下。最终包的文档更新不改变这三个已实测文件：

| 文件 | SHA-256 |
| --- | --- |
| LiteTray.exe | `781c394847c07018bf5a32a3fc2581982ea501ef900ed106ba20ac1332e0cf51` |
| litetray_taskbar_adapter.dll | `1ad52d829520091ed234a099af8a0b8f494833c450c83ef2084083c4e27bff7e` |
| litetray_popup_hook.dll | `0358fad657ec1cd976956faac8309b93c57fd7d5bd12be83d9cb410b3f0cc274` |

原始报告位于被 Git 忽略的 `local/compatibility-9818-20261108-222728/sandbox-live-01/results/`，其中 `*-guest-*` 为隔离前置检查，`*-live-*` 为私有宿主及实际任务栏检查。`verification-summary.json` 保存提炼后的实际宽度和进程连续性证据。新 CLI 与两个生成/客体脚本随源码保留，可生成新的独立运行目录复测。

同一候选源码的 workspace 测试为 **231 项通过、8 项已有忽略项**，Clippy（`-D warnings`）与 release 构建通过；独立 EXE 磁盘导出测试 5 项通过。新增选择回归覆盖旧/新档案、未知与歧义身份、哈希变化、混合 shell/tray 和布局错误。新 live 记录器在 Windows PowerShell 5.1 与 PowerShell 7 各验证 5 种进程结果，非零退出、失败报告、边界不符和超时均不会被记为成功。
