# mise 产品路线图与需求计划

> 最后更新：2025-12-12  
> 版本：v0.2 规划

---

## 一、产品定位

### 1.1 核心价值主张

**mise 是一个「确定性上下文准备工具」**，专为 AI 编程助手（如 GitHub Copilot、Claude）提供可追溯、可复现的代码上下文。

```
┌─────────────────────────────────────────────────────────────┐
│                      开发者提问                              │
│              "帮我重构这个认证模块"                           │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    mise 准备上下文                           │
│  - 精确定位相关代码（anchor、依赖图谱）                        │
│  - 确定性输出（同样输入 = 同样输出）                           │
│  - 可审计（知道 Agent 看到了什么）                            │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    AI Agent 处理                             │
│  - 语义理解                                                  │
│  - 代码生成                                                  │
│  - 重构建议                                                  │
└─────────────────────────────────────────────────────────────┘
```

### 1.2 设计原则

| 原则          | 说明                     | 反例                          |
| ------------- | ------------------------ | ----------------------------- |
| **确定性**    | 相同输入必须产生相同输出 | ❌ 语义搜索（结果随模型变化） |
| **可审计**    | 输出清晰说明数据来源     | ❌ 黑盒 AI 推理               |
| **轻量**      | 快速启动，无重依赖       | ❌ 需要 GPU/大模型            |
| **Unix 哲学** | 单一职责，管道友好       | ❌ 全能 IDE 插件              |
| **离线优先**  | 完全本地运行             | ❌ 依赖云服务                 |

### 1.3 边界定义

#### ✅ mise 做什么

- 文件扫描、过滤、提取
- 模式匹配（正则、AST）
- Anchor 标记管理
- 上下文组装与输出
- 依赖图谱分析
- 变更影响分析

#### ❌ mise 不做什么

- 语义搜索 / AI 嵌入
- 代码生成 / 补全
- 实时诊断 / LSP
- 项目脚手架
- 远程仓库分析

---

## 二、版本规划

### 2.1 版本路线图

```
v0.1 (已完成)          v0.2 (计划中)           v0.3 (未来)
     │                      │                      │
     ▼                      ▼                      ▼
┌─────────┐           ┌─────────┐           ┌─────────┐
│ 核心命令 │           │ 增强功能 │           │ 生态集成 │
│ scan    │           │ deps    │           │ VSCode  │
│ find    │           │ impact  │           │ 扩展    │
│ extract │           │ pack    │           │         │
│ match   │           │ budget  │           │ CI/CD   │
│ ast     │           │         │           │ 集成    │
│ anchor  │           │ anchor  │           │         │
│ flow    │           │ discover│           │ 多语言  │
│ rebuild │           │         │           │ 规则库  │
│ doctor  │           │ watch   │           │         │
└─────────┘           └─────────┘           └─────────┘
```

---

## 三、v0.1 需求清单（已完成 ✅）

### 3.1 核心命令

| 命令                | 状态 | 说明                                                      |
| ------------------- | ---- | --------------------------------------------------------- |
| `mise doctor`       | ✅   | 检查依赖（rg, sg, watchexec）                             |
| `mise scan`         | ✅   | 文件扫描，支持 --type, --hidden, --no-ignore, --max-depth |
| `mise find`         | ✅   | 文件名模糊匹配                                            |
| `mise extract`      | ✅   | 行范围提取，支持 --max-bytes 截断                         |
| `mise match`        | ✅   | 正则匹配（rg 后端）                                       |
| `mise ast`          | ✅   | AST 模式匹配（ast-grep 后端）                             |
| `mise anchor list`  | ✅   | 列出所有 anchor                                           |
| `mise anchor get`   | ✅   | 获取 anchor 内容，支持 --with-neighbors                   |
| `mise anchor lint`  | ✅   | 检查 anchor 配对、重复、空内容                            |
| `mise flow writing` | ✅   | 写作上下文流                                              |
| `mise rebuild`      | ✅   | 重建 .mise/ 缓存                                          |

### 3.2 已修复问题

| 问题                    | 修复                    | PR  |
| ----------------------- | ----------------------- | --- |
| `--ignore false` 不工作 | 改为 `--no-ignore` 标志 | -   |

