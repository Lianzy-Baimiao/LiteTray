# 日期悬停黄历

开发版 5.8：鼠标停留在日历的某一天约 400ms 后显示黄历，离开即收起。适用于任务栏日历弹层、分离后的日历，以及设置内的日历。卡片展示公历日期、农历日期、干支年月日、宜、忌、冲煞和建除值日。宜／忌分别采用绿色和红色文字，高对比度使用系统配色；卡片随 Windows 明暗主题变化，不激活窗口、不改变日期选择。

在 **设置 → 时钟显示 → 日期悬停显示黄历** 控制此功能。默认开启，点击“保存设置”生效。开关与“日历显示农历”独立，关闭日期格内的农历标签不影响黄历。配置字段为 `clock.show_almanac`；已有配置未包含此字段时使用默认值。

![浅色黄历](images/almanac-light.png)

![深色黄历](images/almanac-dark.png)

## 数据与行为

使用 [6tail/tyme4rs](https://github.com/6tail/tyme4rs) 1.5.1，MIT 许可证，许可原文收录在 THIRD-PARTY-NOTICES.md。所有计算都在本地进行，无网络、密钥或服务端。黄历采用传统民俗历法；干支年按立春、干支月按节气换，详情参见 [Tyme 文档](https://6tail.cn/tyme.html)。公共假日和调休仍使用已有国务院假日数据，不由黄历推断。

支持公历 1901–2100 年，范围外明确显示暂无黄历数据。仅在实际停留后生成卡片，短暂经过日期不触发计算；显示数据缓存最多 128 天。切月、更换选中日期、关闭开关、主题／缩放变化、窗口隐藏和鼠标离开时清理悬停状态。卡片按日期所在显示器的工作区定位，支持负坐标及右侧副屏，不抢占前台。

## 验证

- 已知春节、闰六月、甲子日及冲煞样例；范围边界与有界缓存。
- 开关草稿经过缓存页切换与配置保存重载后保留，并与农历标签独立。
- 日期悬停不改变选中日期，关闭开关和切月取消悬停，日期按钮保持原句柄。
- 原生卡片在非输入桌面显示，核对不激活、鼠标穿透、移出销毁、同屏定位及可访问文字。布局检查覆盖 100%、150%、200%，已查看实际浅色和深色渲染。
- 全套回归中发现旧日历测试跨已结束的 STA 线程复用 WinRT 农历工厂时发生访问冲突。GDB 栈定位于 `Calendar::ActivateInstance`；旧原生窗口测试统一使用已有的独立进程／非输入桌面测试入口，避免跨测试共享失效 COM 工厂。生产的农历转换和任务栏适配未改动。

测试不移动用户鼠标、不注入用户桌面输入。原生消息与窗口检查不等于用户真实鼠标操作验收。

```powershell
.\tools\Invoke-Cargo.ps1 -CargoArguments @('test','--workspace','--all-targets','--locked','--','--test-threads=1','--nocapture')
.\tools\Invoke-Cargo.ps1 -CargoArguments @('clippy','--workspace','--all-targets','--locked','--','-D','warnings')
```

截图证据在 `local/ui58-almanac-renders/` 与 `local/ui58-settings-renders/`。完整测试记录在 `local/ui58-tests-final.txt`。

## 部署记录 · 2026-11-07

152 项自动测试通过，6 项显式忽略；原生截图测试另行执行。Clippy `-D warnings` 通过。实际发布 EXE 在三个独立非输入桌面冷启动，日历、托盘、直接打开三个设置入口首次均成功，内部耗时约 20–22ms；十个页面无外层滚动条。新增黄历后 EXE 大小 2,157,568 字节，比 5.7 增加 158,208 字节。

14:16:22 在原目录 `dist/LiteTray-ui54-inline` 后台启动 5.8，宿主 PID 2568；此前没有运行中的 LiteTray。实际标题已确认，设置保持隐藏。Explorer PID 10960 未变化，正式配置文件哈希前后一致，未改变原有城市或日程。两个 DLL 保持原哈希，主副屏任务栏的 `error`、`geometry_error` 均为空，`tray_input.active=true`。

完整包为 `dist/LiteTray-ui58-almanac`。EXE SHA256：`95E8FDD0ADCCA84F7D809F249C583918FA2ED7A33F6079A6EC5CAB96ECAEF734`。部署、发布包和现场证据：`local/ui58-deployment.json`、`local/hidden-startup-packages/5.8-almanac.json`、`local/ui58-package-check.txt`、`local/ui58-live-taskbar.json`。
