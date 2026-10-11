# 当前项目实施记录

## 当前目标

- 目标 ID：20261011-workspace-action-icons
- 目标：接续用户指定 Codex 会话，统一主窗口和子窗口的分屏、移出/返回图标，并把 SFTP 重命名移入远端文件行右键菜单。
- 交付物：共用 SVG 源与 Theme 尺寸/颜色、macOS 无常驻底框的原生按钮、主题同步、单项右键重命名对话框、双语文档与验证记录及分项本地提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/theme.slint`、`ui/components/` 标题栏与覆盖层、`ui/icons/`、`ui/sftp-pane.slint`、`ui/workspace-shell.slint`、`ui/app.slint`、`src/app/macos_window.rs`、`src/app/window_bridge.rs`、`src/app/view/settings.rs`、`src/app/sftp_bridge.rs`、双语架构/使用文档与实施记录。
- 不在本轮范围内：依赖/工具链/CI 升级、SSH/SFTP transport/凭据/配置 schema、工作区转移和 worker 生命周期、推送/发布。
- 所有权：Slint 的 Theme 提供唯一图像源、尺寸和调色板；覆盖层保存重命名草稿并转发用户意图；应用层校验原始 Tab/路径与单项选择，复用既有 worker 操作；macOS bridge 只配置原生按钮与有限 raster。
- 安全边界：原生回调继续捕获 Weak AppWindow；重命名保留名称校验与单项限制，拒绝已切换的目标；不传递网络 handle、凭据或终端内容，不新增队列或文件 I/O。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| WI1 | completed | 共用图标资产与 Slint/原生呈现映射 | Slint 重编、既有图像编码和窗口回归、静态 diff | 用户四张截图作为差异来源 |
| WI2 | completed | SFTP 右键重命名、双语文档、地图及自动门禁 | 重命名目标回归、fmt/check/严格 Clippy/test、两个 macOS target check/Clippy/build、链接/skill/tracker/diff | Intel 使用同版本/架构/特性的已验证本地 Skia 归档；未运行非原生测试 |
| GC2 | completed | 图标、右键重命名、双语文档与记录三项本地提交 | 分项暂存审查、ignore/链接/tracker/diff、提交后状态 | 代码提交 0f1bea4/4b89c9b；文档提交以包含本记录的 commit 为准 |
| WI3 | pending | 用户视觉与真实平台操作验收 | 主/子窗口分屏、移出/返回、主题切换、重命名与键盘焦点 | 不自行捕获或检查应用截图 |

## 已完成

- 已找到指定会话及其前序会话，确认 SFTP 工作已提交；未完成目标为主/子窗口图标统一。
- 已读取用户四张截图，定位主窗口 Slint glyph 与 macOS 子窗口系统符号/默认按钮底框的差异。
- 已读取工程技能、完整 Rust/Slint references、项目地图、架构、环境记忆及 Cargo/CI；本机 Rust/Cargo 1.97.1、两个 macOS target 可用，MSRV 1.92.0 与 Slint 1.18.1 保持。
- 工作区初始干净；先写计划再改代码，没有创建 Worker 记录。
- 用户明确指定在本会话执行并授权中断原会话；已通过原会话控制接口中断其 active turn，状态变为 idle，并停止遗留的本项目构建进程。保留已落盘的代码与文档。
- 已复核 shared SVG/Theme、Windows/Linux Return、macOS 无底座按钮、弱引用动作和主题 tint；补齐原生按钮按标题栏高度居中，缩小仅在本模块调用的 helper 可见性。
- 已移除 SFTP 顶部 Rename/新名称输入；远端行菜单在唯一选择时提供 Rename，未选中命中行先替换选择；共享覆盖层预填名称并支持 Enter/Escape，应用层重新校验原始 Tab/路径、目录 readiness、可见性和唯一选择。
- 已同步双语 SFTP 使用/架构说明与项目地图；名称/路径及 worker 校验继续沿用，不改 SSH trust、配置 schema 或依赖。
- 最终源码已通过完整 Cargo 门禁与 ARM64/Intel macOS 的 CI 同款 check/严格 all-target Clippy/build；默认及两个 target 程序的 Mach-O 架构已核对。
- 已按用户要求整理本地提交：窗口动作图标 0f1bea4、SFTP 右键重命名 4b89c9b；双语文档、项目地图、研究来源及环境/验证记录随包含本记录的文档提交保存。没有推送或发布。

## 验证

- 已完成：环境和源码预检、用户截图差异定位；最终 Slint 重编、2 项重命名目标回归、fmt/check/严格 all-target Clippy、库/应用全量测试（应用 396 项）、默认及两个 macOS target 的 check/严格 Clippy/build、562 条翻译、10 条相对链接与 skill/tracker/diff 校验通过；5 项原生窗口专项包含在全量测试中。
- Intel 首次检查因 Skia 下载 HTTP/2 错误和 googlesource 超时失败；验证本机完整归档的 tag 0.153.3、x86_64/ganesh-gl-jpegd-jpege-metal-pdf key 后，使用临时 SKIA_BINARIES_URL=file URL 完成三个门禁。未改依赖、Cargo 配置或 CI。
- 未完成：用户视觉/交互与真实 SFTP 验收，以及 Windows/Linux 原生验证；未运行非原生 Intel 测试。

## 风险与阻塞

- macOS 子窗口保留原生标题栏；按钮交互和高度受 AppKit 控制，图形、图标尺寸、间距和静态颜色使用共同来源。
- Windows/Linux SDK 不可用，原生 CI 与用户验收保留。
- 主题设置预览/保存须同步原生按钮颜色；系统色彩模式变化沿用既有同步边界。

## 下一步

- 重启新版 `target/debug/ax_ssh`；WI3 由用户确认主/独立窗口图标、主题与焦点，及右键 Rename 的单项、多选禁用、确认/取消和真实远端操作。

## 最后更新时间

- 2026-10-12 +0800：按用户要求完成分项本地提交交付。
