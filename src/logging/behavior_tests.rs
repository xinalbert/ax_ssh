use super::*;

#[test]
fn logging_guard_flushes_and_panic_hook_writes_a_durable_bounded_report() {
    const CHILD: &str = "AXSSH_LOGGING_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
            .env(CHILD, "1").env_remove("RUST_LOG")
            .args(["logging::behavior_tests::logging_guard_flushes_and_panic_hook_writes_a_durable_bounded_report", "--exact", "--nocapture"])
            .output().expect("isolated logging process");
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let root = std::env::temp_dir().join(format!("axssh-logging-{}", uuid::Uuid::new_v4()));
    let guard = LoggingGuard::init_in(root.clone())
        .expect("actual global logger starts in isolated directory");
    assert_eq!(guard.directory(), root);
    let crash = guard.crash_log_path().to_owned();
    assert!(panic::catch_unwind(|| panic!("fixture panic\nwith control\tcharacters")).is_err());
    let report = fs::read_to_string(&crash).expect("panic hook flushed synchronously");
    assert!(report.contains("fixture panic\\nwith control\\tcharacters"));
    assert!(report.contains("panic_location="));
    assert!(report.contains("backtrace:"));
    assert!(report.ends_with("=== end AxSSH crash report ===\n\n"));
    tracing::info!(target: "ax_ssh", "isolated-guard-flush");
    drop(guard);
    let logs = fs::read_dir(&root)
        .expect("private log files")
        .map(|entry| entry.expect("log entry").path())
        .filter(|path| path != &crash)
        .collect::<Vec<_>>();
    assert_eq!(logs.len(), 1);
    assert!(
        fs::read_to_string(&logs[0])
            .expect("flushed log")
            .contains("isolated-guard-flush")
    );
    assert_eq!(panic_payload(&"a\n\r\t\u{7}"), "a\\n\\r\\t?");
    assert_eq!(panic_payload(&42_u32), "<non-string panic payload>");
    assert_eq!(
        panic_payload(&"x".repeat(CRASH_FIELD_LIMIT + 1)).len(),
        CRASH_FIELD_LIMIT + 3
    );
    fs::remove_dir_all(&root).expect("remove isolated logs");
}
