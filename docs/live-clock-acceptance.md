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

## 用户授权重启并切换补丁 · 2026-11-07 00:49

停止旧 h 版 LiteTray PID43448 后，仅重启持有任务栏的 Explorer PID14404；另一个文件管理器进程 PID12816 保留。新任务栏 Explorer PID39992，启动于 00:49:35，新 LiteTray PID32060 从 `dist/LiteTray-ui4-calendar-menu-fix` 后台启动。`local/menu-fix-deployment.json` 记录切换前后的 PID、路径和时间。

模块枚举确认新 Explorer 中只有一份适配 DLL，路径为 menu-fix 目录，旧 h 版已不在该进程中。`local/menu-fix-live.json` 检查主副屏都显示 `11/07 周六 00:50:34` 和 `晴间多云 24°C`，完整托盘宽度、水平/垂直位置和图标身份顺序通过。配置仍为系统外观、所有显示器、Auto 宽度 0、图标复制开启。副屏无激活显示日程页，深色控件与空列表提示正常。此次未发送鼠标键盘输入，真实右键菜单仍待现场复测，不能用模块加载成功代替交互验收。

## 2026-11-07 开发版 5 部署与四项修正

- 01:17 完成实际切换：宿主 PID 7816，任务栏 Explorer PID 24928，运行目录 dist/LiteTray-ui5-responsive。只重启了任务栏所属 Explorer，QQ PID 12728 启动时间仍为 11-06 18:19:42。系统应用错误日志未发现部署后的 Explorer、QQ 或 LiteTray 崩溃。
- 新 QQ 钩子 DLL 为 282112 字节，已在 QQ 通知线程 12724 确认 ready=true；旧的显示后 WinEvent 搬移实现已移除。100 次原生隐藏窗口回归通过。90 秒自然观察期间没有副屏 QQ 悬停样本，严格脚本返回 INCONCLUSIVE，尚不能声称真实首次显示无闪现已验收。
- 有效时钟点击不再被 Explorer Busy 状态丢弃；各屏日历已在运行进程中预建并保持隐藏。自有窗口预热约 327ms，缓存准备/更新 p95 约 459us（100 次，不包含系统输入和合成显示时间）；真实点击体感待用户复测。
- 日程改为六种类型 Tab，切换保留各类型未保存输入。计时页拆为专注、定时提醒、记录、铃声；最小 1060×820 窗口下逐个检查全部 Tab，控件均在视口内，外层滚轮不移动页面。
- 铃声支持 Windows Alarm01–10、本地 WAV、静音、5–120 秒响铃、5 秒试听和停止。原生页面保存/重载验证通过；本轮没有出声试听，避免影响主屏游戏。
- 96 项自动检查、Clippy -D warnings、149 项隔离原生 XAML 检查通过。新版实机检查确认主副屏时间/天气一致，托盘宽度、右侧和垂直对齐、应用图标顺序通过。
- 本地证据：local/ui5-deployment.json、local/ui5-live.json、local/ui5-window-state.json、local/ui5-xaml-check.txt、local/ui5-popup-observations.jsonl；界面检查图在 local/ui5-final-renders。

## 2026-11-07 开发版 5.1：点击留白、置顶与分离

用户已确认 QQNT 指向弹层问题修复；保留该逻辑。继续处理偶发时钟点击不显示：真实任务栏树中的 DateTimeStackPanel 只有 112.8×32 DIP，而专属 SystemTray.IconView 为 120×48 DIP，旧订阅遗漏上下留白。结构回归先得到错误节点 7（预期 2），修复后通过；只有包含 DateTimeIconContent 和两行文字、无其他交互控件的专属 IconView 可扩展，通知铃铛等兄弟区域不拦截，整个输入子树纳入会话守卫。Tapped 改为接收已 handled 事件，使用标准 WinRT IReference<TappedEventHandler> 盒子注册/解除。

原生窗口复现打开后 WA_INACTIVE 立即隐藏。现在开窗时先置顶，延后确认稳定前台状态；初始任务栏焦点回跳不关闭，普通弹层在后续应用切换、点击外部或 Esc 时关闭。分离后停止自动关闭观察，顶部日期/空白区返回 HTCAPTION，可移动；右上角出现关闭按钮。分离窗口不会被新弹层、设置页切换、主题刷新挪动或销毁，跨 DPI 调整自身尺寸，显示器断开后限制在可见工作区。

