# 当前项目实施记录

## 当前目标

- 目标 ID：20260815-sftp-row-context-actions
- 目标：为 SFTP 远端文件列表补齐行级右键菜单，并把远端下载和删除操作从顶部工具栏迁入菜单。
- 交付物：右键命中行成为明确操作目标、远端下载/删除菜单、对应顶部入口移除、Cargo/Slint 完整验证。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/sftp-pane.slint`、`scripts/build_zh_catalog.py`、`translations/zh-CN/`、`docs/{usage,usage.zh,architecture,architecture.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 不在本轮范围内：SFTP worker/transfer 协议、SSH trust、凭据、配置 schema、依赖、Terminal UI 与发布流程。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| SCM1 | completed | SFTP 选择、下载、删除 callback 与现有菜单组件审计 | Slint/Rust 静态审阅 | 复用已有操作链，不扩展 transport 或文件系统边界。 |
| SCM2 | completed | 远端行级右键菜单和明确操作目标 | Cargo 重新编译 Slint | 右键未选中行时只选中该行；已选中行保留当前多选集合。 |
| SCM3 | completed | 顶部下载/删除入口移除与菜单状态收口 | 静态布局审阅 + Cargo check | 远端菜单含下载/删除；本地没有既有删除能力，因此不伪造该动作。 |
| SCM4 | completed | tracker、格式、Clippy、测试和差异门禁 | 完整仓库命令 | GUI 视觉与实际右键手感由用户验收。 |

## 已完成

- 已完成施工前环境预检：独立 Rust 2024 + Slint 1.17.1 项目，MSRV 1.92.0，本机 Rust/Cargo 1.97.1，Cargo locked/offline 门禁可用。
- 已确认 SFTP 文件行目前只有复选框选择和双击打开，没有右键事件或菜单状态；顶部只有远端下载/删除和本地上传，其中远端操作直接调用既有 callback。
- 已确认本轮可复用现有选择与操作 callback，不需要修改 russh/SFTP worker、文件删除实现或传输队列。
- 已用共享 `FlatActionMenu` 为远端文件行增加 Download/Delete 右键菜单；未选中命中行先替换为唯一选择，已选中行保留多选。
- 已从远端顶部工具栏移除 Download/Delete；编辑、重命名、本地上传及路径复制保持原有位置和 callback。
- 已确认本地栏没有删除 callback 或领域实现，本轮不把本地路径误接到远端删除链路。
- 已移除两个仅属于旧顶部按钮的 stale 中文翻译键；菜单继续使用既有 `Download` / `Delete` 翻译。

## 验证

- 已完成：`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、完整 `cargo test --locked --offline`（库 179、应用 172、Doc tests 0）、413 条中文目录、tracker、7 份 Markdown 相对链接和 `git diff --check`。
- 未完成：目标平台 GUI 的右键弹出位置、菜单视觉、单选替换与多选保留手工验收。

## 风险与阻塞

- Slint 行级右键与弹出位置没有独立自动化 UI 测试入口；以单一菜单状态、Cargo 编译和现有 Rust 操作链回归覆盖，最终视觉与手感需用户验收。

## 下一步

- 由用户使用当前构建验收远端文件行右键 Download/Delete、未选中行单选替换和已选中行多选保留。

## 最后更新时间

- 2026-08-15 17:38 CST
