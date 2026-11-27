# 当前项目实施记录

## 当前目标

- 目标 ID：20261127-terminal-protocol-base
- 目标：按 base 计划逐步复现并补齐选定终端协议行为。
- 交付物：最小字节回归、各阶段支持决策、实现、双语说明和验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：[终端协议 base](notes/base/plan.md) 的 B1–B8，当前修复主屏宽字符重排的光标偏移。
- 不在本轮范围内：开启额外远程权限、复刻所有 VT 型号或厂商协议、前序 SFTP 验收。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| B0 | completed | base 参考、来源与入口归档 | 文档/tracker/skill/diff 检查 | 文档完成不代表能力补齐 |
| B1 | completed | 最小复现样本与支持范围决定 | 分片/同批时序/逐字节应答，冻结期望和政策 | 本机字节样本完成 |
| B2 | completed | CSI/SGR 子集方案 | G14/G08/C14/C20，解析到呈现/输入/回写 | 不预先决定升级或修改上游 |
| B3 | completed | DEC/备用屏/键鼠范围方案 | I05/I13/I14/D12，模式开关与查询一致 | 不宣称某型号全仿真 |
| B4 | completed | OSC 范围与差异方案 | OC3/O52e/f/h/O08d、BEL/ST、批次/拥塞 | 保留授权/代次/资源上限 |
| B5 | completed | Unicode 字素/格宽/布局策略 | U07–U13，reflow/删除/选区复制/光标/混合脚本 | 视觉由用户验收 |
| B6 | completed | 扩展取舍记录 | Shell integration/DCS/图形/C1 的真实需求及安全边界 | 不因规范收录而默认启用 |
| B7 | blocked | 实施后逐项状态与双语说明 | 定向回归、仓库 Cargo 门禁、原生平台验收 | 自动化已通过，等待原生 GUI 和真实 SSH 验收 |
| B8 | completed | 修复双格字符在行边界重排时的光标列 | 40→10→40、11 列对照、复制与后续写入 | 101 项模型定向和 1098 项全量通过 |

## 已完成

- B0 已归档；B1 在原实现中复现 SGR 21、LNM、扩展鼠标模式查询、备用屏和 OSC 颜色时序五项差异。
- B2 已通过四项专项，包括 SGR 21、LNM、IND 与鼠标模式查询。
- B3 已验证 1048 配合主屏和 1049 备用屏分别保存光标。
- B4 已验证 OSC 时序、终止、长度限制、剪贴板目标和链接分组，96 项 terminal 模型测试与 54 项 VTE 测试通过。
- B5 已完成 13 项 base 专项，覆盖 UTF-8 单字节分片、光标列、复制、重排和组合字符删除。
- B6 已记录扩展取舍与重新评估条件。
- B7 的双语架构、使用、开发说明和实施矩阵已更新。
- B8 修复宽字符换行占位导致的光标偏移。重排后下一字符追加到原文末尾，复制保留原文。

## 验证

- 已完成：13 项 base 专项、原生 PTY 字节往返、完整 1095 项 Rust 测试（506 library、582 binary、7 integration）、VTE 54 + 34 项、Rust 1.92 check、macOS ARM64 原生及 CI 目标 check/严格 Clippy/build、Windows x86_64 交叉 check/严格 Clippy/build、仓库入口/翻译/图标/skill/tracker/diff。
- B8 已完成：101 项终端模型定向、1098 项全量 Rust 测试（509 library、582 binary、7 integration）、macOS ARM64 check/严格 Clippy/build、Rust 1.92 check、Windows x86_64 交叉 check/严格 Clippy/build。Alacritty 搭配原版 VTE 的 132 项单元、45 项录制和 1 项 doctest 通过。
- 未完成：真实 SSH 服务器、vttest 与 UAX 一致性套件、用户原生 GUI 验收、CI 其它原生平台。

## 风险与阻塞

- 47/1047、文字闪烁及扩展协议的取舍见 [base 计划](notes/base/plan.md#b6-扩展取舍)。
- B7 人工验收需要用户确认原生界面，并需要对应原生 runner 和 SSH 服务器。

## 下一步

- 完成 B7 的对应平台原生运行、真实 SSH 服务器和用户 GUI 验收。
- 前序 SFTP 的 R3 进度保留在[月度记录](changes/2026/11.md)。

## 最后更新时间

- 2026-11-27 17:25 +0800
