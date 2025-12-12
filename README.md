# mise

mise 是一个本地上下文准备工具，用于将项目中的文件、片段和锚点整理成 Agent 可直接消费的上下文候选集合。

它不是搜索引擎，也不生成答案。

⸻

安装

# 示例
cargo install mise

或直接使用源码构建：

cargo build --release


⸻

基本用法

扫描项目

mise scan

查找文件

mise find src --ext md

文本匹配

mise match "TODO" src/

提取指定范围内容

mise extract README.md --lines 20:80

默认输出为 jsonl，适合 Agent 解析。

⸻

Anchor（锚点）

mise 支持在文本中使用显式锚点，定义语义范围。

示例（Markdown）：

<!--Q:begin id=ch01.bg tags=chapter,background v=1-->
这里是第一章的背景设定。
<!--Q:end id=ch01.bg-->

获取锚点内容：

mise anchor get ch01.bg

列出所有锚点：

mise anchor list

检查锚点一致性：

mise anchor lint


⸻

Flow（工作流）

Flow 是对多个基础操作的固定组合，用于快速获得可用上下文。

示例：

mise flow summarize
mise flow writing

Flow 输出的是 组织后的材料，而不是结论。

⸻

输出格式

支持多种输出格式：

--format jsonl   # 默认，Agent 推荐
--format json
--format md
--format raw     # 调试 / 兼容

所有格式均来自同一内部结果模型，仅展示方式不同。

⸻

缓存
	•	mise 在项目内使用 .mise/ 目录存放缓存
	•	缓存可随时删除并重建
	•	不跨项目共享任何状态

强制重建：

mise rebuild


⸻

设计边界（重要）

mise 不会：
	•	生成结论或“答案”
	•	保证上下文完整或正确
	•	做语义理解或向量检索
	•	替代 Agent 的判断

mise 只负责 准备上下文材料。

⸻

适用场景
	•	写作项目（章节、设定、素材管理）
	•	需要反复探索的代码仓库
	•	Agent 工作流中反复调用 find / grep / cat 的场景
	•	接受噪音，但要求可回溯的上下文准备

⸻

一句话

mise 负责把材料摆上桌，
决定怎么用，是你的事。


	2.	一个 “为什么不用向量 / 为什么允许噪音” 的 FAQ 简版

这两样能明显减少误用和 issue。
