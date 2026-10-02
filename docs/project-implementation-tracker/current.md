# 当前项目实施记录

## 当前目标

- 目标 ID：20261002-cursor-redraw-coalescing
- 目标：减少关闭本地光标闪烁后，远端重绘分批发送隐藏/显示光标导致的短暂消失。
- 交付物：应用层固定 24 ms 合并窗口、确定性回归、双语架构说明和可运行 debug 构建。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：终端 model 的只读协议可见性查询、application 输出 effects/快照呈现调度与回归。
- 所有权：协议状态继续由 terminal model 拥有；每个 tab 的有界呈现截止时间由 application state 拥有；Slint 保持已有闪烁开关与光标绘制。
- 不在本轮范围内：依赖升级、SSH trust/凭据、transport 批次策略、发布 tag、参考项目代码。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| CR1 | completed | 环境、渲染链路、所有权核查并先落盘计划 | manifest/build/CI/技能/地图及前轮分批实验 | ?25 可见性独立于本地闪烁开关 |
| CR2 | completed | 固定截止时间合并及确定性回归 | 短暂 hide/show、持续隐藏、重入、全量刷新、2026、首次帧与生命周期 | 单 tab 单截止时间；不拖延协议响应 |
| CR3 | completed | 全量门禁、构建与配套文档 | fmt/check/Clippy/test/build、链接、tracker/skill、diff | GUI 由用户验收 |

## 已完成

- 上一轮只读检查确认关闭闪烁时本地 Timer/phase 不会隐藏光标；独立的远端 `?25l/?25h` 分批发布会改变可见性。
- 已复现分批 hide/show 产生隐藏中间帧，同批和 2026 同步重绘不产生该中间帧。
- 本机 Rust/Cargo 1.97.1，Rust 2024/MSRV 1.92.0、Slint 1.18.1、锁文件和 CI 矩阵未变化。
- 已实现固定 24 ms 呈现合并：只在已有可见帧时合并；显示请求提前解除等待；持续隐藏超时生效；后续输出不得延长窗口。

## 验证

- 已完成：先计划后施工、环境核查、6 项新增光标合并回归（Tokio 虚拟计时）、fmt、locked/offline check、严格 all-target Clippy、完整测试（库 301、应用 309、Doc tests 0）、debug build、4 条 Markdown 相对链接、skill/tracker 与 diff 检查。双语架构/项目地图已刷新；代码与测试已提交为 `aebc44e`，文档单独提交。
- 未完成：GUI 实际体验与 Windows/Linux 原生 CI；未推送提交。

## 风险与阻塞

- 24 ms 窗口是有限延迟，不是远端重绘事务协议；跨更长间隔的 hide/show 仍可能可见。持续隐藏请求最多增加该合并等待，既有 FPS/2026 限制另计。
- 用户尚未提供实际闪烁场景，修复针对已复现的链路，不承诺覆盖完全静止时的其他渲染问题。
- Windows/Linux 原生 CI 与 GUI 验收本机不可完成；不自行捕获应用截图。

## 下一步

- 用户重启最新 `target/debug/ax_ssh`，在关闭光标闪烁时验收原有重绘/输入场景；若完全静止时仍闪烁，继续按实际触发场景定位。

## 最后更新时间

- 2026-10-03 +0800
