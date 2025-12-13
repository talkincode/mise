# mise Copilot Instructions

mise 是一个**命令行上下文准备工具**，将项目中的文件、片段和锚点整理成 Agent 可直接消费的结构化输出。**它只负责读取和组织材料，不生成答案。**

> **重要**: mise 安装在 `~/bin/mise`，所有命令通过 `run_in_terminal` 执行。

## 🚀 会话初始化（每次新对话必做）

```bash
# 1. 检查工具链完整性
mise doctor

# 2. 重建缓存（首次进入或文件有变动时）
mise rebuild

# 3. 了解项目结构
mise scan --type file --max-depth 2 --pretty

# 4. 查看已标记的锚点
mise anchor list --pretty
```

### rebuild 触发时机

- 首次进入项目
- 大量文件增删改后
- 锚点查询结果不符预期
- `.mise/` 目录被删除

## 🎯 何时使用 mise

### ✅ 优先使用 mise

| 场景       | 命令                                             | 优势                            |
| ---------- | ------------------------------------------------ | ------------------------------- |
| 项目结构   | `mise scan --type file --max-depth 3 --pretty`   | 结构化输出，自动过滤 .gitignore |
| 提取片段   | `mise extract file.rs --lines 10:50`             | 精确范围，Token 可控            |
| 搜索代码   | `mise match "pattern" src/ --pretty`             | JSON 输出含行号上下文           |
| 锚点管理   | `mise anchor list/get/mark`                      | 语义标记，无替代方案            |
| 依赖分析   | `mise deps src/cli.rs --deps-format tree`        | 正向/反向依赖可视化             |
| 变更影响   | `mise impact --staged --impact-format summary`   | 结合依赖图分析                  |
| 上下文打包 | `mise flow pack --anchors a,b --max-tokens 4000` | Token 预算控制                  |
| 项目统计   | `mise flow stats --stats-format summary`         | 字符/词/Token 计数              |

### ❌ 不适合 mise

| 场景     | 原因                | 应使用                   |
| -------- | ------------------- | ------------------------ |
| 编辑文件 | mise 只读取         | `replace_string_in_file` |
| 运行命令 | mise 不执行任意命令 | `run_in_terminal`        |
| 创建文件 | mise 不创建文件     | `create_file`            |
| 简单读取 | < 100 行时无优势    | `read_file`              |

## 📋 命令速查

```bash
# 诊断
mise doctor                              # 检查依赖状态
mise rebuild                             # 重建缓存

# 探索
mise scan --type file --max-depth 2 --pretty
mise find "readme"                       # 按名称查找
mise match "TODO|FIXME" --pretty         # 搜索模式
mise ast "fn main" src/                  # AST 搜索

# 提取
mise extract src/main.rs --lines 1:100   # 提取行范围

# 锚点
mise anchor list --pretty                # 列出锚点
mise anchor list --tag core              # 按标签过滤
mise anchor get intro --with-neighbors 3 # 获取锚点+上下文
mise anchor lint                         # 检查问题
mise anchor mark src/cli.rs --start 10 --end 50 --id cli.commands --tags core
mise anchor unmark src/cli.rs --id cli.commands

# 依赖
mise deps src/cli.rs                     # 正向依赖
mise deps src/cli.rs --reverse           # 反向依赖
mise deps --deps-format tree             # 树形视图

# 变更
mise impact                              # 未暂存变更
mise impact --staged                     # 已暂存变更
mise impact --impact-format summary      # 人类可读

# 工作流
mise flow stats --stats-format summary   # 项目统计
mise flow outline --outline-format markdown
mise flow pack --anchors a,b --max-tokens 4000
```

## 🔄 典型工作流

### 代码探索

```bash
mise scan --type file --max-depth 3 --pretty   # 结构
mise match "fn main|async fn" src/ --pretty    # 找入口
mise deps src/main.rs --deps-format tree       # 依赖
```

### PR 审查

```bash
mise impact --staged --impact-format summary   # 变更影响
mise deps changed_file.rs --reverse            # 谁依赖它
```

### 上下文打包

