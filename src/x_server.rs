//! Platform-local X server selection and bounded process startup.

#[cfg(any(target_os = "windows", test))]
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::time::{Duration, timeout};

use crate::config::{X11ServerProvider, X11Settings};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_COMMAND_OUTPUT_BYTES: usize = 1_024;
const MAX_DISPLAY_CANDIDATES: usize = 8;
const MAX_DISCOVERED_PROVIDER_LOCATIONS: usize = 4;
const MAX_DISCOVERED_LOCATION_CHARS: usize = 1_024;
#[cfg(target_os = "windows")]
const X11_TCP_PORT_BASE: u16 = 6_000;

#[cfg(target_os = "macos")]
const XQUARTZ_APP_PATH: &str = "/Applications/Utilities/XQuartz.app";
#[cfg(target_os = "macos")]
const XQUARTZ_BUNDLE_ID: &str = "org.xquartz.X11";
#[cfg(target_os = "macos")]
const MACXSERVER_APP_PATH: &str = "/Applications/MacXServer.app";
#[cfg(target_os = "macos")]
const MACXSERVER_BUNDLE_ID: &str = "com.toddvernon.swiftx.server";

#[derive(Clone, Debug)]
pub(crate) struct XServerPlan {
    provider: X11ServerProvider,
    app_path: Option<PathBuf>,
    launch_on_connect: bool,
    allow_no_auth: bool,
}

impl XServerPlan {
    pub(crate) async fn resolve(settings: X11Settings) -> Result<Self> {
        tokio::task::spawn_blocking(move || resolve_plan(settings))
            .await
            .context("local X server selection task failed")?
    }

    pub(crate) const fn launch_on_x11_request(&self) -> bool {
        self.launch_on_connect
    }

    pub(crate) const fn allow_no_auth(&self) -> bool {
        self.allow_no_auth
    }

    pub(crate) async fn display_candidates(&self) -> Vec<String> {
        let mut candidates = Vec::new();
        match self.provider {
            X11ServerProvider::MacXServer
            | X11ServerProvider::VcXsrv
            | X11ServerProvider::Xming => {
                push_unique(&mut candidates, "127.0.0.1:0".to_owned());
            }
            X11ServerProvider::XQuartz => {
                append_environment_display(&mut candidates);
                if let Some(display) = launchctl_display().await {
                    push_unique(&mut candidates, display);
                }
                for display in xquartz_socket_displays().await {
                    push_unique(&mut candidates, display);
                }
                push_unique(&mut candidates, ":0".to_owned());
            }
            X11ServerProvider::Auto | X11ServerProvider::System | X11ServerProvider::Custom => {
                append_environment_display(&mut candidates);
                push_unique(&mut candidates, platform_default_display());
            }
        }
        candidates.truncate(MAX_DISPLAY_CANDIDATES);
        candidates
    }

    pub(crate) async fn launch(&self) -> Result<String> {
        if !self.launch_on_connect {
            anyhow::bail!("automatic local X server startup is disabled");
        }
        let app_path = self
            .app_path
            .as_ref()
            .context("no local X server application is configured")?;
        ensure_launch_target(self.provider, app_path).await?;

        match self.provider {
            X11ServerProvider::XQuartz => {
                launch_macos_app(app_path, &[]).await?;
                Ok(":0".to_owned())
            }
            X11ServerProvider::MacXServer => {
                if !self.allow_no_auth {
                    anyhow::bail!("MacXServer requires explicit local-only no-auth compatibility");
                }
                launch_macos_app(app_path, &["--host", "127.0.0.1", "--port", "6000"]).await?;
                Ok("127.0.0.1:0".to_owned())
            }
            X11ServerProvider::VcXsrv | X11ServerProvider::Xming => {
                launch_windows_server(app_path, self.allow_no_auth).await
            }
            X11ServerProvider::Custom => {
                launch_custom_server(app_path).await?;
                Ok(platform_default_display())
            }
            X11ServerProvider::Auto | X11ServerProvider::System => {
                anyhow::bail!("the selected X server provider has no launchable application")
            }
        }
    }
}

