//! XLaunch local-server settings, independent of the XLaunch executable.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use tokio::process::Command;
use zeroize::Zeroizing;

const MAX_CONFIG_BYTES: u64 = 64 * 1_024;
const MAX_EXTRA_PARAMS_CHARS: usize = 4_096;
const MAX_EXTRA_ARGS: usize = 128;
const MAX_DISCOVERY_ENTRIES: usize = 4_096;
const MAX_DISCOVERED_CONFIGS: usize = 8;

/// Enumerate filenames only; XML is read later by authenticated preparation.
#[cfg(target_os = "windows")]
pub(super) fn discover_configurations(executable: Option<&Path>) -> Result<Vec<PathBuf>> {
    let user_folders = directories::UserDirs::new()
        .map(|folders| {
            [folders.desktop_dir(), folders.document_dir()]
                .into_iter()
                .flatten()
                .map(Path::to_path_buf)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    discover_in_directories(executable.and_then(Path::parent), &user_folders)
}

#[cfg(target_os = "windows")]
pub(super) fn configuration_path(
    explicit: &str,
    executable: Option<&Path>,
) -> Result<Option<PathBuf>> {
    if !explicit.is_empty() {
        return Ok(Some(PathBuf::from(explicit)));
    }
    select_configuration(discover_configurations(executable)?)
}

fn select_configuration(mut paths: Vec<PathBuf>) -> Result<Option<PathBuf>> {
    ensure!(
        paths.len() <= 1,
        "multiple XLaunch configurations found; choose one in Settings > X11"
    );
    Ok(paths.pop())
}

fn discover_in_directories(
    program_directory: Option<&Path>,
    user_folders: &[PathBuf],
) -> Result<Vec<PathBuf>> {
    if let Some(directory) = program_directory {
        let paths = scan_directories(&[directory.to_path_buf()])?;
        if !paths.is_empty() {
            return Ok(paths);
        }
    }
    scan_directories(user_folders)
}

fn scan_directories(directories: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for (index, directory) in directories.iter().enumerate() {
        if directories[..index].contains(directory) {
            continue;
        }
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error).context("cannot search the XLaunch configuration directory");
            }
        };
        for (index, entry) in entries.enumerate() {
            ensure!(
                index < MAX_DISCOVERY_ENTRIES,
                "XLaunch search directory is too large; enter an explicit configuration path"
            );
            let entry = entry.context("cannot inspect an XLaunch directory entry")?;
            let path = entry.path();
            if !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("xlaunch"))
            {
                continue;
            }
            if entry
                .file_type()
                .context("cannot inspect an XLaunch file type")?
                .is_file()
            {
                let display = path.to_string_lossy();
                ensure!(
                    display.chars().count() <= super::MAX_DISCOVERED_LOCATION_CHARS
                        && !display.chars().any(char::is_control),
                    "XLaunch configuration path cannot be displayed or stored"
                );
                paths.push(path);
                ensure!(
                    paths.len() <= MAX_DISCOVERED_CONFIGS,
                    "too many XLaunch configurations; enter an explicit configuration path"
                );
            }
        }
    }
    paths.sort_unstable();
    Ok(paths)
}

#[derive(Clone, Debug)]
pub(super) struct WindowsLaunchConfig {
    pub(super) display: Option<u16>,
    window_mode: Option<&'static str>,
    clipboard: bool,
    clipboard_primary: bool,
    wgl: Option<bool>,
    disable_access_control: bool,
    extra_args: Vec<String>,
}

impl Default for WindowsLaunchConfig {
    fn default() -> Self {
        Self {
            display: None,
            window_mode: Some("-multiwindow"),
            clipboard: true,
            clipboard_primary: true,
            wgl: None,
            disable_access_control: true,
            extra_args: Vec::new(),
        }
    }
}

