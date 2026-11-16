# 当前项目实施记录

## 当前目标

- 目标 ID：20261116-windows-version-icons
- 目标：按已知 Windows 产品版本显示系统图标，保留未知版本兜底和手动选择。
- 交付物：版本目录、自动识别与旧缓存展示升级、共享图标选择、回归及双语说明。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：系统图标目录、应用层 release/缓存映射、生成 Slint 图标、相关回归和文档；保留此前 Settings 标签 SVG 改动。
- 不在本轮范围内：SSH 探测命令/信任/凭据、配置 schema、依赖升级、安装/发布、GUI 启动或截图。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| W1 | completed | 核对现有资源、版本识别与参考来源 | catalog/manifest/source、Icon-Icons、Microsoft Learn、Simple Icons | 明确产品名与同代资源复用 |
| W2 | completed | Windows 版本映射、缓存兼容与可选图标 | 9 项 windows_ 定向回归、128 项目录生成检查通过 | 现代/经典版本及 Server；不用 10.0 猜版本，不覆盖手动选择 |
| W3 | completed | 完整验证、构建和双语记录 | 1030 项测试、fmt/check/Clippy、原生目标 check/Clippy/build、文档/skill/tracker/diff 通过 | 实际视觉由用户验收 |

## 已完成

- 用户确认加入经典版本；新增 12 个选择项，覆盖 Windows 95/98/Me/2000、XP、Vista、7、8/8.1、10/11 和 Server。
- 经典版本接入有来源与 CC0 许可记录的旗帜 SVG；同代产品共享图形，保留产品名称。
- 现有远端探测返回系统 Caption；Windows 10、11 和 Server 都可能是内核 10.0，按明确产品名称识别。
- 自动识别旧缓存中的通用 Windows 可按已有 release 细化；手动图标保持优先，不改写缓存。
- Settings 标签使用内置描边齿轮 SVG，与 SFTP 图形风格一致；颜色随标签前景色，形状不依赖系统字体/Emoji。
- 资源来源、CC0 说明、根第三方声明、离线预览、名称索引、双语架构和项目地图已同步。

## 验证

- 已完成：9 项 windows_ 定向回归；全量 1030 项测试（471 library + 559 binary，Doc tests 0）；fmt、locked/offline check、严格 all-target Clippy；与 CI 一致的 `aarch64-apple-darwin` 目标 check/严格 Clippy/build。128 项目录/5 文件生成检查、329 项资源哈希与清单一致性、经典 SVG 路径/视口及 16/64px 非空透明像素、Slint 24px 解码通过；56 份 Markdown/边界、项目 skill、tracker/diff 检查通过。
- 未完成：Windows/Linux 原生构建及三平台 GUI 视觉验收；本轮仅在 macOS aarch64 验证，未启动应用。

## 风险与阻塞

- 未知或本地未探测版本继续显示通用 Windows；手动指定图标必须保持优先。
- Windows Server 的长名称图不是小尺寸符号，使用同代 Windows 窗格符号，产品年份保留在原 release 提示。
- 本地构建产物为 `target/aarch64-apple-darwin/debug/ax_ssh`；本轮按用户要求进行本地提交，未推送、打包或发布。
- 提交边界：Settings SVG 独立提交为 `3c9b8a5`；Windows 版本目录、映射、回归及记录归入本轮功能提交。

## 下一步

- 由用户在对应平台确认图标效果；如提供截图，再按实际显示调整。

## 最后更新时间

- 2026-11-16 11:39 +0800
