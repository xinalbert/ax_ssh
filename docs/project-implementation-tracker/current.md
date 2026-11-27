# 当前项目实施记录

## 当前目标

- 目标 ID：20261127-terminal-protocol-base
- 目标：按 base 计划逐步复现并补齐选定终端协议行为。
- 交付物：最小字节回归、各阶段支持决策、实现、双语说明、验证记录，以及 P0 八类能力的范围与验收计划。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：[终端协议 base](notes/base/plan.md) 的 B1–B8 实现/验证记录与 [P0 基础终端核心](notes/base/plan.md#p0-基础终端核心)的待办、标准映射和验收条件；本次只编辑计划。
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
| P0A | pending | 冻结八类支持范围和标准版本 | 标准/源码/现有条目映射与范围决定 | 本次仅写计划，P0 整体未完成 |
| P0B | pending | C0/ESC 基础及异常恢复验收 | 模式/字符集/分片/取消/未知序列 | 先核对已有证据 |
| P0C | pending | CSI 参数、编辑和查询一致性 | 默认值/边界/拥塞/同连接应答 | 不按类名推断全部实现 |
| P0D | pending | SGR 已实现属性与缺项决定 | 叠加/重置/21/文字闪烁范围 | B6 暂缓不等于功能完成 |
| P0E | pending | 光标和状态联动验收 | 原点/边界/保存恢复/宽字符/DSR | 复用 B8，保留未验收项 |
| P0F | pending | 屏幕控制和模式范围 | 主/备用屏/历史/重置及缺项 | 47/1047 等单独决定 |
| P0G | pending | 16 色端到端验收 | 全部前背景索引、属性和恢复 | 当前实现已有，验收待补齐 |
| P0H | pending | 滚动和重排组合验收 | 区域外不变/历史/选区/宽字符 | 复用已有回归 |
| P0I | pending | UTF-8/Unicode 范围与一致性 | 编码/字素/宽度/Bidi 与官方数据 | 不将按码点计列视作完整 Unicode |
| P0J | pending | P0 汇总和关闭报告 | vttest/真实 SSH/原生平台/用户 GUI | 继承 B7 待办，不自动施工 |

## 已完成

- B0 已归档；B1 在原实现中复现 SGR 21、LNM、扩展鼠标模式查询、备用屏和 OSC 颜色时序五项差异。
- B2 已通过四项专项，包括 SGR 21、LNM、IND 与鼠标模式查询。
- B3 已验证 1048 配合主屏和 1049 备用屏分别保存光标。
- B4 已验证 OSC 时序、终止、长度限制、剪贴板目标和链接分组，96 项 terminal 模型测试与 54 项 VTE 测试通过。
- B5 已完成 13 项 base 专项，覆盖 UTF-8 单字节分片、光标列、复制、重排和组合字符删除。
- B6 已记录扩展取舍与重新评估条件。
- B7 的双语架构、使用、开发说明和实施矩阵已更新。
- B8 修复宽字符换行占位导致的光标偏移。重排后下一字符追加到原文末尾，复制保留原文。
- P0 八类能力的标准映射、当前判断、必选验收提案、缺项决定和 P0A–P0J 待办已写入 base；这仅是计划交付，不是 P0 实现完成。

## 验证

- 已完成：本次 P0 计划的 57 个 Markdown 本地链接/中英配对与源入口、tracker、项目 skill、待办对齐、diff 及 cargo check --locked --offline 均通过；没有执行新增 P0 协议测试。
- 已完成：13 项 base 专项、原生 PTY 字节往返、完整 1095 项 Rust 测试（506 library、582 binary、7 integration）、VTE 54 + 34 项、Rust 1.92 check、macOS ARM64 原生及 CI 目标 check/严格 Clippy/build、Windows x86_64 交叉 check/严格 Clippy/build、仓库入口/翻译/图标/skill/tracker/diff。
- B8 已完成：101 项终端模型定向、1098 项全量 Rust 测试（509 library、582 binary、7 integration）、macOS ARM64 check/严格 Clippy/build、Rust 1.92 check、Windows x86_64 交叉 check/严格 Clippy/build。Alacritty 搭配原版 VTE 的 132 项单元、45 项录制和 1 项 doctest 通过。
- 未完成：真实 SSH 服务器、vttest 与 UAX 一致性套件、用户原生 GUI 验收、CI 其它原生平台。

## 风险与阻塞

- 47/1047、文字闪烁及扩展协议的取舍见 [base 计划](notes/base/plan.md#b6-扩展取舍)。
- B7 人工验收需要用户确认原生界面，并需要对应原生 runner 和 SSH 服务器。
- P0 整体未完成；B2–B6 completed 只代表选定子集、策略和取舍记录完成。文字闪烁、字素/Bidi、屏幕模式等缺项不能因暂缓自动豁免；范围和验收仍按 P0A–P0J 推进。

## 下一步

- P0A–P0J 已列为待办；本次授权仅文档，后续实施需用户指令。先冻结逐项范围，不能把总测试数或 B 阶段完成当作 P0 全部通过。
- 完成 B7 的对应平台原生运行、真实 SSH 服务器和用户 GUI 验收。
- 前序 SFTP 的 R3 进度保留在[月度记录](changes/2026/11.md)。

## 最后更新时间

- 2026-11-27 17:46 +0800