---

## 四、v0.2 需求清单（计划中）

### 4.1 依赖图谱（deps）

**优先级**: P0（高）

**需求描述**:

分析代码依赖关系，帮助理解"改这个文件会影响什么"。

**命令设计**:

```bash
# 查看文件依赖了哪些模块
mise deps src/cli.rs
# 输出:
# {"kind":"dep","path":"src/cli.rs","depends_on":["src/core/model.rs","src/backends/scan.rs",...]}

# 查看哪些文件依赖这个模块（反向依赖）
mise deps --reverse src/core/model.rs
# 输出:
# {"kind":"dep","path":"src/core/model.rs","depended_by":["src/cli.rs","src/backends/rg.rs",...]}

# 输出依赖图（DOT 格式，可用 graphviz 渲染）
mise deps --format dot src/
```

**技术方案**:

```rust
// src/backends/deps.rs

use std::collections::HashMap;

pub struct DepGraph {
    /// file -> [依赖的文件]
    pub forward: HashMap<String, Vec<String>>,
    /// file -> [被哪些文件依赖]
    pub reverse: HashMap<String, Vec<String>>,
}

pub fn analyze_deps(root: &Path, scope: &str) -> Result<DepGraph> {
    // 1. 扫描所有源文件
    // 2. 解析 import/use/require 语句（利用 ast-grep 或 tree-sitter）
    // 3. 解析相对路径到绝对路径
    // 4. 构建正向和反向依赖图
}
```

**支持语言（v0.2）**:

- Rust (`use`, `mod`)
- TypeScript/JavaScript (`import`, `require`)
- Python (`import`, `from ... import`)

**验收标准**:

- [ ] `mise deps <file>` 返回该文件的依赖列表
- [ ] `mise deps --reverse <file>` 返回反向依赖
- [ ] `mise deps --format dot` 输出可被 graphviz 渲染的 DOT 格式
- [ ] 循环依赖检测并警告

---

### 4.2 变更影响分析（impact）

**优先级**: P0（高）

**需求描述**:

基于 git diff 和依赖图谱，分析变更的影响范围。

**命令设计**:

```bash
# 分析当前 unstaged 变更的影响
mise impact
# 输出:
# {"changed":"src/cli.rs","direct_impacts":["tests/cli.rs"],"transitive_impacts":["src/main.rs"],"anchors_affected":["cli.scan"]}

# 分析 staged 变更
mise impact --staged

# 分析特定 commit
mise impact --commit abc123

# 分析两个分支差异
mise impact --diff main..feature
```

**技术方案**:

```rust
// src/backends/impact.rs

pub struct ImpactAnalysis {
    pub changed_files: Vec<String>,
    pub direct_impacts: Vec<String>,      // 直接依赖变更文件的
    pub transitive_impacts: Vec<String>,  // 间接受影响的
    pub anchors_affected: Vec<String>,    // 受影响的 anchor
}

pub fn analyze_impact(root: &Path, diff_source: DiffSource) -> Result<ImpactAnalysis> {
    // 1. 获取变更文件列表（git diff）
    // 2. 查询依赖图谱获取直接影响
    // 3. 递归获取传递影响（可配置深度）
    // 4. 交叉比对 anchor 定义位置
}
```

**验收标准**:

- [ ] 正确识别 git diff 中的变更文件
- [ ] 正确计算直接依赖影响
- [ ] 正确计算传递依赖（最多 3 层）
- [ ] 列出受影响的 anchor

---

### 4.3 上下文打包（pack）

**优先级**: P1（中）

**需求描述**:

将多个 anchor 和文件打包成一个上下文包，方便传递给 Agent。

**命令设计**:

```bash
# 打包多个 anchor
mise pack --anchors cli.scan,cli.match,core.model
# 输出完整的 JSONL，包含所有 anchor 内容

# 带 token 预算限制
mise pack --anchors cli.scan,cli.match --max-tokens 8000
# 自动按 confidence 截断，优先保留 high confidence

# 打包文件 + anchor
mise pack --anchors cli.scan --files src/main.rs
```

