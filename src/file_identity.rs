//! Platform-observable file identity shared by validated local I/O.
use anyhow::{Context, Result};
use std::fs::File;
use std::time::SystemTime;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalFileFingerprint {
    length: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(windows)]
    volume_serial: u32,
    #[cfg(windows)]
    file_index: u64,
}

pub fn local_file_fingerprint(file: &File) -> Result<LocalFileFingerprint> {
    let metadata = file
        .metadata()
        .context("cannot inspect local file metadata")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(LocalFileFingerprint {
            length: metadata.len(),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle as _;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };

        let mut information = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: `file` owns a valid Windows file handle for this call, and
        // `information` is initialized writable storage of the documented type.
        let succeeded =
            unsafe { GetFileInformationByHandle(file.as_raw_handle(), &raw mut information) != 0 };
        if !succeeded {
            return Err(std::io::Error::last_os_error())
                .context("cannot inspect Windows local file identity");
        }
        Ok(LocalFileFingerprint {
            length: metadata.len(),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
            volume_serial: information.dwVolumeSerialNumber,
            file_index: (u64::from(information.nFileIndexHigh) << 32)
                | u64::from(information.nFileIndexLow),
        })
    }
    #[cfg(not(any(unix, windows)))]
    {
        Ok(LocalFileFingerprint {
            length: metadata.len(),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
        })
    }
}
