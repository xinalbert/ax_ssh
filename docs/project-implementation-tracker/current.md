# 当前项目实施记录

## 当前目标

- 目标 ID：20261010-system-icon-picker-layout
- 目标：调整服务器和 Local Shell 的系统图标选择器，解决展开后撑大设置行、列表过窄和图标/文字未居中的问题。
- 交付物：共用紧凑图标下拉控件、独立搜索弹层、规范显示名称、键盘/选中反馈、双语文档和验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：Slint 图标选择器及 Settings/Session Editor 接线、应用层纯目录查询、尺寸 token、双语文档与 tracker、本地提交。
- 不在本轮范围内：图标识别或资源目录修改、配置 schema/保存语义、凭据/SSH、原生标题栏、用户 LICENSE 修改、推送或修改发行标签。
- 所有权：Slint 拥有弹层、搜索/焦点和视觉状态；应用层仅提供内置目录的有界显示名称查询；既有保存 callback 保持不变。
- 安全边界：只读查询内置名称，不增加文件/网络/凭据访问，不修改主机信任或服务器识别。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| IP1 | completed | 结合用户截图核对两处布局原因 | 共用选择器、SettingsField/Row、Session Editor | 内联列表扩大设置行，右侧控制只获得狭窄宽度 |
| IP2 | completed | 紧凑控件、独立弹层和准确显示名称 | Slint 编译、目录名称专项、键盘/选中状态静态复核 | 32px 控件，20px 居中图标，搜索和有界列表不占用表单行高 |
| IP3 | completed | 全部自动门禁和本机构建 | fmt/check/Clippy/tests、资源/翻译/diff | 657 项测试，aarch64 macOS check/严格 Clippy/build 通过 |
| IP4 | completed | 双语说明、地图、历史与本地提交 | relative links、skill/tracker validators、index diff | 文档与实现随本地提交收口，保留 LICENSE |
| IP5 | pending | 用户验收服务器/Local Shell 选择器 | 宽窄窗口、搜索、自动/手动、点击外部/Escape、焦点 | 实际视觉验收属于用户 |
| DI5 | pending | 前轮标题栏与 `+` 列表原生验收 | 分屏/返回/恢复、配色与列表键盘 | b6cbad7/f92c2c5 已提交，656 项测试和 Mac native 构建通过 |
| PW5 | pending | 前轮逐服务器凭据清除原生验收 | 状态刷新、确认/取消、真实系统后端 | 前轮实现与自动验证完成 |
| UI7 | pending | 前轮导航系统图标原生验收 | 发行版/手动指定、配色、tooltip/焦点 | 前轮实现与自动验证完成 |

## 已完成

- 用户提供服务器编辑页和 General Settings 的截图，授权调整图标设置布局；确认共享组件直接将六行列表展开在父行中，导致设置行增高和列表挤在右侧。
- 核对既有配置和保存边界：服务器选择仍是编辑 draft，Local Shell 仍独立立即保存；本轮只调整展示和纯目录查询。
- 已实现独立弹层、当前选项/键盘反馈、共享 20px 居中图标和规范名称精确查询；Settings 控件使用完整字段宽度，编辑器使用紧凑左对齐控件。
- 目录名称专项及完整 Rust/Slint 门禁通过；本机 aarch64 macOS 完成显式 target check、严格 all-target Clippy 和链接构建。
- 同步双语用法、架构、项目地图与月度历史，复核实际功能差异后按此前用户授权整理本地提交；未推送或修改发行标签。

## 验证

- 已完成：用户截图与共用组件/父容器/纯目录 callback 调用链核对，使用本地锁定 Slint 1.18.1 的 PopupWindow API；2 项目录名称/搜索专项、fmt、locked/offline check、严格 all-target Clippy、657 项测试（311 library + 346 binary）、aarch64-apple-darwin 显式 check/严格 Clippy/build 和 diff 检查通过；Slint 重新编译，520 条中文翻译、116 项图标/5 个生成文件、697 条 Markdown 相对链接及项目 skill validator 通过；当前/地图/新增历史无新增 tracker 错误，全局保留同一 55 项历史格式问题。
- 未完成：本轮及前轮 DI5/PW5/UI7 原生视觉验收持续保留。本机仅验证可用的 aarch64 macOS SDK/链接环境，其他 CI 原生平台未运行。

## 风险与阻塞

- PopupWindow 内部控件只在弹层内部读取；开启状态使用内置只读 is-open，避免点击外部后状态残留。
- tracker 全局保留 55 项既有历史格式问题，本轮不得新增错误。

## 下一步

- IP5 由用户验证宽窄窗口中的弹层位置、搜索/选择、自动模式、外部/Escape 关闭和焦点，前轮原生验收继续保留。

## 最后更新时间

- 2026-10-10 12:09 +0800：选择器实现、自动验证、双语文档和本地提交内容收口，保留 IP5 与前轮原生验收。
