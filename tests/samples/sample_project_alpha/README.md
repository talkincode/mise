# Sample Project Alpha

<!--Q:begin id=alpha.intro tags=chapter,intro v=1-->

Alpha 项目用于验证 mise 的 scan/find/extract/match/anchor/flow 等命令。

关键字：refactorability, deterministic, traceability。

- 这个 anchor 会被 `mise flow writing --anchor alpha.intro` 作为主证据。
- 共享 tag `intro` 的其它 anchors 会被当作 neighbor/related。
<!--Q:end id=alpha.intro-->

<!--Q:begin id=alpha.api tags=chapter,api v=1-->

API 约定：所有输出应可机器解析（jsonl/json），并且 path 相对 root。

<!--Q:end id=alpha.api-->
