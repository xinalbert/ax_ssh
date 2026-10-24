fn default_edit_limit() -> u64 {
    crate::config::SftpTransferSettings::default().remote_edit_max_file_bytes()
}

use super::*;

struct TempRoot(PathBuf);
impl TempRoot {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("axssh-edit-test-{}", Uuid::new_v4())))
    }
}
impl Drop for TempRoot {
    fn drop(&mut self) {
        if self.0.exists() {
            fs::remove_dir_all(&self.0).expect("test cleanup");
        }
    }
}

fn loaded() -> RemoteEdits {
    let mut edits = RemoteEdits::default();
    edits.open("/srv/file.bin".into()).unwrap();
    let edit = &mut edits.files[0];
    edit.local_path = Some("/private/edit-file.bin".into());
    edit.uploaded = content_stamp(b"original");
    edit.observed = edit.uploaded;
    edit.expected_size = 8;
    edit.expected_modified = Some(123);
    edits
}

fn queue(edits: &mut RemoteEdits, data: &[u8]) -> Option<(Uuid, SftpWriteOperation)> {
    let mut queued = None;
    queue_edit_save(
        &mut edits.files[0],
        edits.auto_upload,
        default_edit_limit(),
        data.to_vec(),
        content_stamp(data),
        |id, operation| {
            queued = Some((id, operation));
            Ok(())
        },
    );
    queued
}

fn ack(id: Uuid, size: u64, modified: u32) -> SftpWriteEvent {
    SftpWriteEvent::Updated {
        operation_id: id,
        path: "/srv/file.bin".into(),
        size,
        modified: Some(modified),
    }
}

#[test]
fn external_edit_rapid_saves_wait_for_ack_and_use_updated_remote_baseline() {
    let mut edits = loaded();
    let (first, op) = queue(&mut edits, &[0, 255, 128]).unwrap();
    assert!(
        matches!(op, SftpWriteOperation::WriteEditFile { data, expected_size: 8, expected_modified: Some(123), .. } if data == [0, 255, 128])
    );
    assert!(queue(&mut edits, b"second").is_none());
    assert!(edits.apply_result(&ack(first, 3, 124)));
    assert_ne!(edits.files[0].uploaded, edits.files[0].observed);
    let (second, op) = queue(&mut edits, b"second").unwrap();
    assert!(
        matches!(op, SftpWriteOperation::WriteEditFile { data, expected_size: 3, expected_modified: Some(124), .. } if data == b"second")
    );
    assert!(!edits.apply_result(&ack(first, 3, 124)));
    assert_eq!(edits.files[0].pending.unwrap().0, second);
    assert!(edits.apply_result(&ack(second, 6, 125)));
    assert!(queue(&mut edits, b"second").is_none());
}

#[test]
fn external_edit_unchanged_saves_and_disabled_auto_upload_do_not_queue() {
    let mut edits = loaded();
    assert!(queue(&mut edits, b"original").is_none());
    edits.auto_upload = false;
    assert!(queue(&mut edits, b"modified").is_none());
    edits.request_upload().unwrap();
    assert!(queue(&mut edits, b"modified").is_some());
}

#[test]
fn external_edit_failure_retains_baseline_and_requires_explicit_retry() {
    let mut edits = loaded();
    let (id, _) = queue(&mut edits, b"modified").unwrap();
    assert!(edits.apply_result(&SftpWriteEvent::Failed {
        operation_id: id,
        message: "remote conflict".into()
    }));
    assert_eq!(edits.files[0].uploaded, content_stamp(b"original"));
    assert_eq!(edits.files[0].expected_modified, Some(123));
    assert!(edits.files[0].local_path.is_some());
    assert!(queue(&mut edits, b"modified again").is_none());
    edits.request_upload().unwrap();
    assert!(queue(&mut edits, b"modified again").is_some());
}

#[test]
fn external_edit_full_worker_queue_does_not_advance_baseline() {
    let mut edits = loaded();
    let edit = &mut edits.files[0];
    queue_edit_save(
        edit,
        true,
        default_edit_limit(),
        b"new".to_vec(),
        content_stamp(b"new"),
        |_, _| anyhow::bail!("queue full"),
    );
    assert!(edit.failed);
    assert!(edit.pending.is_none());
    assert_eq!(edit.uploaded, content_stamp(b"original"));
}

#[test]
fn external_edit_reopen_reuses_copy_and_stop_rejects_late_results() {
    let mut edits = loaded();
    let copy = edits.files[0].local_path.clone();
    let (old, _) = queue(&mut edits, b"new").unwrap();
    let (_, reused, download) = edits.open("/srv/file.bin".into()).unwrap();
    assert_eq!(reused, copy);
    assert!(!download);
    edits.stop_current();
    assert!(!edits.apply_result(&ack(old, 3, 124)));
    edits.open("/srv/file.bin".into()).unwrap();
    assert!(!edits.apply_result(&ack(old, 3, 124)));
    assert!(edits.files[0].local_path.is_none());
}

#[test]
fn external_edit_ack_for_another_file_does_not_change_current_file() {
    let mut edits = loaded();
    let (id, _) = queue(&mut edits, b"new").unwrap();
    edits.open("/srv/second.txt".into()).unwrap();
    assert!(edits.apply_result(&ack(id, 3, 124)));
    assert_eq!(edits.current().unwrap().path, "/srv/second.txt");
    assert!(edits.current().unwrap().local_path.is_none());
}

