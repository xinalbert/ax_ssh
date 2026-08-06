# SFTP 图标与双击打开实现评估

- 主题目录：`docs/benchmark-grounded-method-research/sftp-icons-local-open/`
- 来源跟踪：`source-tracking.md`
- 检索日期：2026-08-06

## 需求定义

- 任务：为 AxSSH 的 SFTP 双栏列表增加远端/本地图标，并定义文件双击行为。
- 输入输出：输入是受当前 Tab snapshot 约束的 `SftpEntry` / `LocalDirectoryEntry`；输出是 Slint 可消费的有界图标 DTO、打开意图、下载进度和错误状态。
- 约束：Rust 2024、MSRV 1.92.0、Slint 1.17.1、Tokio/russh worker 所有权不变；UI 不执行文件系统、网络或平台图标查询；不得把 `third_package/axshell` 加入构建图。
- 目标指标：目录行不阻塞；常见扩展名有系统关联图标；本地 regular file 双击可交给默认应用；远端 regular file 下载完成后可交给默认应用；取消、失败、关闭 Tab 和重启清理可验证。
- 不可接受项：Slint 直接打开任意字符串路径；把远端路径作为本机打开目标；在每一行渲染期间调用平台 API；无界下载/事件队列；保存密码、主机密钥或远端文件内容到 profile 配置；首版隐式实现编辑回传和冲突覆盖。

## 当前 AxSSH 基线

- `ui/sftp-pane.slint` 的 `SftpEntryRow` 只有 `name/path/kind/size/modified/hidden/selected`，当前行用文本 `>`/`@` 表示目录/链接。
- `SftpEntry` 和 `LocalDirectoryEntry` 已带有目录、符号链接、大小和时间元数据；本地目录读取在 blocking 边界执行，并有 250 条、名称、路径和文本预算。
- `src/app/sftp_bridge.rs` 目前只转发目录导航、分页和选择意图；文件双击只对目录调用 `open-directory`。
- `src/ssh/worker.rs` 的 SFTP-only worker 只拥有一个浏览 subsystem；`SshConnection::open_sftp_stream` 已能在同一已认证 transport 上开启独立 subsystem，但下载、进度和取消尚未存在。
- `Cargo.toml` 已有 `open = "5.3.6"`，`src/app.rs` 已使用 `open::that_detached`；因此本地/缓存文件的默认应用打开不需要另引入启动器。
- 当前 Transfers 区域是视觉状态。架构文档已经要求上传/下载/编辑新增确认、进度、取消和冲突契约，本评估遵守该边界。

## 检索范围与纳入标准

- 主源范围：WinSCP、Cyberduck、VS Code、Apple AppKit、Microsoft Shell API、freedesktop 规范、Qt 官方文档、Rust `open` 官方 API 文档，以及本机锁定依赖的 API 生成代码。
- 纳入标准：能说明远端文件如何交给本机应用、临时文件如何管理、系统图标如何按文件类型取得，或能说明远端文件系统编辑与本机外部应用的边界。
- 排除标准：营销页面、没有行为细节的空页面、无法回溯协议或生命周期的二手排名，以及把参考项目源代码当作 AxSSH 实现来源。
- 参考项目：本机 `third_package/axshell` 仅用于行为和边界核对，不作为依赖、源码、生成输入或文档链接目标；相关来源和处理状态见 `source-tracking.md`。

## 候选方法概览

### 1. 系统图标服务（推荐主路线）

- 核心思路：把目录、通用文件和受控常见扩展名归一化为小集合 key；后台/启动阶段解析系统图标并转成 Slint `Image` 可读的 PNG/内存数据；列表渲染只读内存缓存。
- macOS：Apple 文档说明 `NSWorkspace.icon(forFile:)` 和 `icon(forFileType:)` 返回初始 32×32 `NSImage`，图标查询可从任意线程调用；`icon(forFileType:)` 接受扩展名、HFS type 或 UTI，但当前 API 标记为 macOS 10.0-12.0，且锁定的 `objc2-app-kit 0.3.2` 将其标记为 deprecated。实施前应做 P1 API spike，优先验证 `iconForContentType` + `UTType` feature，无法稳定编译时再退回扩展名 API。
- Windows：Microsoft 文档说明 `SHGetFileInfoW` 的 `SHGFI_USEFILEATTRIBUTES` 不要求路径真实存在，可用合成的 `remote.pdf` 查询关联图标；`SHGFI_ICON`/`SHGFI_SMALLICON` 返回 Shell 图标句柄，需在平台适配层完成句柄释放和 PNG 转换。
- Linux：freedesktop Shared MIME-info 规范允许按文件名或内容识别 MIME；Icon Theme 规范把 icon name 和 nominal size 映射到主题文件，支持继承和 `hicolor` 回退。远端文件没有本机路径时，应使用扩展名 MIME 猜测和主题 icon name，不读取远端内容。
- 主要限制：系统图标 API 返回平台对象/句柄，必须在平台层转成有界、可跨线程、可缓存的位图；主题变化需要使缓存 identity 失效。

