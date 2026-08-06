# 来源跟踪

主题：SFTP 图标与双击打开实现评估
检索日期：2026-08-06
说明：本表记录本轮实际检索来源。不存在直接对应的统一 benchmark；产品文档和平台规范均按 proxy evidence 使用。

| source | source type | query/path | date/access date | status | included/excluded reason | evidence use |
|---|---|---|---|---|---|---|
| https://winscp.net/eng/docs/task_edit | 官方产品文档 | WinSCP remote edit/open/double-click | 2026-08-06 | included | 描述临时下载、默认/外部编辑器、双击、监听回传、后台队列和外部进程复用 | 远端临时副本与“不能用子进程退出判断关闭” |
| https://winscp.net/eng/docs/temp_folders | 官方产品文档 | WinSCP temporary folder cleanup | 2026-08-06 | included | 描述退出清理、异常退出后的启动检查和用户配置 | AxSSH cache namespace、重启清理和延迟删除 |
| https://docs.cyberduck.io/cyberduck/edit/ | 官方产品文档 | Cyberduck external editor/save/upload | 2026-08-06 | included | 描述临时副本、保存上传、默认 editor、禁用临时上传和版本化 | 受管编辑属于独立后续目标 |
| https://code.visualstudio.com/raw/api/advanced-topics/remote-extensions.md | 官方架构文档 | VS Code remote extensions file system | 2026-08-06 | included | 描述本地 UI extension 与远端 workspace extension 的职责 | 解释为什么 remote FS/editor 路线不适合首版系统默认应用 |
| https://doc.qt.io/qt-6/qfileiconprovider.html | 官方框架文档 | Qt QFileIconProvider | 2026-08-06 | included | 描述 QFileInfo/IconType provider 抽象 | 图标 provider/cache 的行为参考，不引入 Qt |
| https://code.visualstudio.com/api/extension-guides/file-icon-theme | 官方扩展文档 | VS Code file icon theme associations | 2026-08-06 | included | 描述 folder/name/extension/language ID 映射及匹配优先级 | 静态扩展名 fallback 方案 |
| https://developer.apple.com/documentation/appkit/nsworkspace/icon(forfile:) | 官方平台文档 | NSWorkspace icon for file | 2026-08-06 | included | 描述 32×32 NSImage、路径参数和线程调用语义 | 本地真实文件图标 API 证据 |
| https://developer.apple.com/documentation/appkit/nsworkspace/icon(forfiletype:) | 官方平台文档 | NSWorkspace icon for file type | 2026-08-06 | included | 描述扩展名/HFS type/UTI 输入、32×32 NSImage、线程语义；availability 仅到 macOS 12 | 远端合成扩展名图标及现代 API spike 风险 |
| https://developer.apple.com/documentation/uniformtypeidentifiers/uttype | 官方平台文档 | Apple UTType filename extension/MIME lookup | 2026-08-06 | included | 描述按 extension/MIME/identifier 查类型和 preferred extension | macOS 现代 `iconForContentType` 依赖核对 |
| https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shgetfileinfow | 官方平台文档 | SHGetFileInfoW USEFILEATTRIBUTES | 2026-08-06 | included | 官方参数和 flags 说明，明确不存在的合成路径可按扩展名查询 | Windows 远端 `remote.ext` 图标查询 |
| https://specifications.freedesktop.org/shared-mime-info-spec/latest/ | 官方规范 | freedesktop MIME by filename/content | 2026-08-06 | included | MIME 可由文件名或内容识别，数据库不保存用户偏好 | Linux 远端 extension guess |
| https://specifications.freedesktop.org/icon-theme-spec/latest/ | 官方规范 | freedesktop icon name/size/theme inheritance | 2026-08-06 | included | icon name + nominal size 查文件，继承和 hicolor 回退 | Linux 主题查找与 fallback |
| https://docs.rs/open/5.3.6/open/fn.that_detached.html | Rust crate 官方 API 文档 | open detached default application | 2026-08-06 | included | 明确 detached process 允许目标程序阻塞或长于当前应用 | 本地/远端成功后默认应用启动 |
| https://wiki.filezilla-project.org/Remote_file_editing | 官方 Wiki 页面 | FileZilla remote file editing | 2026-08-06 | excluded | 页面没有足够正文，不能承载可审计结论 | 记录排除，避免误引用 |
| 本机 `third_package/axshell/src/platform/file_icons.rs` | 本地参考源码 | AxShell system file icon cache | 2026-08-06 | included-as-reference | 用户要求的参考项目；只核对行为，不复制源码、不加入 Cargo/build graph | 预热、缓存、目录/通用/扩展名 key、平台分支 |
| 本机 `third_package/axshell/src/sftp/worker.rs`、`src/sftp/transfer.rs` | 本地参考源码 | AxShell transfer/edit worker | 2026-08-06 | included-as-reference | 只核对受管编辑、进度、取消、watch 和确认对话框边界 | 说明首版 read-only open 应与编辑回传分阶段 |
| 本机 Cargo registry `objc2-app-kit 0.3.2` | 锁定依赖源码 | NSWorkspace generated API/features | 2026-08-06 | included-as-implementation-check | 与 Cargo.lock 版本一致，用于确认 `iconForContentType` feature 和 deprecated old API | P1 dependency/API spike |
| 本机 Cargo registry `russh 0.62.2`、`russh-sftp 2.3.0` | 锁定依赖源码 | SFTP stream/session/read API | 2026-08-06 | included-as-implementation-check | 与 Cargo.lock 版本一致，用于核对 subsystem 和 chunked read 选择 | P5/P6 transfer domain 设计 |
