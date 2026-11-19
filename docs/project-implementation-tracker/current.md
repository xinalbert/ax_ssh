# 当前项目实施记录

## 当前目标

- 目标 ID：20261119-windows-drag-resize-guard
- 目标：在 `vendor/i-slint-backend-winit/` 的 Windows 边缘缩放路径加入最大化/全屏守卫，并在 cursor-move 与 press 两侧双重校验 resize 方向，预防 winit 0.30.13 `dragging` 标志卡死导致窗口拖动直到重启才恢复（用户实机现象：Windows 菜单栏拖不动、重启恢复）。
- 交付物：vendor backend 双侧守卫与根 Cargo 集成回归、本机门禁证据、双语契约；独立审查结论与 Windows 实机用户验收交接。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`vendor/i-slint-backend-winit/drag_resize_window.rs`、`vendor/i-slint-backend-winit/winitwindowadapter.rs`、`tests/window_resize.rs`、Cargo 补丁说明与双语架构和跟踪记录；守卫只属于 UI backend 边界，不改依赖版本或 CI 矩阵。
- 不在本轮范围内：SSH/安全边界、字体、Tab 策略、winit/Slint 版本升级、macOS/Linux 与正常状态下的拖动/缩放行为变化、上游 winit 源码改造。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：已结束

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| W1 | completed | Windows 边缘缩放双侧守卫及旧方向清理，不吞普通 press | 根 Cargo 回归获得真实行为 RED 后 GREEN | 用户要求停止原实现子任务，由主控接手完成 |
| W2 | completed | 本机完整门禁与 diff 证据 | fmt/check/严格 Clippy/test/build、diff、tracker/文档校验均 exit 0；1056 passed | 使用根 Cargo 测试入口；Windows 原生构建留原生 CI |
| W3 | completed | 独立审查与 Windows 待验收项交接 | 最终 APPROVE；已登记菜单栏拖动、边缘缩放、最大化交互复测步骤 | 完成的是审查与交接，Windows 实机与原生 CI 仍未验证 |

## 已完成

- 已知触发路径与本地证据逐行核对：`vendor/i-slint-backend-winit/drag_resize_window.rs:13` 在窗口 `!is_decorated() && is_resizable()` 时缓存缩放方向，未按最大化/全屏状态跳过；`winitwindowadapter.rs:1257-1266` 缓存方向、`1304-1306` 左键 press 命中缓存方向即调用 `drag_resize_window` 并吞掉 press；锁定 winit 0.30.13 `platform_impl/windows/window.rs:492-528` 的 `handle_os_dragging` 仅设置 `dragging` 标志且已置位时早退，`platform_impl/windows/event_loop.rs:1207-1216` 只在 `WM_EXITSIZEMOVE` 复位；上游 issue 4821 记录该标志可卡死、PR 4825 为上游修复方向（是否已合入/发布不作确定）。
- 用户指出子任务反复重试并要求主控接手后，原实现子任务已停止。独立 vendor crate 测试存在 workspace/features/dev-dependency 阻塞，不把这些环境/入口错误当作产品回归；改用根 Cargo 集成测试编译真实补丁模块，现有 CI 自动覆盖，不增加依赖或修改矩阵。
- 主控完成 Windows 最大化/全屏 hover/press 双侧校验，阻断时清旧方向、恢复默认光标且保留 Slint press；非 Windows 短路不查询原生状态，保持原行为。按下回归先实际失败（0 passed/1 failed，exit 101），补齐实现后 4 项定向测试通过（exit 0），再补普通缩放/恢复和八方向回归。双语契约和来源已同步。
- 独立 reviewer 针对原审查阻塞项复核真实文件、原始日志与当前暂存内容，最终 APPROVE；确认仅调整测试模块位置消除严格 Clippy 错误，原生产算法未再改变。用户已授权收尾并本地提交，全部改动构成一个 Windows backend 修复主题。
- 交接限制：上一终端字体目标已本地提交为 `16dc858` 与 `6dc6c1a`，用户 GUI 视觉验收仍未完成，该限制继续保留。

## 验证

- 已完成：press 回归真实 RED/GREEN；最终定向 7 项通过，全套 1056 项通过（478 library + 571 binary + 7 integration）。本机 `aarch64-apple-darwin` check、严格 all-target Clippy、fmt/test/build、Markdown/入口、tracker、diff 均 exit 0；原始输出与退出码见授权 scratch 的 `axssh-windows-drag-fix/parent/validation.log`。Clippy 曾发现测试模块后仍有生产 helper，已移到模块前且重验通过，失败日志单独保留。
- 未完成：Windows 原生 CI 与用户实机复测；不宣称 Windows 已验证修复。项目 skill 标准 validator 已通过。

## 风险与阻塞

- 上游根因不能由本仓库单独证明："重启恢复"与 `dragging` 卡死一致，但不能据此宣称已证明用户现象的全部根因；守卫只是对已知触发路径的预防，不能治愈已卡死的进程（需重启一次）。
- 保持锁定 winit 0.30.13 版本与上游源码不变，改动仅限 `vendor/i-slint-backend-winit/` 本地补丁边界；vendor 不得承载 AxSSH UI、会话或 SSH 状态。
- Windows 实机行为无法在本机验证；本机门禁、独立审查与交接完成也不等于 Windows 原生验收通过。

## 下一步

- 按用户授权整理单个本地修复提交，不推送或发布；用户在 Windows 新构建上重启一次后，复测“最大化→抓边缘→还原→拖菜单栏”及普通边缘缩放。其余平台交原生 CI，不扩大功能范围。

## 最后更新时间

- 2026-11-19 18:35 +0800
