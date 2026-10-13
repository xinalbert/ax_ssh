# 当前项目实施记录

## 当前目标

- 目标 ID：20261013-standard-terminal-cursor
- 目标：按标准终端语义发布光标，移除光标隐藏的固定等待，并在绘制前有界处理已经到达的 worker 事件。
- 交付物：简化的终端显隐状态、四协议共用的有界事件处理策略、分包/同步/持续输出回归、双语架构、实施记录和两组本地 Git 提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app/state*`、`src/app/terminal_presentation.rs`、SSH/Local/Telnet/Serial monitor、相关测试、`docs/architecture*.md` 和实施记录。
- 不在本轮范围内：依赖/工具链升级、光标外观或闪烁设置、应用安装、真实远端访问、远端推送和发布。
- 所有权：终端模型维护协议光标和同步状态；应用 monitor 有界处理已到达事件并发布 snapshot；Slint 只绘制。SSH 信任、凭据和 transport 生命周期不变。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| C1 | completed | 移除 24ms 显隐等待和状态 | 显隐、脏行、同步输出回归 | 仅标准 2026 帧可保留旧画面 |
| C2 | completed | 四协议在重绘前有界处理已到达事件 | 分包重绘、处理预算、空/关闭队列 | 无新增计时等待或输出缓冲区 |
| C3 | completed | 双语架构、地图和来源说明 | 相对链接、tracker/skill validator | 不修改历史实施条目 |
| C4 | completed | 完整本机门禁和最终 diff | fmt/check/严格 Clippy/test、ARM64 CI 命令 | GUI 验收由用户进行 |
| C5 | completed | 源码/回归和文档/记录两组本地提交 | staged diff、commit 和最终工作区核对 | 用户已授权提交；保存到本地分支 |

## 已完成

- 用户授权改为标准实现；工作区干净。此前已检查行列映射、Slint overlay、显隐和同步输出源码。
- 前轮 6 个显隐/4 个同步相关测试通过，隔离探针验证正常复位顺序、任意字节分包和同步超时；探针未保留到仓库。
- 核对 xterm DECTCEM、同步输出协议和锁定 Alacritty 的有界读取后唤醒路径；150ms 同步超时沿用上游。
- 本机 Rust 1.99.0、原生 ARM64 和共享 Cargo cache 可用；保留 Rust 2024/MSRV 1.92 及全部 CI target。
- 删除隐藏合并常量、deadline、初始化和保留旧帧分支；只有标准同步输出保留最近发布帧，脏行仍合并。
- 四协议 monitor 共用 64 事件处理预算；预算用完让出 Tokio，再优先已经到期的呈现；空/关闭队列不加等待。
- 首批 9 个 cursor 相关测试通过，覆盖立即显隐、scrollback/resize/断开、任意字节分包和已到达重绘片段的光标复位。
- 12 个呈现调度测试通过，包含 64 事件预算和空/关闭队列回归；双语架构、地图与来源记录已同步。
- 完整 391 library + 445 binary（836）测试通过；默认与原生 ARM64 的严格编译/lint 门禁通过，可运行 ARM64 程序已生成。
- 最终 diff 只含本轮终端状态、monitor/呈现回归与双语文档；无参考项目耦合、秘密日志、新输出队列、UI 阻塞或 public API 扩展。
- 用户明确要求创建提交；index 为空，工作区仅含本轮 10 个 Rust 和 6 个文档文件，ignore 已覆盖 target、日志、macOS/Python 缓存。
- 本地分支 `codex/standard-terminal-cursor` 保存提交；源码和回归提交为 `50c9e9b`，双语文档与本记录作为下一组独立提交交付。

## 验证

- 已完成：本轮实现、9 个 cursor/12 个呈现调度测试、fmt/check/严格 all-target Clippy、836 个 Rust 测试、ARM64 target check/严格 Clippy/build、diff、6 个 Markdown 相对链接和技能校验；tracker 与 HEAD 比较无新增错误，85 个历史格式错误保留。
- 未完成：用户 GUI/LLM CLI 实际场景验收；Windows/Linux 原生 runner 验证由 CI 负责。

## 风险与阻塞

- 标准实现仍会显示未被应用隐藏或同步保护的光标移动；不能承诺所有 CLI 或超时帧均无闪跳。
- 不捕获或检查应用截图；实际 LLM CLI 的视觉验收由用户完成。
- 本机没有 Windows MSVC/Linux 原生 SDK，这些平台保留 CI 验证。
- tracker 历史存在既有格式错误；本轮验证区分基线与新增条目。

## 下一步

- 用户退出旧版后运行 `/Volumes/albert_xin/2026/soft/axsoft/ax_ssh/target/aarch64-apple-darwin/debug/ax_ssh`，验证原来的 LLM CLI 场景。

## 最后更新时间

- 2026-10-13 18:09 +0800
