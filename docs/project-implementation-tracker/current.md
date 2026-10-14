# 当前项目实施记录

## 当前目标

- 目标 ID：20261014-tab-hover-latency
- 目标：核查 Tab 切换/关闭的标准能力与现有路径，将 SSH 延迟提示扩展到 Tab 主体。
- 交付物：共享 Tab 主体 Tooltip、状态和交互验证、双语架构说明与跟踪记录，并按用户追加授权分项本地提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/components/workspace-titlebar.slint`、`src/app/view/tests/text_overflow.rs`、双语架构/使用说明和 `docs/project-implementation-tracker/`。
- 不在本轮范围内：transport/凭据/信任/探测频率、配置 schema、Tab 生命周期语义、依赖/工具链、推送或发布。
- 所有权：Slint 组合已有 Tab 摘要和 Tooltip；Rust 应用层继续负责路由/生命周期，worker 继续负责采样和关闭。
- 安全：不增加网络请求或持久化，不暴露凭据，不改变 SSH host-key 校验。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| T1 | completed | 标准控件与现有切换/关闭/延迟路径核查 | 本地源码、既有回归、Slint 官方文档 | TabWidget 没有现有动态关闭/排序/分离操作；保留工作区组件 |
| T2 | completed | Tab 主体悬浮提示和双语说明 | 编译 UI、指针事件/焦点与状态回归 | 复用 Tooltip 和已翻译延迟文案；拖动预览不提示 |
| T3 | completed | 仓库门禁与记录 | fmt/check/严格 Clippy/test、翻译、相对链接、tracker/diff | 本机调试程序已构建；GUI 验收由用户完成 |
| T4 | pending | 用户 GUI 验收 | 用户确认或提供的截图 | hover、切换、关闭、拖动与焦点 |
| T5 | completed | 按功能和文档分项本地提交 | index 审阅、提交范围、最终工作区 | 源码/回归 8ea0797；文档单独归档 |

## 已完成

- 初始工作区干净；Slint 锁定 1.18.1，Rust edition 2024/MSRV 1.92.0；本机 ARM64 macOS、rustc 1.99.0。
- 核对 Tab 单击/键盘激活、窗口路由内循环切换、拖动排序、Settings 保存后关闭、相邻选择、旧事件隔离和异步 worker shutdown。
- 延迟毫秒/更新时间已进入 UI，原提示仅在状态点；Slint 标准 Tooltip 可观察 hover 并转发点击。
- 透明主体容器覆盖图标/序号/标题/状态点，组合完整标题、系统版本与连接摘要，动作按钮保持独立；拖动源及预览禁用 Tooltip。
- 新增实际指针回归已通过：短标题可 hover、打开中的提示实时更新、测量/超时/过期/不可用替代旧值、非 SSH 只显示已连接、断开清除连接数据、焦点不变、激活/关闭点击可达。
- 双语架构/使用说明、地图和官方文档研究记录已同步；没有新增网络请求、计时器、翻译键、依赖或跨层 API。
- fmt、locked/offline check、严格 all-target Clippy、391 library + 463 binary（854）完整测试与本机 debug build 全通过。
- 用户追加授权本地提交；源码与指针回归以 `8ea0797` 独立保存，双语说明和跟踪记录由配套文档提交归档。没有推送、改写历史或创建 tag。

## 验证

- 已完成：源码/所有权审查、官方能力核查、Slint 实际重编译、专项指针/焦点/状态测试、fmt/check/严格 Clippy/854 项 Rust 测试/build/diff、665 × 11 翻译与生成检查、8 项翻译脚本测试、12 个相对 Markdown 链接、skill 校验；tracker 新记录和 current/map 校验通过，完整校验旧错误数仍为 84（对照 HEAD 无新增）。
- 未完成：用户 GUI 视觉/真实 SSH hover 验收；Windows/Linux 原生 CI 验证未在本机执行。

## 风险与阻塞

- 指针/焦点回归已通过；真实平台 hover、拖动与焦点手感仍需用户验收。
- 本地不能替代 Windows/Linux 原生 CI；GUI 视觉与真实 SSH 悬浮行为待用户验收。
- 历史 tracker 已记有 84 项格式问题；本轮新增记录单独校验，避免改写历史。

## 下一步

- T1–T3 与 T5 已完成；用户使用 `target/debug/ax_ssh` 完成 T4 的视觉、拖动与焦点验收。

## 最后更新时间

- 2026-10-14 16:58 +0800
