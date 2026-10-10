# 当前项目实施记录

## 当前目标

- 目标 ID：20261010-detached-system-icon
- 目标：修复 Tab 独立窗口标题栏和 `+` 已保存连接列表缺少系统图标的问题，并与当前连接及侧栏图标保持一致。
- 交付物：活动连接的有界图标元数据、Windows/Linux 顶栏和 macOS 原生图标、20px 连接列表图标、优先级/分屏/更新回归、双语说明与验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：应用窗口 route/view/native chrome、Slint AppWindow/桌面顶栏与已保存连接列表、既有 AppKit feature、双语用法/架构与 tracker。
- 不在本轮范围内：重新识别服务器、系统名称目录或识别规则修改、配置 schema 或凭据/SSH 修改、用户 LICENSE 修改、推送或修改既有发行标签。
- 所有权：WindowRouter 选取当前活动 pane 的已有 tab summary；view 映射图标 ID/release，并共用侧栏和连接列表的 profile 优先级；Slint 复用目录资源与主题开关；AppKit 窗口拥有原生 image view 和有界图像缓存。
- 安全边界：只读展示既有手动/缓存/本机图标，不探测服务器或访问真实凭据，不传 worker 或终端缓冲区到原生控件。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| DI1 | completed | 确认缺失标题栏图标的原因与用户意图 | detached 布局、tab mapping、native chrome 调用链 | 用户确认指标题栏中的系统图标 |
| DI2 | completed | 当前连接图标同步到两种标题栏 | Slint 编译、AppKit cfg/生命周期与配色 | 不增加独立客户端 Tab 条 |
| DI3 | completed | 活动 pane/识别更新/图像边界回归和自动门禁 | focused tests、fmt/check/Clippy/tests/native build | 最终 656 项完整测试及 native 门禁通过 |
| DI6 | completed | `+` 连接列表显示共用的系统图标 | 20px 共用尺寸、手动/有效缓存/运行时/通用优先级回归 | 2 项列表专项通过；未知系统使用通用服务器符号 |
| DI4 | completed | 双语行为文档、地图与 tracker 收口 | relative links、skill/tracker validators、diff | 保留 55 项历史格式基线，无新增错误 |
| DI7 | completed | 功能与文档本地提交 | 显式暂存、index diff、提交后状态 | 功能 b6cbad7，文档与此记录随独立提交收口；保留 LICENSE，不推送 |
| DI5 | pending | 用户原生验收标题栏与连接列表图标 | Terminal/SFTP/Local Shell、分屏、返回/恢复、配色、列表键盘/点击 | 视觉验收属于用户 |
| PW5 | pending | 前轮逐服务器凭据清除原生验收 | 状态刷新、确认/取消、真实系统后端 | 前轮实现与自动验证完成 |
| UI7 | pending | 前轮导航系统图标原生验收 | 发行版/手动指定、配色、tooltip/焦点 | 前轮实现与自动验证完成 |

## 已完成

- 用户确认缺失的是标题栏中的系统图标；独立窗口隐藏 WorkspaceTitlebar，原生标题栏仅同步标题文字，Windows/Linux 顶栏也未接入图标。
- 开工时工作区仅有用户单独的 LICENSE 修改；既有 annotated tag `2026-10-10` 固定在版本提交 `477a8ea`，保持不变。
- 已实现当前活动 pane summary 映射、两种 16px 标题栏图标、原生窗口拥有的 64x64 raster/source cache 与主题 tint；完整刷新更新图标，terminal-only 输出不转换图像。
- 用户提供截图追加 `+` 连接列表图标需求；已接入启动/刷新两条数据路径，并与侧栏共用 profile 解析及 20px glyph；打开列表不会识别服务器，未知系统不会误用 Tux。
- 已复核显式暂存的完整功能差异并创建 `b6cbad7`；双语说明、地图和本条实施记录单独提交，保留用户 LICENSE 修改及原标签。

## 验证

- 已完成：5 项标题栏及 2 项连接列表专项通过；最终 fmt、locked/offline check、严格 all-target Clippy、656 项完整测试（311 library + 345 binary）和 diff 通过；Slint 重新编译，aarch64-apple-darwin 显式 target check/严格 Clippy/build 通过。
- 已覆盖：创建/恢复/更新、分屏活动 pane、手动/有效缓存/运行时优先级、目标变更拒绝旧缓存、未知系统区别于 Tux、原生像素颜色/透明度/比例边界与空图拒绝。
- 文档/资源：519 条中文翻译、116 项系统目录/5 个生成文件、697 条相对 Markdown 链接、项目 skill validator 和 diff 通过；当前 tracker/地图/本轮历史无新增错误，全局仍为 55 项相同历史格式问题。
- 未完成：原生界面与其他 CI 原生平台验收；本机仅具备可用的 aarch64 macOS SDK/链接环境，其他平台不声称交叉编译通过；PW5/UI7 继续待用户验收。

## 风险与阻塞

- AppKit 原生控件须跟随窗口销毁并保持 main-thread-only；仅图标变化时更新有界 raster，不随终端输出重复转换。
- tracker 全局保留 55 项既有历史格式问题，本轮记录必须无新增错误。

## 下一步

- 用户验收 DI5：独立 Terminal/SFTP/Local Shell 标题栏图标、分屏焦点、返回/恢复、配色，以及 `+` 连接列表的图标、点击和键盘行为；保留 PW5/UI7 用户原生验收。

## 最后更新时间

- 2026-10-10 11:36 +0800：独立窗口与 `+` 连接列表图标、完整验证和本地提交收口，保留用户原生验收。
