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

实际运行上述发布 EXE 的离线导出得到三个 `exact_image`、`all_profile_images_matched=true`，匹配现有四参数借用对象的拖放坐标实现。未增加 OS 版本条件，也无需把同一模块重复加入目录。该离线报告仍为 `disk_only`，不访问 Explorer；下述普通 Pro 运行结果来自独立安装客体，没有借用 LTSC 记录。

介质、完整传输记录、映像 XML、提取结果和发布程序离线报告保存在 `local/compatibility-completion-5.15.1/`，Windows 二进制与系统镜像不随源码发布。

## 普通 24H2 发布包在独立客体通过

原始 Consumer ESD 的 Pro 索引 9 导出为独立部署介质的索引 1，安装于新的虚拟磁盘。实际 Windows 身份为 `Professional / 24H2 / 26100.1742 / x64`，客体名 `LT-W11-24H2`。安装过程中遇到 QEMU WHPX 暖重启的 BIOS 暂停问题，保留日志后从同一安装盘冷启动继续完成；没有把安装基础设施故障归为 LiteTray 运行结果。安装完成后正常关机封存基盘，再以只读基盘和独立可写 overlay 运行产品检查。

运行编号为 `sandbox-20261109-205423-3b6aa4b5`，Medium worker nonce 为 `912941652b7f4bcfac0cb32a1389a07f`。实际加载的系统模块与上述普通 Pro 映像提取的三个哈希一致。134 项独立核验确认：

| 检查 | 结果 |
| --- | --- |
| 86 项发布文件、七份测试脚本及普通权限启动链 | 全部一致 |
| 六项原生进程检查 | 全部实际 exit 0，未超时 |
| 隔离 XAML / 私有 Shell source | 168 项含四项恢复检查 / 六项通过 |
| 真实主屏时钟更新 | 三帧文字及宽度 `56 → 240 → 320 → 320 → 56` 读回通过 |
| Stop 及托盘清理 | 两阶段完整 taskbar 数组、原始时钟字段均恢复 |
| 托盘区域与借用坐标入口 | 五类区域及三个入口准备、停止通过，未激活复制 |
| 五项磁盘副本识别及原系统模块复核 | 全部通过；副本未执行，原模块未改动 |

Explorer PID 3544、线程 4712、主任务栏窗口 65690 及进程启动时间保持。完整归档 SHA-256 为 `ad82c8b3d0bb2d0a9575d7a55490184187a60f8818e372df42b5ed07497a82ba`。它补齐普通 24H2 Pro 的限定运行验收；Home、其他累计更新、输入、多屏和长期刷新仍不能由此推定通过。

## 25H2 发布包的首轮结果

Windows 11 Pro 25H2 `26200.8037` 的独立运行编号为 `sandbox-20261109-195606-b5797761`，使用 QEMU TCG 和既有 `max,vendor=GenuineIntel,pmu=off` 配置。原始基盘仍以只读方式打开。

本轮通过了 Medium worker 启动及身份核验，实际进入产品检查。四项基线命令和私有 Shell source 命令成功；第六项真实任务栏检查实际退出码为 1，`timed_out=false`，报告启动托管阶段通信错误 `1460`。没有活动帧样本，托盘准备和后续磁盘副本检查未完成，因此整轮不通过。

失败发生在首次 Start 命令发送或随后 Active 状态查询范围；发布包的原错误文本不能区分两者。`writes_requested=true`，不能宣称没有发生任务栏写入。清理取得确认，末次完整 taskbar 字段与基线一致，原 Explorer PID 1652 及进程启动时间保持不变。成功恢复不替代失败的启动和缺失场景。

完整归档 SHA-256 为 `47bc0908a44443e1f50a530c2665db3e78631bcefe297ccd5cb28843c254a90c`。当前轮只使用与其 run ID/nonce 相符的材料，归档中的历史结果不计入本轮。后续硬件加速对照必须使用新运行编号并单独报告。

## 25H2 发布包在硬件加速客体通过