99 项自动测试、Clippy -D warnings、150 项隔离 XAML 检查通过。离屏分离测试覆盖 100 次失焦、Esc、第二弹层、主题刷新、原生标题栏命中和关闭。01:42:55 重启任务栏 Explorer 至 PID 37996；新版宿主 PID 9260，路径 dist/LiteTray-ui5-detachable/LiteTray.exe。实际主副屏时间一致、托盘顺序及对齐通过，两个预建日历均 WS_EX_TOPMOST。对已运行新版的隐藏窗口执行分离、100 次失焦和关闭，按钮状态正确、焦点未变。QQ 钩子 ready=true，QQ 未重启。部署后未发现相关应用崩溃事件。

这些验证没有移动主屏鼠标或模拟实际 taskbar 点击；偶发点击问题仍需用户在新版下连续操作复测，不把源代码/隐藏测试当作随机问题已完全消除的实机证明。本地证据：local/detachable-tests.txt、local/detachable-xaml-final.txt、local/detachable-deployment.json、local/detachable-live.json、local/detachable-window-state.json、local/detachable-live-controls.json。

## 2026-11-07 开发版 5.2：独占日历点击与设置页重整

用户报告 5.1 点击时系统日历和 LiteTray 同时出现。5.1 将订阅挂在原生 IconView 并接收已处理事件，可能在类处理器已经执行之后才投递自定义动作。控件层级的点击顺序回放先得到 LiteTray=1、原生=1；5.2 在 DateTimeIconContent 内部选择覆盖完整按钮的最内层布局，普通 Tapped 在冒泡到两个原生类之前处理，已 handled 事件不再投递。空背景用可恢复的透明画刷补齐命中范围。原生订阅、停止恢复、外部属性写入保护继续使用既有租约。

设置页不再在每次导航时销毁整个子控件树：侧栏、标题、底栏保持原 HWND，新内容在隐藏的合成绘制视口内准备后切换。页面布局集中到 settings.rs，原生字体列表、多行输入和时钟预览集中到 widgets.rs。十页按日常、任务栏、应用分组，重新整理时钟、多屏、天气、常规、更新、迁移页面。字体和字号可直接选择；选色器、四种自动宽度预设与同步预览已接入。多行输入有内边距，仅溢出时显示垂直滚动条。页面切换保留草稿，日程与计时继续复用既有类型切换和数据模型。

更新检查取消 GitHub 凭据入口及读取，匿名查询 Releases 和 Tags（分页、语义版本排序、预览版过滤、同版本优先 Release）。只有 Tag 时明确说明没有安装包；404 表示暂不可公开访问，不再要求保存 Token，也不报告为最新。开发仓库公开后无需更换更新流程。

验证：103 项自动测试、Clippy -D warnings、153 项独立 XAML 检查；十页浅深色、计时子页和最小尺寸渲染。新增检查覆盖侧栏 HWND 保留、字体列表、草稿保留、多行滚动条、预设合法性、Tag-only、分页与 Release 优先、私有仓库状态。交互桌面上的旧检查曾因用户自然移动鼠标/切换前台而误失败：测试现允许外部输入变化，仍拒绝任何测试进程窗口获取前台，隐藏定位检查保留 100 次最终位置断言。QQ 正式钩子逻辑未改；未注入鼠标/键盘。XAML 委托与结构回放不冒充 Explorer 的真实点击验收，实际自然点击仍由用户反馈确认。

5.2 已于 2026-11-07 02:19 后台部署到 `dist/LiteTray-ui52-settings/LiteTray.exe`（PID 36060）。只重启了承载任务栏的 Explorer：37996 → 19724，实际仅加载该目录的新适配 DLL。文件管理器 Explorer 12816 和 QQ 主进程 12728 的启动时间未变。真实主副屏时间一致，托盘图标顺序、宽度、水平及垂直对齐检查通过；QQ 钩子 ready=true。两个预建日历 HWND 101519186 / 116527452 隐藏且置顶，设置窗保持隐藏。包内 SHA256 全部核对通过。未执行鼠标或键盘注入，也未显示设置窗来打断主屏。

