# 5.16.3：开机启动路径与天气代理修复

## 开机启动

5.16.2 的现场配置已启用 `start_with_windows`，但当前用户的 Windows `Run\LiteTray` 仍指向旧的 5.13.1 便携目录，而当前运行的是安装目录中的 5.16.2。没有发现 LiteTray 的 `StartupApproved` 禁用记录。旧路径存在，因此只检查启动项是否存在无法检出这个问题，还需检查它是否对应当前安装的程序。

设置保存原先只在勾选状态改变时调用启动项登记。配置已经是 `true` 时，再次保存不会修复旧路径或补回缺失的启动项；程序正常启动也没有同步这两份状态。已有安装程序本身包含启动路径迁移逻辑，现有日志无法证明旧路径最初由哪次操作留下，因此不将来源归因于某一次安装失败。

5.16.3 在正常配置的主实例启动时，按已启用的偏好重新登记当前 EXE 的 `--background` 启动命令。注册失败时程序仍可运行，设置状态显示失败原因。已启用偏好的保存路径也重新登记，停用以及配置保存失败后的原偏好恢复沿用既有处理。使用 `LITETRAY_DATA_DIR` 的隔离实例不会执行启动时的自动修复。

真实原生设置页的回归测试在独立子进程和非输入桌面执行，使用 `RegOverridePredefKey` 将该子进程的 HKCU 重定向到临时键。测试先保持勾选状态为 `true`，写入旧 EXE 路径，再通过真实 `App::save` 检查它变为当前 EXE；删除启动项后再次保存，检查启动项补回。用户实际注册表和配置不参与测试。

## 体感温度

5.16.2 的实际 `--weather-check` 返回 Open-Meteo 网络失败，并成功回退至 MET Norway。缓存的 `feels_like_c` 为 `null`。MET Norway Locationforecast 不提供体感温度，独立天气属性按既有规则显示 `—`；当前温度和湿度仍然有值。

对同一个 Open-Meteo 请求做网络对照：直连在 12 秒内超时，使用电脑已启用的 Windows 系统代理成功返回 HTTP 200，其中温度为 24.8°C，体感为 27.7°C。原构建关闭了 ureq 默认特性，只显式启用了 TLS、gzip 和 JSON；未启用 `win-system-proxy`，所以没有代理环境变量时不读取 Windows 手动代理。

5.16.3 启用 ureq 的 `win-system-proxy`。本次修复覆盖 Windows 共享手动代理地址，代理环境变量继续按 HTTP 库的优先级处理；未新增 PAC 或按协议分别指定代理的解析。它没有修改天气来源顺序或从其他来源拼接缺失字段。

网络回归测试同样隔离 HKCU，在临时键写入启用的 Windows 代理，清除子进程的代理环境变量，再让真实 `http::Client::new().get()` 请求一个 `.invalid` HTTPS 地址。仅当本地代理收到实际 HTTPS `CONNECT` 请求时通过；不依赖公网服务或真实代理配置。

## 验证

两个回归测试修复前均失败，修复后通过。核心库及宿主测试共 235 项通过、9 项显式忽略；宿主及其依赖的 Clippy `-D warnings` 通过。

```powershell
.\tools\Invoke-Cargo.ps1 -CargoArguments @('test','--lib','--bin','LiteTray','--locked','--','--test-threads=1')
.\tools\Invoke-Cargo.ps1 -CargoArguments @('clippy','--lib','--bin','LiteTray','--all-targets','--locked','--','-D','warnings')
```

证据位于 `local/startup-regression-before.log`、`local/system-proxy-regression-before.log`、`local/startup-weather-tests-after.log`、`local/startup-weather-clippy.log`、`local/weather-20261114-before.json` 及两份 Open-Meteo 网络对照响应。安装版后续验证记录保存在同一 `local` 目录。

修复包仅重建宿主，沿用 5.16.2 已发布且哈希一致的任务栏适配和弹窗钩子 DLL；现场原有的任务栏菜单未提交改动未进入本次二进制。此次验证不代替真实 Windows 注销或重启。