**技术方案**:

```rust
// src/flows/pack.rs

pub struct PackOptions {
    pub anchors: Vec<String>,
    pub files: Vec<String>,
    pub max_tokens: Option<usize>,
    pub priority: PackPriority,  // ByConfidence, ByOrder
}

pub fn pack_context(root: &Path, opts: PackOptions) -> Result<Vec<ResultItem>> {
    // 1. 收集所有指定的 anchor 和文件内容
    // 2. 估算 token 数量（简单按字符数 / 4 估算）
    // 3. 如果超出预算，按优先级截断
    // 4. 输出打包结果
}
```

**验收标准**:

- [ ] 支持多 anchor 打包
- [ ] 支持 anchor + 文件混合打包
- [ ] `--max-tokens` 正确限制输出大小
- [ ] 截断时保留 high confidence 内容

---

### 4.4 Anchor 自动发现（anchor discover）

**优先级**: P1（中）

**需求描述**:

自动扫描代码，建议哪些地方应该添加 anchor 标记。

**命令设计**:

```bash
# 扫描并建议 anchor 位置
mise anchor discover
# 输出:
# {"kind":"suggestion","path":"src/cli.rs","range":{"start":45,"end":120},"suggested_id":"cli.scan_command","reason":"函数定义: pub fn scan(...)"}

# 只检查特定目录
mise anchor discover src/

# 自动生成 anchor 标记（dry-run）
mise anchor discover --generate --dry-run

# 实际写入文件
mise anchor discover --generate
```

**发现规则**:

| 规则          | 优先级 | 示例                           |
| ------------- | ------ | ------------------------------ |
| 公开函数/方法 | High   | `pub fn`, `export function`    |
| 公开结构/类   | High   | `pub struct`, `export class`   |
| 模块入口      | High   | `mod.rs`, `index.ts`           |
| 重要注释块    | Medium | `/// # Example`, `/** @api */` |
| 配置文件      | Medium | `Cargo.toml`, `package.json`   |
| 测试函数      | Low    | `#[test]`, `it("...")`         |

**验收标准**:

- [ ] 正确识别 Rust/TypeScript/Python 的公开定义
- [ ] 建议的 anchor ID 有意义（基于函数名/类名）
- [ ] `--generate` 可以自动插入 anchor 标记
- [ ] `--dry-run` 只预览不修改

---

### 4.5 Watch 功能（watch）

**优先级**: P2（低）

**需求描述**:

监听文件变化，自动触发命令。

**命令设计**:

```bash
# 监听变化，自动 rebuild
mise watch

# 监听变化，执行自定义命令
mise watch --cmd "mise flow writing --anchor main.entry"

# 指定监听范围
mise watch --scope src/ --cmd "mise rebuild"

# 防抖配置
mise watch --debounce 500ms
```

**技术方案**:

两种实现方式：

1. **外部调用**（v0.2 推荐）：调用 `watchexec` 命令
2. **内置实现**（v0.3）：使用 `notify` crate

```rust
// src/backends/watch.rs (v0.2: 外部调用)

pub fn watch_external(root: &Path, cmd: &str, debounce_ms: u64) -> Result<()> {
    // 调用: watchexec --debounce {debounce_ms} -- {cmd}
}
```

**验收标准**:

- [ ] 文件变化时正确触发命令
- [ ] 支持 debounce 防抖
- [ ] 支持 --scope 限定监听范围
- [ ] watchexec 不存在时给出安装提示

---

### 4.6 输出格式增强

**优先级**: P2（低）

**需求描述**:

增加更多人类友好的输出格式。

**命令设计**:

```bash
# 树形输出
mise scan --format tree
# 输出:
# src/
# ├── cli.rs
# ├── main.rs
# └── core/
#     ├── mod.rs
#     └── model.rs

# 表格输出
mise anchor list --format table
# 输出:
# ┌──────────────┬───────────────────┬─────────┬──────────┐
# │ ID           │ Path              │ Lines   │ Tags     │
# ├──────────────┼───────────────────┼─────────┼──────────┤
# │ cli.scan     │ src/cli.rs        │ 45-120  │ cli,core │
# │ core.model   │ src/core/model.rs │ 10-80   │ core     │
# └──────────────┴───────────────────┴─────────┴──────────┘
```

