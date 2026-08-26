# 2026-08-26 russh 稳定性升级与发布门禁环境验证

- 项目边界：`Cargo.toml`、`Cargo.lock`、`src/ssh.rs`、`.github/workflows/{ci,release}.yml`、双语架构和项目跟踪；目标是纳入 russh 客户端安全修复，并把 Clippy、MSRV 与 RustSec 检查接入 CI/Release，不改变 SSH profile/known_hosts/凭据所有权。
- 环境记忆状态：Rust 2024、MSRV 1.92.0、Slint 1.17.1、Tokio 1、`russh 0.63.1`、`russh-sftp 2.3.0`；本机 `rustc/cargo 1.97.1`、rustfmt 1.9.0、Clippy 0.1.97 可用。`russh 0.63.1` 的 client host-key API 接收 `PublicKeyOrCertificate`。
- 运行环境：Cargo.lock 已更新并在线补齐 `russh 0.63.1` 及其新传递依赖；普通 CI 新增严格 Clippy、Rust 1.92.0 locked check 和固定到 v2.0.0 提交 SHA 的 `rustsec/audit-check`，Release 先执行 Linux fmt/check/Clippy/test、Python helper 回归和同一 RustSec audit，再进入跨平台构建。vendored softbuffer manifest 删除了指向缺失源码的旧 bench target，使全 workspace Rustfmt 可以执行；不恢复 benchmark 或增加 dev dependency。
- 变化摘要：`src/ssh.rs` 只接受 `PublicKeyOrCertificate::PublicKey`，服务端证书保持 deny-by-default；不调用证书的 `public_key()` 转换，不改变 profile fingerprint、known_hosts revoked/changed 和密码/agent/private-key 生命周期。
- 测试环境：本机先执行在线 `cargo check --locked` 以补齐离线缓存，随后复跑 `cargo check --locked --offline`、严格 Clippy、SSH focused tests、完整 Cargo 测试、YAML/Markdown/tracker 和 `git diff --check`；GitHub-hosted runner 需实际执行 RustSec advisory 数据库检查。
- 环境变化检查：是；直接 Rust 依赖、锁文件和 CI 发布 DAG 发生变化，Rust/Slint/MSRV 与 SSH trust 所有权不变。
- 开工判定：实现完成；目标平台真实 SSH certificate（当前预期拒绝）、agent、SFTP 和跨平台 Release runner 仍需手工/远端验收。

# 2026-08-23 Fontique 路径字体源环境验证

- 项目边界：`src/app/font_bridge.rs`、`src/app/runtime.rs` 及运行时字体资源；目标是移除外部 TTF 的完整 `Vec<u8>` 常驻副本，保留嵌入式 JetBrains UI 字体和 Maple Hani fallback，不改变 renderer、配置 schema、SSH trust、凭据或 transport。
- 环境记忆状态：Rust 2024、MSRV 1.92.0、Slint 1.17.1，锁定 Fontique 0.10.0 API；`Collection::load_fonts_from_paths` 使用路径源，Fontique 源缓存按需加载文件数据。
- 运行环境：本机 `rustc 1.97.1`、`cargo 1.97.1`、rustfmt 1.9.0、Clippy 0.1.97 可用；不新增依赖、不修改 `Cargo.lock`。外部自带字体继续从发行包 `assets/fonts/` 路径发现，JetBrains Mono 继续由 `include_bytes!` 提供。
- 测试环境：`cargo test --bin ax_ssh app::font_bridge --locked --offline` 通过（8 项）；完整 fmt/check/Clippy/test、431 条翻译、tracker validator 和 `git diff --check` 通过。validator 仍报告旧月度记录的历史格式问题；本轮新增记录无错误。
- 环境变化检查：是；字体加载的应用侧所有权从完整 heap buffer 改为路径列表，渲染期 Fontique/Slint glyph cache、mmap、CoreAnimation/Metal/software surface 和 allocator 的生命周期不改变。
- 开工判定：施工完成；目标 macOS 仍需同 renderer/字体负载重复 footprint、`vmmap -summary` 和 heap 采样。

# 2026-08-23 renderer 无关窗口资源释放环境验证

- 项目边界：`src/app.rs`、`src/app/window_bridge.rs`、Slint `AppWindow` model 和窗口 adapter；目标是清理应用拥有的对象，不改变 renderer 选择、SSH trust、凭据或 transport。
- 环境记忆状态：Rust 2024、MSRV 1.92.0、Slint 1.17.1、Tokio 1；`Cargo.toml`/`Cargo.lock` 和现有锁定离线命令保持不变。
- 运行环境：本机 `rustc 1.97.1`、`cargo 1.97.1`、rustfmt 1.9.0、clippy 0.1.97 可用；没有联网或新增依赖。
- 测试环境：`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`、`git diff --check` 已通过；tracker validator 只保留历史条目错误。
- 环境变化检查：否；Software/GPU 只共用应用清理路径，Slint/Fontique/CoreAnimation/Metal/allocator 的平台缓存不承诺即时 RSS 归还。
- 开工判定：施工完成；目标 macOS 的重复 footprint、`vmmap -summary`、线程数和窗口视觉仍需人工验收。

## 2026-08-23 detached model 与字体常驻边界环境验证

- 项目边界：`src/app/window_bridge.rs`、`src/app/font_bridge.rs`、detached Slint workspace；目标是删除不会显示的重复 model，并记录 bundled Fontique 字体的真实常驻边界。
- 环境记忆状态：Rust 2024、MSRV 1.92.0、Slint 1.17.1、Fontique 0.2 系列 API；不新增依赖、不改变 renderer、配置 schema、SSH trust、凭据或 transport。
- 运行环境：`assets/fonts/` 中 JetBrains Mono 四字重约 1.1 MiB、Iosevka Term 四字重约 18.8 MiB、Maple Mono NF CN 两字重约 40 MiB、Monaspace Neon Variable 约 1.6 MiB；JetBrains 仍由 `include_bytes!` 提供，其他 family 仍按选中项从资源路径读取。
- 变化摘要：detached 初始化保留 Terminal/SFTP 所需 model，sidebar/session/editor/options/font-option model 为空；Fontique shared collection 继续按 family 注册且不伪造动态卸载。
- 测试环境：本轮代码修改后执行 `cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`（库 199、应用 193、Doc tests 0）、431 条翻译检查和 `git diff --check`；tracker validator 仍只报告历史条目的格式错误，目标平台资源采样仍需用户用同 renderer/负载重复三轮。
- 开工判定：施工完成；平台字体缓存和 allocator 是否归还 RSS 仍不能从静态检查推断。

