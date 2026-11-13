//! Address-bar conversion. Canonical server paths remain the operational identity.

pub(super) fn remote_display_path(path: &str) -> String {
    let Some(drive_path) = windows_sftp_drive_path(path) else {
        return path.to_owned();
    };
    // Backslashes can be literal POSIX filename characters. Do not make such
    // paths ambiguous by presenting them as Windows separators.
    if path.contains('\\') {
        return path.to_owned();
    }
    if drive_path.len() == 2 {
        format!("{drive_path}\\")
    } else {
        drive_path.replace('/', "\\")
    }
}

pub(super) fn remote_directory_input(path: &str, current: &str, home: &str) -> String {
    // Infer the address convention from the server, never from the client OS.
    // A POSIX directory may legitimately contain colons, quotes or backslashes.
    if windows_sftp_drive_path(current).is_none() && windows_sftp_drive_path(home).is_none() {
        return path.to_owned();
    }
    let unquoted = path
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(path);
    let bytes = unquoted.as_bytes();
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
    {
        format!("/{}", unquoted.replace('\\', "/"))
    } else {
        path.to_owned()
    }
}

fn windows_sftp_drive_path(path: &str) -> Option<&str> {
    let drive_path = path.strip_prefix('/')?;
    let bytes = drive_path.as_bytes();
    (bytes.len() >= 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes.len() == 2 || bytes[2] == b'/'))
        .then_some(drive_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_remote_addresses_round_trip_native_drive_paths() {
        for (canonical, readable) in [
            ("/G:/albertxin/2026/mol_dock", r"G:\albertxin\2026\mol_dock"),
            ("/c:/Program Files/研究 📁", r"c:\Program Files\研究 📁"),
            ("/D:/", r"D:\"),
        ] {
            assert_eq!(remote_display_path(canonical), readable);
            assert_eq!(remote_directory_input(readable, canonical, ""), canonical);
            assert_eq!(
                remote_directory_input(&format!("\"{readable}\""), "/", canonical),
                canonical
            );
        }
        assert_eq!(remote_display_path("/G:"), r"G:\");
    }

    #[test]
    fn pasted_windows_drive_paths_support_other_drives_and_mixed_separators() {
        for (input, expected) in [
            (r"D:\Other folder\结果", "/D:/Other folder/结果"),
            (r"E:/reports\latest", "/E:/reports/latest"),
            (r#""G:\Program Files\研究""#, "/G:/Program Files/研究"),
        ] {
            assert_eq!(remote_directory_input(input, "/G:/work", ""), expected);
        }
    }

    #[test]
    fn posix_paths_and_ambiguous_names_remain_literal() {
        for path in [
            "",
            "/",
            "/home/alice",
            "/mnt/G:/data",
            "/G:relative",
            "/1:/data",
            r"/G:/literal\name",
            r"G:\work",
            r#""G:\work""#,
            "../logs",
            "~/notes",
        ] {
            assert_eq!(remote_display_path(path), path);
            assert_eq!(
                remote_directory_input(path, "/srv/app", "/home/alice"),
                path
            );
        }
        for path in [
            "/G:/data",
            "../logs",
            "~/notes",
            r"G:relative",
            r"\\server\share",
        ] {
            assert_eq!(remote_directory_input(path, "/G:/work", ""), path);
        }
    }
}
