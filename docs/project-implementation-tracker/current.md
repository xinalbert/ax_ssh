# 当前项目实施记录

## 当前目标

- 目标 ID：20261117-shared-button-style
- 目标：统一设置与 SFTP 按钮的留白、轻量边框、强调色和图文对齐，减少撑满工具栏的视觉密度。
- 交付物：共享按钮表面与 Theme token、设置/SFTP 接入、行为验证、双语说明和实施记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/theme.slint`、共享文本/SFTP 控件、Settings 操作按钮和 SFTP 工具栏；必要的无截图交互回归、双语架构及 tracker。
- 不在本轮范围内：导航/页面入口、数据与回调契约、文件图标资源、SSH/传输行为、持久化、提交/推送/发布；已有上传修复保留。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| B1 | completed | 按钮样式和所属边界核对 | 设置/SFTP 控件、Theme、现有交互回归 | 标准按钮与自绘按钮混用；用户确认文字与图标两类均统一处理 |
| B2 | completed | 共享轻量按钮表面、间距和图文对齐 | Cargo 重编译 Slint、已有按钮/路径/焦点回归 | UI owns 样式，标准 Button 保留文字按钮事件语义 |
| B3 | completed | 全量门禁、双语说明与记录 | fmt/check/Clippy/test/build、文档/skill/tracker/diff | 外观与实机交互由用户验收，不自制截图 |

## 已完成

- 设置文字按钮和 SFTP 操作按钮现共用 ActionButtonSurface，采用轻量边框、浅强调色及共享 Theme 几何；工具栏上下留白，图文整体居中，音频保存操作也接入共享控件。
- 本次只改变 UI 呈现，维持原数据、回调、键盘和授权边界；不使用外部研究或多 agent。
- 保留 11 月 16 日上传冲突修复及其 1031 项测试记录，相关传输源码不在本轮编辑范围。

## 验证

- 已完成：Slint 重编译与 locked/offline check、严格 all-target Clippy、完整 1031 项测试（472 library + 559 binary，Doc tests 0）及本机 locked/offline build，产物 `target/debug/ax_ssh` 为 arm64 Mach-O；12 项文本/SFTP 定向回归包括多语言/字体尺寸、长文字 Tooltip、鼠标点击、Tab/Enter/Space、路径/菜单/复制、禁用状态和冲突弹窗；fmt、Markdown/入口、skill/tracker 与 diff 检查通过。
- 未完成：用户外观与平台实机交互验收、其它平台原生 CI。

## 风险与阻塞

- 用户已确认文字/图标两类均需统一；最终视觉接受仍需用户在实际界面确认。
- 本机 macOS aarch64，不能代替其它平台原生 CI；不启动 GUI、捕获或检查自行生成的应用截图。

## 下一步

- 用户使用 `target/debug/ax_ssh` 核对设置与 SFTP 的按钮外观和实机交互，按反馈继续调整。

## 最后更新时间

- 2026-11-17 09:16 +0800
