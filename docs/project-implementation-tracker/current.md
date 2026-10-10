# 当前项目实施记录

## 当前目标

- 目标 ID：20261010-server-credential-reset
- 目标：核对密码保存与字段变更后的凭据清理，并在每台服务器的编辑页显示保存后端；用户单独确认后立即清除已保存密码，保留未保存的其他编辑。
- 交付物：服务器密码状态与明确确认入口、后台凭据/配置事务、并发与失败回归、双语行为文档和验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app/workspace/`、`src/app/credential_tasks.rs`、`ui/session-editor.slint`、`ui/workspace-shell.slint`、普通管理弹窗、`scripts/build_zh_catalog.py`、翻译映射及目录、双语架构/用法文档与 tracker，以及用户此前授权的本地提交整理。
- 不在本轮范围内：直接访问用户当前真实凭据库、全局密钥管理器、自动迁移或删除未引用条目、SSH 信任策略、依赖升级、推送或发布。
- 所有权：Slint 保留编辑草稿并提交确认后的清除意图；应用层持有原 profile、逐 profile mutation token 和共享 persistence gate；既有 credential task 负责 blocking 后端删除与回滚，config 负责私有原子持久化。
- 安全边界：密码不进入只读 UI 模型或配置；仅清除当前服务器引用的密码和自动保险库解锁文件，不修改 host-key fingerprint 或运行中连接。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| PW1 | completed | 核对存储标识、现有删除/迁移/回滚和管理方式 | 配置、凭据 task 与编辑/认证调用链 | 默认应用保险库；系统条目按 service + UUID 定位 |
| PW2 | completed | 编辑页保存后端与立即清除确认，后台事务提交 | Slint 编译、草稿身份与事务边界检查 | 用户选择立即执行并单独确认 |
| PW3 | completed | 清除的状态、并发、持久化失败回归与完整自动门禁 | fmt/check/严格 Clippy/tests/翻译/diff | 不访问真实系统凭据 |
| PW4 | completed | 双语用法/架构、地图与实施记录收口 | 相对链接、skill/tracker validators | 保留既有历史校验问题基线 |
| PW6 | completed | 分段本地提交 | 显式暂存/暂存差异/提交后状态 | 保留用户单独修改的 LICENSE，不推送或发布 |
| PW5 | pending | 用户验收原生清除/取消与系统后端 | 原生界面、保存状态刷新、真实凭据删除 | 不截图应用，视觉验收属于用户 |
| UI7 | pending | 前一轮图标/导航原生验收 | Local Shell/发行版、配色、tooltip/焦点、真实 SSH release | 前一轮实现及本地提交已完成 |

## 已完成

- 已接入服务器编辑页的保存后端状态、立即清除确认和后台事务；只更新持久化 profile 的密码引用及自动解锁标记，保留编辑器 draft、运行中终端、主机信任/系统识别和未保存 Settings 预览；回滚持有原 CredentialStore，测试使用临时保险库。

- 核对系统凭据服务 `com.axsoft.ax_ssh`、账户 `session:<UUID>`；字段更新保留 UUID 时不会影响查找。既有换后端、删 profile、改非密码认证及拒绝旧密码均走事务删除；host/port/username 修改继续保留密码引用。
- 核对默认加密保险库的逐 UUID 加密文件及可选自动解锁文件；补齐原先缺失的逐服务器清除入口。
- 用户明确选择“点击后立即清除，单独确认”；取消确认保留凭据，确认不提交其他编辑草稿。
- 前一轮 116 项系统图标目录、手动指定、目标绑定成功缓存与 20px Group 图标完成，已创建 `bbc4721`、`a412f49`、`a7538fb` 三段本地提交，未推送或发布。

- 已创建功能提交 `4095d78`；双语行为/架构、地图与本条验证记录随第二段文档提交收口；未推送或发布，LICENSE 的用户修改未纳入提交。

## 验证

- 已完成：8 项清除聚焦回归及完整 650 项测试（311 library + 339 binary）通过；fmt、locked/offline check、严格 all-target Clippy、Slint 重新编译、aarch64-apple-darwin 显式 target check/严格 Clippy/build 通过；519 条中文翻译、697 条相对 Markdown 链接、项目 skill validator 和 diff 通过；当前 tracker/地图/本轮历史无错误，全局保留 55 项既有历史格式问题。
- 未完成：原生界面/键盘、确认取消、真实系统凭据库删除及其他 CI 原生平台验收；前一轮图标 UI7 保持待用户验收。

## 风险与阻塞

- 系统凭据库访问可能要求操作系统授权；凭据删除或配置写入失败应保留/恢复旧引用和凭据。
- tracker 全局存在 55 项既有历史格式问题；本轮新增记录与当前计划必须通过。

## 下一步

- 用户验收 PW5：编辑页保存后端状态、清除/取消确认、未保存输入保留与真实系统凭据库；继续原 UI7 图标/导航验收。

## 最后更新时间

- 2026-10-10 09:06 +0800：完整自动检查与原生 macOS 构建通过，功能/文档分段提交，保留用户原生验收。