含额外 GitHub 联网检查的组合命令被自动审批拒绝，仅返回 `blocked by policy`；本地打包、既有授权的版本切换和只读实机检查随后分别成功。公开接口联网实测不计入通过结果；更新逻辑的匿名请求、分页、Tag/Release 优先与错误状态通过自动测试。自然鼠标点击的原生日历互斥结果仍待用户确认。

## 2026-11-07 开发版 5.3：国务院放假与调休安排

接入 NateScarlet/holiday-cn（MIT），内置 2025–2027 年、commit ac19c3a7e7fc9208a4be9ecccd3cb1049b0e8092 的快照，并在后台按月份及相邻年份更新。真实访问了 2026、2027 国务院通知原文；匿名联网检查成功取得 2027 年 34 个公告日期并写入临时缓存。原始 GitHub 失败后尝试 jsDelivr，验证年份、日期范围、名称、重复日期和国务院原文域名；有效缓存 24 小时，失败一小时后重试，错误响应不覆盖已有数据。

设置首页和托盘日历共用同一份假日数据。日期角标“休／班”、节日名称、所选日期的放假或调休补班说明及“放假表”原文入口均已接入；关闭农历显示不会关闭假日标记。农历详情、日程标记、分离和天气并排布局保留。数据更新复用日期 HWND 和字体，仅失效变化区域。未知年份不猜测放假或工作日；日程“工作日”重复继续按周一至周五。

115 项自动测试、Clippy -D warnings 通过；六组浅深色、100/150/200% 与天气空状态离屏图通过检查。修正假日说明和分隔线重叠后重新渲染、测试、构建。新增回归覆盖放假和补班、跨年公告、未公布年份、缓存命中、304、备用下载、坏数据保护、首次断网及隐藏原生日期控件不重建。任务栏适配和 QQ 钩子生产代码未改，两个 DLL 的 SHA-256 与 5.2 相同。

02:38 已切换到 dist/LiteTray-ui53-holidays/LiteTray.exe（PID 13408）。因适配 DLL 校验宿主与自身同目录，切换包目录时按既有授权重启任务栏 Explorer：19724 → 39580。文件管理器 Explorer 12816、QQ 主进程 12728 的启动时间未变。主副屏托盘顺序、宽度、右侧及垂直对齐通过，QQ 钩子 ready=true。正式配置目录已生成 2026.json 与 2027.json 缓存。

运行中的隐藏日历实际翻到 2027 年 2 月，读回“02月06日，春节 · 放假”“02月14日，春节 · 调休补班”，来源按钮显示“2027 国务院安排 · 已更新”；日期控件句柄保持不变，随后恢复原月份。设置与两个置顶日历都保持隐藏，应用未获取前台。没有注入鼠标或键盘；自然点击交互仍以用户使用结果为准。

本地证据：local/holiday-tests-final.log、local/holiday-clippy-final.log、local/holiday-online.log、local/holiday-renders/、local/ui53-deployment.json、local/ui53-live.json、local/ui53-holiday-live.json。

## 2026-11-07 开发版 5.4：SVG 工具栏与弹层内日程维护

弹层顶部的专注、刷新、设置、分离及关闭采用原始 SVG 线条图标，月份箭头、新建及完整管理入口使用同一套图标。SVG 路径嵌入程序，由原生 GDI 按 DPI 超采样绘制，无新增运行时依赖。图标保留原生命名按钮、键盘焦点与悬停提示。移除“放假表”及其原文打开消息入口；公告来源继续记录在数据文档中。放假改用红色实底“休”，补班使用蓝色实底“班”，高对比度下采用 Windows 配色。

点击加号或当日事项，在同一弹层右侧打开 quick_agenda 编辑区，左侧日期仍可直接选择。支持六类事项、标题、日期、可选时间、重复、提前提醒、备注、完成与启用状态，删除采用区内二次点击确认。通过既有 Store::change 原子保存，错误不覆盖数据并保留输入。超过三条事项可在弹层翻页。显式的完整管理图标仍打开大窗口；普通新建、修改、删除没有 OPEN_DATE / OPEN_ENTRY 跳转。

