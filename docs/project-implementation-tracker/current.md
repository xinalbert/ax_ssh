# 当前项目实施记录

## 当前目标

- 目标 ID：20260806-sftp-icons-local-open
- 目标：为 SFTP 双栏列表实现跨平台文件图标、本地文件默认程序打开，以及远端文件只读分块下载后打开；保持 UI、Tokio、russh 和本地文件安全边界。
- 交付物：图标 provider/cache、Slint DTO 与列表渲染、snapshot 重验的本地打开、有界远端下载/进度/取消/私有缓存清理、安全测试、双语文档与完整验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`Cargo.toml`、`Cargo.lock`、`README.md`、`README.zh.md`、`src/{app,sftp,ssh}.rs`、`src/{app,sftp,ssh}/`、`ui/{app,workspace-shell,sftp-pane}.slint`、`docs/{architecture,architecture.zh,usage,usage.zh,development,development.zh}.md`、`THIRD_PARTY_NOTICES.md` 和本目标的研究/跟踪记录。
- 不在本轮范围内：远端上传、显式另存为、受管编辑、文件变化监听、冲突处理、删除/重命名、文件预览、拖放传输，以及任何 `third_package/axshell` 构建耦合。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：已结束

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| P1 | completed | 锁定图标 API、依赖 feature、缓存格式和远端只读打开契约 | 研究来源、锁定 API、Cargo.lock 和平台 cfg 代码核对 | macOS 现代 UTType API 作为首选，旧 API 仅 fallback。 |
| P2 | completed | 私有 `file_icons` provider：目录/链接/通用/扩展名 key、平台 resolver、预热和内存缓存 | provider 单测、缓存 identity/LRU/预热上限和 fallback 测试 | 24x24 RGBA；128 项缓存、每批最多 64 个 key。 |
| P3 | completed | `SftpEntryRow` 图标 DTO 与远端/本地列表渲染映射 | Slint/Cargo 联合编译、row mapping 和稳定尺寸审阅 | UI 只消费有界 owned image，不接触平台对象。 |
| P4 | completed | 本地 regular file 双击 intent、snapshot/path 重验证和 detached opener | 目录/regular file/symlink/路径替换 focused tests | 只允许当前活动 SFTP Tab 的本地目录 snapshot。 |
| P5 | completed | 远端只读下载 domain：metadata/大小上限、chunked read、私有 cache、fsync/rename | loopback 下载、截断、取消、权限、路径安全和缓存清理 tests | 512 MiB、64 KiB chunk、`.part` 发布和启动清理边界已实现。 |
| P6 | completed | SSH worker 独立 SFTP subsystem transfer task、并发/有界事件、取消和 Tab 关闭回收 | pending opening 取消、并发上限、transfer shutdown/join focused tests | 每 Tab 最多 2 个活动 transfer，pending opening 同样计数。 |
| P7 | completed | Transfers 状态 DTO、进度/取消/错误 UI 与远端双击接线 | Slint ABI、状态机和 opener 失败不进入 Completed 测试 | 首版只提供 read-only download-to-open。 |
| P8 | completed | cache 启动清理、权限/symlink 防护和跨平台文件生命周期测试 | 4096 扫描上限、Unix mode、symlink namespace 和外部占用 best-effort 审阅 | Windows ACL、真实平台 opener 和文件占用仍需目标机验收。 |
| P9 | completed | 双语架构/使用/开发文档、依赖许可说明、三平台人工验收和全量门禁 | fmt/check/test/tracker/Markdown/diff 已运行；严格 Clippy 记录 baseline 阻塞 | GUI、默认程序关联、图标外观和真实 SSH/SFTP 兼容性留给用户验收。 |

## 已完成

- 完成 24x24 平台文件图标 provider/cache：macOS NSWorkspace/UTType、Windows Shell/GDI、Linux MIME/freedesktop，并提供 folder/symlink/generic fallback。
- 完成本地 regular file 双击：活动 SFTP Tab snapshot 命中、目录/符号链接拒绝、blocking 重验 canonical parent 后调用 `open::that_detached`。
- 完成远端 regular file 只读下载后打开：独立 SFTP subsystem、有界分块/事件/并发、512 MiB 上限、私有缓存、fsync/atomic rename、取消和失败清理。
- 完成 pending subsystem opening 的取消、Tab shutdown join、transfer 状态/进度/取消 UI，以及启动缓存清理（最多 4096 项）。
- 完成 SFTP 浏览器任务的可移动 shutdown/drop 生命周期；取消命令只有成功入 worker 队列后才进入 `Cancelling`，迟到完成事件不会绕过状态机启动 opener；图标预热改为去重、有界 pending 队列和单 worker 合并调度。
- Windows Shell 图标 resolver 增加 COM apartment 初始化处理（含 `RPC_E_CHANGED_MODE`）以及 `SelectObject`/GDI restore 错误检查；恢复失败时避免释放仍可能选入 DC 的 bitmap。
- 完成中英文 architecture/usage/development 和第三方声明同步；保持 `third_package/axshell` 不在 build graph。
- 修复 macOS 工作区 Tab model 重建后 Settings 菜单再次消失的问题：工作区刷新后重新绑定，AppKit bridge 扫描当前应用 submenu/About 标题，兼容 `Settings...` 与 Unicode 省略号，并在菜单尚未就绪时有限重试。

## 验证

- 已完成：`cargo fmt --all -- --check`、`cargo check --locked --offline`、focused SFTP/图标/本地打开/worker 测试、`cargo test --locked --offline`（库 137、应用 100、Doc tests 0）、tracker validator、Markdown 相对链接检查和 `git diff --check`。
- 已完成：锁文件包含新增平台依赖；未改变 Rust edition `2024` 或 MSRV `1.92.0`；host-key 仍 deny-by-default；未向 Slint 暴露 russh handle、Tokio receiver 或 `RawSftpSession`。
- 未完成：严格 `cargo clippy --all-targets --locked --offline -- -D warnings` 仍被仓库既有 baseline lint 阻塞（`derivable_impls`、`collapsible_if`、`too_many_arguments`、`redundant_closure`、`single_match`、`field_reassign_with_default` 等）；本轮未扩大为无关重构。
- 未完成：macOS/Windows/Linux 实机的 Shell 图标主题、默认程序关联、真实 SFTP 服务器兼容性、焦点/双击/布局和文件占用行为需要用户目标平台人工验收。

## 风险与阻塞

- 平台图标 API、桌面主题和默认程序由目标操作系统决定；Linux freedesktop 主题、Windows Shell DPI/overlay 和 macOS AppKit availability 不能在当前 macOS-only 环境完全验证。
- 外部程序可能长期持有已发布缓存文件；AxSSH 只在后续启动中 best-effort 清理，不强制终止外部进程或覆盖其文件。
- 严格 Clippy 的失败来自既有基线规则；若要清零需另立 lint 清理目标，避免把无关重构混入本轮。

## 下一步

- 由用户在目标平台验收图标、双击默认程序、真实 SSH/SFTP、取消和关闭 Tab 行为，并反馈截图或具体错误。
- 单独建立 lint baseline 清理任务，再决定是否将当前既有 Clippy 问题纳入主门禁。
- 后续功能另立目标：上传、显式另存为、受管编辑/自动回传、冲突、删除/重命名、拖放和修改监听。

## 最后更新时间

- 2026-08-06 17:05 +0800
