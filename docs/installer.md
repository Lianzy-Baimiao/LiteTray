# Windows 安装包

`dist/Setup.exe` 为 LiteTray 5.9.2 的 Windows 11 x64 安装程序。默认简体中文，并根据系统语言支持英文；有程序图标、开始菜单入口、可选桌面快捷方式和 Windows“已安装的应用”卸载入口。

默认安装根目录为 `%LOCALAPPDATA%\Programs\LiteTray`，不请求管理员权限。程序及两份配套 DLL 放入 `versions/<版本>-<内容摘要>/`；三个二进制共同决定摘要，减少 Explorer 保留旧 DLL 时升级覆盖失败的情况。重装会逐文件核对 SHA-256，内容一致的文件不覆盖。

安装前通过 `WM_COMMAND/EXIT` 请求已运行的 LiteTray 正常退出，并等待最多八秒；失败则停止安装，不强制终止进程。取消或失败时尝试后台恢复刚退出的旧实例。安装成功后，原有开机启动项改为安装目录中的新路径；未启用开机启动的用户不会被自动启用。普通向导完成页可选择启动程序；静默安装不自动运行程序。

卸载只退出安装目录内的 LiteTray，并仅删除仍指向该安装根目录的开机启动项。用户配置、日程、天气缓存和凭据保留。安装程序不结束 Explorer 或 Windhawk；任务栏接管仍受当前 Windows 版本和其他托盘修改工具的兼容性检查约束。

## 构建

```powershell
.\tools\Build-Setup.ps1
.\tools\Build-Setup.ps1 -SkipBuild -BinaryPath <完整发布目录>\LiteTray.exe
```

脚本自动从 `UI_REVISION` 读取版本，打包宿主、适配 DLL、图标资源、产品说明及第三方声明。Inno Setup 可通过 `-CompilerPath` 指定，输出目录可通过 `-OutputDirectory` 指定。默认产物：

- `dist/Setup.exe`
- `dist/Setup.exe.sha256`
- `dist/Setup-manifest.json`：版本、二进制摘要和安装后的相对 EXE 路径。

## 5.9.1 验证

使用实际 Setup 在不切换的非输入桌面执行安装和卸载，安装目录包含中文，结果均为退出码 0。已验证正常退出旧宿主的窗口协议、开始菜单快捷方式指向、三份二进制哈希、文件只读占用情况下重装、安装版首开、卸载记录清理，以及用户配置与日程哈希不变。测试的临时开机启动路径已恢复，当前开发实例保持运行。

本地证据：`local/setup591-check.json`、其中记录的安装／重装／卸载日志目录、`local/hidden-startup-packages/5.9.1-installed.json`。安装器编译器为 Inno Setup 7.1.0。
