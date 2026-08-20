[English](development.md) · [文档导航](README.zh.md)

# 开发说明

## 环境要求

- Rust `1.92.0` 或更高版本
- Cargo
- Slint winit 后端支持的桌面环境
- 实机 Serial 测试所需的目标平台驱动和设备权限

根 Cargo 隐式 workspace 只包含 `ax_ssh` package。`third_package/axshell` 是
参考子模块，不是 workspace member 或构建依赖。

## 常用命令

```bash
cargo run --locked
cargo fmt --all -- --check
cargo check --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
cargo test --locked --offline
git diff --check
```

离线命令要求本机 Cargo 缓存中已有 `Cargo.lock` 锁定的依赖；需要从 registry 填充
缓存时移除 `--offline`。

## Renderer 选择

AxSSH 同时启用 Slint 的 Skia 和 software renderer。**Settings > Appearance >
Renderer** 会为下一次启动保存 Automatic、GPU 或 Software。Automatic 在 macOS 使用
`winit-skia`，底层走 Metal，并保留 Slint 的 softbuffer 回退；Windows 和 Linux 使用
`winit-software`，保持现有平台行为。GPU 在受支持桌面平台选择 `winit-skia`，Software 选择
`winit-software`。设置会在首个 `AppWindow` 创建前读取，因此仅在重启后生效。

`SLINT_BACKEND` 的优先级高于已保存偏好，可显式选择 renderer；例如在 macOS 采集对照 sample 时
强制走 software：

```bash
SLINT_BACKEND=winit-software cargo run --locked
```

环境变量和已保存偏好会在首次创建 `AppWindow` 前生效，因此 renderer 初始化失败会在启动阶段直接报告。

开发 profile 禁用 rustc 增量代码生成。AxSSH 较大的 Slint 生成应用单元在 macOS 反复构建后，
可能累积互不兼容的 code-generation 对象，并在最终 arm64 链接时报告内部
`_anon...llvm...` 符号缺失。依赖产物仍会缓存，release profile 继续使用既有 ThinLTO 设置。
旧工作区若已有受影响产物，只清理本 package 的开发输出：
`cargo clean --profile dev --package ax_ssh`。

## 修改规则

- Slint 生成类型集中在 `src/app.rs`；领域模块和传输模块不得依赖 UI。
- 不把密码写入 JSON；`src/credentials.rs` 只能按 profile 通过平台系统凭据库读写一份
  密码，并且只向 SSH worker 返回临时 secret。
- 私钥 profile 只能持久化文件路径；私钥内容和 passphrase 必须在 UI 线程外加载，
  且不得记录或持久化。
- SSH agent profile 只能持久化认证方式。运行时 agent client 必须由 worker 独占，只能在精确
  主机密钥校验后打开，保留 5 个 identity/30 秒上限并在认证后释放；socket 路径、identity 注释
  和密钥数据均不得持久化或记录。agent forwarding 与密钥管理不属于该边界。
- 不为了方便而接受未知 SSH 主机密钥；测试应注入确定性的 trust policy。
- 进程持有的日志 guard 必须存活到应用退出，以刷新有界非阻塞队列；不得记录凭据
  或终端内容。
- UI 边界上的 payload 必须是有上限的自有数据；不得把 russh channel 或 Tokio
  receiver 暴露给 Slint。
- 运行实例必须使用终端 Tab UUID，而不是已保存的 profile UUID；已保存连接的输入、
  resize、输出、重试、关闭和迟到事件都按 `tab_id + profile_id + attempt_id` 路由。
- detached 原生窗口只是视图 owner，不是 transport owner。SSH Terminal/SFTP companion
  必须使用只含受限 UUID 的 `WorkspaceTransfer` 移动；snapshot 由 `WindowRouter` 按窗口路由，
  russh handle、receiver、终端缓冲区和秘密仍留在 `AppState`/worker。返回或关闭 detached 窗口
  只能移除路由，不得重连或关闭已转移的 worker。其客户区只能渲染活动 Terminal/SFTP 内容；
  detached 原生标题显示连接名，macOS 同一行标题栏的纯图标返回按钮通过 Tooltip/无障碍描述说明
  用途，并调用既有路由 handler。主窗口
  Tab 内联动作必须把 Tab UUID 直接传入该 handler，不得依赖多个 callback 的调用顺序。