# 2026-08-22 app 聚合模块拆分环境验证

- 项目边界：`src/app.rs` 与 `src/app/` 私有应用 bridge 模块；按功能拆分 runtime、窗口恢复/actions 和平台辅助，保持 Slint 生成类型只在 app 层，且不改变 config、SSH trust、凭据、transport 或 `third_package/axshell` 边界。
- 环境记忆状态：Rust 2024、MSRV 1.92.0、Slint 1.17.1、Tokio 1；`Cargo.toml`/`Cargo.lock` 是依赖和版本事实，CI 使用 locked Cargo 命令。
- 运行环境：本机 `rustc 1.97.1`、`cargo 1.97.1`、`rustfmt 1.9.0`、`clippy 0.1.97`、Python 3.10.20 可用；本轮不联网、不新增依赖。
- 测试环境：按 `AGENTS.md` 执行 `cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`、tracker/Markdown 检查和 `git diff --check`；本轮 `cargo fmt --all`、`cargo check --locked --offline` 已通过。
- 环境变化检查：否；只移动私有 Rust 模块和更新路由文档，不改变工具链、依赖、构建入口或测试命令。
- 开工判定：施工完成；完整 Clippy、全量测试、tracker/Markdown 和差异门禁已完成，目标平台窗口行为仍待人工验收。

# 项目环境当前态

## 2026-08-22 内存与线程生命周期优化施工预检

- 项目边界：`src/app.rs`、`src/app/file_icons.rs`、`src/app/view/sftp.rs`、双语架构、环境审计和项目跟踪；不修改 Slint/Cargo 依赖版本、SSH trust、凭据、transport 或 `third_package/axshell`。
- 环境记忆状态：Rust 2024、MSRV 1.92.0、Slint 1.17.1、Tokio 1，现有 runtime 由 `Runtime::new()` 创建，文件图标 provider 为进程级 `OnceLock`，字体注册进入共享 Fontique collection 后没有可靠卸载 API。
- 运行环境：本机 Rust/Cargo 1.97.1、rustfmt 1.9.0、Clippy 0.1.97、Python 3.14.3 可用；Cargo.lock 可离线解析，未联网。
- 测试环境：本轮施工前已确认完整命令为 `cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、`cargo test --locked --offline`、翻译/Markdown/tracker 检查和 `git diff --check`。
- 可重复资源复核：macOS 目标平台应在相同负载下记录启动、打开/关闭 Settings、Terminal、SFTP 后的 `footprint -p <pid>`、`vmmap -summary <pid>` 和线程数；至少重复三轮并对照当前 footprint、peak、CoreAnimation、MALLOC_LARGE 与 Tokio/PTY 线程状态。
- 环境变化检查：是；应用拥有的 Tokio worker/blocking 线程上限与空闲回收策略将变化，平台 PTY/Fontique/CoreAnimation/allocator 生命周期保持现状并在文档中标注边界。
- 开工判定：允许开工。

## 2026-08-22 原生窗口激活刷新环境验证

- 项目边界：`Cargo.toml`、`Cargo.lock`、`src/app.rs`、`src/app/{macos_window,window_router}.rs`、`src/app/window_router/tests.rs`、双语架构/usage 和项目跟踪；只增加 Slint/AppKit 窗口激活到终端呈现路由的状态同步，不改变 SSH trust、凭据、worker 或 transport。
- 环境记忆状态：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1；`WindowActiveChanged` 事件钩子保留为快速路径，macOS 另以 `NSWindow.isKeyWindow()` 100ms UI 轮询兜底，`raw-window-handle 0.6.2` 已是现有依赖。
- 运行环境：本机 Rust/Cargo 1.97.1、rustfmt 1.9.0、Clippy 0.1.97、Python 3.14.3 可用；Cargo.lock 通过 offline 解析更新，未联网。
- 测试环境：窗口路由定向测试 1 项通过；`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 `cargo test --locked --offline`（库 199、应用 192、Doc tests 0）、431 条翻译、46 个仓库 Markdown 相对链接和 `git diff --check` 均通过。tracker validator 仍仅报告 5 条既有历史格式问题。
- 环境变化检查：是；增加 macOS AppKit 激活状态读取和 UI 定时器兜底，但不改变 renderer、工具链、配置 schema、运行时安全边界或平台构建矩阵。
- 开工判定：实现和离线门禁已完成；主窗口、detached 窗口及跨应用切换的真实激活事件和低帧率视觉仍待用户在目标平台人工验收。

## 2026-08-22 Terminal 光标闪烁设置环境验证

