# 当前项目实施记录

## 当前目标

- 目标 ID：20261113-macos-native-titlebar-restore
- 目标：撤销无边框自绘顶栏，恢复 macOS 原生标题栏；保留截图所示启动标题栏异常的修复。
- 交付物：原生红绿灯与既有布局、创建时隐藏主窗口标题、原生就绪初始化、回归和本地提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：回退 e999b7a 的无边框布局和控件变更；macOS 窗口创建属性、原生初始化、架构和 tracker。
- 不在本轮范围内：Windows/Linux 行为变更、SSH/凭据/配置 schema、依赖升级、推送/tag/发布、GUI 启动或截图。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| R1 | completed | 审查用户最终范围和原生标题栏基线 | 现有提交、架构和锁定 backend 源码 | 恢复原生方案，保留启动异常处理 |
| R2 | completed | 回退自绘顶栏并接回原生初始化 | 8 项原生窗口定向回归通过 | Windows/Linux 保持改动前行为 |
| R3 | completed | 完整验证、双语说明和本地归档 | Rust 门禁、macOS target、文档/tracker/diff | 单一本地提交，不改写旧历史 |

## 已完成

- 开始时工作区干净，最近两个提交为 e999b7a 和 a9bbc68。
- 用户最终要求撤销标题栏移除，同时保留截图中偶发原生标题条和 Tab 布局异常的修复。
- 范围限定为 macOS；保留原生红绿灯，Windows/Linux 不新增行为。
- ui/ 与视图文件已逐字恢复至 e999b7a 之前；相对原版本只新增五份窗口/renderer bridge 的启动时序修复。
- 创建属性 hook 覆盖 renderer 设置、环境覆盖和回退路径；一次性构造作用域在失败后清理，首个原生事件完成视图/按钮/菜单初始化及位置恢复。
- 完成双语架构、项目地图和追加历史；本轮代码、回归和记录作为单一结果本地归档，不撤回已有历史。

## 验证

- 已完成：8 项 macos_window 定向回归；fmt、Slint 重编译和 locked/offline check、严格 all-target Clippy、1008 项 Rust 测试（461 library + 547 binary）、本机 build；精确 CI 命令 `cargo check --locked --target aarch64-apple-darwin` 和 `cargo clippy --all-targets --locked --target aarch64-apple-darwin -- -D warnings`；11 个翻译目录各 694 条文案和生成一致性、56 份 Markdown/入口边界、tracker/diff 和暂存区范围检查通过。
- 构建产物：`target/debug/ax_ssh`，未启动 GUI。
- 未完成：用户多次冷启动、恢复窗口/分离窗口、拖动/缩放/焦点验收；其它平台原生 CI 未在本机运行。

## 风险与阻塞

- 原生句柄操作必须等待真实原生事件；标题栏属性须在主窗口创建时设置，避免以固定延迟赌启动顺序。
- 不启动或截图 GUI；外观与真实窗口行为由用户确认，其它平台原生 CI 未在本机运行。
- AX Agent Context 无可调用 memory tools，继续使用仓库记录。

## 下一步

- 用户使用新构建确认冷启动外观和窗口行为。当前仅保存本地提交，不推送、tag 或发布。

## 最后更新时间

- 2026-11-13 10:24 +0800