impl WindowsLaunchConfig {
    /// Read only a bounded regular file on the X-server preparation worker.
    pub(super) fn load(path: &Path) -> Result<Self> {
        ensure!(
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("xlaunch")),
            "XLaunch configuration must be a .xlaunch file"
        );
        let file = File::open(path)
            .with_context(|| format!("cannot open XLaunch configuration {}", path.display()))?;
        ensure!(
            file.metadata()
                .context("cannot inspect XLaunch configuration")?
                .is_file(),
            "XLaunch configuration must be a regular file"
        );
        let mut xml = Zeroizing::new(String::new());
        file.take(MAX_CONFIG_BYTES + 1)
            .read_to_string(&mut xml)
            .context("cannot read XLaunch configuration as UTF-8")?;
        ensure!(
            xml.len() as u64 <= MAX_CONFIG_BYTES,
            "XLaunch configuration exceeds 64 KiB"
        );
        Self::parse(&xml)
    }

    fn parse(xml: &str) -> Result<Self> {
        let document = roxmltree::Document::parse_with_options(
            xml,
            roxmltree::ParsingOptions {
                nodes_limit: 32,
                ..Default::default()
            },
        )
        .map_err(|_| anyhow::anyhow!("invalid XLaunch configuration XML"))?;
        let root = document.root_element();
        ensure!(
            root.tag_name().name() == "XLaunch",
            "expected an XLaunch configuration"
        );
        ensure!(
            root.attribute("ClientMode").unwrap_or("NoClient") == "NoClient",
            "XLaunch configuration must use ClientMode=NoClient; start programs through the SSH session"
        );
        let display = match root.attribute("Display").unwrap_or("-1") {
            "-1" => None,
            value => {
                let number = value
                    .parse::<u16>()
                    .context("invalid XLaunch display number")?;
                ensure!(
                    number <= u16::MAX - 6_000,
                    "XLaunch display number is too large"
                );
                Some(number)
            }
        };
        let window_mode = match root.attribute("WindowMode").unwrap_or("MultiWindow") {
            "MultiWindow" => Some("-multiwindow"),
            "Fullscreen" => Some("-fullscreen"),
            "Windowed" => None,
            "Nodecoration" => Some("-nodecoration"),
            _ => bail!("unsupported XLaunch window mode"),
        };
        let boolean = |name, default| -> Result<bool> {
            match root.attribute(name) {
                None => Ok(default),
                Some(value) if value.eq_ignore_ascii_case("true") => Ok(true),
                Some(value) if value.eq_ignore_ascii_case("false") => Ok(false),
                _ => bail!("invalid XLaunch {name} value"),
            }
        };
        let extra_args = split_windows_arguments(root.attribute("ExtraParams").unwrap_or(""))?;
        // These options change the selected endpoint or request XDMCP rather
        // than starting the local server described by this configuration.
        ensure!(
            !extra_args.iter().any(|arg| arg.starts_with(':')
                || matches!(
                    arg.as_str(),
                    "-displayfd" | "-query" | "-indirect" | "-broadcast" | "-multicast"
                )),
            "XLaunch additional parameters must not override the display or request XDMCP"
        );
        Ok(Self {
            display,
            window_mode,
            clipboard: boolean("Clipboard", true)?,
            clipboard_primary: boolean("ClipboardPrimary", true)?,
            wgl: root
                .attribute("Wgl")
                .map(|_| boolean("Wgl", true))
                .transpose()?,
            disable_access_control: boolean("DisableAC", false)?,
            extra_args,
        })
    }

    /// Build owned arguments directly; neither XML nor paths go through a shell.
    pub(super) fn command(
        &self,
        path: &Path,
        display: u16,
        allow_no_auth: bool,
    ) -> Result<Command> {
        let disables_auth =
            self.disable_access_control || self.extra_args.iter().any(|arg| arg == "-ac");
        ensure!(
            !disables_auth || allow_no_auth,
            "VcXsrv/Xming startup without X authority requires explicit local no-auth compatibility"
        );
        let mut command = super::detached_command(path);
        if path.is_absolute()
            && let Some(directory) = path.parent()
        {
            command.current_dir(directory);
        }
        command.arg(format!(":{display}"));
        if let Some(mode) = self.window_mode {
            command.arg(mode);
        }
        command.arg(if self.clipboard {
            "-clipboard"
        } else {
            "-noclipboard"
        });
        if !self.clipboard_primary {
            command.arg("-noprimary");
        }
        if let Some(wgl) = self.wgl {
            command.arg(if wgl { "-wgl" } else { "-nowgl" });
        }
        if self.disable_access_control {
            command.arg("-ac");
        }
        command.args(&self.extra_args);
        Ok(command)
    }
}