暂时隐藏弹层保留草稿，主题刷新不销毁带有编辑内容的日历；跨 DPI 调整复用输入控件。切换到其他事项前保护未保存的输入，Esc 先关闭编辑区。日历仍复用日期按钮和字体，天气闲置时与日历并排。输入框的文字高度与绘制外框分离并加入统一内边距。

117 项自动测试与 Clippy -D warnings 通过。新增原生检查覆盖六类事项的保存重载、修改、无效时间不保存、删除确认、日程翻页、不跳转大窗口、主题刷新保留草稿、保存失败保留输入及 Esc。十张原生渲染图覆盖浅深色、100/150/200% 与天气空状态／内嵌编辑状态，控件边界检查通过。任务栏适配 DLL 和 QQ 钩子 DLL 与 5.3 的 SHA-256 相同。

03:02 后台切换到 dist/LiteTray-ui54-inline/LiteTray.exe，宿主 PID 31380；按已有授权重启任务栏 Explorer 39580 → 34284，文件管理器 Explorer 12816 的启动时间未变。主副屏图标顺序、宽度、右侧和垂直对齐通过。当前 QQ 主进程 20412 已于 02:55 启动，早于本次切换；没有执行 QQ 重启，钩子 ready=true。

真实副屏预建日历 HWND 7930480（left=4389）在隐藏状态下验证新建入口，确认创建内嵌编辑区、六类标签及保存入口，然后取消关闭。放假表按钮不存在，设置窗口始终隐藏，应用未获取前台，agenda.json 哈希前后相同。未注入鼠标或键盘；这些隐藏检查不冒充自然鼠标操作验收。

本地证据：local/ui54-tests-final.log、local/ui54-ui-tests-final.log、local/ui54-clippy-final.log、local/ui54-render-final.log、local/ui54-renders/、local/ui54-deployment.json、local/ui54-live.json、local/ui54-inline-live.json。

## 2026-11-07 开发版 5.4.1：设置首次点击丢失与最小化恢复

用真实原生命名按钮的 BM_CLICK 贯穿“设置图标 → 日历 → 弹层 → 主窗口”消息链，并将窗口放在不会切换到前台的独立测试桌面。修复前 `tools/Invoke-Cargo.ps1 -CargoArguments @('test','--bin','LiteTray','settings_open_first_click','--locked','--','--test-threads=1','--nocapture')` 为 1 通过、2 失败：主窗口重入时 visible=false；先最小化再隐藏时 visible=true 但 restored=false。额外回归确认日历 View 借用期间设置图标也会被忽略，以及旧流程会先隐藏源弹层再显示设置。

根因是两层 try_borrow 失败直接消费设置命令、SW_SHOW 没有恢复最小化窗口，以及源弹层过早隐藏。settings_open.rs 统一设置打开入口，将待处理请求保存在 App 借用之外；界面重入或原生模态窗口期间延后执行，关闭时取消未执行请求。日历设置图标先转发、无需借用日历数据。显示和激活发生在释放 App 借用之后，SW_RESTORE 恢复最小化窗口；普通弹层在设置可见后收起，分离弹层保持。窗口属性记录请求数、完成数、耗时与前台句柄，点击路径不写日志文件。

单独验证每项修复后，7 项设置回归全部通过，其中同一进程内循环 100 次覆盖齿轮、托盘命令和 WM_SHOW_SETTINGS，并交替重入和最小化，检查每次请求只执行一次且设置草稿保留。完整测试共 124 项通过，Clippy -D warnings 通过。将这组激活测试与旧 UI 测试混在同一进程时曾出现跨桌面环境等待；现每项在全新测试子进程中创建非输入桌面，并设 20 秒失败上限。没有调用 SwitchDesktop，没有向用户桌面注入输入。独立桌面验证不替代用户游戏环境中的自然鼠标点击验收。

