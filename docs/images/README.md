# README 预览图 / README previews

当前首页使用 5.12.0 版预览，生成日期为 2026-11-08。

- `taskbar-5.12.0.png`：当前运行的 5.12.0 在副屏上的实际任务栏，按原始像素截取右侧 900 × 40 区域。时钟内容为用户配置的中文模板。
- `calendar-5.12.0-{zh,en}-{light,dark}.png`：从当前版本的原生日历窗口渲染，使用 `render_settings_artifacts` 内的离线示例天气与日程，不读取用户的日程或天气缓存。中英文及明暗主题分别生成，图像未经缩放。
- `compatibility-5.14.0-{zh,en}-light.png`：5.14.0 新增兼容性页的原生窗口预览，在私有桌面渲染，系统版本与检测时间使用固定示例，页面中的模块目录来自内置资料。不是宿主实时检测或新版本实测证明；分别来自两个语言输出的 `settings-light-11.png`。
- `compatibility-5.15.0-{zh,en}-light.png`：5.15.0 按模块实现识别后的页面预览，来自 `local/compatibility-traits/ui-{zh,en}/settings-light-11.png`。沿用私有桌面的原生渲染与固定示例数据，展示模块数量、实现等价数量和逐功能状态，不代表图中系统的本轮运行验收。

The taskbar is a live capture of version 5.12.0. Calendar previews are rendered from the current native window and controls, with synthetic weather and schedules. They do not contain the user's agenda or weather cache. Each language and theme is rendered separately at its original resolution.

在 Windows 上重新生成日历 / Regenerate calendars on Windows:

```powershell
.\tools\Render-UI.ps1 -OutputDirectory local\readme-zh -Language zh-cn
.\tools\Render-UI.ps1 -OutputDirectory local\readme-en -Language en
```

使用输出中的 `settings-light-flyout.png` 和 `settings-dark-flyout.png`。更新图片文件名与两个 README 的引用，避免浏览器继续使用旧图缓存。其他未带版本号的图片用于既有专题文档，不代表当前首页预览。

Use `settings-light-flyout.png` and `settings-dark-flyout.png` from each output directory. Update filenames and both README references when refreshing the preview to avoid stale image caches. Older, unversioned images support existing topic documentation.
