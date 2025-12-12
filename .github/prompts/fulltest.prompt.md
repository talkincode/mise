---
agent: agent
---

# mise 全量端到端测试（fulltest）

你要在本仓库中**实际执行** `mise` 的所有 CLI 命令与关键参数组合，并在最后输出一份**完整、可复现**的测试报告。

## 目标

1. **如果项目尚未编译，先编译出最新二进制**（优先 release）。
2. **以当前仓库为范本**对 `mise` 做自举测试（root=仓库根目录）。
3. **针对 `tests/samples/` 目录下的内容做测试**（root 指向 samples 子项目/目录）。
4. **覆盖到所有命令执行，以及各个参数**（见下方测试矩阵）。
5. 测试完成后输出**完整报告**：包含环境信息、构建信息、执行的每条命令、退出码、stdout/stderr 摘要、关键断言、失败项与复现步骤。

## 约束与通用要求

- 所有命令都在仓库根目录执行，除非特别说明。
- 每次执行都要记录：
  - 工作目录（pwd）
  - 完整命令行（包含参数、root、format）
  - 退出码
  - stdout/stderr（可截断，但必须保留足够复现的关键信息；json 输出需至少保留前/后若干行示例）
- 对于 jsonl/json 输出：必须做一次“可解析性”校验（例如用 `jq`/`python -c`/其它方式解析；如果本机无工具则写明原因并用替代方法验证）。
- 对于 md/raw 输出：检查是否为非空且包含关键字段/片段。
- 若外部依赖缺失（如 `rg`/`sg`/`watchexec`），不得跳过测试而不说明：
  - 先运行 `mise doctor` 记录缺失
  - 对依赖缺失导致无法覆盖的子命令，在报告中标注“环境阻塞”，并给出如何安装/启用的建议（不必真的安装，除非允许）。

## 构建步骤（必须）

### 1) 确认二进制是否存在

- 如果 `target/release/mise` 不存在，或不可执行：执行 release 构建。
- 即便存在，也建议执行一次 release 构建以保证“最新”。

### 2) 构建

- `cargo build --release`
- 在报告中记录：Rust 版本、构建 profile、构建耗时（如可得）、产物路径。

后续执行优先使用：`./target/release/mise`。

## 测试矩阵

你需要覆盖这些命令与参数：

### 全局参数（每类至少覆盖一次）

- `--root <PATH>`：
  - 仓库根目录（自举测试）
  - `tests/samples/sample_project_alpha`
  - `tests/samples/sample_project_beta`
  - `tests/samples/anchors_invalid`
- `--format {jsonl,json,md,raw}`：四种都要跑到（不要求每条子命令都 4 次，但总体要覆盖）。
- `--no-color`：至少跑一次（配合 `--format md` 或默认）。
- `--quiet`：至少跑一次。
- `--verbose`：至少跑一次，并记录 stderr 变化。

### 1) doctor

- `mise doctor`（默认 format=jsonl）
- 再用 `--format md` 跑一次，检查人类可读输出。

### 2) scan

在 `sample_project_beta` 下重点覆盖 ignore/hidden：

- 默认：`scan`（不带 `--type`）
- `--type file`
- `--type dir`
- `--scope <subdir>`（例如 `visible` 或 `deep`）
- `--max-depth 1` 和一个更深的值（例如 3）
- `--hidden`：应能看到 `.hidden.md`
- `--ignore false`：应能看到被 `.gitignore` 忽略的 `ignored/secret.txt` 与 `*.log`

对输出做断言（示例，按实际输出字段调整）：

- 路径为相对 root
- 排序稳定（多跑一次，输出顺序一致）

### 3) find

在 `sample_project_alpha` 下：

- `find "readme"`（大小写不敏感）
- `find alpha --scope docs`
- `find`（pattern 为空时的行为：记录并说明是返回全部还是空集）

### 4) extract

在 `sample_project_alpha` 下：

- 从 `docs/big.txt` 提取一个中间区间：`--lines 10:20`
- 覆盖 `--max-bytes`：设置一个较小值（例如 50~200）触发截断，并在输出 meta 中验证“被截断”的标记（若实现不存在该标记则如实记录）。
- 负例：对不存在的文件/非法 range（例如 `--lines 20:10` 或 `--lines abc`）执行一次，记录退出码与错误信息（这是兼容性测试，不要求成功）。

### 5) match（rg 后端）

分别在：仓库根目录、`sample_project_alpha` 下运行：

- `match "TODO|FIXME"`（无 scope）
- `match "NEEDLE_ALPHA_123" docs`（带 scope）
- `match "NEEDLE_SECRET_888"` 在 `sample_project_beta` 下：
  - 默认 ignore=true 时应搜不到 ignored 内容（如搜到则记录为问题）

### 6) ast（ast-grep 后端）

如果 `sg/ast-grep` 不可用：

- 在报告中标注“环境阻塞”，记录 `mise doctor` 输出并跳过该项。

否则在 `sample_project_alpha` 下：

- `ast "console.log($A)" web`
- `ast "unsafe { $A }" src`

### 7) anchor

在 `sample_project_alpha` 下：

- `anchor list`
- `anchor list --tag intro`
- `anchor get alpha.intro`
- `anchor get alpha.intro --with-neighbors 2`（应带出共享 tag 的其它 anchors，如 `alpha.meeting`/`alpha.inner` 等）

在 `anchors_invalid` 下：

- `anchor lint`
- 断言输出包含错误码：`DUPLICATE_ID`、`UNPAIRED_BEGIN`、`UNPAIRED_END`，以及警告 `EMPTY_ANCHOR`。

### 8) flow writing

在 `sample_project_alpha` 下：

- `flow writing --anchor alpha.intro --max-items 10`
- 断言：
  - 返回集包含主 anchor（高置信度）
  - 包含至少一个 related anchor（中置信度，tag 相关）
  - 若 `rg` 可用，包含一些低置信度的匹配项（否则说明原因）

### 9) rebuild

在 `sample_project_alpha` 下：

- `rebuild`
- 断言 `.mise/` 目录生成（或相应缓存产物生成），并记录生成的文件清单（只需列出文件名与大小）。

### 10) watch（可选，取决于编译 feature）

- 先通过 `mise --help` 或子命令列表判断是否存在 `watch` 子命令。
- 如果存在：使用一个短生命周期的 watch（例如执行一次简单命令），并在报告中说明如何验证；避免长时间阻塞。
- 如果不存在：在报告中记录“未启用 watch feature”。

## 产出：完整报告格式（必须按此输出）

你的最终输出必须是一份 Markdown 报告，至少包含：

1. **摘要**：总命令数、成功/失败/跳过（环境阻塞）计数。
2. **环境**：OS、CPU 架构、Rust 版本、是否安装 rg/sg/watchexec。
3. **构建**：构建命令、产物路径、构建日志摘要。
4. **执行明细表**：每条命令一行（编号、root、format、退出码、关键断言结果、日志/输出定位）。
5. **关键断言**：逐条列出你验证了哪些行为（排序稳定、ignore 生效、hidden 生效、json 可解析等）。
6. **问题与复现**：任何失败都要给出最小复现命令、期望/实际、可能原因。
7. **附录**：必要的 stdout/stderr 片段（尤其是失败用例与代表性 jsonl/json 片段）。
