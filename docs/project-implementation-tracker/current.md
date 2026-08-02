# 当前项目实施记录

## 当前目标

- 目标：用平台系统发现替代已知 X server provider 对固定安装路径的依赖，并把 Terminal 的传输 resize 与本地模型 resize 收敛到单一应用状态入口。
- 交付物：macOS bundle/Windows executable 系统发现与标准路径兜底、仅 Custom 可编辑的 Settings 路径、集中式活动 Terminal resize 方法、回归测试和双语文档。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`Cargo.toml`、`Cargo.lock`、`src/x_server.rs`、`src/app/state.rs`、`src/app/state/tests.rs`、`src/app/terminal_bridge.rs`、`ui/settings/x11.slint`、`docs/{architecture,architecture.zh,usage,usage.zh}.md`、环境与实施记录。
- 不在本轮范围内：安装或下载第三方 X server、修改远端 `sshd_config`、改变 X11 cookie/no-auth 安全默认值、改变终端 reflow 算法、引入新 UI/SSH 框架或引用项目耦合。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| PATHDISC1 | completed | 平台系统发现优先的 X server launch target | provider/path 单元测试、macOS bundle 实测、Cargo check | macOS 用 bundle identifier；Windows 用 PATH/ProgramFiles 兜底。 |
| PATHUI2 | completed | 仅 Custom 可编辑路径的 Settings 与双语契约 | Slint/Cargo 联合编译、静态 UI contract | 已知 provider 不再持久依赖用户路径。 |
| RESIZE3 | completed | `AppState` 单一活动 Terminal resize 入口 | state tests、Terminal focused tests | 仍同时执行 worker resize 和本地 model resize。 |
| GATE4 | completed | 完整 Cargo、格式、tracker、Markdown 和差异门禁 | locked/offline check/test、Rustfmt、validator、`git diff --check` | GUI/真实 Windows 发现留给目标平台确认。 |

## 已完成

- 已完成施工前环境预检：Rust 2024、MSRV 1.92.0、Cargo locked/offline 和三平台 CI 契约未变化；本机缺 Cargo fmt/clippy 子命令。
- 已确认 macOS 当前固定路径可用，但 XQuartz 与 MacXServer 都有稳定 bundle identifier，可由系统应用数据库发现。
- 已确认 Windows 当前只检查 `ProgramFiles`/`ProgramFiles(x86)`，可增加进程 `PATH` 发现而不引入新依赖或注册表 unsafe API。
- 已确认 Terminal bridge 当前分别查找 worker 与本地模型；两个 resize 均必需，但编排可由 `AppState` 单一方法拥有。
- macOS 已通过 `NSWorkspace` bundle identifier 发现 XQuartz/MacXServer，并保留存在性检查后的标准路径兜底；Windows 已增加进程 `PATH` 优先、Program Files 后备的 executable 发现。
- 已知 provider 已忽略持久化 `app_path`；Custom 保留显式路径，并在启动前要求普通文件和 Unix executable 权限。
- Terminal bridge 已只调用 `AppState::resize_active_terminal`；状态层先请求现有 worker resize，再立即更新本地 `TerminalModel`，Serial 仍由 worker no-op 后只更新模型。

## 验证

- 已完成：X11 provider/path 测试 8 项；`cargo check --locked --offline`；完整测试（库 105、应用 68、Doc tests）；全部 `src/` 直接 Rustfmt；Cargo metadata；tracker validator；27 个 Markdown 文件中的 46 个相对链接；参考项目源码/构建/打包零耦合；`git diff --check`。本机应用数据库分别返回 XQuartz 与 MacXServer bundle。
- 未完成：Cargo `fmt`/`clippy` 子命令本机未安装；GUI 路径行显隐、真实 Windows 发现和实际 X server 启动按仓库规则留给目标平台确认。

## 风险与阻塞

- macOS bundle 发现必须保持在 blocking worker 内，不得阻塞 Slint UI 线程；已知 bundle 未发现时仍保留标准路径兜底。
- Windows GUI 进程的 `PATH` 可能不含第三方 server，因此仍需保留 `ProgramFiles` 搜索；真实 Windows 行为只能在目标平台验收。
- Custom 路径仍会启动本机程序，必须继续使用无 shell 的 `Command`、有界状态和存在/类型校验。
- 工作区已有大量用户改动；本轮只增量修改相关位置，不回退无关差异。
- Windows GUI 进程的实际 `PATH` 和第三方安装布局只能由 Windows CI/目标机确认；Program Files 后备仍保留。

## 下一步

- 等待用户在目标平台确认 Settings > X11 的 Custom 路径显隐，以及真实 XQuartz/MacXServer/VcXsrv/Xming 启动；后续路径策略修改继续留在 `src/x_server.rs`，Terminal resize 继续只从 `AppState::resize_active_terminal` 进入。

## 最后更新时间

- 2026-08-02 20:22 CST
