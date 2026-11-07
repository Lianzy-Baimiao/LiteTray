# QQNT 悬停卡片定位

从 5.11 起默认关闭。仅在“多屏托盘 → QQ 悬停卡片副屏定位 · 实验性”显式开启后安装。此实现把 DLL 加载进 QQ 进程，无法保证不会触发 QQ 的安全检测；常规图标复制、时钟、日历、音量操作不依赖它。

旧版使用进程外 WinEvent，在收到 SHOW 时窗口已经显示，因此主屏闪现无法靠随后移动消除。本 DLL 使用 Windows 的线程级 `WH_CALLWNDPROC`，由宿主只安装到已核对的 `QQ.exe` 的 `Electron_NotifyIconHostWindow` 所属线程。

钩子只给符合已采集卡片特征的 `Chrome_WidgetWin_1` 添加窗口子类：分层、置顶、无子窗口/可调整边框/最大化样式，宽高在限定范围内。子类先执行 QQ 原来的 `WM_WINDOWPOSCHANGING`，再修正最终 `WINDOWPOS`，让 Windows 首次应用的位置就在副屏。没有 API detour，不读聊天内容，不生成鼠标或键盘输入。

修正必须同时满足：

- 配置消息来自与本 DLL 同目录的 `LiteTray.exe` 设置窗口，进程 ID 与窗口所有者匹配。
- 宿主进程仍存活、窗口仍存在，30 秒租期有效；后台线程每 10 秒刷新。
- 鼠标实际位于受管理副屏任务栏内，并位于该 QQ 接收窗口的 `Shell_NotifyIconGetRect` 图标矩形内。
- 卡片仍满足样式及尺寸限制；显式隐藏消息保持原样。

停用发送空目标集合并移除线程钩子。现存窗口子类在宿主退出/窗口销毁/租期失效后不再改动坐标，卡片销毁时移除。为避免 QQ 保留卡片时出现悬空函数指针，安装窗口子类后 DLL 保留至 QQ 进程结束；不要求重启 QQ。

`cargo test -p litetray-popup-hook` 创建隐藏原生窗口，在其原始窗口过程强制夹回主屏，连续 100 次调用真实 `SetWindowPos` 验证最终坐标；同时验证停用恢复、普通窗口排除、焦点不变。该测试不等于真实 QQ 首帧交互验收，后者仍使用 `examples/tray_windows.rs` 的自然悬停记录及严格 `Test-TrayPopupPlacement.ps1`。