同一发布包在正常关机后切换到 WHPX。最初沿用 TCG 的 `max,vendor=GenuineIntel,pmu=off` 参数，在 SeaBIOS 阶段遇到 QEMU `Unexpected VP exit code 4` 并暂停；此时没有 Windows 登录、串口 bootstrap 或产品 payload 执行。该暂停实例保留状态和磁盘证据后退出，不能写作产品失败或正常客体关机。

随后使用 WHPX 的 `-cpu max` 冷启动成功，以新运行编号 `sandbox-20261109-202125-52113436` 验证原发布包，未放宽产品命令超时或断言。完整结果为：

| 检查 | 结果 |
| --- | --- |
| 86 项发布文件、普通用户 Medium 令牌、会话及当前 nonce | 一致 |
| 六项原生进程检查 | 全部实际 exit 0，未超时 |
| 隔离 XAML 检查 | 168 项，包含四项恢复回归 |
| 私有 Shell source | 六项通过 |
| 真实主屏时钟更新 | 三帧文字及宽度 `55 → 240 → 320 → 320 → 55` 读回通过 |
| Stop 后原字段恢复 | 通过，原 Explorer PID 4168、线程 4372、窗口 65732 保持 |
| 托盘区域与坐标入口准备、停止 | 通过，未激活副屏复制 |
| 五项磁盘副本识别场景及原系统模块复核 | 全部通过 |

完整归档 SHA-256 为 `7f8e1323704e2131c95ac57287b919961cc1eb4641db5f986a159cdd2dd1702b`，当前 Medium worker nonce 为 `6d6528a03d864927811e5083c49dba60`。它补齐 8037 的 5.15.1 发布包单屏限定验收，不把之前的 TCG 失败改写为通过。加速器及 CPU 模型配置均发生变化，成功结果不能单独确定此前 Start/Status 超时的具体根因。

## 显示变化测试方法

现有 `std VGA + Microsoft BasicDisplay` 客体是单显示器环境。Windows viogpudo 驱动实现也只支持单个显示源，给单张虚拟显卡设置多个输出不等于 Windows 实际获得双屏。多屏候选使用独立虚拟显卡，并要求 Windows 枚举出实际活动显示器、桌面坐标和对应任务栏后才进入产品验收。

单独的分辨率恢复脚本运行真实后台应用，使用普通用户与独立配置目录，实际调用 `ChangeDisplaySettingsEx` 切换并恢复驱动公布的模式。脚本观察操作系统产生的 `WM_DISPLAYCHANGE`、原会话恢复、重新连接以及实际时钟文字与宽度，并在结束时请求应用正常退出、核对原显示模式与 Explorer 连续性。它不发送合成显示变化消息，不把分辨率变化称为显示器插拔或混合 DPI 测试。

恢复后的时间文字可能正常跨分钟变化，因此单独保留“原生文字完全相等”观察值；结构恢复要求控件类型、可见性、高度、容器宽度一致，且没有遗留测试文字。该范围不同于短时主屏检查中的完整原字段相等证明。

### 25H2 实际分辨率变化与后台服务恢复

运行 `sandbox-20261109-204556-2195f404` 使用上述发布包及新的 Medium worker nonce `3d921fbc481f45919ca308f6889b5477`，实际启动后台应用 PID 5484。原 Explorer PID 4168、线程 4372、主任务栏窗口 65732 保持不变。

| 操作 | Windows 实际观察 | LiteTray 实际观察 |
| --- | --- | --- |
| 启动后台服务 | 单屏 `1024×768`、96 DPI | 指定测试文字，时钟容器宽度 240 |
| 切换分辨率 | 驱动当前模式及显示器矩形均为 `800×600` | 收到 `WM_DISPLAYCHANGE`，确认旧状态恢复后重新接管，文字及宽度正确 |
| 恢复分辨率 | 驱动当前模式及显示器矩形均为 `1024×768` | 第二次真实显示通知、恢复、重新接管均完成 |
| 正常退出应用 | 原显示模式、Explorer 和任务栏窗口保持 | 应用 exit 0，原时钟字段、文字及完整 taskbar 数组均恢复 |