#[test]
fn external_edit_file_count_is_bounded_and_failed_download_releases_slot() {
    let mut edits = RemoteEdits::default();
    for index in 0..MAX_TAB_EDITS {
        edits.open(format!("/srv/{index}.txt")).unwrap();
    }
    assert!(edits.open("/srv/overflow.txt".into()).is_err());
    let failed = edits.files[0].id;
    assert!(edits.apply_result(&SftpWriteEvent::Failed {
        operation_id: failed,
        message: "missing".into()
    }));
    assert!(edits.open("/srv/overflow.txt".into()).is_ok());
}

#[test]
fn external_edit_copy_preserves_bytes_extension_unique_identity_and_permissions() {
    let root = TempRoot::new();
    let first = create_copy_in(
        &root.0,
        "/srv/report.json",
        &[0, 255, 128],
        default_edit_limit(),
    )
    .unwrap();
    let second =
        create_copy_in(&root.0, "/srv/report.json", b"second", default_edit_limit()).unwrap();
    assert_ne!(first, second);
    assert_eq!(first.extension().unwrap(), "json");
    assert_eq!(
        read_copy(&first, default_edit_limit()).unwrap(),
        [0, 255, 128]
    );
    assert_eq!(read_copy(&second, default_edit_limit()).unwrap(), b"second");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(first.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(first).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn external_edit_copy_accepts_atomic_replacement_save_and_rejects_oversize() {
    let root = TempRoot::new();
    let path = create_copy_in(
        &root.0,
        "/srv/report.json",
        b"original",
        default_edit_limit(),
    )
    .unwrap();
    let replacement = path.with_extension("tmp");
    fs::write(&replacement, b"new saved content").unwrap();
    // On Windows remove first, matching editors that use rename replacement.
    #[cfg(windows)]
    fs::remove_file(&path).unwrap();
    fs::rename(replacement, &path).unwrap();
    assert_eq!(
        read_copy(&path, default_edit_limit()).unwrap(),
        b"new saved content"
    );
    let opened = open_read_copy(&path, default_edit_limit()).unwrap();
    let file = OpenOptions::new().write(true).open(&path).unwrap();
    file.set_len(default_edit_limit() + 1).unwrap();
    assert!(read_open_copy(opened, default_edit_limit()).is_err());
    assert!(read_copy(&path, default_edit_limit()).is_err());
}

#[test]
fn external_edit_stopping_keeps_modified_copy_and_cache_count_is_bounded() {
    let root = TempRoot::new();
    let path = create_copy_in(&root.0, "/srv/CON.txt", b"original", default_edit_limit()).unwrap();
    assert_eq!(path.file_name().unwrap(), "edit-CON.txt");
    let mut edits = loaded();
    edits.files[0].local_path = Some(path.clone());
    fs::write(&path, b"unsaved remotely").unwrap();
    edits.stop_current();
    assert_eq!(
        read_copy(&path, default_edit_limit()).unwrap(),
        b"unsaved remotely"
    );
    for index in 1..MAX_RETAINED_COPIES {
        fs::create_dir(root.0.join(index.to_string())).unwrap();
    }
    assert!(create_copy_in(&root.0, "/srv/new.txt", b"new", default_edit_limit()).is_err());
    assert_eq!(
        read_copy(&path, default_edit_limit()).unwrap(),
        b"unsaved remotely"
    );
}

#[cfg(unix)]
#[test]
fn external_edit_rejects_symlink_file_and_directory_substitution() {
    use std::os::unix::fs::symlink;
    let root = TempRoot::new();
    let path = create_copy_in(&root.0, "/srv/file.txt", b"original", default_edit_limit()).unwrap();
    let other = root.0.join("other.txt");
    fs::write(&other, b"other").unwrap();
    fs::remove_file(&path).unwrap();
    symlink(&other, &path).unwrap();
    assert!(read_copy(&path, default_edit_limit()).is_err());
    let alias = root.0.join("alias");
    symlink(path.parent().unwrap(), &alias).unwrap();
    assert!(read_copy(&alias.join(path.file_name().unwrap()), default_edit_limit()).is_err());
}

#[test]
fn external_edit_copy_limit_changes_preserve_the_existing_local_copy() {
    let root = TempRoot::new();
    let data = vec![255; default_edit_limit() as usize + 1];
    let raised = 8 * 1024 * 1024;
    assert!(create_copy_in(&root.0, "/srv/large.bin", &data, default_edit_limit()).is_err());
    let path = create_copy_in(&root.0, "/srv/large.bin", &data, raised).unwrap();
    assert_eq!(read_copy(&path, raised).unwrap(), data);
    assert!(read_copy(&path, default_edit_limit()).is_err());
    assert_eq!(read_copy(&path, raised).unwrap(), data);
}

#[test]
fn external_edit_save_rechecks_current_limit_and_recovers_when_raised() {
    let mut edits = loaded();
    let data = vec![171; default_edit_limit() as usize + 1];
    let stamp = content_stamp(&data);
    let edit = &mut edits.files[0];
    let original = edit.uploaded;
    assert!(queue_edit_save(
        edit,
        true,
        default_edit_limit(),
        data.clone(),
        stamp,
        |_, _| panic!("oversized copy must not be queued")
    ));
    assert!(edit.pending.is_none());
    assert!(!edit.failed);
    assert_eq!(edit.uploaded, original);
    assert!(edit.status.contains("copy retained"));
    let raised = 8 * 1024 * 1024;
    let mut queued = None;
    assert!(queue_edit_save(
        edit,
        true,
        raised,
        data,
        stamp,
        |id, operation| {
            queued = Some((id, operation));
            Ok(())
        }
    ));
    let (_, operation) = queued.unwrap();
    assert!(
        matches!(operation, SftpWriteOperation::WriteEditFile { max_bytes, .. } if max_bytes == raised)
    );
    assert_eq!(edit.uploaded, original);
}