- 终端输入、输出批次、事件队列和 scrollback 都必须有上限。
- SFTP 必须使用已认证 SSH worker 的子 subsystem channel；不得把 russh handle 或
  `RawSftpSession` 暴露给应用状态或 Slint。SFTP-only Tab 不得申请 PTY 或交互 shell，但
  必须保留与终端 Tab 相同的主机密钥和凭据门禁。增加文件操作时必须保留入站 packet、路径/名称、
  分页、目录预算、请求和 shutdown 上限。只读 download-to-open 必须保持 512 MiB 文件上限、
  64 KiB 请求 chunk、有界 writer/event queue、逐操作与总超时、逐 Tab 并发上限、取消、私有缓存
  发布和 owned join。未来上传/删除/重命名/编辑必须另行建立确认、冲突与修改测试。
- SFTP 本地文件栏是只读 application bridge 快照。目录读取必须放在有界 blocking task 中，且只返回
  名称、路径、类型、大小和修改时间元数据；Slint 不得访问文件系统。结果回到 UI 前按 Tab 和请求 identity
  丢弃过期项，并保留条目、名称和路径上限。本地打开意图必须命中当前活动 Tab 快照，并在 blocking
  worker 上打开非 symlink regular file handle，将其平台 identity 和长度、修改时间、创建时间
  fingerprint 与列目录快照核对，再从该 handle 复制到有界私有缓存后调用 detached 平台 opener。该
  fingerprint 只能检测当前平台可观察到的变化，不能作为内容完整性保证；调度时不得重新打开已验证的
  源路径。
- SSH profile 的 `sftp_remote_path` 与 `sftp_local_path` 只是非敏感初始化输入，不得进入凭据、日志或运行中
  Tab 的 mutation；持久化前校验有界文本，远端值交给 worker-owned browser，本地值只用于初始化
  application snapshot。旧 profile 的空值必须继续使用 `~`/平台 home 默认值。
- 文件图标平台 API、主题检测、文件读取和图片解码都必须留在 `src/app/file_icons.rs` 的 blocking
  工作中；Slint 只能接收进程内缓存里的有界自有 RGBA 图片。远端名称只作为扩展名/类型提示，绝不
  当成本机路径；每个平台 resolver 都必须保留内建 fallback 并确定性释放 native handle。
- Telnet 必须明确标记为明文；IAC 协商必须在进入终端前解析和过滤，拒绝不支持的选项，
  只有对端接受后才发送 NAWS。Telnet profile 不得新增凭据持久化。
- Serial 枚举必须只读取元数据并在 UI 线程外运行；自动发现期间绝不打开或探测设备，
  只有用户明确连接后才能解析已保存身份并创建设备 worker。Serial resize 只改变本地终端网格。
- `vendor/vt100` 是锁定 `vt100 0.16.2` 宽字符缩窄问题的最小本地补丁。保留其中的 MIT
  文件；只可在有回归测试时调整已说明的 resize 路径，并在上游发布对应修复后移除该补丁。
- `src/terminal/input.rs` 不得依赖 Slint 键值；在 `src/app.rs` 完成映射，并在不构造
  窗口的条件下测试普通/application-cursor 终端字节序列；平台可打印键后备转换归
  Slint bridge 所有。
- 所有平台都要把未分配的 Ctrl 组合留给获得焦点的 PTY，包括 `Ctrl+C` 和 tmux 前缀；
  终端剪贴板默认键在 macOS 使用 `Cmd`，其他平台使用 `Ctrl+Shift`。全局 UI 命令使用
  Slint 在终端输入前处理的原生菜单 accelerator；不要把工作流需要的终端控制组合分配给
  UI 命令。Slint 1.17 会在
  Apple 平台交换 Command/Control 修饰键字段。处理 macOS 键盘事件时，快捷键匹配或
  终端编码前必须在 `src/app.rs` 读取 AppKit 当前物理修饰键状态，避免 Slint 漏掉某一侧
  `flagsChanged` 时左右 Control 语义不一致。
- 持久化菜单快捷键只在 `src/app/input.rs` 转换为 `slint::Keys`；Apple 上 `Cmd` 映射为
  Slint `Control`，物理 `Ctrl` 映射为 Slint `Meta`。菜单 diagnostics 只记录固定 action
  ID；`MenuItem.activated` 无法区分鼠标点击与 accelerator。
- 可见终端保持为渲染网格。隐藏的 Slint `TextInput` 只充当 IME 代理：跟随终端光标
  定位，把未修饰的预编辑按键留给输入法，并确保提交文本只发送一次。
