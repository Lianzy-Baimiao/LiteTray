# Windows XAML 接口与调用边界

`src/xaml_abi.rs` 由 `tools/xaml-bindgen` 从 Windows 自带的 `Windows.UI.Xaml.winmd` 生成。构建使用已签入的定义，不需要开发机的 winmd，CI 也不从运行环境重新生成接口。

生成命令：

```powershell
.\tools\Invoke-Cargo.ps1 -CargoArguments @('run','--manifest-path','tools/xaml-bindgen/Cargo.toml','--','crates/taskbar-adapter/src/xaml_abi.rs')
```

生成器锁定 windows-bindgen 0.66.0。只请求所需接口，其余依赖保留为 usize 占位槽。生成器为少量 WinRT 引用类型参数补足 `void*` ABI 签名，严格检查每个替换恰好出现一次；不插入或删除槽位。单元测试验证 Name、XamlRoot、Content、GetChild、Text 等槽位。

`xaml.rs` 封装引用计数及 HSTRING 生命周期，COM 对象通过 Rc phantom 标记为线程绑定；接口和对象不会在线程之间传递。检查时所有持有引用在同步检查结束前释放。检查只读名称、尺寸和托盘范围内的文本，不设置属性。

`inspect.rs` 依次核对协议、x64 模块身份、PDB GUID/age、映像大小、时间戳、入口所在内存区域及代码字节。发现模块替换或已有 hook 改写入口即返回不可用；不猜测布局，不使用固定备用偏移。任务栏宿主指针仅来自已校验的 GetTaskbarHost 接口，std::shared_ptr 在同一线程由模块自身的 _Decref 释放。

在确认六个内部符号、对象布局和只读 XAML 检查通过之前，不启用任何时钟/托盘写入。仅 `cargo test` 通过不代表 Explorer 实机兼容性验证完成。

## 时钟属性的隔离验证

`clock.rs` 的 `ClockLease` 保存两行 Text、FontSize、FontFamily，以及指定容器 Width 的原始依赖属性。保存阶段不写入任何属性；普通值、UnsetValue 与 Binding 分别恢复为 SetValue、ClearValue 与 SetBinding，保留动态绑定。修改前先构造全部 WinRT 值；中途 setter 失败时尝试恢复所有已接触属性，返回恢复错误并保留待重试状态。Drop 再尝试恢复，不替代生产会话的显式退出协议。

`LiteTray.exe --self-test-clock` 只在调用进程创建 WindowsXamlManager 和新 TextBlock/Grid，完全不查找或接入 Explorer。真实 XAML 检查覆盖值与绑定捕获、文本/字体/宽度修改、tick、非法参数、正常/重复/Drop 恢复，以及故意传入错误属性类型后的部分写入回滚。恢复后改变 DataContext，再断言原绑定继续更新。

Win32 进程承载此 XAML 需要 manifest 中的 `maxversiontested`。使用 Microsoft 文档中的最低 XAML Islands 声明 `10.0.18362.0`，不会据此声称已验证该系统：
https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/host-standard-control-with-xaml-islands-cpp

这些写入目前只由隔离检查调用，尚未连接真实任务栏控制会话。长期接管仍需：原生节点识别、外部属性改动冲突处理、绑定源更新与渲染协调、点击转发、宿主失联恢复、Explorer 重启恢复。不要把隔离验证结果当成这些能力已经完成。