03:20 已部署 5.4.1。两个 DLL 哈希与 5.4 完全相同，因此在现有 dist/LiteTray-ui54-inline 目录只更新宿主与文档，旧 EXE 备份到 local/LiteTray-ui54-before-settings-open.exe；另有完整包 dist/LiteTray-ui541-settings-open。宿主 PID 31380 → 42440，任务栏 Explorer 保持 PID 34284、启动于 03:02:21，文件管理器 Explorer 12816 与 QQ 20412 也未重启。主副屏托盘图标顺序、宽度、右侧和垂直对齐通过，QQ 钩子 ready=true。只读检查确认实际窗口标题为 5.4.1，设置与日历保持隐藏，未模拟正式实例的设置点击来抢占用户前台。

本地证据：local/settings-open-before.log、local/settings-open-queued.log、local/settings-open-restored.log、local/settings-open-handoff-before.log、local/settings-open-calendar-before.log、local/settings-open-regression.log、local/settings-open-tests-final.log、local/settings-open-clippy-final.log、local/settings-open-deployment.json、local/settings-open-live.json、local/settings-open-window-state.json。

## 2026-11-07 开发版 5.4.2：隐藏启动覆盖首次设置显示

用户反馈 5.4.1 第一次点击仍打不开。03:27 读取正式实例的窗口属性：Requested=1、Completed=0、Activated=1，但设置窗口 visible=false。命令已经抵达；5.4.1 的测试没有覆盖真实隐藏启动。

根因是发布时使用 Start-Process -WindowStyle Hidden。Windows 会用 STARTUPINFO 的 SW_HIDE 覆盖进程第一次 ShowWindow 的 SW_SHOW；日历通过 SetWindowPos 显示，不会提前消耗这个覆盖。旧测试提前 ShowWindow(SW_HIDE) 或显示弹层，绕过了真正的首次调用；同一进程重复 100 次也无法覆盖新的进程启动条件。

先增加真实 STARTF_USESHOWWINDOW 的全新子进程测试，不提前隐藏设置。命令 `tools/Invoke-Cargo.ps1 -CargoArguments @('test','--bin','LiteTray','process_startup','--locked','--','--test-threads=1','--nocapture')` 修复前：隐藏启动的齿轮点击、直接打开都失败（visible=false、Requested=1、Completed=0），普通启动通过。直接打开的最小复现排除了日历转发与收起行为。

修复后首次显示立即检查可见性及最小化状态；若启动参数覆盖了显示，在同一请求内调用 SW_RESTORE。确认窗口可见且未最小化后再申请前台。直接启动设置也走统一入口。保留 ShowRetried 窗口属性，便于区分首次显示覆盖。没有增加定时重试、启动延迟或网络操作。

新增 4 项进程启动回归（隐藏启动齿轮、隐藏启动直接打开、普通启动对照、最小化启动）。设置相关 11 项通过，完整测试 128 项通过、4 项原有测试忽略，Clippy -D warnings 通过。子进程仅使用非输入桌面，不调用 SwitchDesktop。

另外使用实际发布 EXE 在独立非输入桌面和独立测试配置中冷启动。5.4.1 齿轮入口再次失败；5.4.2 齿轮、托盘设置命令、直接打开三个入口分别在全新进程中一次成功：Requested=Completed=1，visible=true，minimized=false，ShowRetried=1，内部耗时约 35–43ms。测试包配置关闭任务栏接入和天气；没有操作正式实例的设置入口或用户鼠标键盘。

证据：local/settings-hidden-startup-before.log、settings-hidden-startup-minimized.log、settings-hidden-startup-fixed.log、settings-hidden-startup-tests.log、settings-hidden-startup-clippy.log、settings-hidden-startup-build.log；实际包对照脚本 local/Check-HiddenStartupPackage.ps1，结果 local/hidden-startup-packages/5.4.1-before.json 与 5.4.2-after.json。

