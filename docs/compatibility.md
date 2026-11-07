# 任务栏兼容检查

5.13.0 开始将系统兼容资料集中到 [内置档案](../crates/adapter-protocol/profiles/windows11-26300.9711-x64.json)。宿主 EXE、Explorer 适配器及私有宿主隔离测试共同使用这份资料。

**当前仍只核验 Windows 11 26300.9711 x64 的特定模块。5.13.0 没有扩大系统版本支持范围。** 这版建立后续跨版本适配的基础，提供准确的分功能报告，并去掉当前已知版本在启动时对 PDB 下载和外部符号缓存的依赖。

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
| `unsupported_modules` | 模块可读取，但没有匹配此身份的内置资料 |
| `missing_modules` | 必需模块不存在；请同时查看是否发现另一代任务栏模块 |
| `inspection_failed` | 文件无法读取、PE 数据损坏或布局验证失败 |

`all_profile_images_matched` **不是正在运行或完整兼容的证明**。`loaded_modules_checked`、`runtime_topology_checked` 在这份磁盘报告中始终为 `false`。系统更新可能令磁盘文件与 Explorer 已加载的映像不同，适配器仍独立验证内存身份和函数入口。

`baseline_limitations` 记录参考版本仍存在的限制：通知点击崩溃反馈待复现、系统图标及跨屏菜单验证未全面完成、第三方悬停弹窗需单独验证。这些信息不会因磁盘匹配而被隐藏。

## 接管策略

- 当前档案内置六个宿主符号、两处对象字段、具体虚表及三个托盘坐标入口。生产启动不再调用 DbgHelp、不下载 PDB，也不读取可损坏的外部符号映射。
- 原来的 `--prepare-symbols` 保留为开发分析工具，其输出不能覆盖内置生产档案。
- 运行中的适配器继续检查实际加载路径、PE/PDB 身份、函数字节、线程、控件归属与停止恢复。未知布局仍拒绝接管。
- 这一阶段提供分功能检查，**尚未实现自动停用不兼容托盘并保留时钟的协商流程**。若当前同时请求两项而一项不兼容，整体请求仍拒绝；手动关闭托盘复制后，可以仅检查和启用时钟。
- 内置 JSON 编译进入程序。修改安装目录中的资料文件不会改变接管行为；目前没有在线兼容包或任意外部偏移覆盖入口。

## 验证与后续

`tools/Test-Compatibility.ps1` 复制单独 EXE，在无适配 DLL、故意损坏的测试配置和离线文件树上检查导出、错误分类、禁止覆盖及不修改配置/符号缓存。它适合通用 CI，但不能代替真实任务栏检查。

跨版本设计和一手资料见 [实施方案](windows11-compatibility-plan.md) 与 [研究记录](windows11-compatibility-research.md)。下一里程碑是第二个真实 Windows 11 分支的主副屏接管和恢复验收，而不只是读到另一版 DLL 的身份。

2026-11-08 本机验证：完整 workspace 测试 207 项通过；独立 EXE 导出检查 5 项通过；私有 Shell 宿主 6 项、原生 XAML 159 项通过。后台切换至 5.13.0 后，主副屏 source/时钟/拓扑/几何检查无错误，托盘图标模型顺序、宽度和对齐一致；用户配置及日程文件哈希保持不变。上述结果仍只对应当前参考系统，不涵盖其他 Windows 11 分支。