pub fn provider_options() -> Vec<String> {
    if cfg!(target_os = "macos") {
        ["Auto", "XQuartz", "MacXServer", "Custom"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    } else if cfg!(target_os = "windows") {
        ["Auto", "VcXsrv", "Xming", "Custom"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    } else {
        ["System DISPLAY", "Custom"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }
}

/// Returns discovered, launchable known-provider locations for UI display.
///
/// This synchronous filesystem and platform-service lookup must run in a
/// blocking worker; it never launches a provider or inspects a custom path.
pub fn discovered_provider_locations() -> Vec<String> {
    known_providers_for_current_platform()
        .into_iter()
        .filter_map(|provider| {
            default_app_path(provider).and_then(|path| provider_location_text(provider, &path))
        })
        .take(MAX_DISCOVERED_PROVIDER_LOCATIONS)
        .collect()
}

pub fn provider_index(provider: X11ServerProvider) -> i32 {
    let label = provider_label(provider_for_current_platform(provider));
    provider_options()
        .iter()
        .position(|option| option == label)
        .unwrap_or(0) as i32
}

pub fn provider_for_current_platform(provider: X11ServerProvider) -> X11ServerProvider {
    if cfg!(target_os = "macos") {
        match provider {
            X11ServerProvider::Auto
            | X11ServerProvider::XQuartz
            | X11ServerProvider::MacXServer
            | X11ServerProvider::Custom => provider,
            _ => X11ServerProvider::Auto,
        }
    } else if cfg!(target_os = "windows") {
        match provider {
            X11ServerProvider::Auto
            | X11ServerProvider::VcXsrv
            | X11ServerProvider::Xming
            | X11ServerProvider::Custom => provider,
            _ => X11ServerProvider::Auto,
        }
    } else {
        match provider {
            X11ServerProvider::Custom => X11ServerProvider::Custom,
            _ => X11ServerProvider::System,
        }
    }
}

fn provider_label(provider: X11ServerProvider) -> &'static str {
    match provider {
        X11ServerProvider::Auto => "Auto",
        X11ServerProvider::System => "System DISPLAY",
        X11ServerProvider::XQuartz => "XQuartz",
        X11ServerProvider::MacXServer => "MacXServer",
        X11ServerProvider::VcXsrv => "VcXsrv",
        X11ServerProvider::Xming => "Xming",
        X11ServerProvider::Custom => "Custom",
    }
}

fn known_providers_for_current_platform() -> Vec<X11ServerProvider> {
    if cfg!(target_os = "macos") {
        vec![X11ServerProvider::XQuartz, X11ServerProvider::MacXServer]
    } else if cfg!(target_os = "windows") {
        vec![X11ServerProvider::VcXsrv, X11ServerProvider::Xming]
    } else {
        Vec::new()
    }
}

fn provider_location_text(provider: X11ServerProvider, path: &Path) -> Option<String> {
    let path = path.display().to_string();
    if path.is_empty()
        || path.chars().count() > MAX_DISCOVERED_LOCATION_CHARS
        || path.chars().any(char::is_control)
    {
        return None;
    }
    Some(format!("{}: {path}", provider_label(provider)))
}

fn resolve_plan(settings: X11Settings) -> Result<XServerPlan> {
    let configured_provider = provider_for_current_platform(settings.provider);
    let provider = match configured_provider {
        X11ServerProvider::Auto => auto_provider(),
        provider => provider,
    };
    let app_path = match configured_provider {
        X11ServerProvider::Custom if !settings.app_path.is_empty() => {
            Some(PathBuf::from(&settings.app_path))
        }
        X11ServerProvider::Custom => None,
        _ => default_app_path(provider),
    };
    Ok(XServerPlan {
        provider,
        app_path,
        launch_on_connect: settings.launch_on_connect,
        allow_no_auth: settings.allow_no_auth,
    })
}

fn auto_provider() -> X11ServerProvider {
    #[cfg(target_os = "macos")]
    {
        if default_app_path(X11ServerProvider::XQuartz).is_some() {
            return X11ServerProvider::XQuartz;
        }
        if default_app_path(X11ServerProvider::MacXServer).is_some() {
            return X11ServerProvider::MacXServer;
        }
    }
    #[cfg(target_os = "windows")]
    {
        if default_windows_path("VcXsrv", "vcxsrv.exe").is_some() {
            return X11ServerProvider::VcXsrv;
        }
        if default_windows_path("Xming", "Xming.exe").is_some() {
            return X11ServerProvider::Xming;
        }
    }
    X11ServerProvider::System
}

fn default_app_path(provider: X11ServerProvider) -> Option<PathBuf> {
    match provider {
        #[cfg(target_os = "macos")]
        X11ServerProvider::XQuartz => {
            macos_application_path(XQUARTZ_BUNDLE_ID, Path::new(XQUARTZ_APP_PATH))
        }
        #[cfg(target_os = "macos")]
        X11ServerProvider::MacXServer => {
            macos_application_path(MACXSERVER_BUNDLE_ID, Path::new(MACXSERVER_APP_PATH))
        }
        #[cfg(target_os = "windows")]
        X11ServerProvider::VcXsrv => default_windows_path("VcXsrv", "vcxsrv.exe"),
        #[cfg(target_os = "windows")]
        X11ServerProvider::Xming => default_windows_path("Xming", "Xming.exe"),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
fn macos_application_path(bundle_identifier: &str, fallback: &Path) -> Option<PathBuf> {
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::NSString;

    let discovered = autoreleasepool(|_| {
        let bundle_identifier = NSString::from_str(bundle_identifier);
        NSWorkspace::sharedWorkspace()
            .URLForApplicationWithBundleIdentifier(&bundle_identifier)
            .and_then(|url| url.path())
            .map(|path| PathBuf::from(path.to_string()))
    });
    discovered
        .filter(|path| path.exists())
        .or_else(|| fallback.exists().then(|| fallback.to_owned()))
}

#[cfg(target_os = "windows")]
fn default_windows_path(directory: &str, executable: &str) -> Option<PathBuf> {
    let search_path = std::env::var_os("PATH");
    executable_on_search_path(search_path.as_deref(), executable).or_else(|| {
        ["ProgramFiles", "ProgramFiles(x86)"]
            .into_iter()
            .filter_map(std::env::var_os)
            .map(PathBuf::from)
            .map(|root| root.join(directory).join(executable))
            .find(|path| path.is_file())
    })
}

#[cfg(any(target_os = "windows", test))]
fn executable_on_search_path(search_path: Option<&OsStr>, executable: &str) -> Option<PathBuf> {
    search_path
        .into_iter()
        .flat_map(std::env::split_paths)
        .map(|directory| directory.join(executable))
        .find(|path| path.is_file())
}

fn append_environment_display(candidates: &mut Vec<String>) {
    if let Ok(display) = std::env::var("DISPLAY") {
        let display = display.trim();
        if !display.is_empty() {
            push_unique(candidates, display.to_owned());
        }
    }
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if values.len() < MAX_DISPLAY_CANDIDATES && !values.contains(&value) {
        values.push(value);
    }
}

fn platform_default_display() -> String {
    if cfg!(target_os = "windows") {
        "127.0.0.1:0".to_owned()
    } else {
        ":0".to_owned()
    }
}

async fn ensure_launch_target(provider: X11ServerProvider, path: &Path) -> Result<()> {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || validate_launch_target(provider, &path))
        .await
        .context("local X server path check failed")?
}

fn validate_launch_target(provider: X11ServerProvider, path: &Path) -> Result<()> {
    let metadata = path.metadata().with_context(|| {
        format!(
            "local X server application does not exist at {}",
            path.display()
        )
    })?;
    if provider != X11ServerProvider::Custom {
        return Ok(());
    }
    if !metadata.is_file() {
        anyhow::bail!(
            "custom X server path must be a regular executable file: {}",
            path.display()
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        if metadata.permissions().mode() & 0o111 == 0 {
            anyhow::bail!("custom X server file is not executable: {}", path.display());
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
async fn launch_macos_app(path: &Path, app_args: &[&str]) -> Result<()> {
    let mut command = Command::new("open");
    command.arg("-g").arg(path);
    if !app_args.is_empty() {
        command.arg("--args").args(app_args);
    }
    run_short_command(command, "launch local macOS X server").await
}

#[cfg(not(target_os = "macos"))]
async fn launch_macos_app(_path: &Path, _app_args: &[&str]) -> Result<()> {
    anyhow::bail!("macOS X server applications are unavailable on this platform")
}

#[cfg(target_os = "windows")]
async fn launch_windows_server(path: &Path, allow_no_auth: bool) -> Result<String> {
    if !allow_no_auth {
        anyhow::bail!(
            "automatic VcXsrv/Xming startup requires explicit local no-auth compatibility"
        );
    }
    let display = select_available_windows_display();
    let display_arg = format!(":{display}");
    let mut command = detached_command(path);
    command
        .arg(display_arg)
        .arg("-multiwindow")
        .arg("-clipboard")
        .arg("-ac");
    command
        .spawn()
        .context("failed to launch the configured Windows X server")?;
    Ok(format!("127.0.0.1:{display}"))
}

#[cfg(not(target_os = "windows"))]
async fn launch_windows_server(_path: &Path, _allow_no_auth: bool) -> Result<String> {
    anyhow::bail!("Windows X server applications are unavailable on this platform")
}

async fn launch_custom_server(path: &Path) -> Result<()> {
    let mut command = detached_command(path);
    command
        .spawn()
        .context("failed to launch the configured custom X server")?;
    Ok(())
}

fn detached_command(program: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(false);
    command
}

#[cfg(target_os = "macos")]
async fn run_short_command(mut command: Command, operation: &'static str) -> Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command.spawn().with_context(|| operation)?;
    let status = match timeout(PROCESS_TIMEOUT, child.wait()).await {
        Ok(status) => status.with_context(|| operation)?,
        Err(_) => {
            terminate_child(&mut child).await;
            anyhow::bail!("{operation} timed out");
        }
    };
    if !status.success() {
        anyhow::bail!("{operation} exited with {status}");
    }
    Ok(())
}

async fn launchctl_display() -> Option<String> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let mut command = Command::new("launchctl");
    command
        .arg("getenv")
        .arg("DISPLAY")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    bounded_command_output(command)
        .await
        .ok()
        .and_then(|bytes| {
            std::str::from_utf8(&bytes)
                .ok()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        })
}

async fn bounded_command_output(mut command: Command) -> Result<Vec<u8>> {
    let mut child = command.spawn().context("failed to start helper command")?;
    let stdout = child
        .stdout
        .take()
        .context("helper command stdout was not captured")?;
    let mut output = Vec::new();
    let mut limited = stdout.take((MAX_COMMAND_OUTPUT_BYTES + 1) as u64);
    match timeout(PROCESS_TIMEOUT, limited.read_to_end(&mut output)).await {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => {
            terminate_child(&mut child).await;
            return Err(error).context("failed to read helper command output");
        }
        Err(_) => {
            terminate_child(&mut child).await;
            anyhow::bail!("helper command output timed out");
        }
    }
    if output.len() > MAX_COMMAND_OUTPUT_BYTES {
        terminate_child(&mut child).await;
        anyhow::bail!("helper command output exceeded its size limit");
    }
    let status = timeout(PROCESS_TIMEOUT, child.wait())
        .await
        .context("helper command did not exit before its timeout")??;
    if !status.success() {
        anyhow::bail!("helper command failed with {status}");
    }
    Ok(output)
}

async fn terminate_child(child: &mut Child) {
    let _ = child.kill().await;
    let _ = child.wait().await;
}

#[cfg(unix)]
async fn xquartz_socket_displays() -> Vec<String> {
    use std::os::unix::fs::FileTypeExt as _;

    tokio::task::spawn_blocking(|| {
        let mut displays = Vec::new();
        let Ok(entries) = std::fs::read_dir("/private/tmp") else {
            return displays;
        };
        for entry in entries.flatten().take(256) {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if !name.starts_with("com.apple.launchd.") {
                continue;
            }
            let socket = entry.path().join("org.xquartz");
            if socket
                .metadata()
                .is_ok_and(|metadata| metadata.file_type().is_socket())
            {
                displays.push(format!("{}:0", socket.display()));
                if displays.len() == MAX_DISPLAY_CANDIDATES {
                    break;
                }
            }
        }
        displays
    })
    .await
    .unwrap_or_default()
}

#[cfg(not(unix))]
async fn xquartz_socket_displays() -> Vec<String> {
    Vec::new()
}

#[cfg(target_os = "windows")]
fn select_available_windows_display() -> u16 {
    for display in 0..64u16 {
        let Some(port) = X11_TCP_PORT_BASE.checked_add(display) else {
            break;
        };
        if std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).is_ok() {
            return display;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_options_and_indices_follow_the_current_platform() {
        let options = provider_options();
        assert!(!options.is_empty());
        assert!(provider_index(X11ServerProvider::Auto) >= 0);
        if cfg!(target_os = "macos") {
            assert!(options.contains(&"XQuartz".to_owned()));
            assert!(options.contains(&"MacXServer".to_owned()));
        } else if cfg!(target_os = "windows") {
            assert!(options.contains(&"VcXsrv".to_owned()));
            assert!(options.contains(&"Xming".to_owned()));
        } else {
            assert_eq!(options[0], "System DISPLAY");
            assert_eq!(
                provider_for_current_platform(X11ServerProvider::Auto),
                X11ServerProvider::System
            );
        }
    }

    #[test]
    fn discovered_provider_locations_only_describe_supported_known_providers() {
        let providers = known_providers_for_current_platform();
        if cfg!(target_os = "macos") {
            assert_eq!(
                providers,
                [X11ServerProvider::XQuartz, X11ServerProvider::MacXServer]
            );
        } else if cfg!(target_os = "windows") {
            assert_eq!(
                providers,
                [X11ServerProvider::VcXsrv, X11ServerProvider::Xming]
            );
        } else {
            assert!(providers.is_empty());
        }
        assert!(discovered_provider_locations().len() <= MAX_DISCOVERED_PROVIDER_LOCATIONS);
    }

    #[test]
    fn provider_location_text_bounds_untrusted_path_display_text() {
        assert_eq!(
            provider_location_text(X11ServerProvider::Custom, Path::new("/opt/xserver")),
            Some("Custom: /opt/xserver".to_owned())
        );
        assert!(
            provider_location_text(X11ServerProvider::Custom, Path::new("bad\npath")).is_none()
        );
    }

    #[test]
    fn macxserver_provider_is_normalized_for_the_current_platform() {
        let normalized = provider_for_current_platform(X11ServerProvider::MacXServer);
        if cfg!(target_os = "macos") {
            assert_eq!(normalized, X11ServerProvider::MacXServer);
        } else if cfg!(target_os = "windows") {
            assert_eq!(normalized, X11ServerProvider::Auto);
        } else {
            assert_eq!(normalized, X11ServerProvider::System);
        }
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn explicit_macxserver_plan_is_loopback_and_requires_compatibility_to_launch() {
        let plan = XServerPlan::resolve(X11Settings {
            provider: X11ServerProvider::MacXServer,
            app_path: String::new(),
            launch_on_connect: true,
            allow_no_auth: false,
        })
        .await
        .expect("plan should resolve without launching");
        assert_eq!(plan.provider, X11ServerProvider::MacXServer);
        assert_eq!(plan.display_candidates().await, ["127.0.0.1:0"]);
        assert!(!plan.allow_no_auth());
    }

    #[tokio::test]
    async fn auto_provider_ignores_a_stale_custom_path() {
        let plan = XServerPlan::resolve(X11Settings {
            provider: X11ServerProvider::Auto,
            app_path: "/opt/custom/x-server".to_owned(),
            launch_on_connect: true,
            allow_no_auth: false,
        })
        .await
        .expect("auto plan should resolve without launching");
        assert_ne!(plan.provider, X11ServerProvider::Custom);
        assert_ne!(plan.app_path, Some(PathBuf::from("/opt/custom/x-server")));
    }

    #[tokio::test]
    async fn known_provider_ignores_a_saved_custom_path() {
        let provider = if cfg!(target_os = "macos") {
            X11ServerProvider::XQuartz
        } else if cfg!(target_os = "windows") {
            X11ServerProvider::VcXsrv
        } else {
            X11ServerProvider::System
        };
        let plan = XServerPlan::resolve(X11Settings {
            provider,
            app_path: "/opt/custom/x-server".to_owned(),
            launch_on_connect: true,
            allow_no_auth: false,
        })
        .await
        .expect("known provider plan should resolve without launching");

        assert_ne!(plan.app_path, Some(PathBuf::from("/opt/custom/x-server")));
    }

    #[tokio::test]
    async fn custom_provider_keeps_its_configured_path() {
        let plan = XServerPlan::resolve(X11Settings {
            provider: X11ServerProvider::Custom,
            app_path: "/opt/custom/x-server".to_owned(),
            launch_on_connect: true,
            allow_no_auth: false,
        })
        .await
        .expect("custom provider plan should resolve without launching");

        assert_eq!(plan.app_path, Some(PathBuf::from("/opt/custom/x-server")));
    }

    #[test]
    fn executable_search_path_prefers_the_first_existing_file() {
        let root = std::env::temp_dir().join(format!("axssh-x11-path-{}", uuid::Uuid::new_v4()));
        let first = root.join("first");
        let second = root.join("second");
        std::fs::create_dir_all(&first).expect("first search directory should be created");
        std::fs::create_dir_all(&second).expect("second search directory should be created");
        std::fs::write(first.join("xserver.exe"), [])
            .expect("first test executable should be created");
        std::fs::write(second.join("xserver.exe"), []).expect("test executable should be created");
        let search_path = std::env::join_paths([&first, &second]).expect("search path should join");

        assert_eq!(
            executable_on_search_path(Some(&search_path), "xserver.exe"),
            Some(first.join("xserver.exe"))
        );

        std::fs::remove_dir_all(root).expect("test search directories should be removed");
    }

    #[test]
    fn custom_launch_target_rejects_a_directory() {
        let root = std::env::temp_dir().join(format!("axssh-x11-custom-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("test directory should be created");

        let error = validate_launch_target(X11ServerProvider::Custom, &root)
            .expect_err("custom launch target must be a file");
        assert!(error.to_string().contains("regular executable file"));

        std::fs::remove_dir_all(root).expect("test directory should be removed");
    }

    #[cfg(unix)]
    #[test]
    fn custom_launch_target_requires_unix_executable_permission() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = std::env::temp_dir().join(format!("axssh-x11-mode-{}", uuid::Uuid::new_v4()));
        let target = root.join("xserver");
        std::fs::create_dir_all(&root).expect("test directory should be created");
        std::fs::write(&target, []).expect("test launch target should be created");
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644))
            .expect("test launch target permissions should be set");

        let error = validate_launch_target(X11ServerProvider::Custom, &target)
            .expect_err("non-executable custom launch target must be rejected");
        assert!(error.to_string().contains("not executable"));

        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))
            .expect("test launch target permissions should be updated");
        validate_launch_target(X11ServerProvider::Custom, &target)
            .expect("executable custom launch target should be accepted");

        std::fs::remove_dir_all(root).expect("test directory should be removed");
    }
}
