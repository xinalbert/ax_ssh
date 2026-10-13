# 当前项目实施记录

## 当前目标

- 目标 ID：20261013-low-coverage-behavior
- 目标：针对 Codecov 低覆盖应用模块补行为测试，修复实际发现的生命周期与并发 fixture 问题。
- 交付物：目录监听、终端剪贴板、连接启动和媒体行为回归；默认并发测试验证；同平台 LCOV 对比及双语覆盖率说明。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app/local_directory_watch.rs`、`src/app/terminal_bridge/`、`src/app/connection/worker_start.rs`、`src/app/terminal_media.rs`、`src/app/behavior_tests.rs`、`src/ssh/worker/media/`、对应测试和覆盖率文档。
- 不在本轮范围内：依赖/toolchain 升级、CI 矩阵调整、GUI 视觉改版、真实系统凭据/设备/远端访问、tag/Release。
- 所有权：应用层保留状态与弱 UI 路由；原生通知/剪贴板仅通过私有适配边界注入；SSH transport 和主机信任规则不变。
- 既有改动：保留上一轮 Windows 测试导入 cfg 修复；不创建 Worker 记录。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| L1 | completed | 低覆盖证据、工具链核对及计划 | macOS ARM64/Rust 1.99/llvm-cov 0.9.1、既有报告与源码 | 基线串行插桩 804 项通过；默认并发音频 fixture 超时 |
| L2 | completed | 目录监听生命周期与通知行为回归 | 5 项暂停时钟/私有后端测试 | 单槽通道，切目录/重试/退出/迟到事件 |
| L3 | completed | 剪贴板、认证启动和媒体行为回归 | 16 项应用 binary 回归与 loopback 字节 | 不访问真实剪贴板/keyring；SSH 仅 loopback listener |
| L4 | completed | 默认并发 shell fixture 稳定性 | 默认并发普通/插桩 820 项通过；macOS fixture 启动诊断与 10 秒测试预算 | 不忽略失败、不放宽生产超时 |
| L5 | completed | 双语说明、项目地图、同平台覆盖率对比和门禁 | fmt/check/Clippy/test/LCOV/target check/build/diff | Windows/Linux 保留原生 CI；不截图 |

## 已完成

- 已读取项目技能、Rust/Slint/架构契约、地图与前轮记录，保留原工作区 Windows cfg 修复。
- 本机串行 LCOV 已存在 `target/coverage/codecov-serial.lcov`，业务源码按 DA 行归一化为 64.80%，app 子树 56.46%；inline 测试代码也进入原始报告。
- 目录监听、终端 clipboard、connection worker_start 分别 0/107、0/88、0/146；媒体 60/559。以实际行为优先补测试，拒绝通过排除业务行提高比例。

## 验证

- 已完成：391 library + 429 binary、Doc-tests 0；默认并发普通/插桩及串行 LCOV 均通过；fmt/check/严格 Clippy、ARM64 target check/build、diff check 通过；双语 coverage、地图和月度记录已同步。
- 未完成：Windows/Linux 原生 runner、真实系统凭据/设备/GUI/SSH 仍按项目边界由 CI/用户验收。

## 风险与阻塞

- macOS POSIX 音频 fixture 曾在 dyld 启动通知阶段偶发阻塞；测试增加了仅 macOS 的 fixture gate、隔离 stderr 和 10 秒启动预算，生产 helper 未改变。
- 本机无 Windows MSVC/Linux SDK；可运行 macOS 原生检查，跨平台通过不作推测。
- 原生 GUI、通知/剪贴板设备和真实 SSH 行为仍由用户验收，测试用确定性适配器。

## 下一步

- 交付覆盖率报告与测试回归；Windows/Linux 继续由原生 CI 验证。

## 最后更新时间

- 2026-10-13 12:10 +0800
