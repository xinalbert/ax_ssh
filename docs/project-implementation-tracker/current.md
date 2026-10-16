# 当前项目实施记录

## 当前目标

- 目标 ID：20261016-dependency-refresh
- 目标：更新用户选定的 Tokio、arboard、serde_json、uuid、rubato 和 base64，并完成 rubato 音频接口迁移及验证。
- 交付物：限定范围的 Cargo 依赖/锁文件、音频适配、有效回归、双语开发说明、实施记录和两笔本地提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`Cargo.toml`、`Cargo.lock`、`src/audio.rs`、双语开发说明与 `docs/project-implementation-tracker/`。
- 不在本轮范围内：CPAL/Slint/russh 升级、工具链或部署下限变更、UI/SSH 协议修改、安装版替换、推送和发布。
- 所有权：Cargo 管理依赖解析；音频 worker 私有的 PcmConverter 适配 rubato，不向 UI 或 SSH 暴露 resampler/buffer。
- 安全：保留 host-key 默认拒绝、凭据与 OSC 52 边界；base64 编码策略不变。音频仍在后台处理、有界排队、取消并关闭设备，不在原生回调分配或等待。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| D1 | completed | 六项版本、MSRV、原生平台和上游 API 核对 | manifest/lock、CI、发布说明和 rubato 源码 | Rust 2024/MSRV 1.92、CPAL 0.17.3/macOS 11 保留 |
| D2 | completed | 依赖更新及 rubato 5.0.1 接口迁移 | 精确解析、锁文件差异和 FFT 固定输入/输出缓冲适配 | 仅更新指定依赖及不可避免的传递依赖，不执行全量 cargo update |
| D3 | completed | 音频尾部、延迟、声道与编码回归 | 聚焦音频、凭据、known_hosts/OSC 52 回归 | 复用已有矩阵，补覆盖迁移风险的数值断言 |
| D4 | completed | Cargo 门禁和安全复查 | fmt/check/严格 all-target Clippy/test/MSRV/audit/diff | 本机 ARM64 macOS；Windows/Linux 原生和真实剪贴板/麦克风另验 |
| D5 | completed | 双语说明及跟踪收尾 | 相对链接、skill/tracker 和最终差异审阅 | 升级实施阶段完成；提交由后续用户授权 |
| DC1 | completed | 六项依赖、音频迁移与双语说明提交 | 显式暂存并审阅完整 staged diff | a4d5b46；沿用上一轮通过的源码，不重跑测试或编译 |
| DC2 | completed | 研究/实施记录提交及收尾 | 链接/current/追加记录/diff 和提交后状态 | 仅本地提交，不推送 |

## 已完成

- 基线 `6a56a97` 工作区干净。前轮审计已核对 53 项直接依赖及 784 个锁定包，发现四项兼容更新；用户随后明确授权 rubato/base64 一并更新。
- rubato 5.0.1 MSRV 为 1.87，满足项目 1.92；官方提供 Fft + FixedSync::Both 与零拷贝 SequentialSliceOfVecs 适配器，可保持现有流式分块/尾部排空设计。
- CPAL 0.18.2 要求 macOS 14.2，故保持项目主动锁定的 0.17.3；Slint 本地补丁与 russh/RSA 公告例外不在更新范围。
- 用户授权本地提交后，五份依赖/音频适配/双语说明已提交为 `a4d5b46`，完整 staged diff 已审阅；研究/实施记录单独归档。本轮不重复测试或编译。

## 验证

- 已完成：六项指定依赖解析/源码迁移；12 项音频聚焦测试及全量 875 项测试（394 library + 481 binary）通过；fmt、locked/offline check、严格 all-target Clippy 无 warning；Rust 1.92.0 ARM64 macOS target check 通过。790 个锁定包安全复查在既有 RSA 公告例外下通过，仅保留 bincode/ttf-parser 停止维护提示。10 条相对链接、项目 skill、本轮 current/新增跟踪记录与 diff 检查通过，完整 tracker 的 84 项历史格式错误未增加。
- 未完成：Windows/Linux 原生 CI、真实剪贴板与麦克风手工验收；不将本机编译和注入设备测试扩大为跨平台设备验收。

## 风险与阻塞

- 本机仅可原生执行 ARM64 macOS；虽然安装 Windows MSVC Rust target，缺少 Windows SDK/执行环境，Windows/Linux 验证交由原生 CI。
- rubato 的延迟移除、未满块补零、最终包长度和抗混叠已通过数值验证；真实设备和跨平台剪贴板仍需要手工验收。
- 前轮 Windows 滚轮夹具修复的原生 CI 仍待验证；本次依赖更新不视为现场 Codex 滚动问题的修复。
- 完整 tracker 既有 84 项历史格式问题，本轮错误集未增加；current 和追加记录单独通过验证。

## 下一步

- 本记录独立提交后核对两笔提交及最终工作区；Windows/Linux 原生 CI 和真实系统剪贴板/麦克风仍待验证。本轮不推送或替换安装版。

## 最后更新时间

- 2026-10-16 11:24 +0800