五个只读观察进程均实际 exit 0、未超时，独立报告与标准输出一致。两次恢复耗时约 6.8 秒，包含显示变化后的稳定等待。未重启 Explorer、未强制结束应用、未写入显示注册表、未发送合成显示消息。这一结果只覆盖单屏分辨率变化，不替代显示器断开重连、双屏复制、混合 DPI 或鼠标交互验收。

本轮外层启动器仍报告失败：`Start-Process -PassThru` 未保留有效退出码，得到 `exit_code=null`。已保留这一基础设施失败；不能把整轮外层测试标为通过。内层 worker/launcher 正常返回，场景 summary 无测试或清理错误；独立审计从当前 nonce 的原始模式、任务栏报告、服务事件及正常退出记录完成 62 项核验，确认上述业务场景通过。后续启动器改用保留进程句柄的 `System.Diagnostics.Process`，不改变场景断言。

完整归档 SHA-256 为 `de1f8b67f411790377c6909a97667d3a5e01cc8d599c7d29e446f50da8aaa7ea`。本地复核脚本与结果为 `review_8037_mode_recovery.py` 和 `8037-mode-recovery-independent-review.json`。此前 `sandbox-20261109-203950-10c8c6b4` 因持久 PowerShell 中重复 `Add-Type` 而未进入场景的失败归档也继续保留。

### 普通 24H2 的实际分辨率恢复

独立运行 `sandbox-20261109-210028-fdb9d3bc`，nonce `54b047a83ef4435e9f771ed4aad11759`，使用相同发布包及保留句柄的启动器完成 `1024×768 → 800×600 → 1024×768`。两次真实 `WM_DISPLAYCHANGE` 后均先确认恢复、再重新接管，五个实际时钟观察进程全部 exit 0。原 Explorer PID 3544 保持，后台应用 PID 4864 经正常 Exit 返回 0，外层启动器也实际返回 0。74 项独立核验通过。

退出后原生时钟的结构、宽度、可见性和日期均恢复；时间从 `9∶00 PM` 变成 `9∶01 PM`，因此如实保留 `native_text_exact_match=false`。前后读请求时间为 `21:00:59.330810` 和 `21:01:17.404973`，符合正常跨分钟走时。它只证明这次时间更新，不能代表持续刷新验收。

完整 taskbar 数组不完全相等：除了两处同一时间文本，隐藏图标箭头的六个子元素各有上边界 +1、下边界 +2 像素的差异。逻辑元素属性相同，几何变化原因未确定，未把它写作完整托盘几何恢复通过。场景依据运行前固定的时钟结构恢复规则判定，未因本次结果放宽断言。

完整归档 SHA-256 为 `901a4aaf4de1016db4ac1b2fb9a9955b01aac237eb3fb6bf2ec0e21e70b27668`。复核脚本和结果为 `review_24h2_mode_recovery.py` 与 `24h2-mode-recovery-independent-review.json`。

## 通信诊断的补充

提交 `c6f147cee11aa02b21e8065054b26deb191fdf9f` 区分启动/停止命令发送失败与随后状态确认失败，未确认恢复时继续保留原连接。调用 `SendMessageTimeoutW` 前清除旧的 Win32 last-error；返回失败但没有系统错误码时如实说明，避免将之前的错误码归给本次通信。这些改动尚未进入上述已发布 5.15.1 二进制。

本地五项传输与恢复回归、17 项服务状态机检查、格式检查、Clippy 和 release 构建通过。[GitHub CI 42616877805](https://github.com/Lianzy-Baimiao/LiteTray/actions/runs/42616877805) 的第二次执行完整通过。第一次只有原有页面切换的冷态 100ms 断言失败，实测 131.012ms，暖态 p95 为 30.405ms；失败日志保留，复跑未改变源码、阈值或测试。两次结果不作为虚拟机兼容验收。
