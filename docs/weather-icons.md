# 5.10.1 天气图标清晰度

原图标由 `dashboard::weather_icon` 直接调用 GDI 的 Ellipse、LineTo、PolyBezier 绘制，没有抗锯齿，也没有使用天气来源的图片或系统图标库。22 逻辑像素的逐小时图标原先只有一像素细线；整数坐标缩放使轮廓呈阶梯状，“晴间多云”与晴、雨与雪／雷雨还共用了相同图形。

现在使用 `assets/icons/weather/` 中的十个原创 SVG，内嵌编译，不依赖下载、字体或额外运行时。区分晴、晴间多云、阴／多云、雨、雷雨、雪、雨夹雪、雾、冰雹和未知；颜色跟随 Windows 主题及现有高对比度配色。

渲染按实际物理尺寸计算 8×8 像素覆盖率，生成透明边缘，内部笔画保持不透明；按 1:1 合成到真实背景，不将已生成的小位图拉大。24 单位设计使用圆头圆角 1.8 单位线宽，小时小图至少 1.75 物理像素宽。缓存最多 64 份图标，尺寸和颜色均参与键值；翻页和普通重绘复用已有图标，DPI 或主题变化得到对应尺寸／颜色，旧项按容量淘汰并释放 GDI 资源。

## 验证

通过真实调用点绘制 22／44 逻辑像素、100%／125%／150%／200% 缩放、明暗两色，检查抗锯齿过渡、实色笔画、绘制不超出边界和无彩色细边。旧代码在全部样本中 `antialiased=0`；新代码通过。另检验天气类型映射和缓存容量／重用，并检查完整日历及小时预报明暗截图。

命令：`tools/Invoke-Cargo.ps1 -CargoArguments @('test','--bin','LiteTray','weather_icons_have_smooth','--','--nocapture')`。

本地证据：`local/weather-icons-before.txt`、`local/weather-icons-after.txt`、`local/weather-icons-before/`、`local/weather-icons-after/`、`local/ui5101-hourly-renders/`。

## 发布验证 · 2026-11-07

173 项测试通过、7 项忽略（包含单独执行的导航性能测试），Clippy 无警告。首次整组测试中导航冷启动为 121ms，超过原有 100ms 上限；单独复测 77ms、切换中位 7.8ms、最大 11.1ms，通过后将它与其余串行测试分开执行。实际发布 EXE 在非输入桌面首开设置成功，十一页无外层滚动条。安装、占用文件重装、安装后首开、卸载及数据保留检查通过。

23:48:45 后台切换为 5.10.1，宿主 PID 29908，运行目录 `dist/LiteTray-ui54-inline/`。EXE SHA-256：`699E15D7303196A1F802868AAEFF6FC6FC622C09BB4F025C0379746039A9EBE5`。Explorer PID 444、适配 DLL、弹窗钩子和用户配置均未改变。主副屏实际任务栏读回日期、时间与“晴间多云 24°C”，几何检查无错误，`tray_input.active=true`。

完整包：`dist/LiteTray-ui5101-weather-icons/`；安装包：`dist/Setup.exe`（5.10.1）。现场证据：`local/ui5101-deployment.json`、`local/ui5101-taskbar-summary.json`、`local/ui5101-package-check.txt`、`local/setup5101-check.json`。尝试捕获隐藏正式日历的 PrintWindow 返回黑图，未作为图标视觉验证证据；视觉检查采用上述同一绘制调用点和完整原生日历渲染。
