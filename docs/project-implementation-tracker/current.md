# 当前项目实施记录

## 当前目标

- 目标 ID：20260804-OPTIONALPASSWORDSAVE01
- 目标：将会话编辑器中的 SSH 密码输入与密码持久化解耦，默认支持一次性快速连接，只有用户明确选择记住密码时才要求凭据存储信息。
- 交付物：可选的 Remember password/存储后端 UI、一次性密码跨主机密钥确认的短期内存链路、保存与连接分流回归、双语架构说明和验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/{session-editor,workspace-shell,app}.slint`、`src/app/{workspace,state}.rs`、`src/app/connection/{request,host_key,authentication}.rs`、相关应用测试、双语架构说明与实施记录。
- 不在本轮范围内：配置 schema、默认凭据后端设置、认证弹层的既有 Remember password 行为、SSH 主机密钥信任策略、明文凭据持久化、`third_package/axshell` 耦合和 GUI 截图验收。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| CONTRACT1 | completed | 编辑器密码使用与保存意图的跨层合同 | callback/状态链路审阅、focused tests | 默认一次性；只有明确记住时持久化。 |
| UI2 | completed | Remember password、凭据后端与条件式保险库口令控件 | Slint/Cargo 联合编译 | 保险库口令仅在记住到 encrypted-vault 时显示。 |
| FLOW3 | completed | 一次性密码跨保存、主机密钥确认和认证的短期内存链路 | workspace/state/connection focused tests | 不进入配置、Slint snapshot 或日志。 |
| DOC4 | completed | 双语架构契约和跟踪记录 | tracker/Markdown 检查 | 保持安全边界说明对齐。 |
| VERIFY5 | completed | 全仓 Rust/Slint 门禁与差异安全审阅 | fmt/check/clippy/test/diff | GUI 视觉由用户在目标平台验收。 |

## 已完成

- 已确认现有编辑器把任何非空 SSH 密码都解释为持久化更新，因此默认 encrypted-vault 时会立即强制显示保险库口令。
- 已确认认证弹层本身已有可选 Remember password 语义，可作为编辑器交互与后端选择的现有模式。
- 已将编辑器默认改为一次性密码；Remember password 未勾选时不设置或更新 `credential_storage`，单独保存会明确提示密码未保存。
- 已增加条件式凭据后端选择和保险库口令字段；只有 Remember password + encrypted-vault 同时成立时才要求保险库口令。
- 已将一次性密码绑定到对应 Terminal Tab，并在主机密钥拒绝/失败、Tab 关闭、phase 回到 Idle 或 worker 取走秘密时清除。
- 已同步中英文架构、使用说明和项目地图，明确默认一次性与显式保存语义。

## 验证

- 已完成：根指令、项目 skill、Rust/Slint 边界、项目地图、双语架构和现有凭据/连接代码审阅；`cargo fmt --all -- --check`、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 110、应用 82、Doc tests 0）、workspace 18 项 focused tests、一次性密码 Tab/host-key phase 回归、tracker validator、Markdown 相对链接和 `git diff --check`。
- 未完成：目标平台 GUI 字段显隐、焦点顺序和真实 SSH 快速连接验收；严格全目标 Clippy 仍命中既有仓库基线。

## 风险与阻塞

- 一次性密码可能需要等待未知主机密钥确认；必须只由对应 Terminal Tab 短期持有，并在拒绝、失败、关闭或 worker 启动时立即清除。
- 编辑既有已保存密码的 profile 时，未勾选 Remember password 只覆盖本次连接，不应隐式删除原有凭据引用。
- `cargo clippy --all-targets --locked --offline -- -D warnings` 仍被既有 config/local shell/Telnet/SSH worker 与应用 lint 阻挡；允许这些已知类别后的补充运行仍命中既有 `items_after_test_module`、输入布尔式、large enum 和旧测试初始化 lint，本轮新增文件未出现独立 lint。

## 下一步

- 用户在目标平台确认默认不显示 Vault password，勾选 Remember password 后按后端条件显示，并验证 **Save & connect** 不会再次询问 SSH 密码。

## 最后更新时间

- 2026-08-04 14:12 +0800
