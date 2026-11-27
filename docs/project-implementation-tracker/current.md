# 当前项目实施记录

## 当前目标

- 目标 ID：20261126-ssh-forwarding-enabled
- 目标：为每个 SSH profile 保存 forwarding 历史和独立启用状态，并在 SSH 连接建立后自动启动启用规则。
- 交付物：有界 profile 规则、独立启用开关、多规则并行 worker、自动启动、Forwarding Manager 表格页、规则增删改和检查、可配置快捷键、导出脱敏、回归测试及双语说明。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：现有 Local/Remote/Dynamic、TCP/Unix socket 转发入口、SSH profile 配置、worker 生命周期和转发管理页。
- 不在本轮范围内：跳板、外部代理命令、连接复用、批处理、历史规则自动启动策略以 enabled 状态为准。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| H1 | completed | `SshConfig` forwarding preset 模型、校验、去重和上限 | config focused tests、serde round trip | 最多 16 条，旧配置默认空列表，每条有 enabled 状态 |
| H2 | completed | 成功启动后的 profile 持久化、独立开关与导出脱敏 | session management focused test、persistence gate | 每条规则独立保存 enabled，导出清空本机端点历史 |
| H3 | completed | 转发对话框历史下拉、字段回填和独立 enabled 开关 | Slint/Cargo check | 选择只回填，字段和开关继续可编辑 |
| H4 | completed | 独立 Forwarding Manager 表格页、双语文档、tracker 和完整仓库验证 | fmt、check、Clippy、test、translation、tracker | GUI 视觉和真实 SSH daemon 仍需人工验收 |
| F1 | completed | 转发管理快捷键、旧配置兼容、菜单和设置录制 | 配置迁移、无窗口按键回归、macOS ARM64 check/Clippy、完整 Cargo 测试 | macOS `Cmd+Shift+F`，Windows/Linux `Ctrl+Shift+F` |
| H5 | completed | 管理页新建、编辑、删除和规则检查 | 配置事务、回环 SSH 监听状态、无窗口菜单和完整 Cargo 检查 | 保存和查询沿用 profile 及 worker 边界 |

## 已完成

- 已有转发实现支持 Local/Remote/Dynamic、TCP/Unix socket、SOCKS5、Agent forwarding、远程命令和 OpenSSH 配置预览。
- `SshConfig` 已增加可选 forwarding history，每条规则保存 enabled 状态，成功规则按最近使用顺序去重并限制为 16 条。
- 转发工具通过当前窗口和 SSH profile 读取历史，选择历史后填充 mode/listen/target，并可独立切换 enabled；Forwarding Manager 集中显示所有服务器规则。
- profile 导出和导入脱敏会清空历史，避免传播本机 Unix socket 等端点。
- 转发管理快捷键复用 Pane 菜单动作，可在 Settings > Shortcuts 改键，旧配置占用默认键时保留原绑定。
- 转发管理支持离线保存、新建、编辑、确认删除和逐条检查。在线检查查询指定规则的监听状态及实际端口，规则修改在下一次连接时生效。
- 转发管理页通过空白处右键菜单新建规则，可从已保存的 SSH 服务器中选择。行右键和行操作菜单新建时预选该行服务器，可在表单中切换。
- 转发服务器下拉框的选项和选中项都显示侧栏的服务器图标，未知系统使用通用服务器图标。
- `ServerComboBox` 接收一个服务器选项模型，转发下拉和顶栏连接列表复用 `ServerListItem`。

## 验证

- 已完成：489 library + 578 binary + 7 integration tests、fmt、locked/offline check、严格 all-target Clippy、macOS ARM64 check/Clippy/build、718 条翻译目录检查、仓库边界、tracker 和 diff 检查。
- 已完成：快捷键变更后 490 library + 580 binary + 7 integration tests、fmt、locked/offline check、严格 Clippy、macOS ARM64 目标 check/Clippy、728 条翻译目录检查、仓库边界、tracker 和 diff 检查。
- 已完成：规则管理变更后 490 library + 581 binary + 7 integration tests、fmt、locked/offline check、严格 Clippy、macOS ARM64 目标 check/Clippy、741 条翻译目录检查、仓库边界、tracker 和 diff 检查。
- 未完成：真实 OpenSSH server 和原生 GUI 操作验收。

## 风险与阻塞

- enabled 规则会在 SSH 连接成功后自动启动，真实服务端的 forwarding policy 仍决定规则能否成功。
- GUI 视觉、键盘焦点和真实 daemon 互操作需要目标平台人工验收。

## 下一步

- 在应用内验收转发管理的快捷键、规则表单、行菜单和到真实 SSH 服务的监听状态。

## 最后更新时间

- 2026-11-27 12:03 +0800