**验收标准**:

- [ ] `--format tree` 输出树形结构
- [ ] `--format table` 输出表格（使用 unicode box drawing）
- [ ] 表格自动适应终端宽度

---

## 五、v0.3+ 远期规划

### 5.1 VS Code 扩展

**需求描述**:

- 侧边栏显示 anchor 列表
- 点击跳转到 anchor 位置
- 右键菜单"Add Anchor Here"
- 依赖图谱可视化

### 5.2 CI/CD 集成

**需求描述**:

- GitHub Action: 变更时自动运行 `mise anchor lint`
- PR 评论: 自动添加"影响范围分析"
- 缓存 `.mise/` 目录加速

### 5.3 多语言规则库

**需求描述**:

- 预置常见语言的 AST 模式（安全漏洞、代码异味）
- 规则可共享（类似 eslint config）

---

## 六、技术债务与改进

### 6.1 已知技术债务

| 项目           | 说明                        | 优先级 |
| -------------- | --------------------------- | ------ |
| 错误信息国际化 | 当前只有英文错误信息        | P3     |
| Windows 支持   | 当前明确不支持              | P3     |
| 性能优化       | 大项目（>10k 文件）扫描较慢 | P2     |
| 测试覆盖率     | 缺少集成测试                | P1     |

### 6.2 代码质量改进

- [ ] 添加 `cargo clippy` CI 检查
- [ ] 添加 `cargo fmt` CI 检查
- [ ] 增加单元测试覆盖率到 80%
- [ ] 添加 golden test 防止输出格式回归

---

## 七、里程碑计划

### v0.2 里程碑

| 阶段 | 时间  | 交付物                   |
| ---- | ----- | ------------------------ |
| 设计 | W1    | deps/impact 命令详细设计 |
| 开发 | W2-W3 | deps 命令实现            |
| 开发 | W4    | impact 命令实现          |
| 开发 | W5    | pack 命令实现            |
| 测试 | W6    | 集成测试 + 文档更新      |
| 发布 | W7    | v0.2.0 release           |

### 验收标准（v0.2）

- [ ] 所有 P0/P1 需求完成
- [ ] 所有命令有 `--help` 文档
- [ ] README 更新使用示例
- [ ] `mise doctor` 检测新依赖
- [ ] 无 P0/P1 级别 bug

---

## 八、附录

### A. 竞品对比

| 功能        | mise | Agent 原生 | Sourcegraph |
| ----------- | ---- | ---------- | ----------- |
| 确定性输出  | ✅   | ❌         | ✅          |
| 语义搜索    | ❌   | ✅         | ✅          |
| 离线使用    | ✅   | 部分       | ❌          |
| Anchor 系统 | ✅   | ❌         | ❌          |
| 依赖图谱    | 计划 | ❌         | ✅          |
| 轻量级      | ✅   | -          | ❌          |

### B. 用户故事

**US-001**: 作为开发者，我希望在 Code Review 时知道这次 PR 影响了哪些模块，以便评估风险。

**US-002**: 作为技术文档作者，我希望精确引用代码片段，并在代码变更时收到通知。

**US-003**: 作为 AI 助手使用者，我希望控制给 Agent 的上下文量，避免 token 浪费。

**US-004**: 作为新加入项目的开发者，我希望快速了解模块间的依赖关系。

### C. 术语表

| 术语           | 定义                                 |
| -------------- | ------------------------------------ |
| **Anchor**     | 代码中的标记区域，用于精确定位和引用 |
| **Confidence** | 结果的可信度（high/medium/low）      |
| **ResultItem** | mise 统一输出模型                    |
| **Flow**       | 预定义的上下文组装流程               |
| **Pack**       | 多个上下文打包成一个输出             |

---

## 九、变更日志

| 日期       | 版本       | 变更                         |
| ---------- | ---------- | ---------------------------- |
| 2025-12-12 | v0.2 draft | 初始版本，基于 v0.1 测试反馈 |
