# 当前项目实施记录

## 当前目标

- 目标 ID：20260817-direct-tag-release
- 目标：将日期 tag 的 GitHub 发布流程收敛为一次直接触发，移除 CI dispatch、轮询和二次 Release dispatch。
- 交付物：有效 annotated `YYYY-MM-DD[-N]` tag 推送后直接构建并发布 GitHub Release；无编排 workflow 的 CI、tag 创建说明、文档与环境记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`.github/workflows/{ci,release}.yml`、删除 `.github/workflows/{create-dated-release,retry-existing-release}.yml`、发布文档与 `docs/project-{implementation-tracker,env-audit}/`。
- 不在本轮范围内：Rust/Slint 运行时代码、Cargo 依赖和锁文件、版本映射脚本、SSH trust、凭据、既有 tag 的移动或重建。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| DR1 | completed | 由 tag push 直接触发、校验并发布的 Release workflow | YAML 解析、release metadata verify、静态审阅 | Release 是唯一发布链路；tag 必须为 annotated。 |
| DR2 | completed | 移除 workflow 内 tag 创建，CI 仅覆盖默认分支/PR | YAML 解析、workflow 静态审阅 | 不保留 dispatch、轮询、workflow_call 或 token 例外。 |
| DR3 | completed | 英中发布说明、项目地图和环境记录同步 | 翻译/Markdown/tracker 检查 | AxShell 只作行为参考，不进入构建或发布物。 |
| DR4 | completed | 完整本地质量门禁 | Python、Cargo、tracker、diff 检查 | 远端发布矩阵须由下一个新 tag 在 GitHub 验证。 |

## 已完成

- 已审计 AxShell 的直接 tag 触发 Release 方式；本项目保留自己的日期格式、版本元数据与五平台构建矩阵。
- 已确认当前复杂链路为 `tag -> Retry -> CI dispatch/wait -> Release dispatch`，且 `2026-08-17` 已在旧工作流快照下创建，不能回溯触发新工作流。
- 已确认发布安全边界：Release 在构建前校验 tag object 为 annotated 且 `scripts/release_version.py verify` 通过；仅最终 publish job 拥有 `contents: write`。
- 已确认 GitHub Actions 的默认 `GITHUB_TOKEN` 推送不会触发另一个 `push` workflow；为避免 Create workflow 产生无法自动发布的 tag，采用 AxShell 的单一外部 tag push 入口。
- 已删除 Create/Retry 编排 workflow；Release 直接监听 `20*-*-*`，CI 仅在默认分支成功后更新共享 Cargo cache。
- 已同步中英文发布操作、架构契约、项目地图和环境当前态；发布者先用现有脚本同步并提交元数据，再推送 annotated 日期 tag。

## 验证

- 已完成：Git 工作区、工作流、版本验证脚本、AxShell 参考工作流、发布文档、项目地图与环境记录的静态审阅；YAML、现有 annotated tag/metadata、12 项 Python 回归、413 条翻译、Cargo fmt/check/Clippy、完整 Cargo 测试、Markdown 相对链接和 `git diff --check`。
- 未完成：GitHub-hosted 平台构建和实际 Release 发布；tracker validator 已运行，但仍报告既有历史记录的 39 条时间字段错误，本轮记录未新增错误。

## 风险与阻塞

- GitHub Actions 的 tag glob 只能做候选过滤；严格日期和元数据校验仍由 `scripts/release_version.py verify --tag` 在 Release job 内执行。
- 已有 tag 不会因为默认分支上的 workflow 修改而重新触发；失败的后续新 tag 发布应从同一 Release run 使用 GitHub Actions re-run。

## 下一步

- 用下一枚同步元数据后的 annotated 日期 tag 在 GitHub-hosted Actions 验证直接发布。

## 最后更新时间

- 2026-08-17 14:28 +0800
