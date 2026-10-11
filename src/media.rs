//! Bounded PNG attachments and private, session-owned temporary media files.

use std::fs::{DirBuilder, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use image::ImageEncoder;

pub const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_IMAGE_SIDE: usize = 4096;
pub const MAX_ATTACHMENTS: usize = 32;

/// RGBA pixels copied from the platform clipboard, encoded off the UI thread.
pub fn encode_clipboard_png(width: usize, height: usize, rgba: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        width > 0 && height > 0 && width <= MAX_IMAGE_SIDE && height <= MAX_IMAGE_SIDE,
        "clipboard image must be at most 4096 × 4096 pixels"
    );
    ensure!(
        rgba.len() == width * height * 4,
        "invalid clipboard image pixels"
    );
    let mut output = LimitedPng(Vec::new());
    image::codecs::png::PngEncoder::new(&mut output)
        .write_image(
            rgba,
            width as u32,
            height as u32,
            image::ExtendedColorType::Rgba8,
        )
        .context("cannot encode clipboard image (maximum PNG size: 16 MiB)")?;
    Ok(output.0)
}

struct LimitedPng(Vec<u8>);
impl Write for LimitedPng {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if data.len() > MAX_IMAGE_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other("PNG size limit exceeded"));
        }
        self.0.extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Deletes only the uniquely created file and directory owned by this value.
/// Filesystem operations belong on a worker thread, including destruction.
pub struct PrivateMediaFile {
    directory: PathBuf,
    path: PathBuf,
}

impl PrivateMediaFile {
    pub fn create(extension: &str) -> Result<(Self, File)> {
        ensure!(
            matches!(extension, "png" | "wav"),
            "unsupported media extension"
        );
        let directory = std::env::temp_dir().join(format!("axssh-media-{}", uuid::Uuid::new_v4()));
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = DirBuilder::new();
            builder.mode(0o700);
            builder
        };
        #[cfg(not(unix))]
        let builder = DirBuilder::new();
        builder
            .create(&directory)
            .context("cannot create private media directory")?;
        let owner = Self {
            path: directory.join(format!("input.{extension}")),
            directory,
        };
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&owner.path)
            .context("cannot create private media file")?;
        Ok((owner, file))
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn output_path(&self) -> PathBuf {
        self.directory.join("transcript")
    }
}

impl Drop for PrivateMediaFile {
    fn drop(&mut self) {
        // whisper-cli may leave its text result here. Never traverse a tree or
        // remove a path that was supplied by a user or a remote application.
        for path in [&self.path, &self.directory.join("transcript.txt")] {
            if let Err(error) = std::fs::remove_file(path)
                && error.kind() != io::ErrorKind::NotFound
            {
                tracing::warn!(%error, "cannot remove private media file");
            }
        }
        if let Err(error) = std::fs::remove_dir(&self.directory) {
            tracing::warn!(%error, "cannot remove private media directory");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn png_pixels_and_limits() {
        let png = encode_clipboard_png(1, 1, &[255, 0, 0, 255]).unwrap();
        assert_eq!(image::load_from_memory(&png).unwrap().width(), 1);
        assert!(encode_clipboard_png(0, 1, &[]).is_err());
        assert!(encode_clipboard_png(4097, 1, &[]).is_err());
        assert!(encode_clipboard_png(1, 1, &[]).is_err());
        let mut sink = LimitedPng(vec![0; MAX_IMAGE_BYTES]);
        assert!(sink.write_all(&[1]).is_err());
    }
    #[test]
    fn private_file_permissions_and_cleanup() {
        let (owner, file) = PrivateMediaFile::create("png").unwrap();
        let path = owner.path().to_owned();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
            assert_eq!(
                std::fs::metadata(&owner.directory)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
        drop(file);
        drop(owner);
        assert!(!path.exists());
    }
}
