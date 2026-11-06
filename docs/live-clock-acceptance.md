# 原位时钟实机检查 · 2026-11-06

环境：Windows 11 26300.9711 x64。主屏 DISPLAY1 为 3840×2160，副屏 DISPLAY2 为 2560×1600，位于主屏右侧。两条任务栏由同一 Explorer 线程持有。用户自行关闭 Windhawk；测试没有改动 Windhawk 配置，也没有重启 Explorer。

## 已验证

1. 新版设置真实启动，显示原生日期网格与天气卡片。用户要求不干扰主屏游戏后，停止 UI 输入，改为后台测试。
2. `LiteTray.exe --test-secondary-clock <report.json>` 只计划副屏目标。Start 返回 Active；原生 `TimeInnerTextBlock` 连续保留 `LT CHECK 01` 到 `LT CHECK 06`，每次在系统刷新后读取实际任务栏树核对。主屏没有出现测试内容。
3. Stop 返回 Closed，副屏恢复当时最新的系统时间与日期。测试前后主屏保持系统时间。
4. 独立配置 `clock_enabled=true`、选定副屏，以 `--background` 启动。21:07:15 与 21:09:03 两次原生读取分别显示对应秒数，第二行均为真实天气“晴间多云 26°C”。整个过程中设置窗口未显示。
5. 核对测试进程路径后终止该后台实例，模拟崩溃。2 秒后重新读取 Explorer，副屏已恢复系统的时间和日期；Explorer 进程持续存活。

后台宿主 PID32012 曾观测到工作集 22,323,200 字节、私有内存 3,424,256 字节、累计 CPU 0.0625 秒。数据仅描述该次宿主，不包含 DLL 对 Explorer 的增量成本。后续重新启动的进程不沿用此 PID。

## 本地证据

原始报告位于忽略提交的 `local/`：`secondary-clock-test-1.json`、`secondary-running-1.json`、`secondary-running-2.json`、`secondary-owner-exit.json`。这些报告来自真实任务栏元素，非预览窗口或模拟文本。

## 尚未验收

- 真实鼠标点击原位时钟打开日历、键盘激活和悬停提示。
- Explorer 重启、睡眠、显示器热插拔与混合 DPI 变化。服务逻辑和退避有自动测试，但未主动改变用户桌面环境。
- Taskbar multi-tray 的应用级悬停、菜单、overflow 弹出位置、控制中心左键弹层与拖放。应用和系统图标显示及位置已通过下面的后续检查。
- Windows 10、其他 Windows 11 构建与长期稳定性。

## 自动宽度与应用图标复制

用户要求主副屏都接管后，已将范围保存为所有显示器。随后按用户要求增加原生 Auto 宽度，放弃固定 170/260 的设置。配置 `clock.width=0`，两行较长内容决定时钟宽度；当前短日期、星期、秒和天气在两屏测得约 112.8 逻辑像素。

`--test-secondary-tray` 实机检查中，主屏 2 个原生 NotifyIconView 在副屏生成了同样 2 个，每个宽 32；NotificationAreaIcons 为 64，隐藏图标入口为 32。测试数据路径 `local/native-tray-a.json`。时钟短内容时宽 74.4，换成长天气后宽 276.8，换回后恢复 74.4；没有修改主屏时钟。Stop 确认 Closed 后恢复。`local/auto-tray-running-a.json` 另外记录了两屏常驻的 Auto 宽度和原生图标数量。

这里复制的是原生模型与控件，非绘制截图或图标位图。集合增删传播、来源引用替换与恢复另外由真实 XAML 自检覆盖；该次后台实机检查没有模拟鼠标或更改用户的图标显示偏好，不能据此声称点击、右键和跨屏菜单位置全部通过。

最终 `local/native-tray-b.json` 通过相同检查，额外验证 StartTray 独立会话：不接管时钟时副屏仍生成应用图标，两屏显示系统时间；Stop 后实际图标集合恢复到启动前。最新通用 XAML 139 项与自有私有宿主 6 项通过，单元测试 83 项通过。

## 副屏整组对齐与 QQNT 悬停修复

用户指出副屏托盘未与主屏对齐后，补充从已验证 XAML source 读取物理矩形、模型身份、鼠标命中和启用状态的只读诊断。修复前 `Test-TrayAlignment.ps1 -ReportPath local/tray-geometry-before.json` 稳定失败：副屏缺少 MainStack、NonActivatableStack、ControlCenterButton。应用图标数量相等不足以证明整组同步。

补齐这三组及 ShowDesktopStack；模型、Width/MinWidth、Opacity、IsHitTestVisible 和 Visibility 统一捕获、同步、恢复。未固定托盘总宽度。`local/tray-geometry-after.json` 实际结果：主屏托盘 `[3335,2100,3840,2160]`，副屏 `[5895,1540,6400,1600]`，均为 404 DIPs。七组可见区域从屏幕右侧计算的偏移一致，两个应用图标的模型身份及顺序一致。后续 `local/tray-hover-running-1.json`、`tray-hover-running-2.json` 仍通过更严格的宽度、水平/垂直对齐检查。原生图标和父区域 HitTest=true、Enabled=true、Opacity=1。

