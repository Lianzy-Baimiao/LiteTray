# 任务栏兼容检查

5.13.0 开始将系统兼容资料集中到内置档案。当前同时保留 [Windows 11 26300.9711 x64](../crates/adapter-protocol/profiles/windows11-26300.9711-x64.json) 和新增的 [26300.9818 x64](../crates/adapter-protocol/profiles/windows11-26300.9818-x64.json) 两组精确模块资料；宿主 EXE、Explorer 适配器及私有宿主隔离测试共同使用同一套选择规则。

**当前内置支持范围为上述两组已核验的 x64 模块，验收范围按功能和场景分别记录。** 9818 已通过单显示器 Sandbox 中的真实主屏时钟接管、更新和停止恢复，以及托盘入口准备/停止检查；日历点击、滚轮音量和托盘菜单等实际输入尚未验收。它是新增累计更新模块组，不代表完成另一独立 Windows 11 分支或完整多屏矩阵。符号、ABI 与本次实测证据见 [9818 兼容记录](compatibility-9818.md)。

## 导出方式

设置 → 常规设置 → **导出诊断信息**，后台生成 `%LOCALAPPDATA%\LiteTray\diagnostics.json`，其中 `compatibility` 包含磁盘兼容检查。

也可以单独运行，不读取用户配置、不加载适配 DLL、不访问 Explorer 进程、不联网：

```powershell
.\LiteTray.exe --compatibility-report .\compatibility.json
```

报告文件必须尚不存在，避免覆盖其他文件。检查从其他 Windows 镜像提取的目录：

```powershell
.\LiteTray.exe --compatibility-report .\offline.json --system-root 'D:\Extracted\Windows'
```

此处的目录须保留 `System32`、`SystemApps` 等相对结构。不加载其中的 DLL，不要求该系统可启动。离线报告的 `os_build` 为 `null`，不会错误套用正在运行的系统版本。

## 读懂结果

报告分开列出 `clock_appearance`（时钟文字/样式）、`clock_input`（时钟交互）、`tray_mirroring`（应用托盘复制）和 `system_areas`（系统区域）。后两项额外依赖托盘坐标入口。

| 状态 | 含义 |
| --- | --- |
| `disk_matched_live_check_required` | 磁盘模块和内置布局匹配；接管前仍必须在所属线程核验已加载模块、窗口、对象及 XamlRoot |
| `unsupported_modules` | 模块可读取，但未匹配唯一内置资料，或身份/SHA-256 与所选资料不符 |
| `missing_modules` | 必需模块不存在；请同时查看是否发现另一代任务栏模块 |
| `inspection_failed` | 文件无法读取、PE 数据损坏或布局验证失败 |

`all_profile_images_matched` **不是正在运行或完整兼容的证明**。`loaded_modules_checked`、`runtime_topology_checked` 在这份磁盘报告中始终为 `false`。系统更新可能令磁盘文件与 Explorer 已加载的映像不同，适配器仍独立验证内存身份和函数入口。

`profile_id` 由 `taskbar.dll` 的完整 PE/PDB 身份唯一选择，并核对同一次读取所得的 SHA-256。随后 `twinui.pcshell.dll` 和 `SystemTray.dll` 必须匹配同一份资料；不会拼接不同版本的模块。任务栏身份未知、匹配歧义或哈希不符时，`profile_id` 为 `null`，`profile_selection_reason` 说明原因，不默认归到 9711。资料选中后仍可能有其他模块或布局检查失败，应同时读取各模块及各功能的状态。

`baseline_limitations` 记录参考版本仍存在的限制：通知点击崩溃反馈待复现、系统图标及跨屏菜单验证未全面完成、第三方悬停弹窗需单独验证。这些信息不会因磁盘匹配而被隐藏。

## 接管策略

- 每份档案内置六个宿主符号、两处对象字段、具体虚表及三个托盘坐标入口。9818 的地址和入口字节来自对应微软 PDB 及映像重新解析；字段和 ABI 经过重新核验后复用现有解析方案。生产启动不再调用 DbgHelp、不下载 PDB，也不读取外部符号映射。
- 原来的 `--prepare-symbols` 保留为开发分析工具，其输出不能覆盖内置生产档案。
- 运行中的适配器继续检查实际加载路径、PE/PDB 身份、函数字节、线程、控件归属与停止恢复。未知布局仍拒绝接管。
- 这一阶段提供分功能检查，**尚未实现自动停用不兼容托盘并保留时钟的协商流程**。若当前同时请求两项而一项不兼容，整体请求仍拒绝；手动关闭托盘复制后，可以仅检查和启用时钟。
- 内置 JSON 编译进入程序。修改安装目录中的资料文件不会改变接管行为；目前没有在线兼容包或任意外部偏移覆盖入口。

## 验证与后续

`tools/Test-Compatibility.ps1` 复制单独 EXE，在无适配 DLL、故意损坏的测试配置和离线文件树上检查导出、错误分类、禁止覆盖及不修改配置/符号缓存。它适合通用 CI，但不能代替真实任务栏检查。

当前证据范围：

| 模块资料 | 已有验证 | 尚不能据此宣称 |
| --- | --- | --- |
| 26300.9711 x64 | 保留原有内置资料；更新前已有主副屏 source、时钟、拓扑、几何与托盘模型顺序/宽度/对齐记录 | 本次 9818 候选包重新完成了 9711 的真实桌面回归，或旧有全部交互限制已经解决 |
| 26300.9818 x64 | 新资料精确匹配；Sandbox 基线四项命令均 exit 0、原生 XAML 164 项通过；应用进程内私有 Shell source 6 项通过；真实主屏时钟宽度 240→320，停止后恢复原文字、宽度和可见性；5 类必需托盘区域与坐标入口完成准备/停止，未启用托盘复制 | 日历点击、滚轮音量、托盘复制及菜单/悬停交互、多显示器和混合 DPI 已完成验收 |

9818 的真实任务栏测试期间，Explorer PID/TID 与主任务栏 HWND 保持不变，未以重启 Explorer 代替恢复。Sandbox 来宾标为 Enterprise 24H2 `26100.9818`，但本次三份关键模块与宿主 `26300.9818` 相同；该标签不构成独立 24H2 系统镜像覆盖。详细结果与证据路径见 [9818 兼容记录](compatibility-9818.md)。

跨版本设计和一手资料见 [实施方案](windows11-compatibility-plan.md) 与 [研究记录](windows11-compatibility-research.md)。下一里程碑仍是独立 Windows 11 分支的主副屏完整功能矩阵；本次新增模块组的主屏接管结果不满足这一完整退出条件。

2026-11-08 更新前的 9711 / 5.13.0 基线记录：完整 workspace 测试 207 项、独立 EXE 导出检查 5 项、私有 Shell 宿主 6 项、原生 XAML 159 项通过。后台切换至 5.13.0 后，主副屏 source/时钟/拓扑/几何检查无错误，托盘图标模型顺序、宽度和对齐一致；用户配置及日程文件哈希保持不变。这是历史基线，不是本次 9818 候选包在 9711 上重跑的结果。