- 项目边界：`src/config/{settings,tests}.rs`、`src/app/{settings_bridge,view/settings}.rs`、`ui/{app,settings,settings/appearance,workspace-shell,terminal-pane,components/terminal-grid}.slint`、翻译、双语 usage/architecture 和项目跟踪；不修改 Cargo 依赖、锁文件、Rust/Slint 版本、SSH trust、凭据或 transport。
- 环境记忆状态：继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1、Cargo.lock 和既有 Settings 长 callback；`terminal_cursor_blink` 通过 serde default 向后兼容，不需要 schema bump。
- 运行环境：本机 Rust/Cargo 1.97.1、rustfmt 1.9.0、Clippy 0.1.97、Python 3.14.3 可用；无新增依赖或工具链。
- 测试环境：`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 `cargo test --locked --offline`（库 199、应用 191、Doc tests 0）、定向 config/app 测试、中文 catalog 构建/检查和 `git diff --check` 均通过；tracker validator 仅报告既有历史格式问题。
- 环境变化检查：否；仅增加 Appearance 设置和 Slint 光标呈现逻辑，不改变运行时边界或平台依赖。
- 开工判定：实现已完成；目标平台 GUI 验收仍待用户确认。

## 2026-08-17 直接日期 Tag 发布环境预检

- 项目边界：`.github/workflows/{ci,release}.yml`、删除 `create-dated-release.yml` 与 `retry-existing-release.yml`、发布双语说明、`README.md` 及项目跟踪；不修改 Rust/Slint 运行时、SSH trust、凭据、依赖、锁文件、缓存键或 Release build matrix。
- 环境记忆状态：已复核 Rust 2024、MSRV 1.92.0、锁定 `Cargo.lock`、GitHub-hosted Actions 和日期版本脚本。日期 tag 及版本元数据继续由 `scripts/release_version.py` 校验；Release 在 tag push 后直接检查 annotated tag，不再需要 tag CI dispatch/wait。
- 运行环境：保持现有 Cargo/Rust 与 GitHub Actions 运行环境；移除不再使用的 `actions/github-script@v8`，不新增依赖或 action。
- 测试环境：YAML 解析、Python release helper/Highlights 12 项回归、`release_version.py verify --tag 2026-08-17`、`cargo fmt/check/clippy/test --locked --offline`、Markdown 相对链接和 `git diff --check` 已通过；tracker validator 只报告 39 条既有历史时间字段错误。GitHub-hosted runner 在下一个有效日期 tag 上验证直接发布链路。
- 环境变化检查：是；发布者在本地同步并推送 annotated 日期 tag，tag 直接进入 Release 的校验、构建和发布，CI 只在默认分支保存共享 cache。
- 开工判定：允许开工。

## 2026-08-14 同日 Release 修订与重试环境验证

- 项目边界：`.github/workflows/{ci,create-dated-release,retry-existing-release,release}.yml`、`scripts/{release_version,generate_release_highlights}.py` 与回归、日期发布元数据及文档；不改应用运行时、依赖、缓存键、SSH trust 或凭据。
- 环境记忆状态：已复核 Rust 2024、MSRV 1.92.0、锁定 `Cargo.lock`、Python 3 发布版本/Highlights 回归、GitHub Actions workflow-dispatch 以及现有 GitHub-hosted runner。既有 `2026-08-14` tag 保持在此前 `master` 提交；当前工作树为第二个发行准备 `2026-08-14-1`，其 Cargo/lockfile/macOS 元数据为 `2026.8.14+1` / `20260814.1`。
- 运行环境：保持现有 Rust/Tokio/Slint/Cargo locked 环境；Actions 使用 GitHub-hosted runner 与 `actions/github-script@v8` 的 Node 24 运行时，不新增 Rust 或 Python 依赖。
- 测试环境：执行 Python release helper 回归、Ruby YAML 解析、版本 verify、locked/offline Cargo 门禁、tracker/Markdown/diff 检查；最后由 GitHub-hosted tag CI 和 Release workflow 验证远端权限、dispatch、平台打包与附件。
- 环境变化检查：是；Release tag 现接受可选正整数修订后缀，版本脚本统一派生 Cargo、Debian、macOS 字段；Retry Existing Release 仍没有 tag 写权限。
- 开工判定：允许开工。

## 2026-08-14 本地 PTY shutdown CI 回归施工预检

- 项目边界：`src/local_shell.rs` 的本地 PTY reader/worker/child 生命周期及其测试；不调整 Slint、应用 bridge、SSH trust、凭据或其他 transport。
- 环境记忆状态：已复核 Rust 2024、MSRV 1.92.0、锁定 `Cargo.lock`、本地 PTY 的 32 项命令/事件容量、取消 flag、child killer/process group 和有超时 join。GitHub-hosted macOS 的完整 Cargo 测试报告满事件队列 shutdown 超过 5 秒；本机首次定向测试通过。
- 运行环境：保持既有 Rust/Tokio/portable-pty 与 locked/offline Cargo 环境；不新增依赖、不改工具链或 CI 配置。
- 测试环境：先重复 `local_shell::tests::shutdown_terminates_a_running_shell_with_a_full_event_queue`，随后执行 `cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、`cargo test --locked --offline` 和 `git diff --check`。
- 环境变化检查：否；问题属于 worker-owned 取消与有界事件交付，不涉及网络、秘密、持久化或 UI 线程。
- 开工判定：允许开工。

## 2026-08-14 本地 PTY shutdown CI 回归环境验证

