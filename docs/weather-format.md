# 5.10 天气格式

设置 → **天气格式** 可以配置 `%weather%` 的展开内容。“时钟显示”页也有“设置天气格式”入口。提供简洁、体感与湿度、温差与降雨、风向与风速四种预设；选择后可继续编辑。属性下拉框把选中的变量插入光标位置，支持替换选区。页面内即时示例预览与实际任务栏共用格式化代码，示例不会发送到任务栏。

默认格式仍为 `%weather_condition% %weather_temp%`，旧配置自动使用该默认值，显示保持“多云 25°C”这一形式。保存设置后应用到任务栏，无需重新接入 Explorer。第一行、第二行和悬停提示也能直接使用各个天气变量。

## 可用属性

| 变量 | 内容及单位 |
| --- | --- |
| `%weather_condition%` | 天气状况 |
| `%weather_temp%` | 温度，°C |
| `%weather_feels%` | 体感温度，°C |
| `%weather_humidity%` | 相对湿度，% |
| `%weather_wind%` | 风速，km/h |
| `%weather_wind_ms%` | 风速，m/s |
| `%weather_wind_dir%` | 八方位风向，如东北风 |
| `%weather_low%` / `%weather_high%` | 今日最低／最高温，°C |
| `%weather_rain%` | 当前小时的降水概率，% |
| `%weather_pressure%` | 来源返回的气压，hPa |
| `%weather_visibility%` | 能见度，km |
| `%weather_city%` | 城市 |
| `%weather_source%` | 实际使用的数据来源 |
| `%weather_updated%` | 数据时间，按电脑本地时区显示 HH:mm |
| `%weather_status%` | 当前观测／预估、备用来源或离线缓存状态 |

数值默认四舍五入到整数，可写 `:0`、`:1`、`:2` 指定小数位，例如 `%weather_temp:1%` 显示 `23.4°C`。精度下拉框可统一修改当前天气模板中已有的数值变量。文字变量不接受数值精度。普通文字、空格和分隔符直接输入；独立百分号使用 `%percent%`。

示例：

```text
%weather_condition% %weather_temp:1% · 体感 %weather_feels% · 湿度 %weather_humidity%
%weather_low%～%weather_high% · 降雨 %weather_rain%
%weather_wind_dir% %weather_wind_ms:1%
```

## 数据边界

所有属性沿用当前选中的同一家天气来源，不混合不同城市或不同来源的观测。不增加请求频率或密钥要求。Open-Meteo 现有免费请求额外取得风向和海平面气压；MET Norway、和风、OpenWeather、wttr.in 按各自返回值读取风向、气压及能见度。Open-Meteo 当前接口未提供的能见度保持缺失。

来源不提供的属性、非有限数值、越界数据统一显示 `—`。今日高低温只匹配电脑当地日期；降水概率只取覆盖当前时刻的一个小时，过期和未来小时不能代替当前小时。MET Norway 的降雨量不转换为降水概率。天气完全不可用或已关闭时，`%weather%` 显示“天气暂无数据”，独立属性显示 `—`。

MET Norway 不提供体感温度，因此 Open-Meteo 失败并切换到该备用来源时，当前温度和湿度仍可能有值，`%weather_feels%` 会显示 `—`。5.16.3 补齐 Windows 共享手动代理支持，解决只设置系统代理时首选来源仍然直连失败的问题；详见[启动与天气修复记录](startup-weather-5.16.3.md)。

天气格式不能包含 `%weather%` 自身，防止递归展开；来源返回的 `%time%` 等文字只作为文字显示，不执行第二次模板替换。

## 验证与部署

2026-11-07：171 项工作区测试通过、7 项忽略；Clippy 无警告。原生 UI 检查覆盖 11 页、明暗主题、最小窗口和不同缩放，天气格式页没有外层滚动条。新增检查覆盖模板校验、单位与精度、缺失／非法值、跨小时与跨日、旧缓存字段兼容、光标选区替换、预设、页面切换、配置保存及隐藏宿主向时钟和提示传递完整天气。多个原生窗口测试同时运行时性能测试超时；单独复测中位 7.4ms、最大 8.8ms，串行完整测试通过。

23:30 切换至 5.10（PID 15052），沿用 `dist/LiteTray-ui54-inline/`，Explorer PID 444 未重启，适配器和弹窗钩子未变化。旧窗口退出前出现未保存提示，备份原配置及输入后保存退出，保留字号等已有设置。

23:34 通过实际设置页的编辑与保存路径，主副屏原生任务栏均读回“晴间多云 24.3°C · 体感 27°C · 湿度 81%”，`tray_input.active=true`；随后用同一保存路径恢复原天气格式，其余设置保持一致。测试窗口保持隐藏，前台未改变。正式包首次打开设置及十一页检查通过，Setup 5.10.0 的安装、占用文件重装、首开、卸载与数据保留检查通过。

证据：`local/ui510-tests-final.txt`、`local/ui510-clippy.txt`、`local/ui510-renders/`、`local/ui510-live-format-verified.json`、`local/ui510-eleven-pages-check.txt`、`local/setup510-check.json`。安装包位于 `dist/Setup.exe`。
