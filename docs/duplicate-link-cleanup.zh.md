[English](duplicate-link-cleanup.md) · [架构说明](architecture.zh.md)

# 2026-10-03 重复链路清理记录

用户在全仓只读审查后授权实施。本轮按三批方案清理闲置入口、收敛共同流程并补充回归。
施工前保存工作区差异，保留已有 SFTP 标题栏控件和本地 Hidden 改动。

## 第一批：闲置入口与重复实例

| 项目 | 处理结果 | 依据与有效功能 |
| --- | --- | --- |
| `vendor/vt100/` | 删除历史副本及对应声明 | Cargo manifest/lock 无引用；终端使用 `alacritty_terminal` |
| 两份重复图标 | 删除 formats 目录中的 `terminal_icon.svg`、`terminal_icon.png` | 根 SVG 为唯一源；PNG 与保留的 1024px 文件完全相同；各平台有效图标保留 |
| 远端 load/close UI 链路 | 删除 Slint callback、Rust handler 和闲置辅助函数 | Edit 使用 `ReadText`；关闭使用工作区 Tab 生命周期 |
| 单上传、OpenSftp/CloseSftp 包装 | 删除无仓库调用的方法和命令 | 上传保留批量入口，SFTP 启动/关闭由独立 worker 负责 |
| 旧 SFTP 侧栏模式 | 删除 `standalone` 分支和尺寸 token | 两处调用都使用完整工作区模式 |
| 独立窗口隐藏 SFTP 副本 | 主布局只在主窗口创建 SFTP | 修复隐藏副本覆盖可见窗口拖放区域的问题 |

## 第二批：共同应用流程

| 项目 | 共同入口 | 保留的入口差异 |
| --- | --- | --- |
| Settings 预览/保存 | `SettingsPane.submit-draft` | 共用 84 项草稿参数，Tab ID/关闭标志仍区分预览与持久化 |
| 自带字体读取/注册 | `load_settings_fonts`、`register_settings_fonts` | IO 在 blocking task，注册在 UI 线程；preview 仍按变化的字体加载 |
| 本地/远端目录历史 | `complete_directory_history` | 读取、路径校验、分页和请求隔离分别保留 |
| Telnet/Serial 监控 | `spawn_direct_monitor` | 各自 worker、事件适配、端点状态、协议编码和 attempt 校验保留 |

## 第三批：生命周期与交付

| 项目 | 处理结果 | 保留的约束 |
| --- | --- | --- |
| SSH 新建/恢复探测 | 共用 `spawn_host_key_probe` 和结果状态转换 | 取消、Tab/profile/目标/phase 校验；未知、变更、撤销密钥仍受信任规则约束 |
| shell 内旧 SFTP browser | 删除浏览器创建、事件转发和关闭分支 | SFTP-only worker 独占 browser/transfer；终端 shell/X11 保留 |
| 无 router 单窗口刷新 | 移除旧 snapshot/model 分支 | 启动必须成功安装 router，再接线；统一使用有界多窗口刷新 |
| 原子文件写入 | 共用 `write_file_contents_atomically` | 私有临时文件、fsync、关闭句柄、原子替换、失败清理；用户目录不改权限，私有目录收紧权限 |
| Linux Debian 包 | Cargo metadata 为唯一安装清单，CI 使用 cargo-deb 3.7.0 | 字体及许可证、九档图标、desktop entry、声明文件；目标架构和版本检查；原生 Linux 自动发现运行库依赖 |

移除的 Rust handle 方法为 `request_open_sftp`、`request_close_sftp` 和
`request_open_sftp_upload`。此仓库 `publish = false`；仓库内所有有效调用均已核对。
外部直接依赖这些方法的消费者需要转用 SFTP-only worker 或批量上传入口。

## 验证与验收

- Rust/Cargo 1.97.1 在 `CARGO_NET_OFFLINE=true` 下，对 `aarch64-apple-darwin` 和
  `x86_64-apple-darwin` 分别执行 CI 同款 check、严格 all-target Clippy 和 build，全部通过；
  未运行非原生 Intel 测试。

- 本机 locked/offline fmt、check、严格 all-target Clippy、完整 test 和 debug build 通过；
  库 303 项、应用 312 项，无失败。发布 helper 14 项、512 条翻译及文档/skill/tracker 检查通过。
- amd64/arm64 Debian fixture 各 30 项清单资源逐项验证内容、路径、权限、root 所有权，
  并校验包名、版本与架构。

- 旧代码数值回归：隐藏主布局侧栏从 220 调至 360 时，独立 SFTP local drop width 从
  476 变为 406；修复后侧栏不影响独立窗口区域，窗口缩放及返回主窗口仍有效。
- 新增探测结果回归覆盖 Terminal/SFTP 目标上的 trusted/unknown/changed/revoked 决策，
  以及过期 Tab/profile/目标/phase 和取消结果；transport 拒绝式校验继续由已有 loopback 测试覆盖。
- 新增原子写入回归覆盖父目录权限差异、替换后的 0600 权限、失败时保留原目标并清理临时文件。
- 完整验证结果在[当前实施记录](project-implementation-tracker/current.md)和
  [月度记录](project-implementation-tracker/changes/2026/10.md)中维护。
- GUI 视觉、原生拖放、焦点、真实 SSH/Telnet/Serial 生命周期由用户验收；本机没有 Windows/Linux
  SDK 和原生 runner，这些平台保留 CI 验证。打包 fixture 仅检查资源和控制字段，不代替 Linux 安装测试。

## 打包来源

基于[上游 cargo-deb 合约](https://github.com/kornelski/cargo-deb#packagemetadatadeb-options)
及本机 3.7.0 CLI 核对资源 glob、`target/release/` 替换和 `--no-build` 行为。
没有升级 Rust 依赖、降低 CI 门禁或修改 SSH 信任策略。