- 本地 PTY 的 child、reader、writer、取消和 join 所有权保留在 `src/local_shell.rs`，
  不得把阻塞式 PTY 操作移到 UI 线程。
- macOS 必须关闭整窗背景拖动；只有零 Tab 空白条或最右侧专用留白的鼠标左键 down
  可以调用 UI 线程原生拖动 callback，Tab、Activity Bar、侧栏和终端不得成为拖动区域。
- 自带字体必须放在 `assets/fonts/`，并保留独立许可证和声明。JetBrains Mono 四个字重会编译进
  可执行文件，保证应用和 Terminal 默认字体始终可用；Maple Mono NF CN 是唯一汉字回退，必须作为
  运行时资源随包发布，Iosevka Term 和 Monaspace Neon 是可选主字体。发行包必须把字体目录保留在
  可执行文件旁或 `src/app/font_bridge.rs` 解析的平台资源路径中。外部文件读取必须在 Tokio blocking task，
  所有字体字节只能通过 `FontRegistry` 在 Slint UI 线程注册；不得新增渲染器侧回退路径，构建或运行时不得从
  `third_package/axshell` 加载静态资源。
- `assets/ion/terminal_icon.svg` 是应用图标的唯一源文件。PNG、ICO 和 ICNS 必须作为一组从该
  SVG 重新生成。Slint 窗口使用 256px PNG；Windows 通过
  `packaging/windows/axssh.rc` 嵌入 ICO；macOS 通过
  `packaging/macos/Info.plist` 打包 ICNS；Linux 随 desktop entry 安装 hicolor PNG 图标集。
  不得替换或加载参考工程中的图标。
- 顶层菜单可访问的 About 页面必须保留 `AboutSlint`。AxSSH 选择 Slint 的
  `GPL-3.0-only` 许可选项，标准组件直接提供工具包署名。About 的支持操作复用现有
  AppWindow bridge：打开 issue tracker 或日志目录，或复制非敏感构建元数据；不得上传日志，
  不得暴露 profile、主机、路径或秘密字段。
- 修改面向用户的文档时同步维护中英文页面。

## 平台打包

使用以下命令生成 macOS application bundle：

```bash
packaging/macos/build-app.sh
```

### 在 macOS 上交叉编译 Windows

仓库的 Windows CI 和发布目标是 `x86_64-pc-windows-msvc`。依赖
`aws-lc-sys` 在构建 Windows 汇编时还需要 NASM。MSVC 交叉链接还需要完整的
Homebrew LLVM 工具链：`llvm-lib` 由 LLVM 提供，`lld-link` 由独立的 `lld` formula
提供。在 macOS 上先安装这些工具、Rust target 和 `cargo-xwin`（只需执行一次）：

```bash
brew install nasm llvm lld
rustup target add x86_64-pc-windows-msvc
cargo install cargo-xwin --locked
```

Homebrew 的 LLVM 和 LLD 是 keg-only，不会自动加入 PATH。构建前把它们的工具目录
放在 PATH 前面；下面的 `brew --prefix` 写法同时适用于 Apple Silicon 和 Intel macOS：

```bash
export PATH="$(brew --prefix llvm)/bin:$(brew --prefix lld)/bin:$PATH"
```

在仓库根目录构建 Windows release 二进制：

```bash
cargo xwin build --release --locked --target x86_64-pc-windows-msvc
```

第一次执行 `cargo xwin` 可能需要下载 Windows SDK/CRT 文件，因此需要网络连接。
如果出现 `NASM command not found`、`llvm-lib not found` 或 `lld-link not found`，先安装上面的
工具并导出 PATH，再重新执行同一条构建命令。生成的可执行文件位于：

```text
target/x86_64-pc-windows-msvc/release/ax_ssh.exe
```

将运行时字体资源和许可证声明一起打成便携 ZIP：

```bash
stage="AxSSH-windows-x86_64"
rm -rf "$stage" "$stage.zip"
mkdir -p "$stage/assets/fonts"
cp target/x86_64-pc-windows-msvc/release/ax_ssh.exe "$stage/AxSSH.exe"
cp -R assets/fonts/. "$stage/assets/fonts/"
cp LICENSE THIRD_PARTY_NOTICES.md "$stage/"
ditto -c -k --sequesterRsrc --keepParent "$stage" "$stage.zip"
```

