# mise Copilot Instructions

mise 是一个本地上下文准备工具，用于将项目中的文件、片段和锚点整理成 Agent 可直接消费的上下文候选集合。**它不生成答案，只负责把材料摆好。**

## 🎯 何时使用 mise（优先级指南）

mise 已安装在 `~/bin/mise`，在以下场景**优先使用 mise** 而非原生工具：

### ✅ 强烈推荐使用 mise

| 场景           | mise 命令                            | 优势                            |
| -------------- | ------------------------------------ | ------------------------------- |
| 查看项目结构   | `mise scan --type file`              | 结构化输出，自动过滤 .gitignore |
| 提取文件片段   | `mise extract file.rs --lines 10:50` | 精确范围，避免全文喷出          |
| 搜索代码模式   | `mise match "pattern" src/`          | JSON 输出，含行号和上下文       |
| 查看已标记区域 | `mise anchor list`                   | 快速定位语义边界                |
| 批量标记代码   | `mise anchor batch --json '[...]'`   | Agent 友好的批量操作            |
| 分析变更影响   | `mise impact --staged`               | 结合依赖图分析                  |

### ⚠️ 视情况使用

| 场景                     | 建议                               |
| ------------------------ | ---------------------------------- |
| 简单文件读取（< 100 行） | 可用 `read_file`，mise 无明显优势  |
| 单次 grep 查找           | `mise match` 或 `grep_search` 均可 |
| 复杂 AST 查询            | `mise ast` 需要 ast-grep 安装      |

### ❌ 不适合 mise

| 场景     | 原因                                    |
| -------- | --------------------------------------- |
| 编辑文件 | mise 只读取，不修改（anchor mark 除外） |
| 运行命令 | 使用 `run_in_terminal`                  |
| 语义理解 | mise 不做 AI 推理                       |

## 📋 常用命令速查

```bash
# 查看帮助（推荐先执行）
mise --help                    # 查看所有命令
mise <command> --help          # 查看特定命令的参数

# 项目概览
mise scan --type file --max-depth 2

# 精确提取（替代 cat）
mise extract src/main.rs --lines 1:100

# 搜索 TODO/FIXME
mise match "TODO|FIXME" --pretty

# 查看所有 anchor
mise anchor list --pretty

# 标记代码块（供 Agent 使用）
mise anchor mark src/cli.rs --start 10 --end 50 --id cli.commands --tags core

# 批量标记
mise anchor batch --json '[
  {"path": "src/main.rs", "start_line": 1, "end_line": 30, "id": "main.entry", "tags": ["entry"]}
]'

# 检查外部依赖
mise doctor
```

## 架构概览

```
src/
  cli.rs           # CLI 定义与路由，所有子命令入口
  core/
    model.rs       # 统一结果模型 ResultItem（所有输出必须先映射到此）
    render.rs      # jsonl/json/md/raw 渲染器
  backends/        # 各子命令实现：scan, extract, rg, ast_grep, deps, impact
  anchors/         # Anchor 系统：parse, lint, mark, api
  flows/           # 组合工作流：writing, pack
  cache/           # .mise/ 缓存管理
```

### 核心设计约束

1. **统一结果模型**：所有命令输出必须先产出 `ResultItem`（见 `core/model.rs`），再由 renderer 渲染
2. **结构化错误**：不要 `panic!` 或裸 `eprintln!`，错误用 `Kind::Error` 的 `ResultItem` 返回
3. **路径规范化**：所有路径相对于 `--root`，统一用 `/` 分隔（见 `core/paths.rs`）
4. **稳定排序**：输出前按 `path + range.start` 排序，保证可复现

## 开发工作流

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

| 工具            | 用途                  | 检测 |
| --------------- | --------------------- | ---- |
| `rg` (ripgrep)  | `mise match` 文本搜索 | 必需 |
| `sg`/`ast-grep` | `mise ast` 结构搜索   | 必需 |
| `watchexec`     | `mise watch` 文件监听 | 可选 |

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

## 🔄 典型工作流示例

### 1. 探索陌生代码库

```bash
mise scan --type file --max-depth 3    # 了解结构
mise match "fn main|async fn" src/     # 找入口点
mise anchor list                        # 查看已标记的关键区域
```

### 2. 理解变更影响

```bash
mise impact --staged                    # 分析暂存的改动会影响哪些文件
mise deps src/cli.rs --reverse          # 谁依赖这个文件
```

### 3. 为长期维护标记代码

```bash
# 标记核心模块，方便后续快速定位
mise anchor mark src/core/model.rs --start 1 --end 100 --id core.model --tags core,data
mise anchor mark src/cli.rs --start 500 --end 600 --id cli.commands --tags cli,entry
```
