# 当前项目实施记录

## 当前目标

- 目标 ID：20261119-terminal-shortcut-system-fonts
- 目标：`⌘`、`⌃` 采用所选字体优先、缺字时系统字体回退、内置字体兜底的覆盖解析，取消固定字体与固定比例缩字号。
- 交付物：快捷键字形解析改动、本机完整门禁证据与中英架构说明；实现已通过独立审查，用户视觉验收仍待完成。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：快捷键字形的字体解析与展示（`src/app/font_bridge.rs`、`src/app/terminal_render/typography.rs`、`src/app/view/terminal.rs`、`ui/components/terminal-grid.slint`、`ui/terminal-pane.slint`）、双语架构说明、验证与实施记录；保留此前未提交的终端像素度量等改动。
- 不在本轮范围内：Telnet NVT、SSH 信任与凭据、全窗口或屏幕几何查询、Unicode 宽度策略重写、依赖升级和发布操作。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：已结束

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| S1 | completed | 所选字体优先、系统回退、内置兜底的快捷键字形解析 | 定向 RED/GREEN 16 项、字体注册/发现刷新、真实字体数据与缺字排字 | 委派的实现子任务已按用户要求停止，未创建 Worker 记录 |
| S2 | completed | 双语契约与本机完整验证、新构建 | fmt/check/严格 Clippy/test/build、diff | 父控重跑四项门禁均退出码 0，1049 项测试通过 |
| S3 | completed | 独立实现审查与验收边界交接 | 实际源码与原始测试日志审查结论 APPROVE | GUI 字形由用户确认，其它平台由原生 CI |

## 已完成

- 委派的实现子任务已按用户要求停止，当前没有进行中的实现；最终源码只把 `⌘`/`⌃` 的角色字体改为按实际覆盖解析：所选字族优先 → 预先保存的平台 Monospace 与原始 Common/Han 候选 → 后台发现的有界系统等宽字族 → 已注册内置字族兜底。
- 字号不再缩放，协议列宽、原始文字、选区/复制与按键处理保持不变；本轮不声明墨迹必然容纳于单格（实测系统字形的墨迹宽度可超过所选字体的窄格）。
- 父控独立重跑 `cargo check --locked --offline`、严格 `cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`（478 library + 571 binary 共 1049 项，Doc tests 0）与 `cargo build --locked --offline`，退出码均为 0；`cargo fmt --all -- --check` 与 `git diff --check` 此前均为 0。
- 子任务定向证据：快捷键 16 项、`app::font_bridge::tests` 20 项、`terminal_typography` 8 项、`terminal_render` 28 项通过，Slint 由 Cargo 重新编译；中英架构说明已与新的解析顺序同步。
- 独立 reviewer 审查实际差异与原始日志后结论为 APPROVE，未发现阻止本地提交的新增源码缺陷。本机 `--target aarch64-apple-darwin` 的 check 与严格 all-target Clippy 亦均退出码 0。

## 验证

- 已完成：本机 fmt/check/严格 Clippy/完整测试/build 与 diff 检查（退出码 0），以及上述快捷键、字体注册、终端排字与渲染定向回归；证据日志为 `axssh-precommit-validation.log`、`shortcut-font-tdd/green-shortcuts-final.log` 与 `shortcut-font-tdd/static-checks.log`。
- 未完成：用户对本机新构建的视觉验收；Windows/Linux/Intel macOS 原生 CI。单格无重叠/无裁切未主张；本地提交由主控按像素几何与字体回退两个主题整理，不推送或发布。

## 风险与阻塞

- 保持 Slint 负责实际布局、Rust 负责协议和 transport；不以排版字形宽度修改模型列宽，也不以固定缩放掩盖超宽问题。
- 系统字体的覆盖与轮廓随平台和已安装字体变化，实际回退字族只能由真实排字结果确定；不能以存在字形或等宽标志代替排字验证。

## 下一步

- 用户以本机新构建复查 `⌘`/`⌃` 实际字形及跨屏度量；其它平台交由原生 CI。当前无活跃实现任务；仅整理并验证本地提交，不继续扩大功能实验。

## 最后更新时间

- 2026-11-19 15:08 +0800
