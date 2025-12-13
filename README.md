# mise

mise 是一个本地上下文准备工具，用于将项目中的文件、片段和锚点整理成 Agent 可直接消费的上下文候选集合。

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
cargo install mise
```

或本地构建：

```bash
cargo build --release
```

## 命令速查

| 命令      | 用途               | 示例                                     |
| --------- | ------------------ | ---------------------------------------- |
| `scan`    | 扫描项目结构       | `mise scan --type file --max-depth 2`    |
| `find`    | 按名称查找文件     | `mise find readme --scope docs`          |
| `extract` | 提取文件片段       | `mise extract src/main.rs --lines 10:50` |
| `match`   | 文本搜索 (ripgrep) | `mise match "TODO\|FIXME" src/`          |
| `ast`     | AST 结构搜索       | `mise ast "fn main" --scope src`         |
| `deps`    | 依赖分析           | `mise deps src/cli.rs --reverse`         |
| `impact`  | 变更影响分析       | `mise impact --staged`                   |
| `anchor`  | 锚点管理           | `mise anchor list --tag chapter`         |
| `flow`    | 组合工作流         | `mise flow pack --anchors intro`         |
| `doctor`  | 检查依赖状态       | `mise doctor`                            |
| `rebuild` | 重建缓存           | `mise rebuild`                           |

## 基本用法

### 扫描项目

```bash
mise scan                           # 扫描所有文件和目录
mise scan --type file               # 仅列出文件
mise scan --type dir --max-depth 2  # 仅列出目录，深度限制
mise scan --scope src --hidden      # 扫描 src/，包含隐藏文件
```

### 查找文件

```bash
mise find cargo                     # 查找路径包含 "cargo" 的文件
mise find readme --scope docs       # 在 docs/ 下查找
```

### 文本匹配（ripgrep 后端）

```bash
mise match "TODO" src/              # 在 src/ 中搜索 TODO
mise match "TODO|FIXME"             # 正则搜索多个模式
mise match "unsafe" src tests       # 在多个目录中搜索
```

### 提取指定范围内容

```bash
mise extract README.md --lines 1:40       # 提取第 1-40 行
mise extract src/main.rs --lines 10:60 --max-bytes 20000
```

### AST 结构搜索（ast-grep 后端）

```bash
mise ast "console.log(\$A)" src           # 搜索 console.log 调用
mise ast "unsafe { \$A }"                 # 搜索 unsafe 块
```

默认输出格式为 jsonl，适合 Agent 解析。

## 依赖分析（deps）

分析代码文件之间的依赖关系，支持 Rust、TypeScript/JavaScript、Python。

```bash
mise deps src/cli.rs                # 分析 cli.rs 依赖了哪些文件
mise deps src/cli.rs --reverse      # 分析哪些文件依赖了 cli.rs
mise deps                           # 分析整个项目的依赖图
```

### 输出格式

```bash
mise deps --deps-format jsonl       # JSON Lines（默认）
mise deps --deps-format json        # 完整 JSON
mise deps --deps-format dot         # Graphviz DOT（可视化）
mise deps --deps-format mermaid     # Mermaid 图（嵌入 Markdown）
mise deps --deps-format tree        # ASCII 树形视图
mise deps --deps-format table       # ASCII 表格
```

生成依赖图可视化：

```bash
mise deps --deps-format dot | dot -Tpng -o deps.png
mise deps --deps-format mermaid >> README.md
```

## 变更影响分析（impact）

结合 git diff 与依赖图，分析代码变更的影响范围。

```bash
mise impact                         # 分析未暂存的变更
mise impact --staged                # 分析已暂存的变更
mise impact --commit abc123         # 分析特定提交
mise impact --diff main..feature    # 比较分支差异
mise impact --max-depth 5           # 设置传递影响的最大深度
```

### 输出格式

```bash
mise impact --impact-format jsonl   # JSON（默认）
mise impact --impact-format json    # 美化 JSON
mise impact --impact-format summary # 人类可读摘要
mise impact --impact-format table   # ASCII 表格
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
mise anchor list                    # 列出所有锚点
mise anchor list --tag chapter      # 按标签过滤
mise anchor get ch01.bg             # 获取特定锚点内容
mise anchor get intro --with-neighbors 3  # 获取相关锚点
mise anchor lint                    # 检查锚点配对、重复 ID 等问题
```

### 锚点标记（mark）

Agent 可以用 mark 命令在代码中插入锚点标记：

```bash
# 单个标记
mise anchor mark README.md --start 10 --end 25 --id intro
mise anchor mark src/main.rs --start 1 --end 50 --id main.entry --tags entry,core

# 预览模式（不实际修改）
mise anchor mark doc.md --start 5 --end 10 --id sec1 --dry-run

