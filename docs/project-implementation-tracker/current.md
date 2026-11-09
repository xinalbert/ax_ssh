# 当前项目实施记录

## 当前目标

- 目标 ID：20261109-linux-skia-toolchain
- 目标：修复 Linux CI/Release 在 Skia 0.153.3 C++20 编译阶段失败的问题。
- 交付物：统一的 Linux 工具链配置、双语开发说明、可追溯的复现与验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`.github/actions/`、`.github/workflows/ci.yml`、`.github/workflows/release.yml`、双语开发说明及实施记录。
- 不在本轮范围内：应用源码、Rust/Slint 依赖升级、平台移除、SSH 信任与凭据、GUI 验收、移动或复用旧 release tag。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| L1 | completed | 核对 Skia 工具链要求并在 Ubuntu 22.04 复现 | 相同 C++ 源文件的旧/新编译器对照 | 完整远端日志 API 要求认证；不猜测首条诊断 |
| L2 | completed | CI/Release 共用显式 Linux Clang 配置 | actionlint、真实 Linux 安装及源文件编译 | 保留 runner、全部 target 和既有门禁 |
| L3 | completed | 双语说明与实施记录、最终验证 | Cargo native check、链接/skill/tracker/diff 校验 | 远端完整多平台发布仍需 Actions 验证 |

## 已完成

- 工作树初始干净；master 为 2a25fc2，现有 annotated tag 2026-11-09 不移动、不删除、不复用。
- Release run 42569322308 的 Linux x86_64/aarch64 构建均失败；两个 macOS 构建已成功，发布需等待全部构建及 Universal 组装成功。
- 本轮拥有者是构建环境配置，不改变应用模块契约或 SSH 安全边界。
- 本机 Docker 为 Linux ARM64；在 Ubuntu 22.04 中用 Clang 14 + GCC 12 标准库头文件复现 SkPDFTag.cpp 的相同 6 个错误，首条为 fData.emplace 的 C++20 聚合圆括号初始化不受支持。
- 相同源码、标准库头文件与编译参数改用 Clang 21.1.8 后成功生成 AArch64 ELF object；glibc 保持 2.35。
- 从实际 composite action 提取的两个 shell 步骤已在容器执行成功：LLVM 签名软件源、全部依赖安装、CC/CXX/CLANGCC/CLANGCXX/LIBCLANG_PATH 导出均已验证；最终源码审查另补 CLANG_PATH 供 bindgen include 发现，并纳入静态校验。
- CI 两个 Linux target、coverage、MSRV 和 Release preflight/正式构建共用该 action；缓存恢复在工具链选择之后，并包含编译器/libclang 环境指纹。

## 验证

- 已完成：旧/新编译器真实 Skia object 对照、共用 action 的容器执行、libclang/系统库路径、Cargo locked/offline check、fmt、14 项发布辅助测试、actionlint、56 份 Markdown/边界及 skill/tracker/diff 校验；矩阵/原生测试规则及 Cargo 门禁与 HEAD 一致。
- 未完成：完整 Linux Cargo/Skia 构建、Windows/macOS 矩阵和 Release 打包仍需远端 Actions。

## 风险与阻塞

- 远端完整 job 日志下载返回 403，需要仓库授权；用户提供的错误尾部与本机相同 Skia 源码可用于复现。
- 容器复现不能代替五目标 GitHub Actions 和发行打包；旧 tag 的工作流不会自动获得新提交的修复。

## 下一步

- 本次交付为包含工具链修复、双语说明和验证记录的独立本地提交；要发布修复，必须推送包含新工作流的提交，使用未占用修订 tag 并先同步/提交发行元数据，不能只重跑旧 tag。

## 最后更新时间

- 2026-11-09 14:50 +0800