- 变化摘要：取消现在在每次本地 PTY event 投递前优先终止未投递事件；process group 强杀成功后直接返回，不再额外锁定并调用 child-killer 的 SIGHUP 宽限路径。
- 受影响文件：`src/local_shell.rs`、`docs/project-{implementation-tracker,env-audit}/`。
- 环境变化：无。未新增依赖、未修改 `Cargo.lock`、工具链、Slint、SSH trust、凭据或 CI 配置。
- 验证结果：7 项 `local_shell` 定向回归、满队列 shutdown 连续 20 次压力运行、`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 `cargo test --locked --offline`（库 178、应用 162、Doc tests 0）均通过。
- 人工/远端验收：需要将日期 tag 指向修复提交后手动 dispatch GitHub-hosted macOS CI；成功后才能运行 Release workflow。

## 2026-08-14 macOS 多架构 Release 施工预检

- 项目边界：`.github/workflows/release.yml` 与双语发布说明；只调整 GitHub-hosted macOS 发行包的组装及附件，不变更本地应用运行时。
- 环境记忆状态：已复核 `Cargo.toml`、`Cargo.lock`、`.github/workflows/{ci,create-dated-release,release}.yml`、`packaging/macos/Info.plist`、`packaging/macos/build-app.sh` 和当前发布文档。发布矩阵已构建 `aarch64-apple-darwin` 与 `x86_64-apple-darwin`，但仅上传 Universal bundle。
- 运行环境：保持 Rust 2024、MSRV 1.92.0、锁定 `Cargo.lock`、Python 3 标准库及现有 GitHub Actions action 版本；不新增依赖或工具链。
- 测试环境：本机执行 YAML/Shell/Markdown/tracker/diff 检查和 `cargo check --locked --offline`；`lipo`、codesign、两个 macOS target 和 GitHub Release 附件由 GitHub-hosted macOS runner 在下一个日期 tag 验证。
- 环境变化检查：否；不改 CI tag 成功门禁、target 专属 Cargo cache、日期版本脚本、应用代码、SSH trust 或凭据。
- 开工判定：允许开工。

## 2026-08-14 macOS 多架构 Release 环境验证

- 变化摘要：日期化 Release 现在发布 `macos-aarch64`、`macos-x86_64` 与 `macos-universal` 三份 app ZIP。每个原生 job 使用已构建的 target binary 和共同的本仓库 bundle 资源；Universal job 从两个原生 bundle 合成 fat executable 后重新签名。
- 受影响文件：`.github/workflows/release.yml`、`packaging/macos/build-app.sh`、`docs/development{,.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 环境变化：无。未新增依赖、未修改 `Cargo.lock`、工具链、日期 tag/CI 门禁、target 专属 cache、应用代码、SSH trust 或凭据。
- 验证结果：Ruby YAML 解析、POSIX shell 语法、Python release-version/Highlights 9 项回归、本机 arm64、x86_64 和 Universal bundle 的 `lipo`、ad-hoc codesign、ZIP round-trip 和资源检查（Universal 保留 17 个运行时字体文件），以及完整 locked/offline fmt/check/Clippy/Cargo test（库 177、应用 162、Doc tests 0）均通过。
- 人工/远端验收：下一个有效日期 tag 需在 GitHub-hosted macOS runner 验证 Intel target、Universal `lipo`、三份附件名称及目标 Apple Silicon/Intel 启动；发布仍是 ad-hoc 签名，未包含 notarization。

## 2026-08-14 终端缓冲区 resize 施工预检

- 项目边界：`src/terminal.rs` 与终端 resize 语义的双语/跟踪文档；不调整 `ui/`、PTY worker、SSH trust 或凭据。
- 环境记忆状态：已复核 `Cargo.toml`、锁定的 `alacritty_terminal 0.26.0`、现有 `terminal` 单元测试和 `.github/workflows/ci.yml`；本机 `cargo fmt`/`cargo clippy` 可用。
- 运行环境：保持 Rust 2024、MSRV 1.92.0、Slint 1.17.1、Cargo locked/offline；不新增依赖、不改 `Cargo.lock`。
- 测试环境：定向 `terminal` 回归后执行 `cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline` 和 `git diff --check`。
- 环境变化检查：否；变更仅为有界终端模型的缓冲区语义，不触及 SSH trust、凭据、网络或 UI 线程。
- 开工判定：允许开工。

## 2026-08-14 终端缓冲区 resize 环境验证

- 变化摘要：`TerminalModel` 现仅委托锁定 `alacritty_terminal::Term::resize`；主屏放大只恢复真实 scrollback，历史不足时保留内容顶部并在底部补空行。
- 受影响文件：`src/terminal.rs`、`docs/{architecture,architecture.zh,usage,usage.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 环境变化：无。未新增依赖、未修改 `Cargo.lock`、工具链、CI、Slint、PTY worker、SSH trust 或凭据边界。
- 验证结果：`cargo test --lib terminal::tests --locked --offline`（25 项）、`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 `cargo test --locked --offline`（库 173、应用 160、Doc tests 0）、tracker、相对 Markdown 链接和 `git diff --check` 均通过。
- 人工验收：不自动截图；用户仍需在目标平台确认无 scrollback 的普通 shell 纵向放大后输出位于顶部、有 scrollback 时仅显示真实历史，以及极小 pane 的裁剪和 IME 坐标。

## 项目类型

- 独立 Rust 2024 桌面应用，根 Cargo 包为 `ax_ssh`。
- 项目边界：`<repo-root>`；`third_package/axshell` 仅是参考资料，不进入构建图。

## 运行环境

- Rust edition：2024。
- MSRV：Rust 1.92.0。
- 本机工具链：`rustc 1.97.1`、`cargo 1.97.1`、`rustfmt 1.9.0`、`clippy 0.1.97`、Python 3.14.3；`cargo fmt` 与 `cargo clippy` 子命令均可用。
- UI/运行时：Slint 1.17.1、Tokio 1、russh 0.63.1、russh-sftp 2.3.0、libmudtelnet-rs 2.0.10、tokio-serial 5.5.0；`hmac 0.13.0` 与 `sha1 0.11.0` 复用锁定依赖实现 OpenSSH hashed host 匹配；Slint 1.17.1 的 `ComponentHandle::show/hide`、`Window::on_close_requested`、多个 `AppWindow` 实例和共享 `slint::run_event_loop()` 已在本轮本地 crate 源码核对并由 `cargo check` 编译；russh 已锁定版本内建 Unix/macOS `SSH_AUTH_SOCK` 和 Windows named-pipe agent client 及外部 signer API，client host-key callback 使用 `PublicKeyOrCertificate`；Tokio 启用 `process` 供有界 `xauth` 子进程调用，`rand 0.10` 是运行时依赖，用于生成单次 X11 fake cookie；macOS 已锁定的 `objc2-app-kit 0.3.2` 启用 `NSWorkspace`、`NSCell` 和 `NSColor`，以配置 detached 标题栏的 image-only 返回图标及与 Terminal 连续的 sRGB 背景；Slint 已启用 `unstable-fontique-010` 运行时字体注册，并直接依赖锁定的 `fontdb 0.23.0` 扫描系统等宽字体。
- 依赖管理：Cargo，锁文件为 `Cargo.lock`。

## 测试环境

- 单元与集成测试：Cargo 原生测试，包括 config、terminal、应用状态、本地 PTY、loopback SSH/Telnet、内存 SSH agent protocol + 外部 signer、SFTP packet/path/state 边界和 Serial descriptor 匹配。
- CI：GitHub Actions 在 Ubuntu、macOS、Windows 上运行 format、check、test、发布元数据和 Git-backed Release Highlights 测试；tag 发布 workflow 在 GitHub-hosted Windows、Ubuntu x86_64/ARM64 与 macOS runner 上构建发行包。
- 本机已安装 `cargo fmt` 与 `cargo clippy` 子命令；CI 仍是目标平台构建与 GitHub Release 的执行位置。
- Terminal IME 焦点策略：复用 `TerminalPane` 组件的 terminal identity/focused/connected/visible/divider-release 转换同步聚焦既有透明 `TextInput` proxy；仅新建 pane 在首次布局后执行一次可见、focused、connected 重验，避免重建实例在未布局时取得无效原生焦点。

## 关键命令

```bash
cargo run --locked
cargo fmt --all -- --check
cargo check --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
cargo test --locked --offline
git diff --check
```

## 外部依赖

- Slint 桌面后端需要目标平台图形环境。
- Linux CI 安装 `pkg-config`、`libfontconfig1-dev` 和 `libxkbcommon-dev`。
- 系统凭据集成测试可能触发平台授权，默认 ignored。
- SSH agent 认证在连接时读取当前运行环境：Unix/macOS 使用 `SSH_AUTH_SOCK`，Windows 使用该变量或 OpenSSH 默认 named pipe；AxSSH 最多尝试 5 个 identity，并对 agent 连接、列举、协商、签名和认证应用 30 秒总上限。自动测试使用内存 agent，不访问系统 agent；真实解锁/确认、多 identity 和失败行为需在目标平台手工验证。
- SSH known_hosts 读取平台用户的 `~/.ssh/known_hosts`（Windows 为 `%USERPROFILE%\\.ssh\\known_hosts`），限制 4 MiB/16,384 行；有效记录共享信任，unknown/changed 继续显式确认，`@revoked` 不可绕过。显式确认后的 unknown 追加、changed 替换和 revoked 移除使用 fsync/原子 rename；真实权限、系统 SSH 互操作和 revoked 修复仍需目标平台手工验证。
- Serial 实机测试依赖目标平台驱动、设备权限和硬件；自动测试不打开真实串口。
- `tokio-serial 5.5.0` 声明 MSRV 1.71；`libmudtelnet-rs 2.0.10` 声明 MSRV 1.66，均低于项目 MSRV 1.92.0。
- `libmudtelnet-rs 2.0.10` 的跨调用不完整 IAC/协商帧和转义 IAC 存在已确认边界；本项目 64 KiB 有界分帧适配由逐字节与 Telnet loopback 回归覆盖。
- `russh-sftp 2.3.0` 未声明 MSRV，且 raw client 内部使用 unbounded packet sender；项目以 MSRV/CI、单浏览 session、串行请求、256 KiB 入站 frame、250 条分页和 2,000 条/2 MiB 目录预算约束其使用。
- 真实 SFTP 服务兼容性与 GUI 文件面板需要目标环境手工验证。
- X11 forwarding 依赖目标平台可用的本机 X server。普通 SSH shell 创建只发送 forwarding request，不读取本机 `DISPLAY`、不运行 `xauth`、不探测端点且不启动 provider；远端实际打开 X11 channel 后才进行本机准备。AxSSH 从 Settings 显示 macOS bundle identifier 或 Windows `PATH`/Program Files 检测到的只读已知位置，且仅在 Custom 时接受用户提供的 executable 路径。安全默认仍要求 local-only `DISPLAY` 和可查询精确 `MIT-MAGIC-COOKIE-1` 的 `xauth`。MacXServer 和自动启动的 VcXsrv/Xming 只有在显式 no-auth 兼容下使用 loopback/`-ac`。真实 XQuartz/MacXServer、X.Org/Xwayland、VcXsrv/Xming 行为需目标平台手工验证，AxSSH 不安装软件或修改远端 `sshd_config`。
- JetBrains Mono 四个默认 TTF 由 Rust `include_bytes!` 编译进可执行文件，不经 Slint import；其余自带 TTF 作为 `assets/fonts/` 运行时资源保留在发行包。系统字体扫描依赖 `fontdb` 的预定义目录，必须在 Tokio blocking worker 中执行；各平台真实可见字体和打包后 Resources 路径须手工验收。
- 当前 Cargo 构建版本为 `2026.8.17`；公开 release tag 使用 `YYYY-MM-DD[-N]`，版本字段由 `scripts/release_version.py` 从 tag 统一派生并校验 Cargo/lockfile/macOS plist。`scripts/release_version.py` 与 `scripts/generate_release_highlights.py` 只依赖 Python 标准库；后者在已检出的 tag 历史上生成分类 Markdown，定向测试用临时 Git 仓库覆盖 tag range、去重、跟踪提交排除、同日修订比较和失败路径。发布者在默认分支用版本脚本同步并提交元数据后推送 annotated tag；Release 直接监听候选日期 tag，先验证 annotated tag 与元数据，再重建 `--release --locked` binary 并发布。CI 只在成功默认分支 run 后按 Rust target 保存共享 Cargo cache，Release 只读取对应 cache。真实 GitHub-hosted ARM、Windows 和 macOS build，以及 GitHub Release 仍需远端仓库权限和网络执行。

## 证据文件

- 2026-08-10 macOS detached 返回控件已从 58px 文字按钮改为 28px image-only 系统图标，带模板 fallback、Tooltip 与无障碍描述；完整 `cargo test --locked --offline` 通过（库 141、应用 121、Doc tests 0），`Cargo.lock` 未改变。`cargo fmt`/`cargo clippy` 子命令本机未安装，真实标题栏显示和操作需用户验收。
- 2026-08-10 一个可见 Terminal Tab 管理多 pane 的路由已通过 `cargo check --locked --offline` 重新编译完整 Slint 图和完整 `cargo test --locked --offline`（库 141、应用 121、Doc tests 0）；没有 Cargo 依赖、锁文件、工具链或 CI 契约变化。`cargo fmt`/`cargo clippy` 子命令本机未安装；目标平台 Tab/pane 可见性、焦点、点击/快捷键、detached Return 和实际 SSH/Telnet/Serial 生命周期仍需用户验收。
- 2026-08-10 Terminal pane divider 的 0.1-0.9 有界比例、嵌套 identity 和跨 Tab/detach/return 生命周期已通过定向测试及完整 `cargo test --locked --offline`（库 141、应用 124、Doc tests 0）；主窗口和 detached 窗口共用同一 Rust-owned `PaneTree` 与 Slint divider。没有环境、依赖或安全边界变化；真实拖动、键盘/无障碍操作和 PTY resize 需用户验收。
- 2026-08-10 Terminal Edit Copy/Paste/Select All 复用现有 Slint 1.17.1 `MenuItem.shortcut`、配置快捷键 parser 和主/独立窗口共用的 `TerminalPaneGroup`；没有新增 crate、配置字段、工具链、CI 或 transport 要求。直接 Rustfmt、`cargo check --locked --offline`、3 项定向测试、完整 `cargo test --locked --offline`（库 141、应用 125、Doc tests 0）、tracker validator、46 个 Markdown 相对链接和 `git diff --check` 已通过；目标平台原生菜单/焦点验收仍待用户完成。
- 2026-08-11 Terminal divider drag/focus 修复继续使用锁定 Slint 1.17.1 的稳定 `ModelRc::set_row_data` 和本地有界 revision；不新增依赖、不变更配置 schema、工具链、CI、worker 或 SSH 安全边界。`cargo check --locked --offline`、2 项 model identity 定向测试和完整 `cargo test --locked --offline`（库 141、应用 127、Doc tests 0）通过；直接 Rustfmt、tracker/Markdown/diff 门禁与目标平台 GUI 验收待本轮收口。
- 2026-08-11 detached Terminal 分屏入口复用锁定 Slint 1.17.1 的组件导出和既有 `pane-command` callback；没有新增 crate、配置字段、工具链、CI、worker 或 SSH 安全边界。`cargo check --locked --offline`、完整 `cargo test --locked --offline`、46 个 Markdown 相对链接和 `git diff --check` 通过；`cargo fmt`/`cargo clippy` 子命令仍未安装，真实 detached 工具栏可见性、点击、焦点和 PTY resize 需目标平台验收。
- 2026-08-11 工作区窗口与分屏 glyph 统一只使用锁定 Slint 1.17.1 的本地 Rectangle/color API；没有新增 crate、配置、工具链、CI、worker 或 SSH 边界变化。`cargo check --locked --offline`、完整 `cargo test --locked --offline`、46 个 Markdown 相对链接和 `git diff --check` 通过；`cargo fmt`/`cargo clippy` 子命令仍未安装，目标平台图标锐度、方向可辨性和原生 Return 协调性待用户验收。
- 2026-08-11 detached Terminal 分屏入口改为 macOS 原生标题栏 image-only 按钮；继续使用锁定 `objc2-app-kit 0.3.2`、Slint 1.17.1、Rust 2024 和 MSRV 1.92.0。未新增 crate、配置 schema、工具链、CI、worker、SSH trust 或凭据边界。直接 `rustfmt`、`cargo check --locked --offline` 和完整 `cargo test --locked --offline`（库 145、应用 134、Doc tests 0）通过；`cargo fmt`/`cargo clippy` 子命令缺失，目标 macOS 标题栏和 PTY resize 待用户验收。
- 2026-08-11 Terminal URL/路径目标打开继续使用锁定的 Rust 2024、MSRV 1.92.0、Slint 1.17.1、已有 `open` crate 与 Cargo locked/offline 门禁；未新增 crate、版本、`Cargo.toml`、`Cargo.lock`、配置 schema、工具链或 CI 变化。直接 `rustfmt`、4 项 parser、宽字符 cell 坐标和 runtime SFTP companion 定向测试、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 146、应用 139、Doc tests 0）、tracker validator、Markdown 相对链接和 `git diff --check` 通过。`cargo fmt`/`cargo clippy` 子命令仍缺失；目标平台 `Cmd/Ctrl` hover/click、实际默认 opener 和 SFTP 认证由用户验收。
- 2026-08-11 SFTP 当前路径复制按钮继续使用锁定的 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和既有系统剪贴板 callback；未新增 crate、版本、`Cargo.toml`、`Cargo.lock`、配置 schema、工具链或 CI 变化。复制只转发已发布的有界路径，不读取文件系统或接触 SSH/SFTP worker。直接 `rustfmt`、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 146、应用 139、Doc tests 0）、tracker validator、Markdown 相对链接和 `git diff --check` 通过。`cargo fmt`/`cargo clippy` 子命令仍缺失；目标平台标题栏布局、Tooltip、焦点和剪贴板结果由用户验收。
- 2026-08-12 Terminal target hover 下划线继续使用锁定的 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和已有 URL/SFTP 依赖；未新增 crate、版本、`Cargo.toml`、`Cargo.lock`、配置 schema、工具链或 CI 变化。短暂 DTO 只转发 active、row 和半开 cell 区间，完整行、目标文本、worker 和 transport 不跨 UI 边界。直接 Rustfmt、`cargo check --locked --offline`、6 项 parser/修饰键定向测试、宽字符 cell span 测试、完整 `cargo test --locked --offline`（库 147、应用 141、Doc tests 0）、tracker validator、Markdown 相对链接和 `git diff --check` 已通过；`cargo fmt`/`cargo clippy` 子命令仍缺失，目标平台 Cmd/Ctrl hover/click 由用户验收。
- 2026-08-12 Terminal 语义高亮可见性修复继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和既有终端 ANSI 色表；未新增 crate、版本、`Cargo.toml`、`Cargo.lock`、配置 schema、工具链或 CI 变化。仅私有渲染映射对有界可见默认 ASCII run 着色，真实 `TerminalModel` snapshot 回归覆盖到 Slint render run 的五类语义色；直接 Rustfmt、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 147、应用 145、Doc tests 0）通过。Cargo fmt/Clippy 子命令仍缺失，GUI 颜色与 ANSI/真彩色保留由用户验收。
- 2026-08-12 日期化发布链路使用 Rust 2024、MSRV 1.92.0、现有锁文件和 Python 3 标准库；新增 GitHub Actions target 专属 Cargo cache 复用、`YYYY-MM-DD` 到包版本映射和跨平台打包，不新增 Rust crate。`cargo fmt --all -- --check`、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 148、应用 147、Doc tests 0）、Python release-version 3 项单元测试、plist/shell/YAML 和差异检查通过。严格 Clippy 被用户工作区的 `src/ssh/x11.rs` 两项和既有测试模块排列两项 lint 阻断；真实 GitHub-hosted 构建、发布和 macOS Gatekeeper 仍待远端执行。
- 2026-08-12 macOS detached 标题栏连续性修复继续使用锁定的 `objc2-app-kit 0.3.2`，仅额外启用其已锁定的 `NSColor` feature；没有新增 crate 或 lockfile 解析变化。AppKit 仅在 UI 主线程接收 Slint `Color` 值，使用透明标题栏、sRGB window background 和系统重叠窗口 symbol；Settings 仅沿 live weak UI route 同步既有外观配置，绝不携带 worker、terminal buffer、SSH trust 或凭据。`cargo fmt --all -- --check`、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 148、应用 149、Doc tests 0）和 `git diff --check` 通过；严格 Clippy 和 tracker validator 分别被范围外既有 lint 与历史时间字段阻断。目标 macOS 标题栏材质、图标、Tooltip、Return 和主题预览同步仍待用户验收。

- `Cargo.toml`
- `Cargo.lock`
- `.github/workflows/ci.yml`
- `docs/development.md`
- `AGENTS.md`

## 本轮施工预检

- 项目边界：独立 Rust 桌面应用；本轮范围为终端尺寸共享契约（`src/terminal_dimensions.rs` 及其配置、模型、worker 消费者）和 `ui/terminal-pane.slint`、`ui/components/terminal-grid.slint` 边界。
- 环境记忆状态：已读取并与 `Cargo.toml`、`Cargo.lock`、`.github/workflows/ci.yml`、`AGENTS.md` 及当前终端源码路由复核。
- 运行环境：Rust 2024、MSRV 1.92.0、Slint 1.17.1；Cargo 及锁文件是唯一包管理与可复现构建来源。
- 测试环境：`cargo check --locked --offline`、`cargo test --locked --offline`、`cargo fmt --all -- --check`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`git diff --check`。
- 环境变化检查：否；本轮不变更依赖、工具链、CI、Cargo.lock、TerminalModel/PTY 协议语义、SSH trust 或凭据边界。
- 开工判定：允许开工；新增的是仓库内 Rust 共享常量模块和 Slint 几何消费，不引入运行时依赖。

## 本轮实施结果

- 目的：消除 TerminalPane/TerminalGrid 首帧、分屏和窗口变化时的顶部异常空带，并统一重复的终端尺寸上下限和垂直光标边界。
- 改动范围：`src/terminal_dimensions.rs`、终端配置/模型/PTY worker、`ui/terminal-pane.slint`、`ui/components/terminal-grid.slint`、设置控件/翻译与双语契约。
- 执行内容：Rust 统一 `10x3..300x100` 模型/设置契约和 `300x100` 后端最大值；保留 PTY/worker 的 `1x1` 最小入口。普通高度网格从内容区顶部开始，非完整字符格余量留在底部；低于三行保底时才使用负偏移裁剪旧顶部行；网格、预编辑层和 IME proxy 共用 pane 的单一 `cursor-cell-y`。
- 验证结果：`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、focused tests、完整 Cargo 测试、翻译检查、tracker 校验和 `git diff --check` 均已通过。
- 风险/待办：用户需要完全退出旧 AxSSH 进程并运行新构建确认正常终端顶部无异常空带、连续 resize 不再逐行右移、极小 pane 仍保留底行、鼠标/IME 坐标一致；当前未自动截图。

## 2026-08-13 连续 resize 与 SSH PTY 环境记录

- 变化摘要：共享 `TerminalSize` 现在由 Rust 模块统一模型、AppState、设置和 local/SSH/Telnet backend 的尺寸边界；`AppState::resize_terminal` 先规范化尺寸，再将同一尺寸发送给 worker 并应用本地模型。SSH 交互 PTY 请求明确启用 `OPOST`/`ONLCR`，避免远端裸 LF 造成逐行右移。
- 受影响文件：`src/terminal_dimensions.rs`、`src/terminal.rs`、`src/app/state/tabs.rs`、`src/local_shell.rs`、`src/telnet.rs`、`src/ssh.rs`、`src/ssh/tests.rs`、`src/ssh/worker.rs`、`src/ssh/worker/shell.rs`、双语架构与跟踪记录。
- 更新后的命令或环境：仍为 Rust 2024、MSRV 1.92.0、Slint 1.17.1；未新增依赖、未修改 `Cargo.lock`、SSH trust、凭据或 worker 生命周期。
- 验证结果：`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、完整 Cargo 测试（库 173、应用 160、Doc tests 0）、连续 resize/尺寸/SSH PTY focused tests、413 条翻译检查、tracker validator 和 `git diff --check` 均已通过。
- 人工验收：不自动截图；仍需用户在目标 macOS 上完全退出旧进程后确认窗口连续拖动、分屏极小尺寸、鼠标和 IME 坐标。

## 2026-08-13 Terminal 内容区边界环境记录

- 变化摘要：修复 `TerminalPane`/`TerminalGrid` 正常高度下的顶部异常空带；普通网格从内容区顶部绘制，非完整字符格余量留在底部，根 pane 移除额外顶部轨道，低于三行保底时才使用负偏移锚定底行。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和 `cargo fmt/check/clippy/test --locked --offline`；不新增依赖、不修改 `Cargo.lock`、TerminalModel、PTY、worker、SSH trust 或凭据边界。
- 验证结果：`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 Cargo 测试（库 168、应用 160、Doc tests 0）和 `git diff --check` 通过。正常窗口、分屏极小尺寸、鼠标/IME 视觉与坐标仍需目标平台用户验收；本轮未自动截图。

## 2026-08-12 Terminal Tab 焦点与窗口框线环境门禁

- 日期：2026-08-12
- 变化摘要：`TerminalPane` 使用 Slint-local 的 1ms focus pending timer，在 Tab/pane identity、visible、connected 或 focused 更新后等待当前 UI 更新完成，再确认可见、focused、connected 后聚焦透明 IME proxy；移除 pane 自身边框，`AppWindow` 在客户区绘制唯一的 `Theme.frame-border` 框线。
- 受影响文件：`ui/{app,terminal-pane}.slint`、`docs/{architecture,architecture.zh,usage,usage.zh}.md`、`docs/project-implementation-tracker/`。
- 更新后的命令或环境：保持 Rust 2024、MSRV 1.92.0、Slint 1.17.1 和 locked/offline Cargo 门禁；未新增 crate、配置 schema、工具链、CI、Rust callback、worker、SSH trust 或凭据边界。
- 验证结果：`cargo fmt --all -- --check`、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 148、应用 147、Doc tests 0）和 `git diff --check` 通过。严格 Clippy 被范围外 `src/ssh/x11.rs` 的两项 byte-slice lint 与 `src/app/{terminal_bridge,view/terminal}.rs` 的两项测试模块顺序 lint 阻断；目标平台 Tab 首次输入、分屏点击焦点和主/独立窗口框线留用户验收。

## 2026-08-23 连续窗口 resize 环境验证

- 项目边界：`src/app/terminal_bridge.rs`、`src/app/state/tabs.rs`、`src/terminal.rs`、`src/{local_shell,telnet,ssh/worker}.rs` 及 resize 路由测试；不修改 Cargo 依赖、Slint/Cargo 版本、SSH trust、凭据或 transport 选择。
- 环境记忆状态：Rust 2024、MSRV 1.92.0、Slint 1.17.1、Tokio 1、russh 0.62.2；本机 `rustc/cargo 1.97.1`、rustfmt 1.9.0、Clippy 0.1.97 可用，Cargo.lock 未改变。
- 运行环境：Slint `TerminalPane` 继续使用 16ms latest-size timer；WindowRouter 的 terminal-only dirty gate 只更新可见 pane，Serial worker 没有 PTY resize 通道。
- 测试环境：`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、完整 `cargo test --locked --offline`（库 201、应用 193、Doc tests 0）、resize focused tests、翻译检查和 `git diff --check` 通过；tracker validator 已执行但仍报告既有历史条目格式错误，本轮新增记录未引入错误；本轮 Markdown diff 未新增链接。
- 环境变化检查：否；本轮只改变应用层刷新范围、尺寸比较和 worker resize 通知去重，不增加线程、surface、依赖或持久化字段。
- 开工判定：施工完成；目标 macOS Software/GPU renderer 的实际拖动闪烁、IME/选区/焦点和真实 SSH/Telnet/Local 行列同步仍需用户验收。

## 最后确认时间

- 2026-08-23 11:30 +0800：连续窗口 resize 的 pane-only snapshot、尺寸/worker 去重和完整 locked/offline Cargo 门禁已通过；tracker validator 仍只报告旧历史条目格式错误，目标平台 GUI/真实 transport 仍待用户验收。
- 2026-08-23 09:40 +0800：MEM5 detached model 与字体审计已通过 locked/offline fmt/check/Clippy/test、翻译和差异门禁；目标平台仍需验证 Software/GPU 窗口关闭后 footprint、vmmap 和线程回收，平台 allocator/Fontique/CoreAnimation/Metal RSS 不作即时归还承诺。
- 2026-08-22 19:10 +0800：内存/线程生命周期实现和完整离线 Cargo 门禁已完成；目标平台重复 `footprint`/`vmmap -summary` 采样、GUI 和真实 transport 仍待用户验收。
- 2026-08-17 14:28 +0800：Release 现在仅由外部推送的 annotated 日期 tag 直接启动；Create/Retry、tag CI dispatch/wait 与 GitHub Script action 已删除。两个 YAML、现有 tag/元数据、12 项 Python、413 条翻译、fmt/check/严格 Clippy、完整 Cargo 测试（库 179、应用 167、Doc tests 0）、Markdown 相对链接和差异检查通过；tracker validator 仅保留 39 条既有历史时间字段错误。下一枚新 tag 仍需 GitHub-hosted 平台/发布验收。
- 2026-08-15 17:38 CST：SFTP 远端文件行右键菜单继续使用 Rust 2024、MSRV 1.92.0、锁定 Slint 1.17.1 和既有 `FlatActionMenu`/选择/下载/删除 callbacks；未新增 crate、配置 schema、工具链、CI、worker、SSH trust、凭据或本地文件删除能力。fmt、locked/offline check、严格 Clippy、完整测试（库 179、应用 172、Doc tests 0）、413 条中文目录、tracker、Markdown 相对链接和差异检查通过；真实右键菜单视觉与手感留用户验收。
- 2026-08-12 09:00 CST
- 2026-08-12 21:13 +0800：复核 release helper 扩展仍只使用 Python 标准库和 Git；CI 额外运行 Git-backed Highlights 回归，Rust 依赖、工具链、构建矩阵、日期 tag、CI 门禁和发行包内容未改变。
- 2026-08-12 22:20 +0800：本地 SFTP 只读打开的目录快照/打开重验从平台文件 identity 强化为 identity 加长度、修改时间和创建时间 fingerprint；未改 Rust 依赖、工具链、CI、SSH trust 或凭据。Linux 和 Windows CI 证明快速同长度写入可保留相同可查询时间字段，因此 fingerprint 只能拒绝可观察到的变化，不能作为原地内容完整性证明。
- 2026-08-13：SFTP 写操作继续使用 Rust 2024、MSRV 1.92.0、Slint 1.17.1、Tokio/russh-sftp 与既有 locked/offline 命令；未新增依赖、锁文件、工具链、CI、配置 schema、SSH trust 或凭据。worker 限制文本 4 MiB、上传 512 MiB，上传先写私有远端临时文件再 rename；自动上传/监控默认关闭。真实权限、拖拽和 GUI 仍待目标平台验收。

## 2026-08-13 SSH known_hosts 兼容环境记录

- 变化摘要：新增直接锁定依赖 `hmac 0.13.0`/`sha1 0.11.0`，实现 bounded OpenSSH known_hosts 解析、shared trust、`@revoked` 拒绝和原子记录管理；不改变 Rust 2024、MSRV、Slint/Tokio/russh 版本。
- 更新后的命令或环境：继续使用 `cargo fmt/check/clippy/test --locked --offline`、翻译、tracker 和 diff 门禁；系统文件路径与权限依赖目标平台。
- 验证结果：known_hosts 定向 4 项、完整 Cargo 测试（库 167、应用 160、Doc tests 0）、locked check、严格 Clippy、翻译、tracker 和 diff 检查通过。真实 SSH 工具互操作、系统权限和 GUI revoked/changed 呈现仍需用户验收。

## 2026-08-12 界面语言环境记录

- 日期：2026-08-12
- 变化摘要：新增直接依赖 `sys-locale 0.3.2`，并使用锁定 Slint 1.17.1 的 bundled translations 接口内嵌 `zh-CN` PO 目录；配置 schema 升至 v21。
- 受影响文件：`Cargo.{toml,lock}`、`build.rs`、`src/config/`、`src/app/`、`ui/`、`translations/`、翻译检查脚本和双语文档。
- 更新后的命令或环境：继续使用 Rust 2024、MSRV 1.92.0 和 locked/offline Cargo 门禁；`sys-locale` 已存在本地锁定依赖缓存，目录额外使用 Python 3 检查，并在可用时执行 `msgfmt --check --check-format`。
- 验证结果：`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整测试（库 150、应用 152、Doc tests 0）、386 条目录覆盖/占位符检查、`msgfmt`、Python 编译、46 项 Markdown 相对链接和差异检查通过。新增请求代次回归证明迟到语言保存不能覆盖最新选择；tracker validator 只剩 16 条旧月度记录时间字段错误。目标平台 locale 检测、即时切换和多窗口视觉由用户验收。

## 2026-08-23 dirty-region backend patch 环境验证

- 项目边界：`vendor/i-slint-backend-winit/`、`vendor/softbuffer/`、`Cargo.toml`/`Cargo.lock` 和 backend 文档；不修改 Slint UI 业务组件、terminal parser/worker、SSH trust、凭据或 transport。
- 环境记忆状态：Rust 2024、MSRV 1.92.0、锁定 Slint 1.17.1、softbuffer 0.4.8；本机 rustc/cargo 1.97.1，Cargo 可离线解析本地 patch。两个 vendor 目录保留上游许可和最小行为差异。
- 运行环境：winit software renderer 将每个 Slint dirty rectangle 转换为 `softbuffer::Rect`；macOS CoreGraphics surface 使用持久物理像素 buffer，并通过固定 256×128 物理像素的 child-layer tile 提交，只有 damage 相交 tile 创建新的独立 `CGImage`。
- 测试环境：`cargo fmt --all -- --check`、vendor backend rustfmt、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、完整 `cargo test --locked --offline`（库 202、应用 197、Doc tests 0）、`cargo build --locked --offline`、中文 catalog/check 和 `git diff --check` 通过。仓库未提供 `scripts/validate_tracking_docs.py`，该命令未执行。
- 环境变化检查：是；新增两个本地 crates.io patch 和 macOS 持久 framebuffer/tiled surface 行为，非 macOS backend dispatch 未改行为；升级 Slint/softbuffer 必须重新核对 damage、buffer age 和 layer 坐标契约。
- 开工判定：代码施工完成；目标 macOS Software renderer 的持续输出、resize、窗口隐藏/恢复、Retina DPI、光标/选区/IME 和同负载 sample/A-B 仍需人工验收。
