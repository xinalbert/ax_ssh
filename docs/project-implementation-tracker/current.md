# 当前项目实施记录

## 当前目标

- 目标 ID：20260812-ui-language-selection
- 目标：在 Settings > General 提供可即时应用并持久化的界面语言下拉框，支持 Follow system、English 与简体中文。
- 交付物：稳定语言配置类型与 schema 迁移、Slint 运行时语言切换和内嵌翻译资源、Settings 草稿/保存链路、双语架构与使用说明、离线门禁。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/config/` 语言设置、`src/app/` Settings 映射与 locale 切换、`ui/` 用户可见文案和 General 语言选择器、`translations/` 简体中文资源、构建入口及对应文档记录。
- 不在本轮范围内：未完成翻译的语言、远端 Terminal 内容、用户自定义 profile/path、日志内部文本、SSH trust/认证/worker、依赖升级、CI/发行工作流和参考工程源码。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| LANG1 | completed | 增加稳定语言领域类型、schema v21 默认迁移和配置回归 | config 定向测试 | 默认 Follow system；持久化稳定代码，不依赖 Slint。 |
| LANG2 | completed | 接入 Slint bundled translations、General 下拉和即时 locale 切换 | Cargo 编译与 386 条 UI 文案覆盖审计 | 下拉显示 Follow system、English 与简体中文。 |
| LANG3 | completed | 同步双语契约、项目地图并完成完整门禁 | fmt/check/clippy/test/tracker/Markdown/diff | 自动化已完成；目标平台视觉和交互由用户验收。 |
| LANG4 | completed | 防止快速连续语言选择的迟到保存覆盖最新请求 | 代次回归与完整离线门禁 | blocking 保存和 UI 分发均重验最新请求代次。 |

## 已完成

- 已核实项目当前没有 locale/i18n 配置，用户可见文案主要为 Slint 硬编码英文，动态状态和错误由 Rust 应用桥产生。
- 已核实锁定的 Slint 1.17.1 支持 `@tr(...)`、构建时内嵌 PO 资源以及创建首个组件后通过 `select_bundled_translation` 即时刷新所有翻译绑定。
- 状态所有权确定为 `AppSettings` 保存稳定语言代码；Slint 只维护设置草稿、显示名称和选择意图，文件保存继续使用既有应用配置边界。
- 已增加默认 Follow system、English 与简体中文三种稳定选择；中文系统 locale 解析为 `zh-CN`，其它系统 locale 回退 English。
- 已完成 386 条静态 Slint 文案的简体中文目录、生成器、空翻译/陈旧项/编号占位符检查，并把语言选择接入独立持久化事务和所有存活窗口。
- 已为独立语言保存增加有界请求代次；旧 blocking 任务取得状态锁后若已过期则不写盘，迟到 UI completion 也不会覆盖最新语言或状态提示。

## 验证

- 已完成：根规则、AxSSH Rust/Slint skill、tracker 规则、配置/Settings/应用映射、Slint 1.17.1 bundled translation API、config/Settings 定向测试、迟到语言保存回归、完整 Cargo/翻译/Markdown/diff 门禁和提交前边界审计。
- 未完成：目标平台 UI 验收。tracker validator 基线仍有 16 条本目标之前的旧月度记录缺失或使用非法时间字段，本轮记录字段合法。

## 风险与阻塞

- 仅翻译设置页面会制造无效语言选项，因此简体中文必须覆盖应用拥有的用户可见 UI 文案；远端 Terminal 内容和用户数据不翻译。
- Rust 动态状态不能直接使用 Slint `@tr`；本轮不进行脆弱的运行时字符串替换，技术错误详情保持原文。后续全量本地化应先把应用状态重构为稳定消息 ID 和参数。
- 工作树包含此前 Settings、发布、终端、窗口和 Tooltip 改动；本轮在其上增量修改，不回退或覆盖无关工作。

## 下一步

- 提交当前 UI/i18n 结果后，由用户验收目标平台焦点、滚动、hover、标题栏、边界和语言切换。

## 最后更新时间

- 2026-08-12 17:31 +0800：完成 schema v21、Follow system/English/简体中文选择、386 条静态 UI 中文目录和独立保存/多窗口切换链路，进入完整门禁。
- 2026-08-12 17:44 +0800：fmt/check/严格 Clippy、完整 301 项 Rust 测试、翻译目录、46 项 Markdown 链接与差异门禁通过；LANG1-LANG3 完成。
- 2026-08-12 17:47 +0800：新增 LANG4 语言请求代次，防止快速连续选择时迟到的 blocking 保存或 UI completion 覆盖最后一次选择，进入完整离线门禁。
- 2026-08-12 17:53 +0800：完整 302 项 Rust 测试、严格 Clippy、386 条翻译和提交前边界审计通过；LANG4 完成，目标仅剩用户视觉与交互验收。
