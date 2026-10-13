use super::*;

#[test]
fn posix_breadcrumbs_keep_unicode_and_literal_windows_characters() {
    let rows = breadcrumbs("/home/研究/C:\\data", PathStyle::Posix);
    assert_eq!(
        rows.iter()
            .map(|row| row.label.as_str())
            .collect::<Vec<_>>(),
        ["/", "home", "研究", "C:\\data"]
    );
    assert_eq!(rows[2].path, "/home/研究");
    assert_eq!(rows[3].path, "/home/研究/C:\\data");
    assert!(rows[3].current);
    assert!(!rows[0].current);
    let network_root = breadcrumbs("//server/folder", PathStyle::Posix);
    assert_eq!(network_root[0].path, "//");
    assert_eq!(network_root[1].path, "//server");
    assert_eq!(
        breadcrumbs("/", PathStyle::Posix),
        [Breadcrumb {
            label: "/".into(),
            path: "/".into(),
            current: true
        }]
    );
}

#[test]
fn windows_drive_and_unc_navigation_targets_preserve_their_prefix() {
    for (path, root_path, child_path) in [
        ("C:\\Users\\研究\\Downloads", "C:\\", "C:\\Users"),
        ("C:/Users/研究/Downloads", "C:/", "C:/Users"),
        (
            "\\\\nas\\share\\项目\\data",
            "\\\\nas\\share\\",
            "\\\\nas\\share\\项目",
        ),
        ("\\\\?\\C:\\Users\\研究", "\\\\?\\C:\\", "\\\\?\\C:\\Users"),
        (
            "\\\\?\\UNC\\nas\\share\\项目\\data",
            "\\\\?\\UNC\\nas\\share\\",
            "\\\\?\\UNC\\nas\\share\\项目",
        ),
        (
            "\\\\?\\unc\\nas\\share\\项目\\data",
            "\\\\?\\unc\\nas\\share\\",
            "\\\\?\\unc\\nas\\share\\项目",
        ),
    ] {
        let rows = breadcrumbs(path, PathStyle::Windows);
        assert_eq!(rows[0].path, root_path, "{path}");
        assert_eq!(rows[1].path, child_path, "{path}");
        assert!(rows.last().expect("leaf").current);
    }
}

#[test]
fn deep_paths_keep_root_nearest_omitted_parent_and_leaf_with_a_fixed_budget() {
    let path = "/a/b/c/d/e/f/g/h/i/目录/";
    let rows = breadcrumbs(path, PathStyle::Posix);
    assert_eq!(rows.len(), MAX_SEGMENTS);
    assert_eq!(rows[0].path, "/");
    assert_eq!(rows[1].label, "…");
    assert_eq!(rows[1].path, "/a/b/c/d/e/f");
    assert_eq!(rows[2].path, "/a/b/c/d/e/f/g");
    assert_eq!(rows.last().expect("leaf").path, path.trim_end_matches('/'));
    assert_eq!(rows.iter().filter(|row| row.current).count(), 1);
    let dense = format!("/{}", "a/".repeat(1000));
    let rows = breadcrumbs(&dense, PathStyle::Posix);
    assert_eq!(rows.len(), MAX_SEGMENTS);
    assert!(
        rows.iter()
            .map(|row| row.path.len() + row.label.len())
            .sum::<usize>()
            < (MAX_PATH_BYTES * MAX_SEGMENTS * 2)
    );
}

#[test]
fn relative_oversized_control_and_unsupported_device_paths_stay_in_edit_mode() {
    for path in [
        "",
        "relative/path",
        "~/data",
        "C:relative",
        "\\\\.\\pipe\\service",
        "\\\\?\\VolumeUnknown\\file",
    ] {
        assert!(breadcrumbs(path, PathStyle::Posix).is_empty());
        assert!(breadcrumbs(path, PathStyle::Windows).is_empty());
    }
    assert!(
        breadcrumbs(
            &format!("/{}", "a".repeat(MAX_PATH_BYTES)),
            PathStyle::Posix
        )
        .is_empty()
    );
    assert!(breadcrumbs("/home/line\nfeed", PathStyle::Posix).is_empty());
}
