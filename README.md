# mise

mise 是一个本地上下文准备工具，用于将项目中的文件、片段和锚点整理成 Agent 可直接消费的上下文候选集合。

它不生成答案，只负责把材料摆好。

## 特点

• 面向 Agent 的 结构化输出（jsonl / json / md）
• 压缩并编排 find / grep / cat 类操作
• 支持显式 Anchor（锚点），精准圈定上下文
• 允许噪音，但要求 可回溯、可标注
• 项目内缓存，可随时重建

## 平台支持

• ✅ Linux
• ✅ macOS
• ❌ Windows（不支持，WSL 不保证）

## 安装

`cargo install mise`

或本地构建：

'cargo build --release'

## 基本用法

扫描项目

`mise scan`

查找文件

`mise find src --ext md`

文本匹配（ripgrep 后端）

`mise match "TODO" src/`

提取指定范围内容（替代 cat）

`mise extract README.md --lines 20:80`

默认输出格式为 jsonl，适合 Agent 解析。

## Anchor（锚点）

在文本中定义显式语义范围：

```markdown
<!--Q:begin id=ch01.bg tags=chapter,background v=1-->

这里是第一章的背景设定。

<!--Q:end id=ch01.bg-->
```

使用方式：

```bash
mise anchor list
mise anchor get ch01.bg
mise anchor lint
```

Anchor 用于作者主动声明上下文边界，而不是自动推断。

## Flow（工作流）

Flow 是对多个基础操作的固定组合，用于快速准备可用上下文。

`mise flow writing --anchor ch01.bg`

Flow 输出的是组织后的材料，不是结论。

## 输出格式

```text
--format jsonl # 默认，Agent 推荐
--format json
--format md
--format raw # 调试 / 兼容（不保证可解析）
```

所有格式来自同一内部结果模型，仅展示方式不同。

## 缓存

• 项目内缓存目录：.mise/
• 缓存可随时删除并重建
• 不跨项目共享任何状态

`mise rebuild`

## 第三方依赖

mise 会自动集成以下工具（如存在）：

• ripgrep (rg)：文本匹配
• ast-grep (sg)：AST 结构匹配
• watchexec（可选）：文件变更触发

检查依赖状态：

`mise doctor`

## 设计边界（重要）

mise 不会：

• 生成答案或结论
• 保证上下文完整或正确
• 做语义理解或向量检索
• 替代 Agent 的判断

mise 提供的是 上下文候选集合，不是事实来源。

## 适用场景

• 写作项目（章节、设定、素材管理）
• 需要反复探索的代码仓库
• Agent 工作流中频繁调用文件检索的场景
• 接受噪音，但要求可回溯的上下文准备

一句话

mise 负责备料，不负责出菜。
