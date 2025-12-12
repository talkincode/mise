# mise v0.1 工程实现蓝图

## 仓库结构建议

```bash
mise/
  Cargo.toml
  src/
    main.rs
    cli.rs
    core/
      mod.rs
      model.rs          # ResultModel / enums / meta / errors
      render.rs         # jsonl/json/md/raw renderer
      paths.rs          # root 相对路径规范化、分隔符统一
      util.rs
    cache/
      mod.rs
      store.rs          # .mise/ 读写 files.jsonl anchors.jsonl meta.json
      meta.rs           # cache version / policy hash
    anchors/
      mod.rs
      parse.rs          # 扫描文件解析 <!--Q:begin ...--> 块
      lint.rs
      api.rs            # list/get
    backends/
      mod.rs
      scan.rs           # walkdir+ignore 自研 scan/find
      extract.rs        # ranged read + truncation
      rg.rs             # 调用 rg --json + 解析映射
      ast_grep.rs       # 调用 sg/ast-grep + 解析映射
      watch.rs          # (v0.1 可选) 调 watchexec
      doctor.rs         # 检测依赖
    flows/
      mod.rs
      writing.rs        # flow writing 组合 anchor + rg
    tests/
      fixtures/
      golden.rs
```

## 核心接口与数据模型（先写这个，后面才不会散架）

2.1 core/model.rs（统一结果模型）

强制约束：任何命令输出都要先产出这些结构，再渲染。

```rust
pub enum Kind { File, Match, Extract, Anchor, Flow, Error }

pub enum Confidence { High, Medium, Low }

pub enum SourceMode { Scan, Rg, AstGrep, Anchor, Mixed }

pub struct RangeLine { pub start: u32, pub end: u32 }
pub struct RangeByte { pub start: u64, pub end: u64 }

pub enum Range { Line(RangeLine), Byte(RangeByte) }

pub struct Meta {
  pub mtime_ms: Option<i64>,
  pub size: Option<u64>,
  pub hash: Option<String>,
  pub truncated: bool,
}

pub struct MiseError { pub code: String, pub message: String }

pub struct ResultItem {
  pub kind: Kind,
  pub path: Option<String>,        // root 相对路径，统一用 '/'
  pub range: Option<Range>,
  pub excerpt: Option<String>,
  pub confidence: Confidence,
  pub source_mode: SourceMode,
  pub meta: Meta,
  pub errors: Vec<MiseError>,
}
```

## Renderer：core/render.rs

• `render_jsonl(Vec<ResultItem>)` -> String
• `render_json(Vec<ResultItem>)` -> String
• `render_md(Vec<ResultItem>)` -> String
• `render_raw(...)`：仅用于透传外部工具输出（raw 模式）

要求
• 稳定排序（渲染前统一 sort：path + range.start）
• 截断必须 meta.truncated=true

### CLI 规范（cli.rs）

全局参数
• --root `<path>`（默认当前目录）
• --format jsonl|json|md|raw（默认 jsonl）
• --no-color（默认 true，保守）
• --quiet / --verbose（可选）

子命令（v0.1）
• scan
• find（可作为 scan 的别名或带过滤的 scan）
• extract `<path>` --lines a:b [--max-bytes N]
• anchor list|get|lint
• match `<pattern>` [scope...]
• ast <rule_or_pattern> [scope...]
• flow writing --anchor `<id>` [--max-items N]
• rebuild
• doctor
• watch（可选）

### 后端实现策略（调用但协议化）

- scan/find（自研）

backends/scan.rs
• walkdir 遍历 + ignore 规则（.gitignore 可开关）
• 输出 Kind::File 的 ResultItem
• 注意：统一 path 规范化（用 core/paths.rs）

- extract（自研）

backends/extract.rs
• 只允许 --lines a:b
• 默认 --max-bytes 65536，超出截断
• 输出 Kind::Extract，必须带 range

- match（调用 rg）

backends/rg.rs
• 依赖：rg 必须存在，否则返回 Kind::Error 的结构化结果（不要 panic）
• 调用：rg --json `<pattern> <scopes...>`
• 解析 JSON 事件，产出 Kind::Match
• 映射：path、line、excerpt、source_mode=Rg

- ast（调用 sg/ast-grep）

backends/ast_grep.rs
• 优先检测 sg，其次 ast-grep
• 输出为 Kind::Match，source_mode=AstGrep
• v0.1 只做“能跑通 + 基本 range/excerpt”，别做 rule 管理

- doctor（依赖检测）

backends/doctor.rs
• 检测：rg、sg/ast-grep、watchexec（可选）
• 输出结构化结果（md/jsonl 都能读）

- watch（可选）

backends/watch.rs
• v0.1 直接调用外部 watchexec 最省事：
• 默认触发 mise rebuild
• 支持 --cmd "..."

### Anchor 系统（anchors 模块）

- 解析规则（anchors/parse.rs）

支持：

<!--Q:begin id=xxx tags=a,b v=1-->

...

<!--Q:end id=xxx-->

输出 anchor item：
• id、tags、v、path、range(line)、hash（内容 hash）

- lint（anchors/lint.rs）

检查：
• begin/end 配对
• 重复 id
• 空/过大范围（比如 > N 行提示）
• v 未变但 hash 大变（提示语义漂移）

### Cache（轻量）

cache/store.rs
• .mise/meta.json：cache_version、root、policy_hash、generated_at
• .mise/files.jsonl：scan 输出
• .mise/anchors.jsonl：anchor list 输出

mise rebuild：
• 直接重建这三样
• 缓存损坏就删了重建，不做增量聪明

### Flow（flows/writing.rs）

flow writing --anchor `<id>`
行为（v0.1）： 1. anchor get `<id>` 作为硬证据（confidence high） 2. 取同 tags 的其他 anchors（medium，最多 N 个） 3. 用 rg 补充 scope 内的匹配（low/medium，最多 N 个） 4. 输出：
• md：按【硬证据/软相关/噪音】分区，每条带 path:line
• json：evidence + suggested_calls（例如建议再查某些 anchor 或 match）

### 测试与验收（别省）

- Golden tests（最小一套）
• fixtures：小型项目目录（含 anchors + 代码 + 文本）
• 测 scan 输出稳定
• 测 anchor lint 的典型错误
• mock 或固定 rg 输出样例，测映射一致（不依赖系统环境更稳）

- 验收点（Definition of Done）
• 任一命令在 --format jsonl 下，输出可被稳定解析
• raw 模式必须明确“不保证可解析”
• 错误必须结构化返回，不允许只有 stdout/stderr 噪音

Agent 任务拆分顺序（强烈推荐） 1. core/model.rs + core/render.rs 2. CLI 骨架（clap）+ --format 路由 3. scan + extract 4. anchors: list/get/lint（先只跑 markdown） 5. cache: rebuild 6. rg backend: match 7. flow writing 8. doctor 9. （可选）watch