### 2. 静态扩展名/文件名图标主题（推荐回退）

- 核心思路：类似 VS Code File Icon Theme，按 folder、folder name、file name、file extension 和 language ID 映射到静态 SVG/PNG 或字体 glyph。
- 适用条件：平台 API 不可用、无桌面主题、测试环境需要确定性图标，或希望为常见源码文件保持统一视觉。
- 主要限制：它不是系统默认关联图标；映射表和资产会持续维护，不能声称等价于 Finder/Explorer 文件图标。

### 3. 跨平台框架文件图标提供器（行为参考）

- Qt `QFileIconProvider` 以 `QFileInfo` 或通用 `IconType` 提供文件/目录图标，是成熟框架对“文件系统图标由 provider 隔离”的行为参考。
- AxSSH 不引入 Qt；可借鉴 provider/cache 边界，把系统 API 和 Slint DTO 隔离。

### 4. 远端临时副本后交给外部应用（推荐首版远端双击）

- WinSCP：远端文件先下载到 temporary directory，再交给编辑器或关联应用；默认双击可编辑；监听本地副本变化并上传。其文档特别说明外部编辑器可能很快退出或只通知已有进程，因此不能用启动子进程退出判断“文件已关闭”。临时目录在退出时清理，并在下次启动检查遗留目录。
- Cyberduck：外部编辑器打开临时下载副本，用户在编辑器中保存时上传；还提供临时文件上传开关和版本化目录，说明“受管编辑”远大于一次性 Open。
- AxSSH 取舍：第一阶段只做 read-only download-then-open，不监听、不自动上传、不判断编辑器关闭、不覆盖远端；后续另建受管编辑目标。

### 5. 远端文件系统代理/内置编辑器

- VS Code Remote 的 UI extension 在本地、workspace extension 在远端，编辑器通过内部 remote file system provider 透明读写。
- 这条路线适合 AxSSH 自己拥有编辑器或协议代理，不适合当前“使用系统默认本地程序打开”的需求；不进入本轮。

## Benchmark / 公开评测对比

未检索到与“远端文件图标 + SFTP 双击交给本地默认应用”直接对应的统一 benchmark、leaderboard 或 shared task。以下是 proxy evidence，不构成性能排名，也不把不同产品的功能数量合并评分。

