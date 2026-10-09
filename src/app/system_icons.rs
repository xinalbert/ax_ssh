//! Pure platform/icon identity mapping and bounded local Linux discovery.

#[cfg(target_os = "linux")]
use std::fs::File;
#[cfg(any(target_os = "linux", test))]
use std::io::Read;
#[cfg(target_os = "linux")]
use std::sync::OnceLock;

#[cfg(any(target_os = "linux", test))]
const MAX_OS_RELEASE_BYTES: u64 = 8192;
#[cfg(target_os = "linux")]
static LOCAL_PLATFORM: OnceLock<&'static str> = OnceLock::new();

mod catalog;

use catalog::{LINUX_DISTRIBUTIONS, PLATFORM_FAMILIES};

pub(super) fn local_platform() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        LOCAL_PLATFORM.get().copied().unwrap_or("linux")
    }
    #[cfg(not(target_os = "linux"))]
    {
        match std::env::consts::OS {
            "windows" => "windows",
            "macos" => "macos",
            _ => "",
        }
    }
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
    let release = format!(" {} ", normalized(release));
    let contains = |alias: &str| release.contains(&format!(" {alias} "));
    if let Some((platform, _)) = LINUX_DISTRIBUTIONS
        .iter()
        .find(|(_, aliases)| aliases.iter().any(|alias| contains(alias)))
    {
        return platform;
    }
    // A Linux kernel label can contain "microsoft" under WSL.
    PLATFORM_FAMILIES
        .iter()
        .filter(|(key, _)| *key == "linux")
        .chain(PLATFORM_FAMILIES.iter().filter(|(key, _)| *key != "linux"))
        .find(|(_, aliases)| aliases.iter().any(|alias| contains(alias)))
        .map(|(platform, _)| *platform)
        .unwrap_or("")
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
            ("Microsoft Windows Server 2022", "windows"),
            ("macOS 15.5", "macos"),
        ] {
            assert_eq!(platform_for_release(release), expected, "{release}");
        }
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
