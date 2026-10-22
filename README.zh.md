[English](README.md)

<img src="assets/ion/terminal_icon_all_formats/terminal_icon_256.png" alt="AxSSH 软件图标" width="128" height="128">

# AxSSH

[![CI](https://img.shields.io/github/actions/workflow/status/xinalbert/ax_ssh/ci.yml?branch=master&label=CI)](https://github.com/xinalbert/ax_ssh/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/xinalbert/ax_ssh/branch/master/graph/badge.svg)](https://app.codecov.io/gh/xinalbert/ax_ssh)
[![Latest release](https://img.shields.io/github/v/release/xinalbert/ax_ssh)](https://github.com/xinalbert/ax_ssh/releases/latest)
[![License](https://img.shields.io/github/license/xinalbert/ax_ssh)](LICENSE)
[![MSRV](https://img.shields.io/badge/MSRV-1.92.0%2B-dea584?logo=rust)](https://www.rust-lang.org/)
[![Platforms](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-1f6feb)](https://github.com/xinalbert/ax_ssh/releases/latest)
[![English](https://img.shields.io/badge/English-Click-1f6feb)](docs/usage.md)
[![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-%E7%82%B9%E5%87%BB-dea584)](docs/usage.zh.md)
[![繁體中文](https://img.shields.io/badge/%E7%B9%81%E9%AB%94%E4%B8%AD%E6%96%87-%E9%BB%9E%E6%93%8A-dea584)](docs/usage.zh-TW.md)
[![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-%E3%82%AF%E3%83%AA%E3%83%83%E3%82%AF-blue)](docs/usage.ja.md)
[![한국어](https://img.shields.io/badge/%ED%95%9C%EA%B5%AD%EC%96%B4-%EC%97%B4%EA%B8%B0-blue)](docs/usage.ko.md)
[![Deutsch](https://img.shields.io/badge/Deutsch-Klicken-blue)](docs/usage.de.md)
[![Español](https://img.shields.io/badge/Espa%C3%B1ol-Abrir-blue)](docs/usage.es.md)
[![Français](https://img.shields.io/badge/Fran%C3%A7ais-Ouvrir-blue)](docs/usage.fr.md)
[![Português (Brasil)](https://img.shields.io/badge/Portugu%C3%AAs%20%28Brasil%29-Abrir-blue)](docs/usage.pt-BR.md)
[![Русский](https://img.shields.io/badge/%D0%A0%D1%83%D1%81%D1%81%D0%BA%D0%B8%D0%B9-%D0%9E%D1%82%D0%BA%D1%80%D1%8B%D1%82%D1%8C-blue)](docs/usage.ru.md)
[![Українська](https://img.shields.io/badge/%D0%A3%D0%BA%D1%80%D0%B0%D1%97%D0%BD%D1%81%D1%8C%D0%BA%D0%B0-%D0%92%D1%96%D0%B4%D0%BA%D1%80%D0%B8%D1%82%D0%B8-blue)](docs/usage.uk.md)
[![English (United Kingdom)](https://img.shields.io/badge/English%20%28United%20Kingdom%29-Click-1f6feb)](docs/usage.en-GB.md)

在 **Settings > General > Language** 选择界面语言。
支持的 locale 与维护方式见[翻译说明](translations/README.zh.md)。

AxSSH 是一个由 Rust、Slint 和 Tokio 构建的跨平台桌面终端工作区，在同一个原生应用中
集中管理已保存的 SSH、Telnet、Serial 和本地 shell 会话，以及彼此独立的 Terminal/SFTP Tab。

## 功能概览

- 使用密码、私钥或运行时 SSH agent 连接，并在首次信任主机前明确确认 SHA-256 主机密钥指纹。
- 在本地或远端终端中使用有界回滚、ANSI 渲染、文本选择、剪贴板和原生输入法支持；Windows 物理数字
  统一键盘边界和原生输入法支持；支持的平台上物理小键盘也能传递给终端程序，
  包括 DEC application-keypad 模式。
- 在独立的 SFTP Tab 中浏览本地和远端文件，提供有界目录读取、递归下载、上传、重命名、删除，以及通过系统关联程序编辑远端文件。
- 支持分屏和独立的原生工作区窗口；Settings 同时展示可配置快捷键和固定的平台快捷键。普通输入框支持原生编辑
  与粘贴，秘密字段仍不可复制。

## 快速开始

安装 Rust `1.92.0` 或更高版本，并使用 Slint winit 后端支持的桌面环境，然后运行：

```bash
cargo run --locked
```

未知或发生变化的 SSH 主机密钥会被拒绝；核对并明确确认服务器的 SHA-256 指纹后才能继续。
会话创建、终端操作、SFTP 和设置说明见[使用指南](docs/usage.zh.md)。

开发命令、renderer 选项、打包方式和验证门禁见[开发说明](docs/development.zh.md)。

## 发布

GitHub Releases 会提供 Windows x86_64、Linux x86_64/aarch64，以及 macOS Apple Silicon、
Intel 和通用应用包。Release tag 使用 `YYYY-MM-DD` 或 `YYYY-MM-DD-N`；在默认分支同步版本
元数据后，推送 annotated tag 会直接启动发布 workflow。所需命令见[发布说明](docs/development.zh.md#github-发布)。

## 文档

- [使用指南](docs/usage.zh.md)
- [开发与验证](docs/development.zh.md)
- [架构说明](docs/architecture.zh.md)
- [文档导航](docs/README.zh.md)

## 支持与安全

可通过 [issue tracker](https://github.com/xinalbert/ax_ssh/issues/new) 或应用 About 页的
**Report a bug** 提交问题或功能建议。只有用户明确选择保存时才会写入密码，并且仅使用系统
凭据库或加密应用保险库；会话 JSON 不会写入明文密码、私钥 passphrase、终端输出或运行中的
worker 状态。Telnet 不加密；未知或发生变化的 SSH 主机密钥在明确确认前始终被阻止。

## 许可证

AxSSH 原创软件和原创应用资源采用
[GNU General Public License version 3 only](LICENSE)。仓库内的第三方源码和自带字体
继续适用各自的许可证，详见[第三方声明](THIRD_PARTY_NOTICES.md)。

应用 About 页面同时显示 AxSSH 的许可证标识和 Slint 标准署名组件。
