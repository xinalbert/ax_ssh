//! Private checkpoints for ordinary SFTP transfers. No live handles are persisted.

use super::*;
use serde::{Deserialize, Serialize};

pub(super) const CHECKPOINT_INTERVAL: u64 = 8 * 1024 * 1024;
const RECORD_LIMIT: u64 = 32 * 1024;
const MAX_RECORDS: usize = SFTP_TRANSFER_QUEUE_WINDOW;

/// A task which can be offered after an authenticated connection is restored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SftpRecovery {
    pub transfer_id: Uuid,
    pub remote_path: String,
    pub local_path: PathBuf,
    pub total_bytes: u64,
    pub offset: u64,
    pub upload: bool,
    pub publication_uncertain: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(super) enum TransferDirection {
    Download,
    Upload,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(super) struct ResumeKey {
    pub(super) transfer_id: Uuid,
    pub(super) direction: TransferDirection,
    pub(super) remote_path: String,
    pub(super) local_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(super) struct ResumeRecord {
    version: u8,
    origin: String,
    pub(super) key: ResumeKey,
    pub(super) partial_id: Uuid,
    pub(super) offset: u64,
    pub(super) digest: [u8; 32],
    pub(super) size: u64,
    pub(super) remote_source: Option<RemoteFileMetadata>,
    pub(super) local_source: Option<LocalFileIdentity>,
    pub(super) local_partial: Option<PartialIdentity>,
    pub(super) publishing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(super) struct PartialIdentity {
    created: Option<SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

impl PartialIdentity {
    pub(super) fn of(metadata: &fs::Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Self {
            created: metadata.created().ok(),
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ResumeStore(Arc<ResumeStoreInner>);

#[derive(Debug)]
struct ResumeStoreInner {
    directory: PathBuf,
    origin: String,
    _lock: LocalFile,
}

impl PartialEq for ResumeStore {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for ResumeStore {}

impl ResumeStore {
    pub(crate) fn open(directory: PathBuf, origin: String) -> Result<Self> {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&directory)
            .context("cannot create SFTP recovery directory")?;
        anyhow::ensure!(
            fs::symlink_metadata(&directory)?.is_dir(),
            "SFTP recovery location is not a directory"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
                .context("cannot restrict SFTP recovery directory")?;
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("lock"))
            .context("cannot open SFTP recovery lock")?;
        lock.try_lock()
            .context("SFTP recovery is in use by another worker")?;
        Ok(Self(Arc::new(ResumeStoreInner {
            directory,
            origin,
            _lock: lock,
        })))
    }

    fn path(&self, id: Uuid) -> PathBuf {
        self.0.directory.join(format!("{id}.json"))
    }

    fn abandoned(&self, id: Uuid) -> bool {
        self.path(id).with_extension("cancelled").is_file()
    }

    fn records(&self) -> Result<impl Iterator<Item = Uuid>> {
        Ok(fs::read_dir(&self.0.directory)?.filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name();
            Uuid::parse_str(name.to_str()?.strip_suffix(".json")?).ok()
        }))
    }

    fn read(&self, id: Uuid) -> Result<Option<ResumeRecord>> {
        let path = self.path(id);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("cannot inspect SFTP checkpoint"),
        };
        anyhow::ensure!(metadata.is_file(), "SFTP checkpoint is not a regular file");
        let mut bytes = Vec::new();
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        options
            .open(path)?
            .take(RECORD_LIMIT + 1)
            .read_to_end(&mut bytes)?;
        anyhow::ensure!(
            bytes.len() as u64 <= RECORD_LIMIT,
            "SFTP checkpoint is too large"
        );
        let record: ResumeRecord =
            serde_json::from_slice(&bytes).context("invalid SFTP checkpoint")?;
        anyhow::ensure!(
            record.version == 1 && record.key.transfer_id == id && record.offset <= record.size,
            "invalid SFTP checkpoint identity or offset"
        );
        validate_remote_path(&record.key.remote_path)?;
        anyhow::ensure!(
            record.key.local_path.is_absolute(),
            "invalid SFTP checkpoint local path"
        );
        Ok(Some(record))
    }

    pub(super) fn load(&self, key: &ResumeKey) -> Result<Option<ResumeRecord>> {
        anyhow::ensure!(
            !self.abandoned(key.transfer_id),
            "SFTP transfer was cancelled"
        );
        let Some(record) = self.read(key.transfer_id)? else {
            return Ok(None);
        };
        anyhow::ensure!(
            record.origin == self.0.origin && &record.key == key,
            "SFTP checkpoint belongs to a different connection or destination"
        );
        anyhow::ensure!(
            !record.publishing,
            "SFTP publication was not confirmed; check the destination before retrying"
        );
        Ok(Some(record))
    }

    pub(super) fn create(&self, key: ResumeKey, size: u64) -> Result<Option<ResumeRecord>> {
        if self.records()?.take(MAX_RECORDS).count() == MAX_RECORDS {
            return Ok(None);
        }
        Ok(Some(ResumeRecord {
            version: 1,
            origin: self.0.origin.clone(),
            key,
            partial_id: Uuid::new_v4(),
            offset: 0,
            digest: Sha256::digest([]).into(),
            size,
            remote_source: None,
            local_source: None,
            local_partial: None,
            publishing: false,
        }))
    }

    pub(super) fn save(&self, record: &ResumeRecord) -> Result<()> {
        let bytes = serde_json::to_vec(record).context("cannot encode SFTP checkpoint")?;
        anyhow::ensure!(
            bytes.len() as u64 <= RECORD_LIMIT,
            "SFTP checkpoint is too large"
        );
        crate::config::write_private_file_atomically(&self.path(record.key.transfer_id), &bytes)
            .context("cannot record SFTP checkpoint")
    }

    pub(super) fn remove(&self, id: Uuid) -> Result<()> {
        match fs::remove_file(self.path(id)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context("cannot remove SFTP checkpoint"),
        }
    }

    pub(crate) fn recoveries(&self) -> Result<Vec<SftpRecovery>> {
        Ok(self
            .records()?
            .take(MAX_RECORDS)
            .filter(|id| !self.abandoned(*id))
            .filter_map(|id| match self.read(id) {
                Ok(Some(record))
                    if record.origin == self.0.origin
                        && (record.offset != 0
                            || record.local_partial.is_some()
                            || record.publishing) =>
                {
                    Some(SftpRecovery {
                        transfer_id: id,
                        remote_path: record.key.remote_path,
                        local_path: record.key.local_path,
                        total_bytes: record.size,
                        offset: record.offset,
                        upload: record.key.direction == TransferDirection::Upload,
                        publication_uncertain: record.publishing,
                    })
                }
                Ok(_) => None,
                Err(error) => {
                    debug!(transfer_id = %id, %error, "cannot recover SFTP checkpoint");
                    None
                }
            })
            .collect())
    }

    pub(crate) fn abandoned_ids(&self) -> Result<Vec<Uuid>> {
        Ok(self
            .records()?
            .take(MAX_RECORDS)
            .filter(|id| self.abandoned(*id))
            .collect())
    }

    pub(crate) fn discard(&self, id: Uuid) -> Result<Option<String>> {
        let Some(record) = self.read(id)? else {
            return Ok(None);
        };
        if record.origin != self.0.origin || record.publishing {
            return Ok(None);
        }
        let upload_path = match record.key.direction {
            TransferDirection::Download => {
                let name = record
                    .key
                    .local_path
                    .file_name()
                    .context("invalid SFTP local name")?
                    .to_string_lossy();
                let path = record
                    .key
                    .local_path
                    .with_file_name(format!(".{name}.axssh-{}.part", record.partial_id));
                if fs::symlink_metadata(&path).is_ok_and(|metadata| {
                    metadata.is_file()
                        && Some(PartialIdentity::of(&metadata)) == record.local_partial
                }) {
                    fs::remove_file(&path).context("cannot remove owned SFTP download data")?;
                }
                None
            }
            TransferDirection::Upload => {
                let (parent, name) = record
                    .key
                    .remote_path
                    .rsplit_once('/')
                    .context("invalid SFTP upload path")?;
                Some(remote_upload_partial_path(parent, name, record.partial_id))
            }
        };
        self.remove(id)?;
        let marker = self.path(id).with_extension("cancelled");
        if let Err(error) = fs::remove_file(marker)
            && error.kind() != io::ErrorKind::NotFound
        {
            debug!(transfer_id = %id, %error, "cannot clear SFTP cancellation marker");
        }
        Ok(upload_path)
    }
}

/// Cancel checkpoints for jobs whose SSH transport is currently unavailable.
pub fn abandon_sftp_recoveries(directory: &Path, ids: &[Uuid]) -> Result<()> {
    for id in ids {
        let path = directory.join(format!("{id}.json"));
        if path.is_file() {
            crate::config::write_private_file_atomically(&path.with_extension("cancelled"), b"")?;
        }
    }
    Ok(())
}

pub(super) fn remote_upload_partial_path(parent: &str, name: &str, id: Uuid) -> String {
    format!("{parent}/.{name}.axssh-upload-{id}")
}

pub(super) fn hash_local_prefix(
    file: &mut LocalFile,
    length: u64,
    cancellation: &TransferCancellation,
) -> Result<Sha256> {
    let mut hasher = Sha256::new();
    let mut remaining = length;
    let mut buffer = vec![0; DOWNLOAD_CHUNK_BYTES as usize];
    while remaining != 0 {
        if cancellation.is_cancelled() {
            return Err(cancelled_error());
        }
        let count = remaining.min(buffer.len() as u64) as usize;
        file.read_exact(&mut buffer[..count])
            .context("cannot read SFTP checkpoint prefix")?;
        hasher.update(&buffer[..count]);
        remaining -= count as u64;
    }
    Ok(hasher)
}

pub(super) async fn verify_remote_prefix(
    session: &RawSftpSession,
    handle: &str,
    record: &ResumeRecord,
    cancellation: &TransferCancellation,
    deadline: Instant,
) -> Result<()> {
    let mut hasher = Sha256::new();
    let mut next = 0;
    let mut requests = RequestWindow::new();
    while next < record.offset || requests.len() != 0 {
        cancellation.wait_until_running().await?;
        while next < record.offset && requests.len() < REQUEST_WINDOW {
            let count = (record.offset - next).min(u64::from(DOWNLOAD_CHUNK_BYTES)) as u32;
            requests
                .push(read_remote_chunk(
                    session,
                    handle,
                    next,
                    count,
                    record.offset,
                    cancellation,
                    deadline,
                ))
                .await?;
            next += u64::from(count);
        }
        hasher.update(
            requests
                .next()
                .await
                .context("SFTP checkpoint read ended unexpectedly")??,
        );
    }
    let digest: [u8; 32] = hasher.finalize().into();
    anyhow::ensure!(
        digest == record.digest,
        "SFTP partial data does not match the source; restart the transfer"
    );
    Ok(())
}

pub(super) async fn load_checkpoint(
    store: ResumeStore,
    key: ResumeKey,
    size: u64,
) -> Result<(Option<ResumeRecord>, bool)> {
    tokio::task::spawn_blocking(move || {
        if let Some(record) = store.load(&key)? {
            anyhow::ensure!(
                record.size == size,
                "SFTP source size changed; restart the transfer"
            );
            Ok((Some(record), true))
        } else {
            Ok((store.create(key, size)?, false))
        }
    })
    .await
    .context("SFTP checkpoint load task failed")?
}

pub(super) async fn save_checkpoint(store: &ResumeStore, record: &ResumeRecord) -> Result<()> {
    let (store, record) = (store.clone(), record.clone());
    tokio::task::spawn_blocking(move || store.save(&record))
        .await
        .context("SFTP checkpoint save task failed")?
}

pub(super) async fn remove_checkpoint(store: &ResumeStore, id: Uuid) {
    let store = store.clone();
    match tokio::task::spawn_blocking(move || store.remove(id)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => warn!(transfer_id = %id, %error, "cannot clear SFTP checkpoint"),
        Err(error) => warn!(transfer_id = %id, %error, "SFTP checkpoint removal task failed"),
    }
}
