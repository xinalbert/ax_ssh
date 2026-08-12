# 当前项目实施记录

## 当前目标

- 目标 ID：20260812-release-highlights
- 目标：让按日期发布的 GitHub Release 自动展示可读的分类 Highlights，并继续附带 GitHub 的完整自动 release notes。
- 交付物：可定向测试的 Highlights 生成器、`publish` workflow 接线、双语发布说明与实施记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`.github/workflows/release.yml` 的发布正文、`scripts/` 的纯标准库生成和测试、`README{,.zh}.md`、`docs/development{,.zh}.md` 与发布/环境跟踪记录。
- 不在本轮范围内：日期 tag 规则、Cargo/lockfile/包元数据、CI 成功门禁和缓存、三平台打包内容、应用运行时、Slint、SSH trust/认证/worker、依赖升级和参考工程源码。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| RELH1 | completed | 生成、分类和去重 release Highlights 的标准库脚本及定向测试 | `python -m unittest` 与临时 Git tag range 回归 | 只消费已签出的 Git 历史；不使用外部服务。 |
| RELH2 | completed | 现有 `publish` job 的 Highlights 正文与自动 release notes 接线 | YAML 解析、Shell 静态检查和工作流输入审阅 | 日期 tag 验证、CI 门禁、缓存与发布资产未变。 |
| RELH3 | completed | 双语发布说明、环境/实施记录和收口检查 | 文档链接、tracker validator、Cargo check 与 `git diff --check` | GitHub 实际页面渲染仍须在下一次 tag 发布后确认。 |

## 已完成

- 已完成施工前预检：项目为独立 Rust 2024/Cargo 应用；本轮仅改变 CI 发布元数据和 Python 标准库辅助脚本，不改 Rust 依赖或运行时边界。
- 已确认项目地图覆盖 `.github/workflows/`、`scripts/` 与发布文档，不需要结构性刷新；未联网、未使用多 agent。
- 已确认现有 Release 已使用 `generate_release_notes: true`，但没有 `body_path` 或自定义 Highlights 生成步骤。
- 已新增 `scripts/generate_release_highlights.py`：只读当前 tag 与可达日期 tag 的 Git 历史，输出比较链接、每类最多 8 条的去重条目和不可变 commit 链接；跟踪类提交不进入 Highlights。
- 已将生成器接入 `publish` job 的 `body_path`，同时保留 `generate_release_notes: true`；CI 现在运行 release-version 与 Highlights 两组 Python 回归。
- 已同步根 README、开发和架构的中英文发布契约，并刷新环境当前态和项目地图。

## 验证

- 已完成：根规则、AxSSH Rust/Slint 与 Python 代码规范、环境当前态、项目地图、Python 定向回归/编译、YAML/Shell、Markdown、tracker、Cargo 离线和差异检查。
- 未完成：GitHub 远端发布页面的实际拼接渲染，以及三平台构建/资产发布，须在下一次日期 tag 发布时确认。

## 风险与阻塞

- Highlights 依据提交主题关键字分类，属于可解释的启发式摘要；未匹配提交仍由 GitHub 自动 release notes 覆盖。
- `body_path` 与 `generate_release_notes` 的最终拼接由 GitHub Release 服务在真实 tag 发布时呈现；本地仅能验证生成正文和 workflow 输入。
- 工作树有用户已有未提交改动；本轮只增量触及发布相关文件和所需记录，不回退其他改动。

## 下一步

- 在下一次日期 tag 发布后检查 Highlights、GitHub 自动说明和 compare 链接的线上呈现。

## 最后更新时间

- 2026-08-12 18:02 +0800：切换至日期化 GitHub Release Highlights 目标；完成环境与工作流基线审阅，RELH1 开始实施。
- 2026-08-12 21:13 +0800：完成 Git-backed Highlights 生成器、workflow 接线、双语发布契约与全部本地门禁；RELH1-RELH3 完成，保留远端发布页面验收。
