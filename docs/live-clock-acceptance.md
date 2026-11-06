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
- Taskbar multi-tray 的鼠标回调坐标、菜单、overflow 弹出位置、系统图标、控制中心与拖放。实际应用图标生成已通过下面的后续检查。
- Windows 10、其他 Windows 11 构建与长期稳定性。

## 自动宽度与应用图标复制

用户要求主副屏都接管后，已将范围保存为所有显示器。随后按用户要求增加原生 Auto 宽度，放弃固定 170/260 的设置。配置 `clock.width=0`，两行较长内容决定时钟宽度；当前短日期、星期、秒和天气在两屏测得约 112.8 逻辑像素。

`--test-secondary-tray` 实机检查中，主屏 2 个原生 NotifyIconView 在副屏生成了同样 2 个，每个宽 32；NotificationAreaIcons 为 64，隐藏图标入口为 32。测试数据路径 `local/native-tray-a.json`。时钟短内容时宽 74.4，换成长天气后宽 276.8，换回后恢复 74.4；没有修改主屏时钟。Stop 确认 Closed 后恢复。`local/auto-tray-running-a.json` 另外记录了两屏常驻的 Auto 宽度和原生图标数量。

这里复制的是原生模型与控件，非绘制截图或图标位图。集合增删传播、来源引用替换与恢复另外由真实 XAML 自检覆盖；该次后台实机检查没有模拟鼠标或更改用户的图标显示偏好，不能据此声称点击、右键和跨屏菜单位置全部通过。

最终 `local/native-tray-b.json` 通过相同检查，额外验证 StartTray 独立会话：不接管时钟时副屏仍生成应用图标，两屏显示系统时间；Stop 后实际图标集合恢复到启动前。最新通用 XAML 139 项与自有私有宿主 6 项通过，单元测试 83 项通过。