`local/tray-alignment-live-test.json` 的副屏实机检查通过 Start/Active、长短文本 Auto 宽度、六次走时、独立 StartTray 与 Stop/Closed 恢复。Explorer PID6584/启动时间未变。测试期间没有鼠标/键盘输入，没有重启 Explorer。

随后用户报告副屏 QQNT 悬停无弹层。新增直接调用生产路由路径的真实 XAML island 回归，首次失败为 `message=0x113, target=0`。修复 WM_TIMER、鼠标悬停/离开及完整 pointer 消息范围，并按实际任务栏根窗口确定来源。相同回归转为通过，现共有 146 项原生 XAML 检查。程序已后台加载修复，并在只读报告中保留最近 32 次托盘坐标转换用于下一次真实悬停核对；截至 `tray-hover-running-2.json` 尚无新悬停样本，因此不声称 QQNT 实际弹层已经验收。

## QQNT 弹层横坐标钳制 · 2026-11-07

`local/tray-popup-all-qq.jsonl` 终于抓到真实 QQNT 卡片：鼠标位于副屏任务栏 `[5967,1572]`，QQ.exe 的 `Chrome_WidgetWin_1` 窗口为 `[3490,898,3840,1541]`。宽 350、高 643，横坐标贴住主屏右边界 3840，纵坐标却来自副屏。此时 `Shell_NotifyIconGetRect` 对该 QQ 进程的 icon ID 3 返回副屏矩形 `[5975,1540,6015,1600]`。由此确认修正入口应在 QQ 卡片自己的摆放阶段。此前记录的 10×12 `Xaml_WindowedPopupClass` 不能作为 QQ 卡片证据。

新增 `platform/tray_popup.rs`，通过进程外 WinEvent 监听 QQ 的窗口显示与移动。仅在物理鼠标位于已管理副屏的该进程托盘图标内时，接管匹配类名、样式与尺寸的悬停卡片；调整位置时保留焦点、窗口尺寸和 Z 序。普通聊天窗不符合候选条件。每次卡片最多四次纠正，窗口隐藏或短时间窗口到期后解除跟踪。最近纠正记录保存在本机 `popup-placement.json`。

自有原生窗口回归确认候选匹配、目标位置、尺寸不变、鼠标和前台窗口不变。旧真实样本经 `Test-TrayPopupPlacement.ps1` 得到 FAIL，证明观测能识别原问题。00:22 左右，后台观察器 `tray-popup-after-f.jsonl` 记录两轮真实副屏悬停，最终位于 `[5820,889,6170,1532]`，中间主屏悬停保持在主屏。首轮在主屏错误位置观测到卡片，约 215ms 后转到副屏；严格逐帧检查仍为 FAIL，最终位置修正通过，不称为无闪动。记录文件名沿用观察器启动版本，实际修正时运行 g 版本。

## 开发版 4 外观与日程

副屏以 `--show-secondary` 无激活方式实际显示了日历、日程页面，检查了深色下拉框、输入框、按钮、标题栏图标和固定底部操作区。主屏没有输入注入，也没有重启 Explorer。`local/ui4-live-final.json` 的主副屏托盘宽度、水平/垂直位置和应用图标顺序一致，自动宽度配置仍为 0。

`local/ui4-icon-verify.json` 验证 EXE 包含九个图标帧，按 96/120/144/192 DPI 加载出预期像素尺寸。日程/计时的原生控件检查创建六种条目并重载本地文件，验证暂停/继续与专注操作；到期和去重由模型测试覆盖，没有为了验收向用户桌面触发测试提醒。

离屏检查覆盖十页浅深色、长页底部与日历多 DPI，图片为示例数据。已修复离屏渲染时非客户区偏移造成的标题/内容错位。日历农历回归包括 2026 春节、中秋和 2025 闰六月。系统主题变化通知、系统强调色、高对比度的读取路径已实现；未主动切换用户系统主题，高对比度现场和多屏混合 DPI 仍待进一步验收。

## 日期切换闪烁与右键崩溃补丁

日期切换回归直接点击自有隐藏窗口的真实日期按钮。旧代码每次销毁 49 个原生控件，连续两次测试稳定失败；修复改为保留控件与字体、更新前后两格及当日安排，并对按钮缓冲绘制。测试覆盖浅深色、设置/弹出面板、每组连续 60 次点击、跨年方向键、前后月份、回到今天和秒刷新。检查窗口及按钮身份、被重绘区域与天气区不变；全过程不激活桌面窗口。

用户报告右键导致任务栏重启后，Windows Winlogon 日志记录 00:35:44、00:35:56、00:36:07 三次外壳意外停止。隔离进程调用生产 `shared_target` 路径，旧实现稳定以 `0xC0000409` 终止，调试器读到 fast-fail 原因 10。原因是 `VisualTreeHelper.GetParent` 的入参应为 `IDependencyObject`，却传入同一对象的 `IFrameworkElement` 指针。修正查询接口后通过；通用树遍历入口也做相同规范化。

新增原生检查覆盖无父级目标、普通任务栏嵌套目标和共享系统托盘目标，从 FrameworkElement、DependencyObject 和 IUnknown 三种接口进入。全部 149 项原生检查通过，94 项自动测试与 Clippy 通过。当前旧 h 版 DLL 固定加载在 Explorer PID31420 中；没有为了测试主动重启 Explorer，新补丁尚待重新加载后实机右键验收。
