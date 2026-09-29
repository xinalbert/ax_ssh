# 当前项目实施记录

## 当前目标

- 目标 ID：20260929-terminal-grid-alignment
- 目标：修复长盒线与独立竖线的终端字符格错位，并核对当前构建的工作区退出/启动恢复路径。
- 交付物：精确的终端单格度量、双语架构说明、恢复路径核查、验证记录及本机应用更新。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/terminal-pane.slint`、工作区恢复只读核查、双语架构与跟踪文件、macOS 本机应用包。
- 不在本轮范围内：`third_package/axshell`、SSH host-key/认证、终端 transport、用户 GUI 视觉验收。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| GRID1 | completed | 消除 50-cell probe 取整导致的长盒线累计偏移 | Slint 重编译、字号定向回归 | 保留盒线合并绘制 |
| GRID2 | completed | 核对退出快照和启动恢复；仅修可证实缺陷 | 工作区快照结构和保存/恢复源码审查 | 当前运行包已含窗口保持提交；无可证实的新恢复缺陷 |
| GRID3 | completed | 双语架构、完整门禁、应用包替换 | fmt/check/Clippy/test/diff/tracker/Markdown、签名验证 | GUI 由用户验收 |

## 已完成

- 源码确认 Slint 1.18.1 对 `Text.preferred-width` 向上取整；50 个 Latin cell 平均值相对真实字距最多偏高 0.02 逻辑像素/列，长盒线和独立竖线因此可在右端出现数物理像素的错位。
- 当前运行包已包含 2026-09-26 的窗口保持实现；退出前会捕获窗口几何并 flush 最终工作区快照，启动时从私有快照恢复。当前私有快照只有一扇主窗口，无法证明曾有的独立窗口为何缺失。
- 把隐藏 Latin 测量样本扩展到 1000 格；保留现有连续盒线 shaping。定向 UI 数值回归用 17.11px 自带等宽字体验证 10.266px 单格 advance，旧 50 格 probe 会给出 10.28px。
- 构建本机 arm64 发布版，备份旧应用可执行文件后原子替换 `AxSSH.app` 的核心并重新 ad hoc 签名；运行中的用户进程保持不变，新核心在下次启动生效。

## 验证

- 已完成：Slint 1.18.1 本地源码与工作区恢复路径核查；定向 UI 数值回归、fmt、locked/offline check、严格 Clippy、完整测试（库 288、应用 288、Doc tests 0）、release build、Markdown 相对链接及 diff 检查通过；包内与构建二进制 UUID 一致，签名严格验证通过。tracker validator 已执行，本轮条目无报错。
- 未完成：用户重启后的边框视觉与真实窗口/工作区恢复验收；tracker validator 仍被既有 8/9 月历史及 research 时间字段错误阻断。

## 风险与阻塞

- 原先未恢复的独立窗口不在当前私有快照中，缺少发生当时的快照；不能据此臆断恢复器缺陷。
- GUI 视觉由用户验收；代理不采集自身应用截图作为证据。
- 应用包更新前的可执行文件备份位于 `/private/tmp/axssh-update.mQRfl9/AxSSH.before`，为临时目录内容。

## 下一步

- 请用户正常退出并重启安装包，确认 tmux 边框和工作区恢复；如仍未恢复，保留退出前后 `workspace.json` 的窗口/Tab 数量以定位持久化还是展示环节。

## 最后更新时间

- 2026-09-29 10:49 +0800
