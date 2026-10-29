use super::*;
use std::collections::HashSet;

#[test]
fn local_upload_names_have_no_client_character_count_cap() {
    let name = format!("{}.txt", "n".repeat(600));
    assert_eq!(
        upload_local_name(std::path::Path::new(&name)).unwrap(),
        name
    );
    assert!(upload_local_name(std::path::Path::new("line\nbreak")).is_err());
}

#[tokio::test]
async fn streaming_upload_discovery_keeps_more_than_512_files_and_deep_paths() {
    let root = std::env::temp_dir().join(format!("ax-ssh-stream-upload-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create upload fixture root");
    for index in 0..520 {
        std::fs::write(root.join(format!("file-{index:03}.txt")), b"content")
            .expect("create upload fixture file");
    }
    let mut deep = root.clone();
    for index in 0..20 {
        deep.push(format!("level-{index:02}"));
    }
    std::fs::create_dir_all(&deep).expect("create deep upload fixture");
    std::fs::write(deep.join("last.txt"), b"deep").expect("create deep upload file");
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("file-000.txt"), root.join("linked-file.txt"))
        .expect("create link that should be reported without stopping the scan");

    let (sender, mut receiver) = tokio::sync::mpsc::channel(8);
    let scan_root = root.clone();
    let discovery = tokio::task::spawn_blocking(move || {
        discover_local_uploads_streaming(vec![scan_root], "/srv", &[], &sender)
    });
    let mut remote_paths = HashSet::new();
    while let Some(file) = receiver.recv().await {
        remote_paths.insert(file.remote_path);
    }
    let summary = discovery
        .await
        .expect("streaming discovery task")
        .expect("streaming discovery succeeds");
    assert_eq!(summary.discovered_files, 521);
    #[cfg(unix)]
    assert_eq!(summary.skipped_entries, 1);
    #[cfg(not(unix))]
    assert_eq!(summary.skipped_entries, 0);
    assert_eq!(remote_paths.len(), 521);
    let root_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .expect("fixture root name");
    let deep_remote_path = format!(
        "/srv/{root_name}/level-00/level-01/level-02/level-03/level-04/level-05/level-06/level-07/level-08/level-09/level-10/level-11/level-12/level-13/level-14/level-15/level-16/level-17/level-18/level-19/last.txt"
    );
    assert!(remote_paths.contains(&deep_remote_path));
    std::fs::remove_dir_all(root).expect("remove upload fixture");
}