| 方法/产品 | 公开评测任务 | Metric/可比结果 | 协议备注 | 来源 |
|---|---|---|---|---|
| WinSCP temporary edit/open | 远端文件外部打开与回传 | 未提供统一 benchmark 数值；文档给出生命周期行为 | Windows 客户端，文档描述默认双击、临时目录、后台队列和启动清理 | [task_edit](https://winscp.net/eng/docs/task_edit)、[temp_folders](https://winscp.net/eng/docs/temp_folders) |
| Cyberduck external editor | 远端文件编辑保存 | 未提供统一 benchmark 数值；文档给出上传/版本行为 | 外部编辑器、临时副本、保存上传，支持禁用临时文件上传 | [Edit Files](https://docs.cyberduck.io/cyberduck/edit/) |
| Qt `QFileIconProvider` | 本地文件系统图标 | 未提供跨平台图标准确率/延迟 benchmark | 框架 provider 行为，不是独立 SFTP 产品 | [Qt 6 docs](https://doc.qt.io/qt-6/qfileiconprovider.html) |
| VS Code File Icon Theme | 文件名/扩展名图标映射 | 未提供统一 benchmark 数值；规范定义匹配优先级 | 静态主题映射，非系统关联图标 | [File Icon Theme](https://code.visualstudio.com/api/extension-guides/file-icon-theme) |

因此本项目的可验收指标应是 AxSSH 自己的契约测试和三平台人工验收：首屏列表不阻塞、常见扩展名命中/回退稳定、路径不越界、下载可取消、失败不打开半文件、缓存可清理，而不是跨产品伪排名。

## 推荐结论

- benchmark-best：未定义。没有直接可比的公开 benchmark。
- constraint-best：系统图标 provider + 受控扩展名 fallback；本地文件校验后 `open::that_detached`；远端文件使用同一已认证 SSH transport 开独立 SFTP subsystem，下载到 `ProjectDirs` 私有 cache 后再调用 `open::that_detached`。
- evidence-best：WinSCP/Cyberduck 的临时副本生命周期证据，Apple/Microsoft/freedesktop 的平台图标 API/规范证据，Rust `open::that_detached` 的 detached process 语义证据。
- 推荐理由：复用现有 `open` 依赖和 `russh` 认证生命周期；不把远端路径伪装成本地路径；可以把图标、打开意图、下载 transfer 和未来编辑回传分成可测试的所有权边界。

### 建议的第一阶段用户语义

1. 目录双击仍然只导航。
2. 本地 regular file 双击：Rust 重新确认该路径仍在当前 local snapshot，重新 canonicalize，确认是 regular file 后调用 `open::that_detached`。
3. 远端 regular file 双击：Rust 重新确认条目仍在当前 remote snapshot 且不是目录/链接，创建有界 transfer，下载成功并 fsync/rename 后才调用 `open::that_detached`。
4. 远端链接、目录、不可见/已离开 snapshot 的条目：拒绝打开并显示 bounded status；不把 UI 传来的任意字符串直接交给 OS。
5. 进程退出或后续启动清理只针对 AxSSH 自己的 cache namespace；外部应用仍持有的文件延迟删除并记录待清理状态，不强制删除。

### 预定平台实现方向

- macOS：P1 验证 `objc2-app-kit 0.3.2` 的 `NSWorkspace::iconForContentType` 和 `objc2-uniform-type-identifiers` feature；以 `iconForFileType` 作为兼容 fallback。不得把 AppKit 对象跨 UI/worker 边界传递。
- Windows：扩展 `windows-sys` feature 到 Shell/GDI 所需 API，在 `SHGetFileInfoW` 合成路径上使用 `SHGFI_USEFILEATTRIBUTES | SHGFI_ICON | SHGFI_SMALLICON`；复制 PNG 后立即 `DestroyIcon`。
- Linux：用 MIME extension guess + freedesktop theme lookup；优先当前主题，递归继承，最后 `hicolor`/通用文件图标；依赖的 MSRV、feature 和许可证在 P1 锁定。
- 所有平台：目录/链接/通用文件回退图标始终存在；图标缓存按 platform/theme/API version identity 失效，列表只消费已解析内存结果。

## 不确定项与不可比项

- 未找到统一公开 benchmark，因此没有“最快/最准图标方案”的证据。
- Apple `icon(forFileType:)` 在 Apple 文档当前页面的 availability 仅到 macOS 12，并标记为 deprecated；现代 `UTType` bridge 的 Cargo feature 和最低 macOS target 需要 P1 编译 spike。
- Windows Shell 图标句柄到 PNG 的 DPI、主题和线程行为需要 Windows 目标机验收；远端合成路径不能提供真实文件的 overlay 状态。
- Linux 桌面可能没有 GTK theme metadata；MIME/icon theme 依赖的系统文件、sandbox 和 Wayland/X11 环境需要目标机验收。
- 外部应用可复用已有进程并立即退出，不能用 child process lifetime 推断编辑器关闭；这也是首版不做自动回传的原因。
- 远端 symlink 是否解析由不同 SFTP server 决定；首版建议拒绝链接双击/下载，后续以明确 readlink、权限和目标范围契约再开放。
- 远端大文件、断线重连、缓存空间不足和用户主动关闭 SFTP Tab 时的 transfer 语义需要实现阶段定义；不能通过现有“视觉 Transfers”假设带过。
- FileZilla 当前官方 Wiki `Remote_file_editing` 页面没有可用正文，因此不承载核心结论，详见来源跟踪中的 excluded 记录。

## 参考来源

- 主源：
  - [WinSCP Editing/Opening Files](https://winscp.net/eng/docs/task_edit)
  - [WinSCP Temporary Folders](https://winscp.net/eng/docs/temp_folders)
  - [Cyberduck Edit Files](https://docs.cyberduck.io/cyberduck/edit/)
  - [Apple NSWorkspace `icon(forFile:)`](https://docs.developer.apple.com/documentation/appkit/nsworkspace/icon%28forfile%3A%29)
  - [Apple NSWorkspace `icon(forFileType:)`](https://docs.developer.apple.com/documentation/appkit/nsworkspace/icon%28forfiletype%3A%29)
  - [Apple UTType](https://docs.developer.apple.com/documentation/uniformtypeidentifiers/uttype-swift.struct)
  - [Microsoft `SHGetFileInfoW`](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shgetfileinfow)
  - [freedesktop Shared MIME-info specification](https://specifications.freedesktop.org/shared-mime-info-spec/latest/)
  - [freedesktop Icon Theme specification](https://specifications.freedesktop.org/icon-theme-spec/latest/)
  - [Qt `QFileIconProvider`](https://doc.qt.io/qt-6/qfileiconprovider.html)
  - [VS Code File Icon Theme](https://code.visualstudio.com/api/extension-guides/file-icon-theme)
  - [VS Code Remote Extensions architecture](https://code.visualstudio.com/raw/api/advanced-topics/remote-extensions.md)
  - [`open::that_detached`](https://docs.rs/open/5.3.6/open/fn.that_detached.html)
- 补充来源：
  - AxShell 本机参考 checkout：`third_package/axshell/src/platform/file_icons.rs`、`third_package/axshell/src/sftp/worker.rs`、`third_package/axshell/src/sftp/transfer.rs`；仅行为参考。
  - `objc2-app-kit 0.3.2`、`russh 0.62.2`、`russh-sftp 2.3.0` 的本机 Cargo registry 源码；仅用于核对锁定 API，不进入项目源码或构建图。
