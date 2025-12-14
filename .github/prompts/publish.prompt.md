---
agent: agent
---

# misec 版本发布流程

你要协助完成 `misec` 的版本发布工作，包括版本号更新、代码提交和 Git 标签创建。

## 目标

1. **确定当前版本**：读取 `Cargo.toml` 中的 `version` 字段
2. **计划下一个版本号**：遵循语义化版本（SemVer）规则
3. **更新版本号**：修改 `Cargo.toml` 中的版本
4. **提交并推送代码**：使用规范的 commit message
5. **创建并推送 Git 标签**：确保标签与版本号一致

## 版本号规则（SemVer）

- **MAJOR.MINOR.PATCH** 格式
- **PATCH**（修订版本）：向后兼容的 bug 修复
- **MINOR**（次要版本）：向后兼容的新功能
- **MAJOR**（主要版本）：不兼容的 API 变更

## 执行步骤

### 1) 确定当前版本

```bash
# 读取当前版本
grep '^version' Cargo.toml | head -1
```

### 2) 确认版本类型

与用户确认本次发布是：

- `patch`：bug 修复（如 0.1.2 → 0.1.3）
- `minor`：新功能（如 0.1.2 → 0.2.0）
- `major`：重大变更（如 0.1.2 → 1.0.0）

如果用户未指定，默认使用 `patch` 版本递增。

### 3) 更新 Cargo.toml

修改 `Cargo.toml` 中的 `version` 字段为新版本号。

### 4) 验证构建

```bash
# 确保新版本可以正常构建
cargo build --release

# 验证版本号已更新
./target/release/misec --version
```

### 5) 提交代码

```bash
# 检查变更
git status
git diff Cargo.toml

# 提交版本更新
git add Cargo.toml Cargo.lock
git commit -m "chore(release): bump version to vX.Y.Z"

# 推送到远程
git push origin main
```

### 6) 创建并推送标签

```bash
# 创建标签（版本号前加 v 前缀）
git tag -a vX.Y.Z -m "Release vX.Y.Z"

# 推送标签
git push origin vX.Y.Z
```

## 检查清单

发布前请确认：

- [ ] 所有测试通过：`cargo test`
- [ ] 构建成功：`cargo build --release`
- [ ] 工作目录干净（除版本更新外）：`git status`
- [ ] 当前在 main 分支：`git branch --show-current`
- [ ] 与远程同步：`git pull origin main`

## 输出报告

完成后提供以下信息：

```
## 发布报告

- 旧版本：X.Y.Z
- 新版本：X.Y.Z
- 版本类型：patch/minor/major
- 提交 SHA：xxxxxxx
- 标签名称：vX.Y.Z
- 发布时间：YYYY-MM-DD HH:MM:SS

## 执行的命令

1. git add Cargo.toml Cargo.lock
2. git commit -m "chore(release): bump version to vX.Y.Z"
3. git push origin main
4. git tag -a vX.Y.Z -m "Release vX.Y.Z"
5. git push origin vX.Y.Z
```

## 回滚说明

如果发布出现问题，可以执行以下步骤回滚：

```bash
# 删除远程标签
git push origin --delete vX.Y.Z

# 删除本地标签
git tag -d vX.Y.Z

# 回滚提交
git revert HEAD

# 推送回滚
git push origin main
```

## 注意事项

1. **不要**在有未提交更改时发布
2. **不要**跳过构建验证步骤
3. 标签名称**必须**与 `Cargo.toml` 中的版本号一致（加 `v` 前缀）
4. 如果有 CI/CD 流程，等待流水线完成后再确认发布成功
