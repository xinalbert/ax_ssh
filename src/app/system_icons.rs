//! Platform icon identity mapping and bounded local system discovery.

#[cfg(target_os = "linux")]
use std::fs::File;
#[cfg(any(target_os = "linux", test))]
use std::io::Read;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::sync::OnceLock;

#[cfg(any(target_os = "linux", test))]
const MAX_OS_RELEASE_BYTES: u64 = 8192;
#[cfg(any(target_os = "linux", target_os = "macos"))]
static LOCAL_PLATFORM: OnceLock<&'static str> = OnceLock::new();

mod catalog;

use catalog::{LINUX_DISTRIBUTIONS, PLATFORM_FAMILIES};

pub(super) fn local_platform() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        LOCAL_PLATFORM.get().copied().unwrap_or("linux")
    }
    #[cfg(target_os = "macos")]
    {
        LOCAL_PLATFORM.get().copied().unwrap_or("macos")
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        match std::env::consts::OS {
            "windows" => "windows",
            _ => "",
        }
    }
}

#[cfg(target_os = "macos")]
pub(super) fn discover_local_platform() -> &'static str {
    LOCAL_PLATFORM.get_or_init(discover_macos_platform)
}

#[cfg(target_os = "macos")]
fn discover_macos_platform() -> &'static str {
    let mut version = [0u8; 64];
    let mut length = version.len();
    // SAFETY: The name is NUL-terminated. The writable output buffer matches
    // the initial length, and both pointers remain valid for the call.
    let status = unsafe {
        libc::sysctlbyname(
            c"kern.osproductversion".as_ptr(),
            version.as_mut_ptr().cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        let error = std::io::Error::last_os_error();
        tracing::debug!(%error, "could not read the local macOS product version");
        return "macos";
    }
    version
        .get(..length)
        .and_then(|bytes| std::ffi::CStr::from_bytes_with_nul(bytes).ok())
        .and_then(|version| version.to_str().ok())
        .map(|version| macos_platform(&normalized(version)))
        .unwrap_or("macos")
}

// Called only from the startup blocking worker; UI mappings read the cache.
#[cfg(target_os = "linux")]
pub(super) fn discover_local_platform() -> &'static str {
    LOCAL_PLATFORM.get_or_init(|| {
        for path in ["/etc/os-release", "/usr/lib/os-release"] {
            match read_os_release(path) {
                Ok(release) => return platform_for_os_release(&release),
                Err(error) => tracing::debug!(%error, path, "local OS release source unavailable"),
            }
        }
        "linux"
    })
}

#[cfg(target_os = "linux")]
fn read_os_release(path: &str) -> std::io::Result<String> {
    read_release(File::open(path)?)
}

#[cfg(any(target_os = "linux", test))]
fn read_release(source: impl Read) -> std::io::Result<String> {
    let mut content = String::new();
    source
        .take(MAX_OS_RELEASE_BYTES + 1)
        .read_to_string(&mut content)?;
    if content.len() as u64 > MAX_OS_RELEASE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "OS release exceeds 8 KiB",
        ));
    }
    Ok(content)
}

fn normalized(value: &str) -> String {
    let value = value
        .replace("(R)", "")
        .replace("(r)", "")
        .replace("(TM)", "")
        .replace("(tm)", "");
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(any(target_os = "linux", test))]
fn platform_for_os_release(content: &str) -> &'static str {
    let mut id = None;
    let mut name = None;
    let mut pretty = None;
    for line in content.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches(['"', '\'']);
        match key.trim() {
            "ID" if !value.is_empty() => id = Some(normalized(value)),
            "NAME" => name = Some(value),
            "PRETTY_NAME" => pretty = Some(value),
            _ => {}
        }
    }
    if let Some(id) = id {
        return LINUX_DISTRIBUTIONS
            .iter()
            .find(|(_, aliases)| aliases.contains(&id.as_str()))
            .map(|(platform, _)| *platform)
            .unwrap_or("linux");
    }
    let platform = platform_for_release(
        pretty
            .filter(|name| !name.is_empty())
            .or(name)
            .unwrap_or(""),
    );
    if LINUX_DISTRIBUTIONS.iter().any(|(key, _)| *key == platform) {
        platform
    } else {
        "linux"
    }
}

pub(super) fn platform_for_release(release: &str) -> &'static str {
    let release = normalized(release);
    let words = format!(" {release} ");
    let contains = |alias: &str| words.contains(&format!(" {alias} "));
    if let Some((platform, _)) = LINUX_DISTRIBUTIONS
        .iter()
        .find(|(_, aliases)| aliases.iter().any(|alias| contains(alias)))
    {
        return platform;
    }
    // A Linux kernel label can contain "microsoft" under WSL.
    if !contains("linux") {
        if let Some(product) = ["macos ", "mac os x ", "mac os ", "os x "]
            .iter()
            .find_map(|prefix| release.strip_prefix(prefix))
        {
            return macos_platform(product);
        }
        if let Some(platform) = versioned_platform("windows", contains) {
            return platform;
        }
        if contains("windows server") {
            return "windows-server";
        }
    }
    PLATFORM_FAMILIES
        .iter()
        .filter(|(key, _)| *key == "linux")
        .chain(PLATFORM_FAMILIES.iter().filter(|(key, _)| *key != "linux"))
        .find(|(_, aliases)| aliases.iter().any(|alias| contains(alias)))
        .map(|(platform, _)| *platform)
        .unwrap_or("")
}

