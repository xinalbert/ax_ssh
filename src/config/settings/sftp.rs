//! Persistent sftp settings and normalization.

use super::*;

const MAX_SFTP_TRANSFER_FILTER_PATTERNS: usize = 64;
const MAX_SFTP_TRANSFER_FILTER_PATTERN_CHARS: usize = 256;
const MAX_SFTP_TRANSFER_FILTER_TEXT_CHARS: usize = 16 * 1024;

/// Controls which file names are omitted from SFTP uploads and downloads.
///
/// Patterns are matched against a single file or directory name. `*` matches
/// any sequence of characters; path separators are deliberately not accepted.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SftpTransferFilterSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub use_platform_defaults: bool,
    #[serde(default)]
    pub custom_patterns: Vec<String>,
}

impl SftpTransferFilterSettings {
    pub fn normalized(enabled: bool, use_platform_defaults: bool, custom_text: &str) -> Self {
        Self {
            enabled,
            use_platform_defaults,
            custom_patterns: normalize_sftp_transfer_filter_patterns(custom_text),
        }
    }

    pub fn effective_patterns(&self) -> Vec<String> {
        if !self.enabled {
            return Vec::new();
        }
        let mut patterns = if self.use_platform_defaults {
            platform_default_sftp_transfer_filter_patterns()
        } else {
            Vec::new()
        };
        for pattern in &self.custom_patterns {
            if !patterns.iter().any(|existing| existing == pattern) {
                patterns.push(pattern.clone());
            }
        }
        patterns
    }

    pub fn platform_default_patterns_text() -> String {
        platform_default_sftp_transfer_filter_patterns().join("\n")
    }

    pub fn reset_to_platform_defaults() -> Self {
        Self::default()
    }

    pub(super) fn normalize_in_place(&mut self) {
        *self = Self {
            enabled: self.enabled,
            use_platform_defaults: self.use_platform_defaults,
            custom_patterns: normalize_sftp_transfer_filter_patterns(
                &self.custom_patterns.join("\n"),
            ),
        };
    }
}

impl Default for SftpTransferFilterSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            use_platform_defaults: true,
            custom_patterns: Vec::new(),
        }
    }
}

fn normalize_sftp_transfer_filter_patterns(text: &str) -> Vec<String> {
    if text.chars().count() > MAX_SFTP_TRANSFER_FILTER_TEXT_CHARS {
        return Vec::new();
    }
    let mut patterns = Vec::new();
    for raw in text.lines() {
        let pattern = raw.trim();
        if pattern.is_empty()
            || pattern.chars().count() > MAX_SFTP_TRANSFER_FILTER_PATTERN_CHARS
            || pattern.chars().any(char::is_control)
            || pattern.contains(['/', '\\'])
            || patterns.iter().any(|existing| existing == pattern)
        {
            continue;
        }
        if patterns.len() >= MAX_SFTP_TRANSFER_FILTER_PATTERNS {
            break;
        }
        patterns.push(pattern.to_owned());
    }
    patterns
}

