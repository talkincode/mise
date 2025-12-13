# misec

misec (mise context) 是一个本地上下文准备工具，用于将项目中的文件、片段和锚点整理成 Agent 可直接消费的上下文候选集合。

它不生成答案，只负责把材料摆好。

## 特点

• 面向 Agent 的 结构化输出（jsonl / json / md）
• 压缩并编排 find / grep / cat 类操作
• 支持显式 Anchor（锚点），精准圈定上下文
• 依赖分析与变更影响追踪
• 项目内缓存，可随时重建

## 平台支持

• ✅ Linux
• ✅ macOS
• ❌ Windows（不支持，WSL 不保证）

## 安装

```bash
# 从 crates.io 安装
cargo install misec

# 从 GitHub 安装
cargo install --git https://github.com/talkincode/mise

# 或下载预编译二进制
# https://github.com/talkincode/mise/releases
```

本地构建：

```bash
cargo build --release
# 二进制文件在 target/release/misec
```

## 命令速查

| 命令      | 用途               | 示例                                       |
| --------- | ------------------ | ------------------------------------------ |
| `scan`    | 扫描项目结构       | `misec scan --type file --max-depth 2`    |
| `find`    | 按名称查找文件     | `misec find readme --scope docs`          |
| `extract` | 提取文件片段       | `misec extract src/main.rs --lines 10:50` |
| `match`   | 文本搜索 (ripgrep) | `misec match "TODO\|FIXME" src/`          |
| `ast`     | AST 结构搜索       | `misec ast "fn main" --scope src`         |
| `deps`    | 依赖分析           | `misec deps src/cli.rs --reverse`         |
| `impact`  | 变更影响分析       | `misec impact --staged`                   |
| `anchor`  | 锚点管理           | `misec anchor list --tag chapter`         |
| `flow`    | 组合工作流         | `misec flow pack --anchors intro`         |
| `doctor`  | 检查依赖状态       | `misec doctor`                            |
| `rebuild` | 重建缓存           | `misec rebuild`                           |

## 基本用法

### 扫描项目

```bash
misec scan                           # 扫描所有文件和目录
misec scan --type file               # 仅列出文件
misec scan --type dir --max-depth 2  # 仅列出目录，深度限制
misec scan --scope src --hidden      # 扫描 src/，包含隐藏文件
```

### 查找文件

```bash
misec find cargo                     # 查找路径包含 "cargo" 的文件
misec find readme --scope docs       # 在 docs/ 下查找
```

### 文本匹配（ripgrep 后端）

```bash
misec match "TODO" src/              # 在 src/ 中搜索 TODO
misec match "TODO|FIXME"             # 正则搜索多个模式
misec match "unsafe" src tests       # 在多个目录中搜索
```

### 提取指定范围内容

```bash
misec extract README.md --lines 1:40       # 提取第 1-40 行
misec extract src/main.rs --lines 10:60 --max-bytes 20000
```

### AST 结构搜索（ast-grep 后端）

```bash
misec ast "console.log(\$A)" src           # 搜索 console.log 调用
misec ast "unsafe { \$A }"                 # 搜索 unsafe 块
```

默认输出格式为 jsonl，适合 Agent 解析。

## 依赖分析（deps）

分析代码文件之间的依赖关系，支持 Rust、TypeScript/JavaScript、Python。

```bash
misec deps src/cli.rs                # 分析 cli.rs 依赖了哪些文件
misec deps src/cli.rs --reverse      # 分析哪些文件依赖了 cli.rs
misec deps                           # 分析整个项目的依赖图
```

### 输出格式

```bash
misec deps --deps-format jsonl       # JSON Lines（默认）
misec deps --deps-format json        # 完整 JSON
misec deps --deps-format dot         # Graphviz DOT（可视化）
misec deps --deps-format mermaid     # Mermaid 图（嵌入 Markdown）
misec deps --deps-format tree        # ASCII 树形视图
misec deps --deps-format table       # ASCII 表格
```

生成依赖图可视化：

```bash
misec deps --deps-format dot | dot -Tpng -o deps.png
misec deps --deps-format mermaid >> README.md
```

## 变更影响分析（impact）

结合 git diff 与依赖图，分析代码变更的影响范围。

```bash
misec impact                         # 分析未暂存的变更
misec impact --staged                # 分析已暂存的变更
misec impact --commit abc123         # 分析特定提交
misec impact --diff main..feature    # 比较分支差异
misec impact --max-depth 5           # 设置传递影响的最大深度
```

### 输出格式

```bash
misec impact --impact-format jsonl   # JSON（默认）
misec impact --impact-format json    # 美化 JSON
misec impact --impact-format summary # 人类可读摘要
misec impact --impact-format table   # ASCII 表格
```

## Anchor（锚点）

在文本中定义显式语义范围：

```markdown
<!--Q:begin id=ch01.bg tags=chapter,background v=1-->

这里是第一章的背景设定。

<!--Q:end id=ch01.bg-->
```

### 锚点查询

```bash
misec anchor list                    # 列出所有锚点
misec anchor list --tag chapter      # 按标签过滤
misec anchor get ch01.bg             # 获取特定锚点内容
misec anchor get intro --with-neighbors 3  # 获取相关锚点
misec anchor lint                    # 检查锚点配对、重复 ID 等问题
```

### 锚点标记（mark）

Agent 可以用 mark 命令在代码中插入锚点标记：

