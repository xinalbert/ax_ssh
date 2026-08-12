# 当前项目实施记录

## 当前目标

- 目标 ID：20260812-local-open-fingerprint
- 目标：强化 SFTP 本地只读打开的文件替换检测，避免快速复用平台文件 ID 时错误接受已替换文件。
- 交付物：跨平台身份与元数据指纹、同路径替换回归、双语安全契约与实施记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app/local_files.rs` 的本地文件 snapshot 与重验、相关 SFTP 本地打开安全说明，以及环境/实施记录。
- 不在本轮范围内：SFTP 协议、远端下载、Slint UI、SSH trust/认证/worker、凭据、日期 tag/发布工作流、依赖升级和参考工程源码。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| LOF1 | completed | 采集并比对跨平台本地文件身份与元数据指纹 | `app::local_files` 定向回归 | 只读 SFTP 本地打开路径；不改变 opener 或缓存所有权。 |
| LOF2 | completed | 覆盖快速同路径替换和现有拒绝路径 | 定向测试与完整 `cargo test` | 身份、长度与时间指纹均需匹配。 |
| LOF3 | completed | 同步双语安全契约、环境和实施记录并完成门禁 | Cargo fmt/check/clippy/test、Markdown/tracker/diff | 不修改依赖、CI 或公开发布标签。 |

## 已完成

- 已完成施工前预检：项目保持 Rust 2024、MSRV 1.92.0、Slint 1.17.1 与锁定离线 Cargo 门禁；本轮不改依赖、工具链或 CI。
- 已确认项目地图已覆盖 `src/app/local_files.rs`、`src/app/sftp_bridge.rs` 与本地打开的 handle/copy 所有权，因此不需要地图结构性刷新。
- 已复跑用户报告的 `local_open_validation_rejects_a_replaced_regular_file` 和完整 `app::local_files` 测试组，当前 `2026-08-12` 标签的工作树均通过；日志中的 149 个应用测试来自身份快照加入前的旧版本。
- 已将目录条目的内部快照改为 `LocalFileFingerprint`，在打开后对同一 handle 复核平台对象 identity、长度、修改时间与创建时间；无法采集指纹时继续拒绝打开。
- 已新增同长度原地改写回归，保留目录、符号链接、路径逃逸和删除后同路径替换的拒绝测试；最终复制仍只使用已验证 handle。

## 验证

- 已完成：根规则、AxSSH Rust/Slint 实施规范、环境记录、项目地图、7 项 `app::local_files` 定向回归、`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 `cargo test --locked --offline`（库 150、应用 153、Doc tests 0）、tracker、Markdown 链接和 `git diff --check`。
- 未完成：目标 Windows 与 Linux 文件系统的手工替换场景尚未执行；CI 将在受支持平台重新编译本次条件编译代码。

## 风险与阻塞

- 文件系统的对象身份可能在删除后快速复用；本轮将身份比对扩展为身份加元数据指纹，仍只从验证后的打开 handle 复制。
- 在重验打开后到私有快照复制前仍存在不可消除的外部文件系统变化；因为复制使用已验证 handle，后续路径替换无法重定向 opener。

## 下一步

- 推送本次安全修复提交；已发布日期标签 `2026-08-12` 保持不变。

## 最后更新时间

- 2026-08-12 22:20 +0800：根据失败报告切换至本地文件替换检测强化；完成环境、项目地图与 `local_files` 定向回归复核，LOF1 开始实施。
- 2026-08-12 22:31 +0800：完成 identity 加元数据指纹、同长度原地修改回归、双语安全契约和全部本地门禁；LOF1-LOF3 完成，待推送后由多平台 CI 复编译。
