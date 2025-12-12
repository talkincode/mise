# mise v0.1 具体需求清单（含第三方集成）

## 边界与平台

• 支持：Linux / macOS
• 不支持：Windows（启动时检测 cfg!(windows)，直接报错退出，提示 WSL 不保证）
• 工作目录：默认当前项目根（可 --root 指定）
• 项目缓存目录：{root}/.mise/

## 核心契约：统一结果模型

所有命令（自研或外部工具）最终必须映射到 同一个内部 Result Model，再由 renderer 输出。

### Result Model 必备字段（最小集合）

• kind: file | match | extract | anchor | flow | error
• path: 相对 root 的路径（统一为 / 分隔）
• range: 可选 { start_line, end_line } 或 { start_byte, end_byte }
• excerpt: 可选（可截断，截断要标记）
• confidence: high | medium | low
• source_mode: scan | rg | ast_grep | anchor | mixed
• meta:
• mtime（epoch ms）
• size（bytes）
• hash（可选，默认 sha1/xxh3 二选一）
• truncated（bool）
• errors: 可选数组（每条含 code/message）

你允许噪音，但不允许“无法回溯”。所以 path + range/hash 至少要有一个能定位的锚点。

## 输出格式（renderer）

支持 --format jsonl|json|md|raw：
• 默认：jsonl
• jsonl/json/md 必须从 Result Model 渲染
• raw 允许直接透传外部工具原输出（仅调试），但必须：
• --raw 模式下同时输出一行 header 元信息到 stderr（表明“不可解析、不稳定”）

## 自研子命令（必须做）

### mise scan

用途：生成文件清单（为后续查询/缓存提供基础）
• 参数：
• --scope `<path>`（默认 root）
• --max-depth `<n>`（默认不限）
• --hidden（默认 false）
• --ignore（默认 true：尊重 .gitignore/.ignore，可切换）
• --type file|dir（默认 file）
• 输出：kind=file 结果集，稳定排序（path）

### mise extract `<path>` --lines a:b

用途：替代 cat，只允许范围读取，避免喷全文
• 参数：
• --lines a:b（必填）
• --max-bytes `<n>`（默认例如 64KB，超出截断）
• 输出：kind=extract，含 range、excerpt、truncated

### mise rebuild

用途：重建 .mise/ 缓存（轻量即可）
• 生成：
• files.jsonl（scan 输出）
• anchors.jsonl（anchor list 输出）
• meta.json（版本、策略、root、生成时间）

⸻

## Anchor 系统（必须做）

4.1 语法（建议）

支持 HTML 注释式（不污染正文）：

<!--Q:begin id=ch01.bg tags=chapter,background v=1-->

...

<!--Q:end id=ch01.bg-->

4.2 命令
• mise anchor list：列出所有锚点（id/tags/path/range/v/hash）
• mise anchor get `<id>` [--with-neighbors N]：取内容（可带邻居锚点）
• mise anchor lint：检查
• begin/end 配对
• id 重复
• 空范围/超大范围
• v 未变但 hash 大变（提示“语义漂移风险”）

⸻

## 集成第三方命令（重点）

原则：调用外部能力，但输出必须协议化（映射到 Result Model）。

5.1 集成 ripgrep：rg

依赖检测
• 启动或首次调用时检查 rg 是否存在（which rg）
• 不存在：返回结构化 error（含安装提示）

命令：mise match `<pattern>` [scope...]
实现方式：
• 调 rg --json `<pattern> <scope...>`
• 解析 rg JSON 输出（match/begin/end 等事件）
• 映射为：
• kind=match
• range：line/col（至少 line）
• excerpt：命中行（可加前后 N 行 window）
• source_mode=rg
• confidence=medium/high（精确匹配高，模糊/正则复杂中）
• 排序：path + line

你不要直接用系统 grep，BSD/GNU 差异会把你搞烦。

5.2 集成 ast-grep：ast-grep/sg

依赖检测
• 检查 ast-grep 或 sg（二选一，优先 sg）
• 不存在：结构化 error + 安装提示

命令：mise ast <rule_or_pattern> [scope...]
最低可用：
• 调用 ast-grep 的 JSON 输出模式（如果支持 --json / -o json 之类）
• 映射为：
• kind=match
• source_mode=ast_grep
• range：line range
• excerpt：匹配片段（截断标记）
• 先不做复杂 rule 管理（v0.1 不要扩张）

ast-grep 你说得对：别自己写。你只要当“结构化后端”。

5.3 集成 watchexec（可选但很有用）

用途：变更触发 mise rebuild 或某个 flow，适合你“改动就重建”的工作流。

命令：mise watch [--cmd "..."]
• 默认行为：监听 root 下文本文件变更，触发 mise rebuild
• 若用户传 --cmd：变更时执行该命令（例如 mise flow writing --anchor ch03.bg）
• 实现方式（两种任选其一）： 1. 直接调用外部 watchexec（最省事） 2. 用 Rust 文件监听库（以后再做，v0.1 先别）

⸻

## Flow（v0.1 做一个就够）

### mise flow writing --anchor `<id>` [--max-items N]

行为：
• 优先取 anchor 内容（high confidence）
• 再补充：
• 同 tags 的其他 anchor（medium）
• rg 在 scope 内对关键字的补充命中（low/medium）
• 输出：
• md：按「硬证据/软相关/噪音」分区，所有片段带 path:line
• json：同样结构化（evidence/structure/suggested_calls）

⸻

## 依赖管理与提示（必须）

• mise doctor：
• 检查：rg、sg/ast-grep、watchexec（可选）
• 输出缺失项 + 安装提示（按系统：brew/apt）

⸻

## 质量门槛（别偷懒）

• golden tests：
• match 的解析映射稳定（给固定输入文件）
• renderer 输出一致（jsonl vs json 内容等价）
• 错误必须结构化：
• 不要只 eprintln! 一句就完事

⸻

推荐集成的第三方工具清单（你可以先选这 3 个）
• 必选：rg（match 后端）
• 必选：ast-grep/sg（结构匹配后端）
• 可选：watchexec（watch 后端）
