# 私有子宿主接口的特征识别

同一种 source 字段布局可以对应不同私有接口。早期与晚期 21H2 的实际 x64 shell 都有 13 槽子宿主虚表，字段链都是 `host + 0x20 → child + 0x10 → source`，但接口 IID 不同。因此 `SourceRecipe` 描述字段、虚表和调用形态，`ChildInterface` 单独描述经过核对的接口身份；不按 Windows 版本号推断 IID。

| 实际样本 | source 布局 | 私有 IID | 验证范围 |
| --- | --- | --- | --- |
| twinui 22000.282 | `xaml_explorer_host_child_xfg_v1`，13 槽 | `eef8f632-6c0d-5fcb-b6a1-354819e51f3b` | 22000.318 主屏时钟已完成正式包实测 |
| twinui 22000.3196 | 同上 | `ec97afd0-5c0d-581b-9508-074773c6f1e0` | 独立模块静态验证，未登记运行支持 |
| 现有较新 source 方案 | `xaml_explorer_host_child_v1`，含工厂证据 | `ec97afd0-5c0d-581b-9508-074773c6f1e0` | 各内置资料保持原有验收范围 |

旧资料省略 `child_interface`，继续使用其原先已经核对的接口。该字段不参与旧资料的序列化，保持既有资料摘要不变。组合计划显式保留 shell 的有效接口；匹配同一 shell 时，接口不同就视为解析方案冲突。现代 source 方案不能指定早期 eef8 接口。

运行时仍先核对模块、具体虚表和入口，再向真实对象查询唯一的准确 IID。没有逐个尝试接口、按显示版本放行或跳过对象所有权检查。此修改没有增加晚期 21H2 的生产配置。

## 3196 静态证据

shell SHA-256：`1c614e888a2f1f0a4090d45b6b65a0ab91f5277140748b05dd614d7e64545c63`。复用已缓存的微软符号服务器响应及匹配 PDB，本次没有新增网络下载。

静态解析器要求命名 GUID、`find_interface` 比较分支和 `GetIids` 两项列表一致。完整查询路径证明返回实现对象 `+0x10` 的 ABI 子接口，成功查询增加自有引用。构造函数将子虚表绑定到这个子对象，并将实现虚表绑定到实现对象。

晚期 ec97 方案另外核对以下引用契约：

- AddRef 的实现引用位置、原子增加、弱引用表示及其强引用计数。
- Release 的 `this - 0x10` 调整、原子减少、最后引用的析构分派。
- 实现虚表 `+8` 的唯一 DIR64 重定位与准确的命名析构入口。
- 弱控制块的独立计数、模块计数一致性、计数下溢终止路径，以及 sized delete 经两个直接跳转到准确 CRT free 导入。
- XAML getter 的返回值转移、异常返回和清理，及实现 `+0x20` / ABI `+0x10` 的 source 字段。

以上是实际使用的接口与所有权契约检查，不是对每个析构后代或未使用私有方法的完整符号执行。13 个子虚表入口、各入口前缀、代码归属与重定位也须逐一匹配。

最小实际字节夹具保存在 `tools/compatibility/fixtures/xfg-source-ec97-v1.json`。新增回归先从真实模块重放得到“早期 IID 不匹配”的拒绝，再增加独立契约；已有早期夹具结果保持一致。突变检查覆盖错误 IID、GetIids、返回对象偏移、引用计数、析构入口、free 导入和 getter 转移。运行命令：

```powershell
python -m unittest discover -s tools/compatibility -p 'test_*.py'
```

本轮完整 Python 集合 72 项通过。Rust 协议回归另检查旧资料序列化不变、未知接口拒绝、现代布局约束、shell 契约冲突与组合计划的接口来源。

## 尚缺的安装与运行证据

KB5044280 的官方清单要求 twinui `22000.3079`，已有符号服务器地址却返回 `22000.3196`。原始拒绝记录保持不变。Taskbar `22000.3260`、ExplorerExtensions `423.29300.0.0` 与这个独立 shell 样本没有同一安装系统的证明，不能组合成“3260 已支持”的生产资料。

缓存的 Taskbar3260 代码另显示通知宿主仍保存 `+0x60` 父 HWND、`+0x28` child，shared_ptr 对象位于控制块 `+0x10`。这只是局部字段证据；尚未完成晚期主屏/副屏通知宿主整条路径、实际控件、接管恢复和多屏验收，因此没有输出新的通知宿主配置。

本地重放与独立复核收据位于 `local/compatibility-expansion-5.16/`：`late-child-red.json`、`late-child-green.json`、`late21-independent-fixture-review-final.json`、`late21-independent-mutations.json`。这些材料均不代表晚期 21H2 的客体运行通过。
