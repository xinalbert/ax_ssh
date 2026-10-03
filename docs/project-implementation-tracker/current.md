# 当前项目实施记录

## 当前目标

- 目标 ID：20261003-compact-group-style
- 目标：简化收起侧栏中 Group 缩写的视觉样式，去掉常驻边框并拉开组名与服务器的字重层级。
- 交付物：无边框 Group 样式、完整校验/debug 构建、双语使用说明与跟踪记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/components/sidebar-controls.slint` 的紧凑文字组项样式及配套说明。
- 所有权：仅 Slint 视觉属性；保留选择、hover、键盘焦点、tooltip、展开状态与 callback 所有权。
- 不在本轮范围内：Rust 数据流、协议/凭据、依赖、持久化格式、展开侧栏与 Full name 布局。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| SG1 | completed | 用户截图、组件/主题、环境与地图核查，先落盘计划 | 锁定 Slint/manifest/build/CI 与组件绑定 | 用户截图可检查，不自行生成应用截图 |
| SG2 | completed | 去常驻 Group 框、字重层级及文字宽度，配套说明 | 静态核对选择/hover/键盘与窄栏/full-name 条件 | 保留现有组间留白 |
| SG3 | completed | 编译、全量门禁、构建与审查 | fmt/check/Clippy/test/build、链接、skill/tracker/diff | GUI 由用户验收 |

## 已完成

- 读取用户提供的截图；Group 边框常驻，组名与服务器缩写都为 700 字重，文字宽度受方形卡片限制。
- 环境记忆与 manifest/build/CI 核对一致：Rust 2024/MSRV 1.92.0，本机 Rust/Cargo 1.97.1，Slint 1.18.1；无依赖/工具链变更。
- 紧凑组项已移除常驻背景/边框，以强调色和 700 字重标识组名，服务器使用已有 600 字重；组项文字使用窄栏可用宽度。
- 已给出可选样式问题；未收到回复时按无边框方案继续，保留选中底色、hover 底色与键盘焦点轮廓。

## 验证

- 已完成：截图/代码/主题/环境及所有权核查；紧凑组项去常驻框/底色、组名/服务器 700/600 字重、组项宽度调整；双语用法与项目地图已更新。
- 已通过：Slint 重编译、fmt、locked/offline check、严格 all-target Clippy、完整测试（库 301、应用 309、Doc tests 0）、debug build、4 条 Markdown 相对链接和 skill/tracker/diff。纯样式变更未新增镜像测试。
- 未完成：GUI 视觉验收和 Windows/Linux 原生 CI。

## 风险与阻塞

- 实际字体和主题下的外观由用户验收；不自行捕获或检查应用截图。
- Windows/Linux 原生 CI 本机不可替代；不改变平台相关代码。

## 下一步

- 用户重启最新 `target/debug/ax_ssh`，检查紧凑侧栏分组、选中/hover/focus 的视觉效果；本次样式及文档作为独立提交保存，不推送。

## 最后更新时间

- 2026-10-03 +0800