把生成的 ZIP 复制到 Windows 主机后解压为目录。必须保持 `assets/fonts/` 位于
`AxSSH.exe` 旁边：Maple Mono NF CN 用于唯一、确定的汉字回退，Iosevka Term 和 Monaspace Neon 仍可选为
主字体。单独测试可执行文件时，内嵌的 JetBrains Mono 仍然可用，但 Terminal 中文回退仍需要 Maple 运行时文件。
交叉编译的二进制仍需在
Windows 上手工验收 ConPTY、原生窗口行为、凭据和真实 SSH 连接。

Windows 的普通 Cargo 构建会经 `build.rs` 嵌入可执行文件资源。Linux 的 `cargo deb` 会读取
`[package.metadata.deb]`，安装 desktop entry、可执行文件、各级 hicolor 图标、`LICENSE` 和
`THIRD_PARTY_NOTICES.md`。macOS bundle 把两份声明放在 `Contents/Resources`；Windows 发行包
必须把它们放在可执行文件旁或安装器文档中。替换图标后，平台 shell 可能需要刷新缓存才会显示新图标。

## GitHub 发布

仓库使用按日期发布。当天首个公开 tag 使用 `YYYY-MM-DD`，当天后续发行使用正整数修订后缀，
例如 `YYYY-MM-DD-1`。首发映射到 Cargo/Debian 的 `YYYY.M.D` 和 macOS build 的
`YYYYMMDD`；修订示例映射到 Cargo `YYYY.M.D+1`、Debian `YYYY.M.D-1` 与 macOS build
`YYYYMMDD.1`，macOS short version 仍为 `YYYY.M.D`。在默认分支使用日期 tag 同步已提交的
发行元数据，再创建并推送 annotated tag：

```bash
python3 scripts/release_version.py sync --tag 2026-08-12
git add Cargo.toml Cargo.lock packaging/macos/Info.plist
git commit -m "Release 2026-08-12"
git tag -a 2026-08-12 -m "AxSSH 2026-08-12"
git push
git push origin 2026-08-12
```

直接 push 有效的 annotated `YYYY-MM-DD[-N]` tag 会启动 Release workflow。它会先校验 tag
和已提交的版本元数据，再构建并生成：

- Windows x86_64 ZIP，包含可执行文件、自带字体和许可证声明
- Linux x86_64 与 aarch64 TAR.GZ，以及对应的 `.deb`
- macOS Apple Silicon（`macos-aarch64`）、Intel（`macos-x86_64`）和从两个原生
  二进制合并的通用 `.app` ZIP；三种 bundle 都包含相同的图标、运行时字体和许可证声明

CI 只在默认分支成功后写入共享 Cargo cache，失败、PR 和 tag job 不会写入；发布 job 只恢复
该 cache，不会写回。缓存键包含 target triple、Rust 版本和 `Cargo.lock` 指纹，所以锁文件变更
或架构不同都不会复用不兼容的缓存。
发布仍会重新执行 `--release --locked` 编译，绝不把 CI 的 check 或 debug 产物作为发行物。

构建前会校验推送的 tag 确实是 annotated release tag，并确认 Cargo package、锁文件和 macOS bundle
元数据一致。不存在 Create 或 Retry workflow、tag CI dispatch 或轮询链路。失败时直接在 GitHub Actions
中重跑同一 tag 的 Release run，tag 不会被创建、覆盖或移动。本地
`packaging/macos/build-app.sh` 只使用已提交的版本，不会修改发布元数据。

创建 GitHub Release 前，`scripts/generate_release_highlights.py` 会读取已检出的 tag 历史，生成带
不可变 commit 链接和完整变更对比链接的分类 **Highlights** 前缀。它会排除实施跟踪类提交主题，并且每条
选中的提交只出现一次，每个分类最多保留最近 8 条。`softprops/action-gh-release` 通过 `body_path` 读取该文件，同时仍启用
`generate_release_notes: true`，因此 GitHub 会在该前缀下提供完整的自动变更列表。CI 通过
`scripts/test_generate_release_highlights.py` 覆盖该辅助脚本及其 Git tag range 行为。

## 运行日志

`src/main.rs` 通过 `src/logging.rs` 初始化唯一的全局 tracing subscriber。文件
writer 按 UTC 日期滚动，最多保留 15 个文件，并在进程持有的 guard 释放时刷新。
日志位于平台本地 AxSSH 应用数据目录的 `logs` 子目录。默认过滤规则为
`ax_ssh=info,russh=warn`，可由 `RUST_LOG` 覆盖。
About 从进程边界接收已经创建的目录，并通过 bridge 打开；Slint 线程不执行文件系统操作。

