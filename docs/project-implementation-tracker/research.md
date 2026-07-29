# 项目研究记录

## 2026-07-29 Rust 与 Slint 工程规范依据

- 时间：2026-07-29 09:34 +0800
- 检索问题：当前 Rust 文件模块布局和 Slint 1.17.1 的文件拆分、生成代码、UI 线程、弱引用、异步调度、Model 与可访问性规范是什么？
- 检索原因：根 `AGENTS.md` 和项目 skill 将长期指导后续实现，必须区分上游当前建议、项目锁定 API 和本仓库架构约束。
- 来源列表：Rust Book 的模块文件章节 <https://doc.rust-lang.org/book/ch07-05-separating-modules-into-different-files.html>；Cargo targets <https://doc.rust-lang.org/cargo/reference/cargo-targets.html>；Slint latest 文件、properties、callbacks、globals、models 与 accessibility 指南；Slint 1.17.1 本地 crate 的 `ComponentHandle`、`invoke_from_event_loop`、`spawn_local` 和 `slint_build::compile` API 文档。
- 关键结论：Rust 当前惯用布局是 `foo.rs` 配合 `foo/bar.rs`，`mod.rs` 是仍支持的旧布局；Slint 组件和 event loop 必须保持 UI 线程亲和，callback 应捕获弱组件引用，后台结果通过 owned `Send + 'static` 数据回到 event loop；Tokio I/O 不应直接交给 Slint local executor；`.slint` 应保持声明式并使用 Model/虚拟化视图承载重复数据。
- 对实施计划的影响：根指令固定不可违反的架构、安全和验证门禁；项目 skill 负责工作流，Rust/Slint 细则拆入按需读取的 references；执行时以 Cargo.lock 中 Slint 1.17.1 为 API 基线，依赖升级时重新核对 latest 指南和迁移说明。
- 未解决问题：Slint `latest` 会随上游更新；未来升级 Slint、renderer/backend 或 Rust MSRV 时必须重新验证本文结论，不能仅沿用当前版本规则。
