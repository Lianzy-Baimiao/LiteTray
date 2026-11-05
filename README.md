# LiteTray · 轻托盘

轻量 Windows 任务栏增强软件：接管 Taskbar multi-tray 与 Taskbar Clock Customization 的目标功能，并提供点击时钟打开的日历、天气面板。

Rust 原生 Windows 应用，GitHub 仓库保持私有。

## 当前实现状态

当前为开发中的原生预览版本：原生设置窗口、托盘入口、日历天气面板、模板配置、多个天气来源、离线缓存、GitHub Releases 更新检查及 Windows 凭据管理器集成已经编写。**Explorer 多屏托盘和原生时钟接管尚未完成，当前 EXE 还不能替代这两个 Windhawk 模块。**

- [产品与实现规格](docs/implementation-plan.md)
- [实现与验证进度](docs/progress.md)
- 本机调查记录在本地 `docs/baseline.md`，包含环境信息，不提交到仓库。
- `tools/Export-WindhawkProfile.ps1`：只读取这两个模块的设置，导出到独立文件，原设置保持不变。
- `local/windhawk-profile.json`：本机导出结果；属于本地配置，不纳入版本控制。

## 只读导出

在项目目录的 PowerShell 中运行：

```powershell
.\tools\Export-WindhawkProfile.ps1
```

默认输出到项目 `local` 目录。已存在输出时不会覆盖，需显式传入 `-Force`。导出不包含模块源码，也不访问天气服务。

## 构建与运行

需要 Rust stable 和 Windows C++ 链接工具。支持 MSVC（建议用于 CI/发布）和 GNU MinGW（本机开发）。

```powershell
# MSVC / 配置完整的工具链
cargo build --release
cargo test --all-targets

# 本机 GNU 工具链；不修改全局环境
.\tools\Invoke-Cargo.ps1 build --release
.\tools\Invoke-Cargo.ps1 test --all-targets
```

本机脚本优先使用 `LITETRAY_MINGW_BIN`，否则读取用户 `tools\mingw64\bin`。GNU 构建产物位于用户 `.cargo-target\litetray\release\LiteTray.exe`。MSVC 默认位于 `target\release\LiteTray.exe`。

运行参数：

```text
LiteTray.exe --settings
LiteTray.exe --calendar
LiteTray.exe --background
LiteTray.exe --diagnose [output.json]
LiteTray.exe --weather-check [output.json]
LiteTray.exe --check-updates
```

默认打开设置；关闭窗口收至托盘，右键托盘菜单可以退出。开机启动是可选项，默认关闭。配置保存在 `%LOCALAPPDATA%\LiteTray`；开发检查可用 `LITETRAY_DATA_DIR` 指向隔离目录。

## 天气来源

| 来源 | 认证 | 用途 |
| --- | --- | --- |
| Open-Meteo | 常规非商业免费接口无需 Key | 当前预估与三日预报 |
| MET Norway | 无需 Key，携带可识别 User-Agent | 逐小时预报托底 |
| 和风天气 | 控制台 API Host + API Key | 当前观测 |
| OpenWeather | API Key | 当前观测 |
| wttr.in | 无需 Key | 最后备用 |

支持添加、移除和调整来源顺序，按超时/错误/数据过期自动切换。缓存与城市坐标绑定；全部失败才使用未超过保留期限的离线缓存，并明确标记“非实时”。免费服务没有稳定性保证，商业发布需核对各服务条款与额度。

API Key 与 GitHub Token 使用 Windows 凭据管理器，不进入 JSON 配置、日志或 Git 仓库。更新检查读取 GitHub Releases，正确区分没有 Release、私有仓库无权限、限流与新版本；不静默执行下载内容。

## 实现方向与兼容范围

按用户决定采用 Rust：`windows-sys` 驱动 Win32 原生窗口和控件，`windows` 调用系统农历，网络使用系统 TLS。网络、配置、日历计算放在独立宿主；后续 Rust DLL 中只保留显示、输入转发和原生托盘适配。

开发目标为 Windows 11 build 26300.9711 x64。架构保留 Windows 10 经典任务栏适配分支；当前尚未完成 Windows 10 或未来版本的运行验证，不宣称所有 Windows 版本兼容。现代 Rust 工具链下不把 Windows 7/8 列为当前支持目标。

首先验证当前 Windows build 上的任务栏接入和退出恢复，再完成多屏托盘、时钟与面板的纵向闭环。完整接管通过之前，不能宣称已经支持替代 Windhawk。
