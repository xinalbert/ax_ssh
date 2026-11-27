//! Local filesystem listener and cleanup for stream-local forwarding.

use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tokio::net::{UnixListener, UnixStream};

pub(super) struct Listener {
    listener: UnixListener,
    path: PathBuf,
    identity: (u64, u64),
}

impl Listener {
    pub(super) async fn bind(path: String) -> Result<Self> {
        tokio::task::spawn_blocking(move || {
            let path = Path::new(&path);
            let parent = path
                .parent()
                .context("Unix socket requires a parent directory")?;
            let name = path
                .file_name()
                .context("Unix socket requires a file name")?;
            let parent = parent
                .canonicalize()
                .context("cannot resolve Unix socket parent directory")?;
            let metadata = parent.metadata()?;
            // SAFETY: geteuid is a read-only query with no arguments or caller memory.
            let uid = unsafe { libc::geteuid() };
            anyhow::ensure!(
                metadata.is_dir() && metadata.uid() == uid && metadata.mode() & 0o077 == 0,
                "choose a private Unix socket directory owned by the current user"
            );
            let path = parent.join(name);
            let listener =
                UnixListener::bind(&path).context("cannot bind the local Unix socket")?;
            let metadata = path
                .symlink_metadata()
                .context("cannot inspect the new Unix socket")?;
            anyhow::ensure!(
                metadata.file_type().is_socket(),
                "local Unix socket was replaced during setup"
            );
            Ok(Self {
                listener,
                path,
                identity: (metadata.dev(), metadata.ino()),
            })
        })
        .await
        .context("Unix socket listener task failed")?
    }

    pub(super) async fn accept(&self) -> Result<UnixStream> {
        let (stream, _) = self.listener.accept().await?;
        Ok(stream)
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let owned = self.path.symlink_metadata().is_ok_and(|metadata| {
            metadata.file_type().is_socket() && (metadata.dev(), metadata.ino()) == self.identity
        });
        if owned && let Err(error) = std::fs::remove_file(&self.path) {
            tracing::warn!(%error, "cannot remove local SSH forwarding socket");
        }
    }
}