fn platform_default_sftp_transfer_filter_patterns() -> Vec<String> {
    #[cfg(target_os = "macos")]
    let patterns = [
        ".DS_Store",
        "._*",
        ".Spotlight-V100",
        ".Trashes",
        ".fseventsd",
    ];
    #[cfg(target_os = "windows")]
    let patterns = ["Thumbs.db", "desktop.ini", "$RECYCLE.BIN"];
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let patterns = [".directory", ".Trash-*", ".thumbnails"];
    patterns.into_iter().map(str::to_owned).collect()
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SftpTransferSettings {
    #[serde(default = "default_sftp_per_tab_transfers")]
    pub per_tab_transfers: u8,
    #[serde(default = "default_sftp_global_uploads")]
    pub global_uploads: u8,
    /// Zero means no client-side limit on one uploaded or downloaded file.
    #[serde(default = "default_sftp_max_file_gib")]
    pub max_file_gib: u32,
    /// Zero means no client-side bandwidth cap.
    #[serde(default)]
    pub rate_limit_mib_per_second: u32,
    /// Maximum size copied when opening a local file from an SFTP tab.
    /// Zero disables both the per-file cap and the private cache byte quota.
    #[serde(default = "default_sftp_local_open_max_file_mib")]
    pub local_open_max_file_mib: u32,
    /// Maximum bytes buffered for a remote external edit, configured in MiB.
    #[serde(default = "default_sftp_remote_edit_max_file_mib")]
    pub remote_edit_max_file_mib: u32,
}

impl SftpTransferSettings {
    pub fn normalized(
        per_tab_transfers: i32,
        global_uploads: i32,
        max_file_gib: i32,
        rate_limit_mib_per_second: i32,
        local_open_max_file_mib: i32,
        remote_edit_max_file_mib: i32,
    ) -> Self {
        Self {
            per_tab_transfers: per_tab_transfers.clamp(
                i32::from(MIN_SFTP_PER_TAB_TRANSFERS),
                i32::from(MAX_SFTP_PER_TAB_TRANSFERS),
            ) as u8,
            global_uploads: global_uploads.clamp(
                i32::from(MIN_SFTP_GLOBAL_UPLOADS),
                i32::from(MAX_SFTP_GLOBAL_UPLOADS),
            ) as u8,
            max_file_gib: normalize_sftp_max_file_gib(max_file_gib),
            rate_limit_mib_per_second: rate_limit_mib_per_second
                .clamp(0, MAX_SFTP_RATE_LIMIT_MIB_PER_SECOND as i32)
                as u32,
            local_open_max_file_mib: normalize_sftp_local_open_max_file_mib(
                local_open_max_file_mib,
            ),
            remote_edit_max_file_mib: remote_edit_max_file_mib
                .clamp(1, MAX_SFTP_REMOTE_EDIT_MAX_FILE_MIB as i32)
                as u32,
        }
    }

    pub fn remote_edit_max_file_bytes(&self) -> u64 {
        u64::from(
            self.remote_edit_max_file_mib
                .clamp(1, MAX_SFTP_REMOTE_EDIT_MAX_FILE_MIB),
        ) * 1024
            * 1024
    }

    pub fn local_open_max_file_bytes(&self) -> u64 {
        if self.local_open_max_file_mib == 0 {
            u64::MAX
        } else {
            u64::from(self.local_open_max_file_mib) * 1024 * 1024
        }
    }

    pub fn policy(&self, override_policy: Option<SftpTransferPolicy>) -> SftpTransferPolicy {
        override_policy.unwrap_or(SftpTransferPolicy {
            per_tab_transfers: self.per_tab_transfers,
            max_file_gib: self.max_file_gib,
            rate_limit_mib_per_second: self.rate_limit_mib_per_second,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SftpTransferPolicy {
    pub per_tab_transfers: u8,
    /// Zero means no client-side limit on one uploaded or downloaded file.
    pub max_file_gib: u32,
    /// Zero means no client-side bandwidth cap for this SFTP connection.
    pub rate_limit_mib_per_second: u32,
}

impl SftpTransferPolicy {
    pub fn normalized(per_tab_transfers: i32, max_file_gib: i32, rate_mib_per_second: i32) -> Self {
        Self {
            per_tab_transfers: per_tab_transfers.clamp(
                i32::from(MIN_SFTP_PER_TAB_TRANSFERS),
                i32::from(MAX_SFTP_PER_TAB_TRANSFERS),
            ) as u8,
            max_file_gib: normalize_sftp_max_file_gib(max_file_gib),
            rate_limit_mib_per_second: rate_mib_per_second
                .clamp(0, MAX_SFTP_RATE_LIMIT_MIB_PER_SECOND as i32)
                as u32,
        }
    }

    pub fn validate(self) -> Result<()> {
        if !(MIN_SFTP_PER_TAB_TRANSFERS..=MAX_SFTP_PER_TAB_TRANSFERS)
            .contains(&self.per_tab_transfers)
            || self.max_file_gib > MAX_SFTP_MAX_FILE_GIB
            || self.rate_limit_mib_per_second > MAX_SFTP_RATE_LIMIT_MIB_PER_SECOND
        {
            anyhow::bail!("SFTP transfer policy is outside supported limits");
        }
        Ok(())
    }

    pub fn max_file_bytes(self) -> u64 {
        if self.max_file_gib == 0 {
            u64::MAX
        } else {
            u64::from(self.max_file_gib) * 1024 * 1024 * 1024
        }
    }

    pub fn rate_bytes_per_second(self) -> Option<u64> {
        (self.rate_limit_mib_per_second != 0)
            .then_some(u64::from(self.rate_limit_mib_per_second) * 1024 * 1024)
    }
}

impl Default for SftpTransferSettings {
    fn default() -> Self {
        Self {
            per_tab_transfers: default_sftp_per_tab_transfers(),
            global_uploads: default_sftp_global_uploads(),
            max_file_gib: default_sftp_max_file_gib(),
            rate_limit_mib_per_second: 0,
            local_open_max_file_mib: default_sftp_local_open_max_file_mib(),
            remote_edit_max_file_mib: default_sftp_remote_edit_max_file_mib(),
        }
    }
}

fn default_sftp_per_tab_transfers() -> u8 {
    DEFAULT_SFTP_PER_TAB_TRANSFERS
}

fn default_sftp_global_uploads() -> u8 {
    DEFAULT_SFTP_GLOBAL_UPLOADS
}

fn default_sftp_max_file_gib() -> u32 {
    DEFAULT_SFTP_MAX_FILE_GIB
}

fn default_sftp_local_open_max_file_mib() -> u32 {
    DEFAULT_SFTP_LOCAL_OPEN_MAX_FILE_MIB
}

fn default_sftp_remote_edit_max_file_mib() -> u32 {
    DEFAULT_SFTP_REMOTE_EDIT_MAX_FILE_MIB
}

fn normalize_sftp_max_file_gib(value: i32) -> u32 {
    if value == 0 {
        0
    } else {
        value.clamp(1, MAX_SFTP_MAX_FILE_GIB as i32) as u32
    }
}

fn normalize_sftp_local_open_max_file_mib(value: i32) -> u32 {
    if value == 0 {
        0
    } else {
        value.clamp(1, MAX_SFTP_LOCAL_OPEN_MAX_FILE_MIB as i32) as u32
    }
}

#[cfg(test)]
mod sftp_transfer_filter_tests {
    use super::*;

    #[test]
    fn custom_patterns_are_trimmed_deduplicated_and_bounded() {
        let settings = SftpTransferFilterSettings::normalized(
            true,
            false,
            "  .DS_Store  \n*.tmp\n.DS_Store\nfolder/name\n",
        );
        assert_eq!(settings.custom_patterns, [".DS_Store", "*.tmp"]);
        assert_eq!(settings.effective_patterns(), [".DS_Store", "*.tmp"]);
    }

    #[test]
    fn reset_uses_current_platform_defaults() {
        let settings = SftpTransferFilterSettings::reset_to_platform_defaults();
        assert!(settings.enabled);
        assert!(settings.use_platform_defaults);
        assert!(settings.custom_patterns.is_empty());
        assert!(!settings.effective_patterns().is_empty());
    }
}
