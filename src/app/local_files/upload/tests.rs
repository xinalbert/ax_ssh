use super::*;

#[test]
fn local_upload_discovery_keeps_all_selected_files_and_nested_paths() {
    let root = std::env::temp_dir().join(format!("ax-ssh-upload-tree-{}", Uuid::new_v4()));
    let folder = root.join("folder");
    let nested = folder.join("nested");
    std::fs::create_dir_all(&nested).expect("nested fixture directory");
    std::fs::write(folder.join("first.txt"), b"one").expect("first fixture file");
    std::fs::write(nested.join("second.txt"), b"two").expect("second fixture file");
    std::fs::write(nested.join(".DS_Store"), b"excluded").expect("filtered fixture file");
    std::fs::write(root.join("third.txt"), b"three").expect("selected fixture file");
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("third.txt"), nested.join("linked.txt"))
        .expect("symlink fixture");

    let files = discover_local_uploads(
        vec![folder, root.join("third.txt")],
        "/srv",
        &[".DS_Store".to_owned()],
        20 * 1024 * 1024 * 1024,
    )
    .expect("all eligible nested files should be discovered");
    let targets = files
        .iter()
        .map(|file| file.remote_path.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(
        targets,
        HashSet::from([
            "/srv/folder/first.txt",
            "/srv/folder/nested/second.txt",
            "/srv/third.txt",
        ])
    );
    let nested_file = files
        .iter()
        .find(|file| file.remote_path.ends_with("second.txt"))
        .expect("nested file should be present");
    assert_eq!(
        nested_file.remote_directories,
        ["/srv/folder", "/srv/folder/nested"]
    );
    std::fs::remove_dir_all(root).expect("fixture should be removed");
}

#[test]
fn local_upload_discovery_uses_per_file_limit_without_batch_byte_cap() {
    let path = std::env::temp_dir().join(format!("ax-ssh-sparse-upload-{}", Uuid::new_v4()));
    let file = std::fs::File::create(&path).expect("create sparse fixture");
    file.set_len(19 * 1024 * 1024 * 1024)
        .expect("size sparse fixture");
    drop(file);
    let accepted = discover_local_uploads(vec![path.clone()], "/srv", &[], 20 * 1024 * 1024 * 1024)
        .expect("single file below the configured limit");
    assert_eq!(accepted.len(), 1);
    let unlimited = discover_local_uploads(vec![path.clone()], "/srv", &[], u64::MAX)
        .expect("zero-sized policy limit should allow this file");
    assert_eq!(unlimited.len(), 1);
    assert!(
        discover_local_uploads(vec![path.clone()], "/srv", &[], 10 * 1024 * 1024 * 1024).is_err()
    );
    std::fs::remove_file(path).expect("remove sparse fixture");
}
