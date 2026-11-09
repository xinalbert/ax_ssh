# 当前项目实施记录

## 当前目标

- 目标 ID：20261109-macos-runner-retirement
- 目标：修复 macOS CI 因退役 runner 配置而自动取消的问题，保留所有平台与原有缓存。
- 交付物：CI/Release 的 macos-15 runner 配置、双语说明、根因证据和验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`.github/workflows/{ci,release}.yml`、双语开发说明和实施/研究记录。
- 不在本轮范围内：应用源码、SSH 信任与凭据、依赖版本、缓存策略或远端删除、推送。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| P1 | completed | 撤销误解产生的改动，核实实际取消原因 | 干净工作树、CI #153 annotations、官方退役公告 | 两个 macOS job 均提示配置不受支持 |
| P2 | completed | CI/Release 五处 runner 更新及双语说明 | YAML 结构对比、官方支持标签 | 五 target、Intel 交叉编译和缓存保持原样 |
| P3 | completed | 静态、编译及文档门禁 | actionlint、Cargo check、Markdown/skill/tracker/diff | 远端新 runner 待推送后验证 |

## 已完成

- 用户澄清 macOS job 自动取消，并未要求停用；已撤销前轮全部未提交修改并确认工作树干净。
- CI #153 的两个 macOS job 显示 The configuration 'macos-14-arm64-us-default' is not supported。
- 官方公告明确 macos-14 于 2026-11-02 退役，推荐 macos-15 等标签；这解释了 2026-11-08 的取消。
- 现有 fail-fast=false，CI 没有 concurrency 自动取消配置；10 GB 缓存限制不是此错误的原因。
- 项目地图已覆盖两个工作流；本轮无新增入口或模块，无需刷新地图。
- 已将 CI 两处、Release 三处 runner 更新为 macos-15；双语开发说明和有来源的根因记录已同步。
- 结构对比确认两个工作流除五处 runner 标签外无变化；保留五 target、测试条件和原缓存。
- 两条旧记录以 d2e3be9 的 Git 入库时间补齐字段，并明确原事件具体时分未知。
- 用户已授权本地提交；历史时间字段以 720ae69 独立提交，runner 修复与配套说明另成一个提交，具体结果以 Git 历史为准。

## 验证

- 已完成：工作树恢复、远端错误与官方 runner 标签核对；actionlint、精确 YAML 对比、locked/offline Cargo check、56 份 Markdown/边界、项目 skill、tracker 和 diff 检查。
- 未完成：新 runner 远端 CI 和 Release 构建；未推送，未清理缓存。本轮不涉及 Rust/Slint 源码，未重复 Rust 测试。

## 风险与阻塞

- GitHub API 匿名请求限流；已通过公开 Actions 页面取得 annotation，但完整日志仍需登录。
- macos-15 是 ARM64 runner；Intel target 延续交叉编译，只编译不运行测试。
- 旧运行重跑仍使用旧提交中的 macos-14；修复需要新工作流进入远端后触发新运行。
- 缓存实际大小未取得，暂不对缓存配置作推测性调整。

## 下一步

- 将变更推送后触发新 CI，确认两个 macOS job 分配到 macos-15；旧 run 重跑不能验证新工作流。缓存容量另依据实际条目评估。

## 最后更新时间

- 2026-11-09 10:29 +0800
