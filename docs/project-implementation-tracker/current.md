# 当前项目实施记录

## 当前目标

- 目标 ID：20261013-sftp-actions-and-navigation
- 目标：按用户截图与选定四项改进 SFTP 主操作、区域层次、导航控件和长路径分段导航。
- 交付物：带方向和选中数量的上传/下载入口；统一 SVG 控件与主题层次；有界的跨平台路径分段模型及编辑模式；双语说明、翻译、回归、编译与交付记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/sftp/`、`ui/sftp-pane.slint`、SFTP 控件/图标与 Theme、`src/app/view/` 的展示模型映射、翻译、双语 usage/architecture 和环境/实施记录。
- 不在本轮范围内：空传输区折叠行为、侧栏/页面路由或全局主题开关、SSH trust/认证、transport/worker、配置 schema、依赖/工具链/CI 与 tag/Release。
- 所有权：应用展示层从当前路径生成小型 owned 分段模型；Slint 负责编辑状态、视觉和已有导航/传输意图；原 worker 校验、限制、取消与发布流程保持。
- 安全：不在 UI 访问文件系统/网络；路径模型数量与重复文本有界，remote 按 POSIX、local 按平台规则处理；不新增秘密或协议句柄。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用
- 交付：沿用用户先前提交并 push 的授权；网络错误采用本机 7897 命令级代理。GUI 视觉/焦点验收按 AGENTS.md 交给用户提供截图。

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| U1 | completed | 环境预检、页面/应用契约、截图与 plan-first 计划 | manifest/lock/build/CI、工具链、地图和现有 callback 核对 | Rust 2024/MSRV 1.92.0；本机 1.97.1，两个 macOS target |
| U2 | completed | 显著上传/下载、方向/数量、统一图标和主题层次 | 既有 callback 与 readiness/selection 条件、Cargo Slint 重编 | 保留紧凑双栏表格和既有分页/排序 |
| U3 | completed | 有界路径分段导航与原始路径编辑模式 | POSIX/Unicode/Windows drive/UNC/deep path 及模型稳定性回归 | 复用 list-directory，不新增 transport API |
| U4 | completed | 双语说明、翻译、项目地图与实施记录 | translation/link/skill/tracker 校验 | 新用户文本覆盖已有 11 个语言目录 |
| U5 | completed | 定向与完整 Rust/Slint/可用平台门禁 | fmt/check/严格 all-target Clippy/test、两个 macOS target check/Clippy/build、diff | 不运行 Intel 非原生测试；Windows/Linux 交原生 CI |
| U6 | in_progress | 分项提交与 push、远端 SHA 核对 | 显式暂存/index 审查、push/ls-remote、最终状态 | 不改全局代理或历史 |
| U7 | pending | 用户 GUI 视觉和键盘/焦点验收 | 用户运行新版并确认/提供截图 | 代理不自行捕获或检查应用截图 |

## 已完成

- 已读取截图、AGENTS.md、Rust/Slint 与相关引用、项目地图、环境和既有 UI/应用映射源码。
- 工作区起始干净；Rust/Cargo 1.97.1、ARM64/Intel macOS target、edition 2024/MSRV 1.92.0 和锁定依赖保持。
- 已核对主操作可复用现有上传/下载 callback；原 toolbar Upload 条件与本地导航不兼容，属于本轮修正范围。
- 页面入口和侧栏不变；前端清晰度规则限于用户选定的四项。先落盘计划，再改实现。

## 验证

- 已完成：环境/契约预检、最终 Slint 重编；60 项 SFTP 专项、391 library + 451 binary（842）完整测试、fmt、locked/offline check 和严格 all-target Clippy 通过。665 消息 × 11 目录、8 Python 翻译测试、119 个仓库文档相对链接、九个 SVG 与 skill/tracker/staged diff 校验通过。窄栏零宽、上传回调和焦点恢复均纳入最终源码。
- 已完成：ARM64/Intel macOS 的 CI 同款 check、严格 all-target Clippy 与 build；Mach-O arm64/x86_64 核对。整个最终门禁期间源码 SHA-256 未变化；未执行 Intel 非原生测试。
- 未完成：提交推送、用户 GUI 视觉/键盘/焦点验收；Windows/Linux 原生构建交 CI。

## 风险与阻塞

- 狭窄双栏需优先保留主操作，辅助控件按宽度收敛；长路径模型不能造成无界重复字符串或 UI 子项。
- Windows 本地路径与 remote POSIX 必须分开，不能按本机 Path 规则解释远端路径。
- GUI 渲染、键盘与焦点由用户验收；不使用代理自主截图，Windows/Linux SDK 不可用。

## 下一步

- 分项提交和 push，核对远端 SHA 与工作区；用户随后运行 `target/aarch64-apple-darwin/debug/ax_ssh` 验收页面和键盘/焦点。

## 最后更新时间

- 2026-10-14 00:12 +0800
