# 5.9.1 字体渲染

日历、天气、黄历和自绘按钮的文字改用 Windows 自带的 DirectWrite。沿用系统界面字体；本机为 Microsoft YaHei UI。农历、节气、天气详情与黄历辅助文字由 11 提高至 12 逻辑像素。

## 诊断与选择

旧 `make_font` 强制使用 GDI `CLEARTYPE_QUALITY`。在相同字体、字号、背景下，原生绘制路径的 24 组样本（100%／125%／150%／200%，12／14／19 像素，明暗背景）都出现 RGB 不相等的笔画边缘。以 100%、12 像素样本为例，检测到 880 个彩边像素。系统字体选择正确，程序已有 PerMonitorV2 清单，也没有发现文字位图被 StretchBlt 放大的路径。

单纯把 GDI 改成 `ANTIALIASED_QUALITY` 能消除彩边，但 100% 浅色界面中的细笔画明显偏淡，因此最终采用 DirectWrite 的灰阶渲染。此变化消除了已复现的彩边；它不是对所有显示器上主观观感或所有 Windows 应用字体完全一致的保证。

## 实现

- `ui/typography.rs`：通过 `IDWriteBitmapRenderTarget1` 在 CPU 位图中渲染；系统渲染参数、GDI compatible 布局、像素对齐，最终按原始物理像素 1:1 复制。支持字体、字重、斜体、对齐、换行、省略号和透明背景上的文字混合。
- 最近使用的 256 份布局缓存按字体描述、字号、文字、边界和排版标志索引。缓存只存布局，背景与颜色每次重绘；主题／系统设置失效时释放缓存。无额外后台线程或 GPU 渲染循环。
- `design::PaintBuffer` 为原生窗口上的自绘控件提供 CPU 绘制缓冲；整块完成后提交一次，避免每一行文字都从窗口读取像素造成同步等待。已有内存 DC 直接复用。
- 黄历卡片用同一个 DirectWrite 排版接口测量换行高度，避免测量与实际文字行数不一致。
- 原生编辑框及普通系统控件继续使用 Windows 的文字与输入实现，HFONT 质量改为 `DEFAULT_QUALITY`，遵循系统字体平滑偏好。旋转、特殊宽度、下划线／删除线字体和不支持的 DC 变换仍交由 GDI 回退。

任务栏上的时钟本来就是 Explorer 的 XAML TextBlock；本次仅更新宿主界面，适配与输入钩子 DLL 无需修改。

## 回归验证

`ui_text_has_neutral_antialiased_edges_at_monitor_scales` 从真实 `make_font` 和 `design::text` 路径验证 24 组样本有文字、有平滑且无彩边。该测试在旧实现上失败，在 DirectWrite 实现上通过。

`cached_text_preserves_background_viewport_and_clip` 检查明暗背景切换、缓存结果一致、视口偏移、裁剪、省略号绘制、换行高度及缓存上限。现有 UI 测试继续覆盖日历日期复用、页签切换、首开、字体选择、黄历及日程布局。

新渲染接入后曾测得设置页切换中位约 75ms；加入 CPU 绘制缓冲后，同一测试中位回到约 9ms，P95 约 20ms，未放宽性能阈值。

离屏对比与探针保存在 `local/font-probe/`、`local/ui591-font-before/`、`local/ui591-directwrite-renders/`、`local/ui591-directwrite-settings/`；这些为本地诊断产物。窗口交互测试均在非输入桌面运行。

最终 162 项测试通过（7 项显式联网／截图测试默认忽略），Clippy `-D warnings` 通过。最终页签切换中位 7.7ms、P95 19.7ms。完整发布 EXE 的日历、托盘、直接请求三个设置入口首开均成功，约 21–23ms；十页无外层滚动条。最终设置与黄历截图在 `local/ui591-final-settings/`、`local/ui591-final-almanac/`。

## 本机部署 · 2026-11-07

20:29:20 将 5.9.1 更新至原目录 `dist/LiteTray-ui54-inline` 并后台启动（PID 29760）；完整包为 `dist/LiteTray-ui591-text`。EXE SHA256：`627DF310320B98D1D858D9E327C03263806FF6BE593FA3048476345015EB2E26`。切换前没有 LiteTray 宿主进程，旧 EXE 备份为 `local/LiteTray-ui59-before-ui591.exe`。Explorer 11064、用户配置和两份适配 DLL 哈希保持不变；正式设置与日历均保持隐藏。

本次登录中 Windhawk 正在运行，Explorer 已加载 `taskbar-multi-tray_1.3.0_967910.dll` 等模块。LiteTray 接入状态为“连续连接失败，已暂停；托盘坐标入口已变化或已被其他组件改写”，只读检查 `tray_input.active=false`。因此不能把这次字体部署记为任务栏接管验收通过；未覆盖既有钩子、关闭 Windhawk 或重启 Explorer。日历、设置字体和离屏功能测试已通过，任务栏接管需要另行解除入口冲突。

证据：`local/ui591-tests.txt`、`local/ui591-clippy.txt`、`local/ui591-package-check.txt`、`local/hidden-startup-packages/5.9.1-text.json`、`local/ui591-deployment.json`、`local/ui591-live-windows.json`、`local/ui591-live-taskbar.json`。
