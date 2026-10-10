# 当前项目实施记录

## 当前目标

- 目标 ID：20261010-release-tag
- 目标：按仓库日期发布约束，将已提交的系统图标和逐服务器密码清除工作标记为本地 annotated tag `2026-10-10`。
- 交付物：一致的 Cargo/lock/macOS 版本元数据、发布检查结果、独立版本提交及不可移动的本地 annotated tag；保留既有原生验收待办。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`Cargo.toml`、`Cargo.lock`、`packaging/macos/Info.plist`、版本脚本与 Highlights 回归、tracker 和本地 Git 提交/标签。
- 不在本轮范围内：功能修改、依赖升级、真实凭据库操作、LICENSE 的用户修改、推送或 GitHub Release 发布。
- 所有权：版本脚本同步三份发行元数据，Git 保存版本提交与 annotated tag；既有 UI/config/SSH 所有权保持不变。
- 安全边界：不访问真实服务器或凭据，不修改主机信任；标签仅指向已提交且版本匹配的对象，不移动或复用已消耗标签。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| REL1 | completed | 检查本地和全部远端标签，选择下一未使用日期标签 | local tags、origin/gitea ls-remote | 三处均没有今日标签，选用 2026-10-10 |
| REL2 | completed | 同步版本并完成发行前检查 | release helpers、元数据 verify、Rust 自动门禁 | Cargo 2026.10.10、macOS build 20261010 |
| REL3 | completed | 独立版本提交与本地 annotated tag，记录对象一致性 | index review、tag object type、tag commit metadata | 标签指向 477a8ea；不推送，保留未提交 LICENSE |
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

- 已创建功能提交 `4095d78` 与双语文档提交 `6057a2e`；未推送或发布，LICENSE 的用户修改未纳入提交。
- 发布标签检查覆盖本地、origin 与 gitea，创建前再次核对今日未使用；版本提交 `477a8ea` 仅包含 Cargo/lock/macOS 三份元数据，版本分别为 `2026.10.10`、`2026.10.10`、macOS build `20261010`。
- 已创建本地 annotated tag `2026-10-10`，对象为 `ae3dcf30ce9dcba3968684f4880c0c037515dc02`，指向 `477a8ea06b06b2ccc0a735503a1b5cc454828777`；从标签对象提取元数据再次 verify 通过，未推送或触发 Release。

## 验证

- 已完成：8 项清除聚焦回归及完整 650 项测试（311 library + 339 binary）通过；fmt、locked/offline check、严格 all-target Clippy、Slint 重新编译、aarch64-apple-darwin 显式 target check/严格 Clippy/build 通过；519 条中文翻译、697 条相对 Markdown 链接、项目 skill validator 和 diff 通过；当前 tracker/地图/本轮历史无错误，全局保留 55 项既有历史格式问题。
- 本轮版本检查：三份元数据、已提交 HEAD 和实际 tag 内容均通过 `verify --tag 2026-10-10`，tag 类型为 `tag`；14 项发布/Highlights 回归、fmt、locked/offline check、严格 all-target Clippy、650 项完整测试与 diff 检查通过。
- 本轮记录检查：697 条相对 Markdown 链接和项目 skill validator 通过；tracker 当前/本轮历史无新增错误，全局保留同一 55 项既有历史格式问题。
- 未完成：原生界面/键盘、确认取消、真实系统凭据库删除及其他 CI 原生平台验收；前一轮图标 UI7 保持待用户验收。

## 风险与阻塞

- 系统凭据库访问可能要求操作系统授权；凭据删除或配置写入失败应保留/恢复旧引用和凭据。
- tracker 全局存在 55 项既有历史格式问题；本轮新增记录与当前计划必须通过。

## 下一步

- 本地标签已完成。后续推送时，必须先将包含版本提交的 `master` 推送，再推送该 annotated tag；tag push 会自动触发 GitHub Release，不得移动、删除或复用此标签。
- 用户验收 PW5：编辑页保存后端状态、清除/取消确认、未保存输入保留与真实系统凭据库；继续原 UI7 图标/导航验收。

## 最后更新时间

- 2026-10-10 09:40 +0800：版本提交和本地 annotated tag 完成，对实际标签元数据核验通过，保留既有原生验收。