# 批量标记（JSON 输入）
mise anchor batch --json '[
  {"path": "README.md", "start_line": 1, "end_line": 10, "id": "intro", "tags": ["doc"]},
  {"path": "src/main.rs", "start_line": 5, "end_line": 20, "id": "main"}
]'

# 从文件读取批量标记
mise anchor batch --file marks.json --dry-run

# 移除锚点标记（保留内容）
mise anchor unmark README.md --id intro
```

Anchor 用于作者主动声明上下文边界，而不是自动推断。

## Flow（工作流）

Flow 是对多个基础操作的固定组合，用于快速准备可用上下文。

### writing - 写作上下文

```bash
mise flow writing --anchor ch01.bg           # 收集写作相关上下文
mise flow writing --anchor intro --max-items 12
```

### pack - 上下文打包

将锚点和文件打包成适合 AI 的上下文包：

```bash
mise flow pack --anchors cli.scan,core.model           # 打包多个锚点
mise flow pack --anchors intro --files README.md       # 锚点 + 文件
mise flow pack --anchors api --max-tokens 8000         # 限制 token 数量
mise flow pack --anchors api --priority confidence     # 按置信度优先
mise flow pack --anchors api --stats                   # 显示统计信息
```

### stats - 项目统计

```bash
mise flow stats                             # 基本统计
mise flow stats --stats-format summary      # 人类可读摘要
mise flow stats --stats-format json         # 完整 JSON
mise flow stats --stats-format table        # Markdown 表格
mise flow stats --scope docs --exts md,txt  # 限定范围和扩展名
mise flow stats --top 20                    # 显示前 20 大文件
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
mise flow outline                           # 完整大纲
mise flow outline --tag chapter             # 按标签过滤
mise flow outline --scope docs              # 限定范围
mise flow outline --outline-format tree     # ASCII 树形视图
mise flow outline --outline-format json     # JSON 输出
mise flow outline --outline-format markdown # Markdown（默认）
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
mise rebuild                        # 重建缓存
```

## 第三方依赖

mise 会自动集成以下工具（如存在）：

• ripgrep (rg)：文本匹配 `mise match`
• ast-grep (sg)：AST 结构匹配 `mise ast`
• watchexec（可选）：文件变更触发 `mise watch`

检查依赖状态：

```bash
mise doctor
```

## 典型工作流组合

### 1. 探索陌生代码库

```bash
# 了解项目结构
mise scan --type file --max-depth 3

# 找入口点
mise match "fn main|async fn" src/

# 查看已标记的关键区域
mise anchor list

# 分析核心模块的依赖
mise deps src/main.rs --deps-format tree
```

### 2. 理解变更影响

```bash
# 分析暂存的改动会影响哪些文件
mise impact --staged --impact-format summary

# 谁依赖这个文件？
mise deps src/cli.rs --reverse

# 生成依赖可视化
mise deps --deps-format dot | dot -Tpng -o impact.png
```

### 3. 为 AI 准备上下文

```bash
# 打包核心模块给 AI
mise flow pack --anchors core.model,cli.commands --max-tokens 4000

# 统计项目规模
mise flow stats --stats-format summary

# 生成文档大纲
mise flow outline --outline-format markdown
```

### 4. 长期维护标记

```bash
# 标记核心模块，方便后续快速定位
mise anchor mark src/core/model.rs --start 1 --end 100 --id core.model --tags core,data
mise anchor mark src/cli.rs --start 500 --end 600 --id cli.commands --tags cli,entry

# 批量标记多个位置
mise anchor batch --json '[
  {"path": "src/main.rs", "start_line": 1, "end_line": 30, "id": "main.entry", "tags": ["entry"]},
  {"path": "src/lib.rs", "start_line": 1, "end_line": 50, "id": "lib.exports", "tags": ["api"]}
]'

# 验证标记正确性
mise anchor lint
```

### 5. 代码审查辅助

```bash
# 查看 PR 影响范围
mise impact --diff main..feature --impact-format summary

# 检查受影响的锚点区域
mise impact --staged --impact-format json | jq '.affected_anchors'

# 打包相关上下文供审查
mise flow pack --anchors affected.module --files CHANGELOG.md
```

## 设计边界（重要）

mise 不会：

• 生成答案或结论
• 保证上下文完整或正确
• 做语义理解或向量检索
• 替代 Agent 的判断

mise 提供的是 **上下文候选集合**，不是事实来源。

## 适用场景

• 写作项目（章节、设定、素材管理）
• 需要反复探索的代码仓库
• Agent 工作流中频繁调用文件检索的场景
• 接受噪音，但要求可回溯的上下文准备

## 一句话

**mise 负责备料，不负责出菜。**
