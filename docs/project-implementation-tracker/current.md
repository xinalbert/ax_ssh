# 当前项目实施记录

## 当前目标

- 目标 ID：20260803-AUTHCONNECT01
- 目标：为新建 SSH 会话增加“保存并连接”，并让认证弹窗按全局默认或用户当次选择安全记住密码存储后端。
- 交付物：保存并连接动作、认证弹窗凭据后端选择、成功认证后按会话持久化后端引用、无凭据导出回归、双语文档和验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/{app,session-editor,theme}.slint`、`ui/components/{overlay-host,security-dialogs}.slint`、`src/app/{connection,workspace,connection/request,connection/authentication}.rs`、相关状态/导出测试、双语架构/使用说明、项目地图与实施记录。
- 不在本轮范围内：在会话编辑器或配置中保存明文密码/私钥口令、把任何凭据或其后端引用加入普通导出、改变主机密钥默认拒绝与确认顺序、修改加密保险库算法、导入参考项目源码或依赖。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| FLOW1 | completed | 新建 SSH profile 的保存并连接动作 | Slint/Rust 联合编译、保存路由定向测试 | 保存成功后复用既有连接入口，先执行 host-key probe。 |
| PROMPT2 | completed | 认证弹窗按设置默认并允许当次选择凭据后端 | 认证选择映射测试、完整测试、静态安全审阅 | 仅认证成功且勾选记住后持久化后端引用。 |
| DOC3 | completed | 双语使用/架构契约和项目地图 | tracker validator、Markdown 结构审阅 | 普通导出继续无凭据。 |
| VERIFY4 | completed | 全仓自动化门禁与差异安全审阅 | fmt/check/test/clippy/tracker/diff | 严格 Clippy 仍受既有基线 lint 阻挡；允许已记录基线后通过。GUI 视觉由用户在目标平台验收。 |

## 已完成

- 已确认现有认证弹窗在主机密钥确认之后出现，配置只保存 `credential_storage` 引用，密码由系统凭据库或 Argon2id + XChaCha20-Poly1305 加密保险库保存。
- 已确认现有成功认证 monitor 才提交待保存密码；失败、迟到 attempt 或未勾选记住不会写入密码。
- 已确认普通导出主动移除 `credential_storage` 和 `host_key_fingerprint`，导入还会重新分配 profile UUID 并再次清理安全字段。
- 已确认设置中的全局 `credential_storage` 可以作为每次新认证弹窗的初始选择，不需要修改配置 schema。
- 已实现新建 SSH 的 Save & connect、普通认证弹窗后端选择和成功认证后的 profile 后端引用持久化；私钥/vault 解锁流程不显示无关选择。
- 已保持普通导出/导入清理、host-key 默认拒绝、密码不进入 profile JSON 和日志的既有安全边界。

## 验证

- 已完成：`cargo fmt --all -- --check`、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 109、应用 75、Doc tests 0）、tracker/Markdown/差异检查；严格全目标 Clippy 确认仅命中既有基线，允许清单后通过。
- 未完成：目标平台 GUI 视觉、键盘焦点、真实 SSH 互操作和凭据后端手工验收。

## 风险与阻塞

- 严格 `cargo clippy --all-targets --locked --offline -- -D warnings` 仍被既有配置、本地 shell、Telnet 和 worker lint 基线阻挡；本轮未扩大范围重构这些无关代码。
- 认证弹窗新增选择控件后必须维持稳定高度、键盘焦点、秘密清理和 vault 解锁专用流程。
- 保存并连接只能在 profile 成功落盘后启动；失败时保留编辑状态，不创建连接或泄露草稿秘密。

## 下一步

- 用户在目标平台确认 Save & connect、弹窗焦点/高度、后端下拉选择、vault 口令条件和首次 host-key/认证顺序。

## 最后更新时间

- 2026-08-03 18:55 +0800