/// Parse the Windows C argument quoting used by XLaunch's ExtraParams field.
fn split_windows_arguments(value: &str) -> Result<Vec<String>> {
    ensure!(
        value.chars().count() <= MAX_EXTRA_PARAMS_CHARS && !value.chars().any(char::is_control),
        "XLaunch additional parameters are too long or contain control characters"
    );
    let mut characters = value.chars().peekable();
    let mut args = Vec::new();
    while characters.peek().is_some() {
        while characters.peek() == Some(&' ') {
            characters.next();
        }
        if characters.peek().is_none() {
            break;
        }
        let mut arg = String::new();
        let mut quoted = false;
        while let Some(character) = characters.peek().copied() {
            if character == ' ' && !quoted {
                break;
            }
            let mut slashes = 0;
            while characters.peek() == Some(&'\\') {
                characters.next();
                slashes += 1;
            }
            if characters.peek() == Some(&'"') {
                arg.extend(std::iter::repeat_n('\\', slashes / 2));
                characters.next();
                if slashes % 2 == 1 {
                    arg.push('"');
                } else if quoted && characters.peek() == Some(&'"') {
                    characters.next();
                    arg.push('"');
                } else {
                    quoted = !quoted;
                }
            } else {
                arg.extend(std::iter::repeat_n('\\', slashes));
                match characters.peek().copied() {
                    None => break,
                    Some(' ') if !quoted => break,
                    Some(next) => {
                        characters.next();
                        arg.push(next);
                    }
                }
            }
        }
        ensure!(
            !quoted,
            "XLaunch additional parameters contain an unclosed quote"
        );
        args.push(arg);
        ensure!(
            args.len() <= MAX_EXTRA_ARGS,
            "too many XLaunch additional parameters"
        );
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search_fixture() -> (PathBuf, PathBuf, Vec<PathBuf>) {
        let root =
            std::env::temp_dir().join(format!("axssh-xlaunch-search-{}", uuid::Uuid::new_v4()));
        let program = root.join("nonstandard-installation");
        let folders = vec![root.join("Desktop"), root.join("Documents")];
        for directory in [&program, &folders[0], &folders[1]] {
            std::fs::create_dir_all(directory).unwrap();
        }
        (root, program, folders)
    }

    #[test]
    fn program_directory_takes_priority_without_reading_xml() {
        let (root, program, folders) = search_fixture();
        let expected = program.join("session.XLAUNCH");
        std::fs::write(&expected, "discovery must not parse this XML").unwrap();
        std::fs::write(folders[0].join("other.xlaunch"), "").unwrap();
        let found = discover_in_directories(Some(&program), &folders).unwrap();
        assert_eq!(found, std::slice::from_ref(&expected));
        assert_eq!(select_configuration(found).unwrap(), Some(expected));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn user_folders_allow_one_candidate_but_require_a_choice_for_several() {
        let (root, program, folders) = search_fixture();
        let expected = folders[1].join("work.xlaunch");
        std::fs::write(&expected, "").unwrap();
        assert_eq!(
            select_configuration(discover_in_directories(Some(&program), &folders).unwrap())
                .unwrap(),
            Some(expected.clone())
        );
        std::fs::write(folders[0].join("home.xlaunch"), "").unwrap();
        let found = discover_in_directories(Some(&program), &folders).unwrap();
        assert_eq!(found.len(), 2);
        assert!(select_configuration(found).is_err());
        assert_eq!(
            discover_in_directories(None, &[folders[1].clone(), folders[1].clone()]).unwrap(),
            [expected]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovery_is_nonrecursive_regular_file_only_and_bounded() {
        let (root, program, folders) = search_fixture();
        let nested = folders[0].join("nested");
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(nested.join("nested.xlaunch"), "").unwrap();
        std::fs::create_dir(program.join("directory.xlaunch")).unwrap();
        assert!(
            discover_in_directories(Some(&program), &folders)
                .unwrap()
                .is_empty()
        );
        assert_eq!(select_configuration(Vec::new()).unwrap(), None);
        for index in 0..=MAX_DISCOVERED_CONFIGS {
            std::fs::write(program.join(format!("{index}.xlaunch")), "").unwrap();
        }
        assert!(discover_in_directories(Some(&program), &folders).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    fn arguments(config: &WindowsLaunchConfig, allow_no_auth: bool) -> Result<Vec<String>> {
        Ok(config
            .command(Path::new("D:/X servers/Xming.exe"), 2, allow_no_auth)?
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect())
    }

    #[test]
    fn defaults_keep_provider_arguments_and_explicit_compatibility() {
        let config = WindowsLaunchConfig::default();
        assert!(arguments(&config, false).is_err());
        assert_eq!(
            arguments(&config, true).unwrap(),
            [":2", "-multiwindow", "-clipboard", "-ac"]
        );
    }

    #[test]
    fn imports_xming_namespaced_config_and_vcxsrv_server_options() {
        let config = WindowsLaunchConfig::parse(
            r#"<XLaunch xmlns="http://www.straightrunning.com/XmingNotes"
            ClientMode="NoClient" Display="2" WindowMode="Fullscreen" Clipboard="false"
            ClipboardPrimary="False" Wgl="False" DisableAC="True" ExtraParams="-dpi 144"/>"#,
        )
        .unwrap();
        assert_eq!(config.display, Some(2));
        assert!(arguments(&config, false).is_err());
        assert_eq!(
            arguments(&config, true).unwrap(),
            [
                ":2",
                "-fullscreen",
                "-noclipboard",
                "-noprimary",
                "-nowgl",
                "-ac",
                "-dpi",
                "144"
            ]
        );
        let auto =
            WindowsLaunchConfig::parse(r#"<XLaunch Display="-1" ClientMode="NoClient"/>"#).unwrap();
        assert_eq!(auto.display, None);
        assert_eq!(
            arguments(&auto, false).unwrap(),
            [":2", "-multiwindow", "-clipboard"]
        );
    }

    #[test]
    fn additional_arguments_keep_windows_paths_and_quoted_values() {
        assert_eq!(split_windows_arguments(r#" -auth "C:\X server\authority" -screen 0 "1280x720@1" "" "a\\\"b" "C:\folder\\" "#).unwrap(),
            ["-auth", "C:\\X server\\authority", "-screen", "0", "1280x720@1", "", "a\\\"b", "C:\\folder\\"]);
    }

    #[test]
    fn authority_parameters_preserve_the_explicit_compatibility_choice() {
        let authorized = WindowsLaunchConfig::parse(
            r#"<XLaunch ExtraParams="-auth &quot;C:\X server\authority&quot;"/>"#,
        )
        .unwrap();
        assert_eq!(
            arguments(&authorized, false).unwrap(),
            [
                ":2",
                "-multiwindow",
                "-clipboard",
                "-auth",
                "C:\\X server\\authority"
            ]
        );
        let compatible = WindowsLaunchConfig::parse(r#"<XLaunch ExtraParams="-ac"/>"#).unwrap();
        assert!(arguments(&compatible, false).is_err());
        assert_eq!(
            arguments(&compatible, true).unwrap(),
            [":2", "-multiwindow", "-clipboard", "-ac"]
        );
    }

    #[test]
    fn reads_a_regular_xlaunch_file_without_launching_a_process() {
        let path = std::env::temp_dir().join(format!("axssh-{}.xlaunch", uuid::Uuid::new_v4()));
        std::fs::write(
            &path,
            r#"<?xml version="1.0" encoding="UTF-8"?><XLaunch Display="3"/>"#,
        )
        .unwrap();
        let config = WindowsLaunchConfig::load(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(config.display, Some(3));
    }

    #[test]
    fn reports_unsupported_configuration_values() {
        for xml in [
            r#"<XLaunch ClientMode="StartProgram"/>"#,
            r#"<XLaunch ClientMode="XDMCP"/>"#,
            r#"<XLaunch Display="-2"/>"#,
            r#"<XLaunch Display="65535"/>"#,
            r#"<XLaunch Clipboard="maybe"/>"#,
            r#"<XLaunch WindowMode="unknown"/>"#,
            "<Other/>",
        ] {
            assert!(WindowsLaunchConfig::parse(xml).is_err());
        }
    }
}
