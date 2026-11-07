# 任务栏接入与 Microsoft Store

核对日期：2026-11-08；对应开发版 5.11.6。

## 实际接入方式

LiteTray 通过线程消息钩子 `SetWindowsHookExW(WH_CALLWNDPROC, ...)` 将任务栏适配 DLL 加载到 Explorer，在 Explorer 的 XAML UI 线程内修改原生时钟控件并连接托盘数据模型。宿主 EXE 管理设置、日历、天气和日程窗口。

这属于用户态的进程内扩展与运行时挂钩：使用公开 Win32/WinRT API，同时依赖未公开的 Shell 对象布局、接口及函数入口。多屏托盘的坐标和菜单兼容还使用 MinHook。它不安装内核驱动，也不替换磁盘上的系统 DLL 文件。

保存的原生属性和绑定用于停止接管时恢复。适配 DLL 为避免迟到回调访问已卸载代码，会保留在 Explorer 中直到进程退出；升级适配器可能需要重启 Explorer。适配器故障可能影响 Explorer 稳定性。

私有入口按已核对的系统模块哈希及结构验证，不匹配时拒绝接管。当前只核对 Windows 11 26300.9711 x64 的指定模块；相近系统版本也不保证可用。

QQ 弹窗位置兼容属于单独的实验功能，启用时会向目标 QQ 线程加载另一个钩子 DLL，默认关闭；它不是时钟替换的必要条件。

源码入口：

- `src/platform/taskbar_clock.rs`、`src/platform/adapter.rs`：Explorer 线程钩子和宿主连接。
- `crates/taskbar-adapter/src/taskbar_source.rs`：系统模块核验和私有 XAML 源解析。
- `crates/taskbar-adapter/src/hooks.rs`、`tray_interaction.rs`、`tray_menu.rs`：运行时函数挂钩、坐标与菜单处理。
- `src/platform/tray_popup.rs`：可选 QQ 弹窗兼容。

## Microsoft Store 可行性

Microsoft Store 接受传统 Win32 EXE/MSI，并不要求所有桌面软件改写为 UWP。但当前版本不适合原样提交：

1. **接口支持性是架构障碍。** [Store Policies 10.2.8](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies#102-security) 要求使用受支持的方法并取得用户同意来修改 Windows 体验，明确包括不得以不受支持的方式使用未公开或不受支持的 API。LiteTray 对 Shell 私有布局和函数的依赖需要解决，用户同意本身不能替代这条要求。这里的判断针对实际实现，不能泛化成“所有 DLL 注入都被商店禁止”。
2. **传统安装包需要签名。** [EXE/MSI 包要求](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msi/app-package-requirements) 和政策 10.2.9 要求安装包及其中所有 PE 文件使用受信任 CA 的代码签名证书签名。当前 Setup.exe、LiteTray.exe 和两个 DLL 均未签名。
3. **分发要求。** EXE/MSI 必须支持静默安装、使用独立完整安装包，并提供每版固定不变的 HTTPS 下载地址。当前 Inno Setup 安装器可以作为后续签名与静默安装验收的基础；每个 GitHub Release 的固定资产地址可以作为候选下载地址，不能使用会随发布变化的 latest 地址。
4. **稳定性还需验收。** 政策 10.2 要求不损害设备或系统功能；当前通知点击崩溃反馈尚待复现，不能宣称已满足认证要求。

仅重新打成 MSIX 不能消除对私有 API 的依赖。若开发商店版本，可保留日历、天气、日程、计时与普通托盘入口，将依赖私有 Shell 接口的原位替换和托盘复制功能移除或重新实现。完整接管版继续通过 GitHub 分发。最终审核结论由 Microsoft 决定。

核对的政策版本为 7.20，2026-10-22 生效。提交前应再次核对政策。

## 通知崩溃反馈的当前状态

5.11.6 发布时，此反馈尚未复现或修复。现场没有对应的错误报告，且 Windows 错误报告被禁用；日志缺失不能用于排除崩溃。现有隔离 XAML 检查并不能代替真实任务栏、通知中心与跨屏点击验收。
