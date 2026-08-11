# 当前项目实施记录

## 当前目标

- 目标 ID：20260812-terminal-semantic-highlights
- 目标：参考 `ax_ashell` 的有界语义标注思路，为可见 Terminal 的明确状态词、HTTP 状态码、URL 和远端路径提供实时颜色提示，并在所有终端主题背景上保持可读。
- 交付物：私有终端渲染层的有界语义匹配、按终端背景校正的语义色、保留 ANSI/真彩色的 run 切分、回归测试与双语契约。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app/terminal_render.rs` 的 UI 独立渲染映射，以及 `docs/{architecture,architecture.zh,usage,usage.zh}.md`、`docs/project-{implementation-tracker,env-audit}/` 的记录。
- 不在本轮范围内：终端 target 打开语义、指针/下划线交互、终端输入、终端缓冲/回滚、设置 schema、主题持久化字段、SSH/SFTP worker、host-key trust、凭据、依赖/工具链、构建文件和 `third_package/axshell`。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| TSEM1 | completed | 有界的状态词、HTTP、URL 和远端路径语义分类及按终端背景校正的颜色 | `terminal_render` unit tests | 仅处理可见 render line，不读取 worker/文件系统；不匹配任意子串。 |
| TSEM2 | completed | 语义 span 切分进终端 render run，保留 ANSI/真彩色、背景、属性和宽字符列 | `terminal_render` unit tests、Cargo check | 仅默认前景/背景的字符可被覆盖，不改变终端已有样式。 |
| TSEM3 | completed | 双语契约、项目/环境记录与完整门禁 | Rustfmt/check/test/tracker/diff | GUI 色彩与不同主题由用户在目标平台验收。 |

## 已完成

- 已完成施工前环境预检：根目录、`Cargo.toml`、`Cargo.lock`、`.github/workflows/ci.yml` 与已有环境记忆一致；本机 Rust 1.96.1/Cargo 1.96.1 可用，`cargo fmt` 与 `cargo clippy` 子命令未安装。
- 已检查 `ax_ashell` 的高亮策略：使用有界、边界感知的可见行匹配，并保持独立于终端 transport；本轮只采用该思路，不复制其代码或引入依赖。
- 已确认 `src/app/terminal_render.rs` 是唯一适合持有渲染期语义色和 run 切分的 owner，既有最小对比度逻辑可对每个终端背景执行校正。
- 已实现 URL/路径、HTTP `2xx-5xx` 及常见成功、警告、错误状态词的边界感知语义色；仅默认 ASCII run 会被切分，ANSI/真彩色、非默认背景、inverse、dim 和非 ASCII run 保留原样。
- 已完成 8 项 `terminal_render` 定向测试，覆盖目标/状态词、词边界、ANSI 保留、所有内置终端色表和浅色自定义背景的 4.5:1 语义色对比度。

## 验证

- 已完成：项目地图、AxSSH Rust/Slint 规范和环境记忆预检；直接 `rustfmt --edition 2024 --check`、8 项 `terminal_render` 定向测试、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 147、应用 144、Doc tests 0）、tracker validator、Markdown 相对链接和 `git diff --check` 均通过。
- 未完成：本机未安装的 `cargo fmt`/`cargo clippy` 子命令，以及目标平台 GUI 验收。

## 风险与阻塞

- 不能覆盖已有 ANSI/真彩色、非默认背景、inverse 或其他显式终端样式，否则会破坏远端程序自身的语义色。
- 语义标注只能扫描有界可见行；不得记录终端文本、创建无界缓存或让渲染循环访问 UI/worker。
- 本机缺少 `cargo fmt` 与 `cargo clippy` 子命令；CI 或具备组件的环境仍需补充两项门禁。

## 下一步

- 由用户在目标平台确认各终端主题下的颜色层次、ANSI 输出保留、Cmd/Ctrl target 下划线与文本选择；CI 或具备组件的环境补充 Cargo fmt/Clippy。

## 最后更新时间

- 2026-08-12 01:25 CST