03:35 已切换至 5.4.2，正式宿主 PID 42440 → 44300，保留原目录 dist/LiteTray-ui54-inline；旧 EXE 备份 local/LiteTray-ui541-before-hidden-startup.exe，完整包 dist/LiteTray-ui542-hidden-startup。EXE SHA256 E85C86E4D6909F831AF5539F62F3C8CE0B8CE85F47B9412ABA63E835CA3C3154。两份任务栏／QQ DLL 哈希保持不变，Explorer 34284、文件管理器 Explorer 12816、QQ 20412 启动时间均未变化。正式窗口标题确认为 5.4.2；设置、日历保持隐藏，打开计数为 0，保留给用户的真实第一次点击。只读任务栏检查主副屏 error/geometry_error 均为空，托盘输入仍 active=true。部署与现场记录见 local/settings-hidden-startup-deployment.json、settings-hidden-startup-live-before.json、settings-hidden-startup-live-after.json、settings-hidden-startup-taskbar.json。

## 2026-11-07 开发版 5.5：设置性能与全界面检查

收到“时钟显示／时钟样式切换卡顿、设置页多余滚动条和留白”及“深度检查整个 UI”的反馈。先用回归复现：两页导航中位 133ms，时钟显示、样式、天气、多屏托盘、关于在最小窗口中超出视口。定位到原生下拉框逐项插入／重绘及重复创建整页，批量填充并按配置、布局缓存两页。保持设置草稿，预设、主题、缩放更新会正确失效。

紧凑重排设置；发现并修复大窗口日历额外留白造成的滚动范围、天气标题与启用复选框的控件重叠，以及 32 号字体预览高度超出行高。宽视口中的日历保持比例并居中，天气凭据增加输入提示。正常窗口内十页无外层滚动条，超长备注和列表在控件内部滚动。

新增 8 项回归，含 100%／125% DPI、两种窗口尺寸、明暗主题、十页、六类日程、四个计时 Tab、弹层六类编辑、真实字体尺寸／文字截断／控件重叠、Tab 路径、字体下拉、长内容、32 号预览、缓存状态和导航绘制性能。完整测试 136 项通过，4 项原有忽略；另外执行原生离屏渲染并查看明暗截图，Clippy 通过。测试新建非输入桌面，没有操作用户的键盘鼠标或切换输入桌面。

实际 5.4.2 与 5.5 发布 EXE 逐页检查：旧版 6 页产生滚动条，新版十页均无；时钟两页来回切换含重绘中位 128.47ms → 13.56ms，多数 11–16ms，单次最大 75.41ms，首次创建仍较慢（本轮时钟 149.79ms、样式 81.97ms）。隐藏启动首次打开继续通过。完整范围、边界与证据见 docs/ui-audit-5.5.md。

04:08 已在原 dist/LiteTray-ui54-inline 目录切换到 5.5，宿主 PID 44300 → 43316；正式窗口标题已确认。旧 EXE 保留 local/LiteTray-ui542-before-ui55.exe。新版 SHA256 6F777B44EA8890A409DBDA2DE481ADCCFB00BEA27E8414994AE7580A8583E409，与实际发布包检查一致。任务栏与 QQ DLL 未变化，Explorer 34284、文件管理器 Explorer 12816、QQ 20412 的进程及启动时间保持不变。主副屏只读检查 error/geometry_error 均为空，托盘输入 active=true。正式设置／日历保持隐藏，未向主屏注入输入。部署记录 local/ui55-deployment.json、ui55-live-windows.json、ui55-taskbar.json。


## 2026-11-07 · 开发版 5.6

完成本轮设置布局重排与日期圆角抗锯齿。时钟新增日期／时间格式及实时例子，支持 `yyyymmdd`，旧模板和配置兼容；专注、定时、铃声、日程和其余设置页重新分组对齐。140 项测试及 Clippy 通过；强化几何检查以捕获细线穿过按钮、空备注框滚动条。三个独立非输入桌面上的真实发布进程均首次成功打开设置，十页无外层滚动条。

04:47:39 后台切换为 5.6，运行宿主 PID 42080；实际目录沿用 `dist/LiteTray-ui54-inline`，完整包 `dist/LiteTray-ui56-layout`。两枚 DLL 哈希未变化，Explorer 34284 与主 QQNT 20412 未重启。主副屏时钟原位内容及托盘代理通过只读检查。具体布局、格式语法、性能数据与证据见 [5.6 检查记录](ui-audit-5.6.md)。
