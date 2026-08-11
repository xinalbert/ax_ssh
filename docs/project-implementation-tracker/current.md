# 当前项目实施记录

## 当前目标

- 目标 ID：20260811-sftp-profile-default-directories
- 目标：在新建和编辑 SSH 连接时配置 SFTP 远端与本地默认目录，并在新 SFTP Tab 初始化时使用它们。
- 交付物：schema v19 兼容 profile 字段、Slint 编辑器映射、worker/state 初始化、回归测试和双语契约。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/config/{session,tests}.rs` 的非秘密 SSH profile 字段，`src/app/{state,view,workspace}/` 的编辑器与 SFTP Tab 映射，`src/ssh/worker{,.rs/sftp.rs}` 的初始远端目录，`ui/{app,workspace-shell,session-editor}.slint` 的 callback 契约，以及配对文档/tracker。
- 不在本轮范围内：目录选择器、本地/远端目录持久化以外的浏览状态、上传或文件修改、凭据、SSH host-key trust、依赖/工具链升级和 `third_package/axshell`。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| SFTPPATH1 | completed | SSH profile schema v19、缺失字段回退与有界校验 | config unit tests | 远端缺失/空值回退 `~`；本地空值代表平台 home。 |
| SFTPPATH2 | completed | 新建/编辑连接的 Slint DTO、草稿与保存 callback | Cargo check | 仅 SSH 显示两个目录字段，秘密字段与 trust 流程不变。 |
| SFTPPATH3 | completed | 新 SFTP Tab 的 local snapshot 与 worker remote browser 初始化 | state/worker tests | 默认值仅在 Tab 创建时应用，之后导航仍是 Tab-local。 |
| SFTPPATH4 | completed | 双语契约、项目地图与完整离线门禁 | fmt/check/clippy/test/tracker/diff | 真实目录可达性和 GUI 布局留目标平台验收。 |

## 已完成

- 新增 SSH profile `sftp_remote_path` 与 `sftp_local_path`；两者均为非秘密数据，限制控制字符和长度，schema 升至 v19。
- 新建/编辑 SSH 连接显示 **SFTP directories**，保存路径会在下一次新建 SFTP Tab 时使用。
- SFTP worker 收到 profile 的远端初始路径；`AppState` 以 profile 本地路径建立本地浏览器 snapshot。旧 profile 保持 `~`/平台 home 回退。

## 验证

- 已完成：`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`（库 145、应用 134、Doc tests 0）、18 个 Markdown 相对链接目标和 `git diff --check`。
- 进行中：无。
- 未完成：目标平台确认新建/编辑 SSH 连接的字段布局，以及可访问远端/本地目录在首次打开 SFTP Tab 时显示正确内容。

## 风险与阻塞

- 路径绝不进入凭据、日志或 host-key 信任数据；新路径不会改变已有 Tab、SSH transport 或凭据生命周期。
- 保存时只作有界文本校验，不探测目录可达性；失败仍由既有 SFTP 或本地目录浏览错误处理显示。
- tracker validator 仍报告月度历史中 7 条既有缺失/无效时间或状态转换格式；本轮记录未新增该类错误。

## 下一步

- 请用户在目标平台验证新建/编辑 SSH 连接的 SFTP 目录字段，以及新 SFTP Tab 的实际初始目录。

## 2026-08-11 本地文件打开 TOCTOU 修复

- 时间：2026-08-11 16:28 +0800
- 触发原因：用户要求修复 SFTP 本地栏在路径验证与 detached opener 之间的文件替换竞态。
- 执行内容：列目录时保存 Unix dev/inode 或 Windows volume/file index；打开时核对只读 handle identity，并从该 handle 复制到现有 512 MiB 上限、私有权限、quota 与原子发布的 SFTP-open cache，平台 opener 只接收已发布快照路径。
- 影响文件：`src/app/{local_files,sftp_bridge}.rs`、`src/sftp{,/transfer,/transfer/cache}.rs`、双语架构/使用/开发文档和项目跟踪记录。
- 安全边界：不改变 SSH host-key、凭据或 transport；不按验证后的源路径重开，路径后续被替换也不能重定向实际打开内容。
- 验证结果：本地 identity/替换拒绝 6 项与 SFTP transfer/cache 18 项定向测试通过；`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、沙箱外完整离线测试（库 143、应用 133、Doc tests 0）和 `git diff --check` 通过。当前环境没有 `rustup`/Windows target，Windows identity 分支留给三平台 CI 类型检查；真实默认程序打开由用户验收。

## 最后更新时间

- 2026-08-11 18:00 +0800
