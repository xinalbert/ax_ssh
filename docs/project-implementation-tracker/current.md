# 当前项目实施记录

## 当前目标

- 目标 ID：20260930-sftp-open-cache-config
- 目标：在 Settings > SFTP 配置本地文件双击打开时的私有快照单文件上限；精确值 0 同时取消单文件和缓存总字节上限。
- 交付物：兼容旧配置的全局设置、缓存复制与配额校验、界面/翻译、回归及双语说明纠错。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/config/`、`src/sftp/transfer/`、`src/app/{settings_bridge,sftp_bridge,view}`、`ui/settings/`、设置契约、翻译与双语文档。
- 不在本轮范围内：`third_package/axshell`、SSH host-key/凭据、服务器级传输覆盖、常规流式上传/下载、远端拖到 Finder、已安装应用包替换和 GUI 视觉验收。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| CACHE1 | completed | 全局缓存单文件设置、旧值默认及配额联动 | 配置 round-trip、边界与配额回归 | 用户确认 0＝不限单文件及缓存总字节；默认 512 MiB |
| CACHE2 | completed | Settings > SFTP 控件与 Rust/Slint 接线 | Slint 重编译、设置搜索和翻译校验 | 本地设置，不进服务器覆盖 |
| CACHE3 | completed | 双语纠错、完整门禁与交付记录 | fmt/check/Clippy/test/diff/tracker | 更正 Finder 拖出不走缓存的错误描述 |

## 已完成

- 只读核对：本地文件双击打开走 `snapshot_local_file_for_open`，当前单文件固定 512 MiB；相同私有缓存的总量固定 1 GiB，已完成条目可按需清退，活动 partial 不会被误删。
- 远端拖到 Finder 走显式目标的 `download_to_local`，不经过这个私有缓存；先前双语说明与实施记录把它写入缓存限制是错误的，本轮更正。
- 工作区已有前一轮 SFTP 与字体相关未提交改动；本轮仅在相交文件上追加当前需求并保留其它改动。
- 用户确认 0 表示不限：配置持久化会保留 0，缓存复制用无限字节阈值；128 文件数、扫描、过期清理和超时边界未放宽。
- 复查发现旧的等待超时未取消后台复制；现用任务生命周期持有取消信号，超时或异步任务结束时通知 blocking 复制，在下一次有界读取前停止并清理 partial。

## 验证

- 已完成：前置源码、Manifest、设置/缓存路径只读核对。
- 已完成：最终版 Slint 重编译、配置/缓存/取消定向测试、fmt、locked/offline check、严格 Clippy、完整测试（库 293、应用 291）、翻译目录与 `msgfmt`、`git diff --check`。tracker validator 仅报既有 54 项历史格式错误，新记录无新增错误。
- 未完成：真实 GUI 视觉与本地大文件打开验收（由用户在新构建上确认）。

## 风险与阻塞

- 单独调高单文件阈值会被现有 1 GiB 缓存配额挡住，须同步让本地快照配额至少覆盖配置值；缓存文件数、扫描、清理与本地 snapshot 超时保持有界。
- 用户明确要求 0 同时取消本地快照单文件与缓存总字节上限；这可能耗尽本地磁盘空间，界面和双语文档已提示。
- GUI 视觉由用户验收，代理不采集自身应用截图作为证据。

## 下一步

- 用户在新构建中验收 Settings > SFTP 的 0/有限值界面与本地大文件打开；本轮不替换已安装应用包。

## 最后更新时间

- 2026-09-30 16:17 +0800