fn macos_platform(product: &str) -> &'static str {
    let label = format!("macos {product}");
    versioned_platform("macos", |alias| {
        label
            .strip_prefix(alias)
            .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with(' '))
    })
    .unwrap_or("macos")
}

fn versioned_platform(family: &str, matches_alias: impl Fn(&str) -> bool) -> Option<&'static str> {
    ax_ssh::config::system_icon_choices()
        .iter()
        .filter(|icon| icon.family == family && icon.id != family && icon.id != "windows-server")
        .flat_map(|icon| icon.aliases.iter().map(move |alias| (icon.id, *alias)))
        .filter(|(_, alias)| matches_alias(alias))
        .max_by_key(|(_, alias)| alias.len())
        .map(|(id, _)| id)
}

// Older automatic caches can refine their family using the stored release.
pub(super) fn platform_for_detection(
    system: &ax_ssh::config::SystemDetection,
) -> Option<&'static str> {
    let icon = system.icon_id().and_then(ax_ssh::config::system_icon_id)?;
    if matches!(icon, "windows" | "macos") {
        let refined = platform_for_release(system.release());
        if refined
            .strip_prefix(icon)
            .is_some_and(|suffix| suffix.starts_with('-'))
        {
            return Some(refined);
        }
    }
    Some(icon)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_releases_keep_distribution_and_edition_identity() {
        for (release, expected) in [
            ("Ubuntu 24.04 LTS", "ubuntu"),
            ("Debian GNU/Linux 12 (bookworm)", "debian"),
            ("Fedora Linux 42 (Workstation Edition)", "fedora"),
            ("Fedora CoreOS 42", "coreos"),
            ("Red Hat Enterprise Linux 9.6", "rhel"),
            ("AlmaLinux 9.6", "almalinux"),
            ("Rocky Linux 9.6", "rocky-linux"),
            ("Alpine Linux v3.21", "alpine-linux"),
            ("Arch Linux", "arch-linux"),
            ("Linux Mint 22.1", "linux-mint"),
            ("openSUSE Leap 15.6", "opensuse-leap"),
            ("openSUSE Tumbleweed", "opensuse-tumbleweed"),
            ("Ubuntu MATE 24.04", "ubuntu-mate"),
            ("Kubuntu 24.04", "kubuntu"),
            ("Raspbian GNU/Linux 12", "raspberry-pi-os"),
            ("Pop!_OS 22.04", "pop-os"),
            ("openEuler 24.03", "openeuler"),
            ("Anolis OS 8.10", "anolis-os"),
            ("UnionTech UOS 20", "uos"),
            ("Amazon Linux 2023", "linux"),
            ("Custom Linux 1.0", "linux"),
            ("Linux 6.12.0", "linux"),
            ("Linux 6.6.87.2-microsoft-standard-WSL2", "linux"),
            ("FreeBSD 14.3", ""),
            ("Archival Server", ""),
            ("Microsoft Windows Server 2022", "windows-server"),
        ] {
            assert_eq!(platform_for_release(release), expected, "{release}");
        }
    }

    #[test]
    fn macos_uses_product_versions_and_names() {
        for (release, expected) in [
            ("macOS 10.13.6", "macos-10-13"),
            ("Mac OS X 10.14.6", "macos-10-14"),
            ("macOS Catalina", "macos-10-15"),
            ("macOS 11.7.10", "macos-11"),
            ("macOS 12.7.6", "macos-12"),
            ("macOS 13.7.8", "macos-13"),
            ("macOS 14.8.2", "macos-14"),
            ("macOS 15.7.7", "macos-15"),
            ("macOS 26.1", "macos-26"),
            ("macOS 27 Golden Gate", "macos-27"),
            ("macOS 10.12.6", "macos"),
            ("macOS 15.0.26", "macos-15"),
            ("macOS 150.0", "macos"),
            ("macOS 28.0", "macos"),
            ("macOS", "macos"),
            ("Darwin 26.1.0", "macos"),
            ("Ubuntu on macOS 15", "ubuntu"),
        ] {
            assert_eq!(platform_for_release(release), expected, "{release}");
        }
    }

    #[test]
    fn windows_products_use_explicit_versions_without_guessing_kernel_numbers() {
        for (release, expected) in [
            ("Microsoft Windows 11 Pro", "windows-11"),
            ("Microsoft Windows 10 Enterprise LTSC", "windows-10"),
            ("Microsoft Windows 8.1 Pro", "windows-8-1"),
            ("Microsoft Windows 8", "windows-8"),
            ("Microsoft Windows 7 Professional", "windows-7"),
            ("Microsoft Windows Vista Home Premium", "windows-vista"),
            ("Microsoft Windows XP Professional", "windows-xp"),
            ("Windows 98 Second Edition", "windows-retro"),
            ("Microsoft Windows 2000 Professional", "windows-retro"),
            ("Windows NT 4.0", "windows-retro"),
            ("WINDOWS-11 Enterprise", "windows-11"),
            ("Windows Server 2025 Datacenter", "windows-server"),
            ("Microsoft Windows Server 2019 Standard", "windows-server"),
            ("Microsoft Windows Server 2012 R2", "windows-server"),
            ("Windows Server 2008 R2 Standard", "windows-server-2008-r2"),
            (
                "Microsoft Windows Server 2008 Standard",
                "windows-server-2008",
            ),
            ("Windows Server 2003 R2", "windows-server-2003"),
            (
                "Microsoft(R) Windows(R) Server 2003, Standard Edition",
                "windows-server-2003",
            ),
            ("Microsoft Windows [Version 10.0.26100.1]", "windows"),
            ("Microsoft Windows NT 10.0", "windows"),
            ("Windows 12", "windows"),
            ("Windows 110", "windows"),
            ("Linux 6.6.87.2-microsoft-windows-11-WSL2", "linux"),
            ("Ubuntu on Windows 11", "ubuntu"),
        ] {
            assert_eq!(platform_for_release(release), expected, "{release}");
        }
    }

    #[test]
    fn classic_windows_flag_decodes_without_fonts_or_a_native_window() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets/os-icons/windows/classic/simple-icons-windowsxp.svg");
        let image = slint::Image::load_from_path(&path).expect("bundled classic flag SVG");
        let pixels = image.to_rgba8().expect("classic flag raster");
        assert_eq!((pixels.width(), pixels.height()), (24, 24));
        assert!(pixels.as_slice().iter().any(|pixel| pixel.a > 0));
        assert!(pixels.as_slice().iter().any(|pixel| pixel.a == 0));
    }

    #[test]
    fn macos_release_icons_decode_with_color_and_transparency() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/os-icons/macos/releases");
        for icon in ax_ssh::config::system_icon_choices()
            .iter()
            .filter(|icon| icon.id.starts_with("macos-"))
        {
            let path = root.join(format!("{}.svg", icon.id));
            let image = slint::Image::load_from_path(&path).expect("bundled macOS release SVG");
            let pixels = image.to_rgba8().expect("macOS release icon raster");
            assert_eq!((pixels.width(), pixels.height()), (32, 32));
            assert!(pixels.as_slice().iter().any(|pixel| pixel.a == 0));
            assert!(
                pixels
                    .as_slice()
                    .iter()
                    .any(|pixel| { pixel.a == 255 && (pixel.r != pixel.g || pixel.g != pixel.b) })
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn local_macos_icon_matches_the_product_version() {
        let output = std::process::Command::new("/usr/bin/sw_vers")
            .arg("-productVersion")
            .output()
            .expect("read macOS product version");
        assert!(output.status.success());
        let version = std::str::from_utf8(&output.stdout).expect("UTF-8 product version");
        let expected = platform_for_release(&format!("macOS {}", version.trim()));
        assert_eq!(discover_macos_platform(), expected);
    }

    #[test]
    fn local_linux_uses_own_id_and_never_inherits_a_parent_icon() {
        for (release, expected) in [
            ("ID=ubuntu\nPRETTY_NAME=\"Ubuntu 24.04 LTS\"", "ubuntu"),
            ("ID=ol\nNAME=\"Oracle Linux Server\"", "oracle-linux"),
            ("ID=arch\n", "arch-linux"),
            ("ID=linuxmint\n", "linux-mint"),
            ("ID=opensuse-tumbleweed\n", "opensuse-tumbleweed"),
            (
                "ID=custom\nID_LIKE=ubuntu\nPRETTY_NAME=\"Ubuntu derivative\"",
                "linux",
            ),
            ("ID=amzn\nID_LIKE=fedora\n", "linux"),
            ("PRETTY_NAME=\"Debian GNU/Linux 12\"\n", "debian"),
            ("NAME=\"Unknown OS\"\n", "linux"),
            ("", "linux"),
        ] {
            assert_eq!(platform_for_os_release(release), expected, "{release}");
        }
    }

    #[test]
    fn local_release_reader_bounds_bytes_and_rejects_invalid_utf8() {
        let maximum = vec![b'x'; MAX_OS_RELEASE_BYTES as usize];
        assert_eq!(
            read_release(maximum.as_slice()).unwrap().len(),
            maximum.len()
        );
        assert!(read_release(&vec![b'x'; MAX_OS_RELEASE_BYTES as usize + 1][..]).is_err());
        assert!(read_release(&[0xff][..]).is_err());
    }
}
