# 当前项目实施记录

## 当前目标

- 目标 ID：20261113-tab-window-drag
- 目标：排查并修复 Tab 与窗口拖动的命中、坐标换算和手势冲突，保留原生 macOS 标题栏。
- 交付物：可复现的拖动回归、最小修复、双语说明、验证记录和本地提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：WorkspaceTitlebar 的指针与滚动坐标、macOS 拖窗命中、窗口动作 bridge、回归及架构/tracker。
- 不在本轮范围内：SSH/凭据/配置 schema、依赖升级、发布/推送/tag、GUI 启动或自制截图。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| D1 | completed | Tab 与原生拖窗事件/坐标审查和复现 | 真实 Slint 指针事件、静态平台审查 | 用户确认部分 Tab 拖动触发整窗移动 |
| D2 | completed | 最小拖动修复及双语说明 | 两项定向回归通过 | 保留原生红绿灯，不扩展应用边界 |
| D3 | completed | 完整验证与实施记录 | Rust 门禁、原生 macOS target、文档/tracker/diff 全部通过 | 实际 GUI 行为由用户确认 |
| D4 | completed | 按用户要求归档本地提交 | 实现提交 08f2325，记录随独立文档提交归档 | 未推送、tag 或发布 |

## 已完成

- 开工时工作区干净，基线为 47d2d01；主窗口使用原生标题栏，标题隐藏/透明属性在创建时生效。
- 原生命中固定使用 80x32 AppKit 点，而 UI 以 Theme 和 Slint 逻辑坐标布局；已审查 Tab 滚动/按下/释放与实际重排。
- 新增实际按下/移动/释放回归：Mac/非 Mac、1x/2x 和滚动后的 Tab 重排坐标在旧实现通过；左侧空白只能依赖原生自动拖窗的情况在新增明确路由测试中失败。
- 原生 mouseDownCanMoveWindow 依据 currentEvent 的当前位置动态重判，未绑定手势初始命中；修复为主客户区一律禁止原生自动接管，由 Slint 实际空白区域显式发起系统拖窗。删除固定 32 点拖动高度，独立窗口原生标题栏不变。
- 左侧留空的 x/y 显式固定在左上角，避免 Slint 对固定尺寸子项的默认居中；双语架构与项目地图已同步，历史追加验证证据。
- 用户授权本地提交；实现、回归与双语架构归档为 08f2325，项目地图和验证记录单独归档。

## 验证

- 已完成：两项 titlebar_drag 定向回归、fmt、Slint 重编译及 locked/offline check、严格 all-target Clippy、1009 项 Rust 测试（461 library + 548 binary）、本机 build；精确 CI 命令 `cargo check --locked --target aarch64-apple-darwin` 和 `cargo clippy --all-targets --locked --target aarch64-apple-darwin -- -D warnings`；11 目录各 694 条翻译与生成一致性、56 份 Markdown/入口、tracker/diff 检查通过。
- 构建产物：`target/debug/ax_ssh`，未启动 GUI。
- 未完成：用户 macOS 实际 Tab/窗口拖动、原生红绿灯和冷启动验收；其它平台原生 CI 未在本机运行。headless 回归验证逻辑坐标和事件，不能复现 AppKit 的原生接管。

## 风险与阻塞

- 原生自动拖窗与 Slint 显式拖窗应具有清晰且一致的命中边界，Tab 必须持有自身手势。
- 所有工作限于 UI/application/macOS 平台层；不触碰 transport、秘密和持久化。

## 下一步

- 用户使用新构建确认部分 Tab 是否还会带动整窗，以及左侧/右侧空白区拖窗。修复已本地提交为 08f2325，本记录随独立文档提交归档；未推送、tag 或发布。

## 最后更新时间

- 2026-11-13 13:26 +0800