```bash
# 单个标记
misec anchor mark README.md --start 10 --end 25 --id intro
misec anchor mark src/main.rs --start 1 --end 50 --id main.entry --tags entry,core

# 预览模式（不实际修改）
misec anchor mark doc.md --start 5 --end 10 --id sec1 --dry-run

# 批量标记（JSON 输入）
misec anchor batch --json '[
  {"path": "README.md", "start_line": 1, "end_line": 10, "id": "intro", "tags": ["doc"]},
  {"path": "src/main.rs", "start_line": 5, "end_line": 20, "id": "main"}
]'

# 从文件读取批量标记
misec anchor batch --file marks.json --dry-run

# 移除锚点标记（保留内容）
misec anchor unmark README.md --id intro
```

Anchor 用于作者主动声明上下文边界，而不是自动推断。

## Flow（工作流）

Flow 是对多个基础操作的固定组合，用于快速准备可用上下文。

### writing - 写作上下文

```bash
misec flow writing --anchor ch01.bg           # 收集写作相关上下文
misec flow writing --anchor intro --max-items 12
```

### pack - 上下文打包

将锚点和文件打包成适合 AI 的上下文包：

```bash
misec flow pack --anchors cli.scan,core.model           # 打包多个锚点
misec flow pack --anchors intro --files README.md       # 锚点 + 文件
misec flow pack --anchors api --max-tokens 8000         # 限制 token 数量
misec flow pack --anchors api --priority confidence     # 按置信度优先
misec flow pack --anchors api --stats                   # 显示统计信息
```

### stats - 项目统计

```bash
misec flow stats                             # 基本统计
misec flow stats --stats-format summary      # 人类可读摘要
misec flow stats --stats-format json         # 完整 JSON
misec flow stats --stats-format table        # Markdown 表格
misec flow stats --scope docs --exts md,txt  # 限定范围和扩展名
misec flow stats --top 20                    # 显示前 20 大文件
```

统计内容包括：

- 字符数、词数、行数
- CJK 字符计数（中日韩）
- 估算 Token 数
- 锚点按标签分布
- 最大文件排名

### outline - 文档大纲

基于锚点生成项目大纲：

```bash
misec flow outline                           # 完整大纲
misec flow outline --tag chapter             # 按标签过滤
misec flow outline --scope docs              # 限定范围
misec flow outline --outline-format tree     # ASCII 树形视图
misec flow outline --outline-format json     # JSON 输出
misec flow outline --outline-format markdown # Markdown（默认）
```

Flow 输出的是组织后的材料，不是结论。

## 输出格式

```bash
--format jsonl  # 默认，Agent 推荐，每行一个 JSON 对象
--format json   # 完整 JSON 数组
--format md     # Markdown，人类可读
--format raw    # 调试用（不保证可解析）
--pretty        # JSON 美化输出
```

所有格式来自同一内部结果模型，仅展示方式不同。

## 缓存

• 项目内缓存目录：.mise/
• 缓存可随时删除并重建
• 不跨项目共享任何状态

```bash
misec rebuild                        # 重建缓存
```

## 第三方依赖

misec 会自动集成以下工具（如存在）：

• ripgrep (rg)：文本匹配 `misec match`
• ast-grep (sg)：AST 结构匹配 `misec ast`
• watchexec（可选）：文件变更触发 `misec watch`

检查依赖状态：

```bash
misec doctor
```

## 典型工作流组合

### 1. 探索陌生代码库

```bash
# 了解项目结构
misec scan --type file --max-depth 3

# 找入口点
misec match "fn main|async fn" src/

# 查看已标记的关键区域
misec anchor list

# 分析核心模块的依赖
misec deps src/main.rs --deps-format tree
```

### 2. 理解变更影响

```bash
# 分析暂存的改动会影响哪些文件
misec impact --staged --impact-format summary

# 谁依赖这个文件？
misec deps src/cli.rs --reverse

# 生成依赖可视化
misec deps --deps-format dot | dot -Tpng -o impact.png
```

### 3. 为 AI 准备上下文

```bash
# 打包核心模块给 AI
misec flow pack --anchors core.model,cli.commands --max-tokens 4000

# 统计项目规模
misec flow stats --stats-format summary

# 生成文档大纲
misec flow outline --outline-format markdown
```

### 4. 长期维护标记

```bash
# 标记核心模块，方便后续快速定位
misec anchor mark src/core/model.rs --start 1 --end 100 --id core.model --tags core,data
misec anchor mark src/cli.rs --start 500 --end 600 --id cli.commands --tags cli,entry

# 批量标记多个位置
misec anchor batch --json '[
  {"path": "src/main.rs", "start_line": 1, "end_line": 30, "id": "main.entry", "tags": ["entry"]},
  {"path": "src/lib.rs", "start_line": 1, "end_line": 50, "id": "lib.exports", "tags": ["api"]}
]'

# 验证标记正确性
misec anchor lint
```

### 5. 代码审查辅助

```bash
# 查看 PR 影响范围
misec impact --diff main..feature --impact-format summary

# 检查受影响的锚点区域
misec impact --staged --impact-format json | jq '.affected_anchors'

# 打包相关上下文供审查
misec flow pack --anchors affected.module --files CHANGELOG.md
```

## 设计边界（重要）

misec 不会：

• 生成答案或结论
• 保证上下文完整或正确
• 做语义理解或向量检索
• 替代 Agent 的判断

misec 提供的是 **上下文候选集合**，不是事实来源。

## 适用场景

• 写作项目（章节、设定、素材管理）
• 需要反复探索的代码仓库
• Agent 工作流中频繁调用文件检索的场景
• 接受噪音，但要求可回溯的上下文准备

## 一句话

**misec 负责备料，不负责出菜。**