单次运行中可用以下命令开启脱敏键盘/UI diagnostics 和 SSH latency 阶段：

```bash
RUST_LOG='ax_ssh=info,ax_ssh::diagnostics=debug,ax_ssh::latency=debug,russh=warn' cargo run --locked
```

`terminal-input` 除 UI 到 worker 总耗时外，还记录 `state_lock_us` 和 `worker_request_us`。
多窗口 `workspace-refresh` 记录 `coalesced_refreshes`、`views_built_us`、`ui_queue_us`、
`ui_apply_us` 及可选的 output-to-UI 时间。这些字段均不包含按键文字、终端内容、主机、路径、
profile 标签或凭据。

diagnostics 只使用固定的 `event`、`key`、`route`、`action` 和 `outcome` 字段。F5、
ArrowUp 等特殊键使用稳定名称；所有可打印文字、IME、密码和粘贴值都只记录为 `Text`，
不记录内容或长度。路径、profile 标签、主机、剪贴板内容和凭据不会进入 diagnostics 字段。
debug 事件写入滚动文件，控制台 writer 仍限制为 INFO。

latency 事件只使用本地 `input_sequence`、固定 `stage` 和微秒耗时。`queue_us` 测量有界
worker queue；`call_us` 只表示 russh data 调用完成，不代表服务端已收到。
`first-output-after-input` 明确标记 `association=temporal-only`。UI 字段分别记录输出到调度、
event-loop queue、应用和客户端输出总耗时，不包含终端内容或字节长度。由于滚动 writer 为
非丢弃模式，性能基线应关闭该 debug target；开启后的日志用于定位阶段，不应作为唯一 benchmark。

## 验证边界

自动检查覆盖 profile 校验、JSON round-trip、Slint 编译、保存并连接路由、认证存储选择映射、日志退出刷新，以及
loopback russh 测试服务器上的拒绝式主机密钥探测、受信密码/私钥认证、只有精确主机密钥匹配后
才执行外部签名的内存 agent protocol、PTY shell
输入输出、resize、worker 断开与 join；单元测试还覆盖 ANSI 解析、有界 scrollback、
终端控制/导航键编码、旧版外观到版本化设置的迁移、同 profile 多 Tab 隔离、本机密钥
发现、加密密钥 passphrase、本地 PTY 生命周期、vt100 字符格渲染、application-cursor
方向键、Shift 可打印键后备转换、原始 C0 控制字节事件、Apple 修饰键还原、Telnet
协商/CRLF/NAWS、Serial USB 稳定身份匹配和直连 attempt 隔离。SFTP 测试覆盖远端
路径/名称校验、分片与超大 packet frame、逐 Tab 快照隔离、浏览事件恢复、regular file
元数据/路径检查、有界分块下载、截断、取消、私有缓存发布/权限/清理、transfer 状态、pending
subsystem 取消和 Tab shutdown join。文件图标测试覆盖有界规范化 key、cache identity、LRU
容量、预热上限和 fallback；本地打开测试覆盖快照归属与 symlink 替换拒绝。忽略测试
`platform_credential_store_round_trips_and_deletes` 会执行真实平台凭据
写入、读取和删除，并可能触发系统授权提示；应在每个受支持的凭据后端上主动运行。
窗口渲染、键盘/焦点、可见的分组/主机密钥/认证弹窗、全屏终端程序，以及真实 SSH/Telnet
服务器仍需 GUI/联机手工验收；其中还包括横向 Tab 滚动、多个真实连接并发、目标平台运行时
SSH agent 选择、解锁/确认、多 identity 与失败行为，以及目标平台
Serial 发现/权限/热插拔与设备输入输出、真实 SFTP 服务兼容性、SFTP 面板焦点/布局、macOS/
Windows/Linux 默认程序调度与文件图标外观/主题变化、协议 resize、切换后的终端焦点保持和原生
标题栏拖动命中区域。
SSH 输入延迟应在同一主机和网络上对比 AxSSH 与系统 `ssh`，优先观察 P50/P95；两者相近通常
说明网络/远端 PTY RTT，占比明显更大的 AxSSH 差值再用上述 latency 阶段定位。