```bash
mise flow pack --anchors core,cli --max-tokens 4000
mise flow stats --stats-format summary
```

## 📁 架构概览

```
src/
  cli.rs           # CLI 定义与路由，所有子命令入口
  core/
    model.rs       # 统一结果模型 ResultItem（所有输出必须先映射到此）
    render.rs      # jsonl/json/md/raw 渲染器
  backends/        # 各子命令实现：scan, extract, rg, ast_grep, deps, impact
  anchors/         # Anchor 系统：parse, lint, mark, api
  flows/           # 组合工作流：writing, pack, stats, outline
  cache/           # .mise/ 缓存管理
```

### 核心设计约束

1. **统一结果模型**：所有命令输出必须先产出 `ResultItem`（见 `core/model.rs`），再由 renderer 渲染
2. **结构化错误**：不要 `panic!` 或裸 `eprintln!`，错误用 `Kind::Error` 的 `ResultItem` 返回
3. **路径规范化**：所有路径相对于 `--root`，统一用 `/` 分隔（见 `core/paths.rs`）
4. **稳定排序**：输出前按 `path + range.start` 排序，保证可复现

## 🛠️ 开发工作流

```bash
make build          # Debug 构建
make release        # Release 构建
make install        # 构建并安装到 ~/bin
make test           # 运行单元测试
cargo test <name>   # 运行特定测试
./fulltest.sh       # 完整端到端测试
```

## 添加新命令的模式

1. 在 `cli.rs` 的 `Commands` 枚举添加子命令定义
2. 在 `backends/` 或相应模块实现 `run_xxx()` 函数
3. 函数签名遵循：`fn run_xxx(root: &Path, ..., config: RenderConfig) -> Result<()>`
4. 内部构建 `ResultSet`，最后用 `Renderer::with_config(config).render(&result_set)` 输出

示例（参考 `backends/scan.rs`）：

```rust
pub fn run_scan(root: &Path, ..., config: RenderConfig) -> Result<()> {
    let result_set = scan_files(root, ...)?;  // 返回 ResultSet
    let renderer = Renderer::with_config(config);
    println!("{}", renderer.render(&result_set));
    Ok(())
}
```

## Anchor 系统

Anchor 是嵌入文本的语义标记，格式：

```
<!--Q:begin id=xxx tags=a,b v=1-->
...content...
<!--Q:end id=xxx-->
```

- 支持在任意注释中使用：`// <!--Q:begin ...-->`, `# <!--Q:begin ...-->`
- `anchors/mark.rs`：提供 `mark`/`batch`/`unmark` 命令用于 Agent 批量标记
- `anchors/parse.rs`：解析逻辑，正则匹配 `<!--Q:begin` 和 `<!--Q:end`

## 外部依赖集成

mise 调用外部工具但协议化输出：

| 工具            | 用途                  | 检测 | 安装                     |
| --------------- | --------------------- | ---- | ------------------------ |
| `rg` (ripgrep)  | `mise match` 文本搜索 | 必需 | `brew install ripgrep`   |
| `sg`/`ast-grep` | `mise ast` 结构搜索   | 必需 | `brew install ast-grep`  |
| `watchexec`     | `mise watch` 文件监听 | 可选 | `brew install watchexec` |

依赖缺失时返回结构化错误，不要 panic。用 `mise doctor` 检查依赖状态。

## 测试约定

- Golden tests 在 `tests/golden.rs`
- 测试 fixtures 在 `tests/fixtures/` 和 `tests/samples/`
- 新功能必须添加对应的单元测试（在模块内 `#[cfg(test)] mod tests`）

## 输出格式

```bash
--format jsonl  # 默认，Agent 推荐，每行一个 JSON 对象
--format json   # 完整 JSON 数组
--format md     # Markdown，人类可读
--format raw    # 调试用，不保证可解析
--pretty        # JSON 美化输出
```

## 关键约定

- 平台：仅支持 Linux/macOS，Windows 启动时直接报错
- 缓存：`.mise/` 目录，可随时删除重建（`mise rebuild`）
- 截断：超过 `--max-bytes`（默认 64KB）时截断，并设置 `meta.truncated = true`
