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
- Taskbar multi-tray 的实际图标、菜单、overflow、控制中心与拖放。已有底层模型事务不等于具备完整交互支持。
- Windows 10、其他 Windows 11 构建与长期稳定性。
