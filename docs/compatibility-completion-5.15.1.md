# Windows 11 补充验证：普通 24H2、25H2 与显示变化

本轮使用已发布的 5.15.1 默认包补充真实系统验证。模块匹配、虚拟机运行、显示模式变化和多屏输入分别记录；一种场景的结果不能代替其他场景。之前的成功与失败记录继续保留于 [5.15.0 验证记录](compatibility-traits-5.15.0.md)。

## 发布包身份

| 文件 | SHA-256 |
| --- | --- |
| `LiteTray.exe` | `837ebebd89c192dff94397565c1498c1007c3a5ae814eb23164a25dcf652e62a` |
| `litetray_taskbar_adapter.dll` | `ac68fa6dbb6c9cdc0d3a4691aaf30033109a2178ad0e92603c16b84fd0db5b62` |
| `litetray_popup_hook.dll` | `f6171c7f4164965a50b56a985198367691703150037c714d8f9336f83992d659` |

build ID 为 `5.15.1-8e688e0f1c01`，清单 SHA-256 为 `7de3103f1f05859ecf6806e4f8dd192c9189a51f049480f0c02a5a705859a1f1`，含 86 个文件条目及清单本身。两个普通 DLL 与 5.15.0 发布文件逐字节相同。另行制作的诊断候选有独立哈希，不计为这个发布包的运行结果。

## 普通 24H2 的独立来源

本轮从微软官方 [24H2 产品目录 CAB](https://download.microsoft.com/download/6/2/b/62b47bc5-1b28-4bfa-9422-e7a098d326d4/products_Win11_20240916.cab) 获取原始 `products.xml`。CAB 经 HTTPS 获取，大小 29,279 字节，SHA-256 为 `d086bad6a9e6012158dbd7ee1d7ec8d1b3085edc559b8a6acd170e81cdf6922f`。第三方索引仅用于发现该官方地址，不执行其脚本。

选用目录中的英文 x64 Consumer ESD：

- 文件：`26100.1742.240909-0928.ge_release_svc_refresh_CLIENTCONSUMER_RET_x64FRE_en-us.esd`
- 官方目录文件大小：3,920,160,752 字节，完整下载长度一致。
- 官方目录 SHA-1：`8a5003ef73725dc129164f9b0261d9b4f5c1195b`，完整下载校验一致。
- 本地追加 SHA-256：`769f5b0e86d304c1a26bd164aabb34ca10a26a96e6b0c871e139dd374ee7adf4`。
- WIM 元数据：索引 **9** 为 Windows 11 Pro，`Professional / Client / amd64 / en-US / 26100.1742`。

ESD 使用官方 XML 原始 `dl.delivery.mp.microsoft.com` HTTP 地址获取；同路径 HTTPS 证书链校验失败，未关闭 TLS 校验。每个分段核对 HTTP 206、范围、长度和实体标记，最终再核对通过 HTTPS 获取的目录所绑定的完整 SHA-1。下载是用于独立虚拟机的 Windows 安装介质，不属于 LiteTray 安装包或更新内容。

只读提取同一 Pro 索引中的三个模块后，它们与已有 `windows11-26100.1742-x64` 参考实现完全相同：

| 模块 | SHA-256 |
| --- | --- |
| `System32/taskbar.dll` | `b3be279bcb5a7fb63dbdc24cc1c55442a3965b68702a98fbdb6496d91cb40de5` |
| `System32/twinui.pcshell.dll` | `3ef119746b60885b8923c49de95860caca0639aec1e2e7aa0a4113356b2cedb9` |
| `SystemApps/MicrosoftWindows.Client.Core_cw5n1h2txyewy/Taskbar.View.dll` | `734e19a7e749f4112acc1e072ab35a11fdd3ffc04d8fcf54f603593758702d5b` |

实际运行上述发布 EXE 的离线导出得到三个 `exact_image`、`all_profile_images_matched=true`，匹配现有四参数借用对象的拖放坐标实现。未增加 OS 版本条件，也无需把同一模块重复加入目录。该结果仍为 `disk_only`，不访问 Explorer；普通 Pro 的运行结果须由独立安装客体另行取得，不能借用 LTSC 记录。

介质、完整传输记录、映像 XML、提取结果和发布程序离线报告保存在 `local/compatibility-completion-5.15.1/`，Windows 二进制与系统镜像不随源码发布。

## 25H2 发布包的首轮结果

Windows 11 Pro 25H2 `26200.8037` 的独立运行编号为 `sandbox-20261109-195606-b5797761`，使用 QEMU TCG 和既有 `max,vendor=GenuineIntel,pmu=off` 配置。原始基盘仍以只读方式打开。

本轮通过了 Medium worker 启动及身份核验，实际进入产品检查。四项基线命令和私有 Shell source 命令成功；第六项真实任务栏检查实际退出码为 1，`timed_out=false`，报告启动托管阶段通信错误 `1460`。没有活动帧样本，托盘准备和后续磁盘副本检查未完成，因此整轮不通过。

失败发生在首次 Start 命令发送或随后 Active 状态查询范围；发布包的原错误文本不能区分两者。`writes_requested=true`，不能宣称没有发生任务栏写入。清理取得确认，末次完整 taskbar 字段与基线一致，原 Explorer PID 1652 及进程启动时间保持不变。成功恢复不替代失败的启动和缺失场景。

完整归档 SHA-256 为 `47bc0908a44443e1f50a530c2665db3e78631bcefe297ccd5cb28843c254a90c`。当前轮只使用与其 run ID/nonce 相符的材料，归档中的历史结果不计入本轮。后续硬件加速对照必须使用新运行编号并单独报告。

## 显示变化测试方法

现有 `std VGA + Microsoft BasicDisplay` 客体是单显示器环境。Windows viogpudo 驱动实现也只支持单个显示源，给单张虚拟显卡设置多个输出不等于 Windows 实际获得双屏。多屏候选使用独立虚拟显卡，并要求 Windows 枚举出实际活动显示器、桌面坐标和对应任务栏后才进入产品验收。

单独的分辨率恢复脚本运行真实后台应用，使用普通用户与独立配置目录，实际调用 `ChangeDisplaySettingsEx` 切换并恢复驱动公布的模式。脚本观察操作系统产生的 `WM_DISPLAYCHANGE`、原会话恢复、重新连接以及实际时钟文字与宽度，并在结束时请求应用正常退出、核对原显示模式与 Explorer 连续性。它不发送合成显示变化消息，不把分辨率变化称为显示器插拔或混合 DPI 测试。

恢复后的时间文字可能正常跨分钟变化，因此单独保留“原生文字完全相等”观察值；结构恢复要求控件类型、可见性、高度、容器宽度一致，且没有遗留测试文字。该范围不同于短时主屏检查中的完整原字段相等证明。

截至上述记录，显示脚本只完成 PowerShell 5.1 解析及 C# 编译检查，实际客体结果尚待追加。WHPX 无磁盘基础试验已经成功创建分区和虚拟 CPU、运行 BIOS 并退出；这只证明加速基础设施可用，不构成 Windows 或 LiteTray 运行验收。
