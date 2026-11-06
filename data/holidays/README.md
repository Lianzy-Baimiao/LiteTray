# 中国节假日内置快照

来源：[NateScarlet/holiday-cn](https://github.com/NateScarlet/holiday-cn)，MIT，Copyright (c) 2019 NateScarlet；完整许可见 LICENSE。

2025、2026、2027 JSON 原样取自 commit `ac19c3a7e7fc9208a4be9ecccd3cb1049b0e8092`，核对日期 2026-11-07。2026 与 2027 的 papers 链接已访问，标题分别为国务院办公厅关于该年部分节假日安排的通知。

程序启动及翻页按需在后台更新，每份有效数据最多每天检查一次；原始 GitHub 下载失败会尝试 jsDelivr，同样需要通过结构与年份校验。缓存位于本地数据目录 holidays/。网络失败保留已验证数据，不将未知年份当作普通工作日。

isOffDay=true 表示公告安排的放假日，false 表示补班日，不代表具体一天的法定假日本体属性。周末连休可能不在上游数据中。年份按国务院文件标题归属，下一年文件可包含上一年十二月的安排。
