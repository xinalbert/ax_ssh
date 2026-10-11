fn default_edit_limit() -> u64 {
    crate::config::SftpTransferSettings::default().remote_edit_max_file_bytes()
}

use super::*;
use russh_sftp::protocol::{Attrs, Data, Handle, Status};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const PATH: &str = "/srv/file.bin";

#[derive(Clone)]
struct RemoteFile {
    data: Vec<u8>,
    permissions: u32,
}

struct Server {
    files: Arc<Mutex<HashMap<String, RemoteFile>>>,
    atomic: bool,
    fail_write: bool,
    change_before_publish: bool,
    change_after_read: bool,
    truncate: bool,
    lstats: usize,
    fstats: usize,
}

fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: String::new(),
        language_tag: String::new(),
    }
}

impl russh_sftp::server::Handler for Server {
    type Error = StatusCode;
    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn init(
        &mut self,
        _: u32,
        _: HashMap<String, String>,
    ) -> std::result::Result<Version, Self::Error> {
        let mut version = Version::new();
        if self.atomic {
            version.extensions.insert(POSIX_RENAME.into(), "1".into());
        }
        Ok(version)
    }

    async fn lstat(&mut self, id: u32, path: String) -> std::result::Result<Attrs, Self::Error> {
        let files = self.files.lock().map_err(|_| StatusCode::Failure)?;
        let file = files.get(&path).ok_or(StatusCode::NoSuchFile)?;
        if path == PATH {
            self.lstats += 1;
        }
        Ok(Attrs {
            id,
            attrs: FileAttributes {
                size: Some(file.data.len() as u64),
                mtime: Some(if self.change_before_publish && self.lstats >= 2 {
                    124
                } else {
                    123
                }),
                permissions: Some(file.permissions),
                ..FileAttributes::empty()
            },
        })
    }

    async fn fstat(&mut self, id: u32, handle: String) -> std::result::Result<Attrs, Self::Error> {
        self.fstats += 1;
        let file = self
            .files
            .lock()
            .map_err(|_| StatusCode::Failure)?
            .get(&handle)
            .cloned()
            .ok_or(StatusCode::NoSuchFile)?;
        Ok(Attrs {
            id,
            attrs: FileAttributes {
                size: Some(file.data.len() as u64),
                permissions: Some(file.permissions),
                mtime: Some(if self.change_after_read && self.fstats >= 2 {
                    124
                } else {
                    123
                }),
                ..FileAttributes::empty()
            },
        })
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        flags: OpenFlags,
        attrs: FileAttributes,
    ) -> std::result::Result<Handle, Self::Error> {
        let mut files = self.files.lock().map_err(|_| StatusCode::Failure)?;
        if flags.bits() == OpenFlags::READ.bits() {
            if !files.contains_key(&filename) {
                return Err(StatusCode::NoSuchFile);
            }
        } else {
            assert!(flags.contains(OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE));
            assert!(filename.starts_with("/srv/.axssh-edit-"));
            assert_eq!(attrs.permissions, Some(0o600));
            if files.contains_key(&filename) {
                return Err(StatusCode::Failure);
            }
            files.insert(
                filename.clone(),
                RemoteFile {
                    data: Vec::new(),
                    permissions: 0o100600,
                },
            );
        }
        Ok(Handle {
            id,
            handle: filename,
        })
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> std::result::Result<Data, Self::Error> {
        let files = self.files.lock().map_err(|_| StatusCode::Failure)?;
        let file = files.get(&handle).ok_or(StatusCode::NoSuchFile)?;
        let end = if self.truncate {
            file.data.len() / 2
        } else {
            file.data.len()
        };
        let start = offset as usize;
        if start >= end {
            return Err(StatusCode::Eof);
        }
        // Deliberately short reads exercise offset completion.
        Ok(Data {
            id,
            data: file.data[start..end.min(start + (len as usize).min(30000))].to_vec(),
        })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> std::result::Result<Status, Self::Error> {
        if self.fail_write {
            return Err(StatusCode::PermissionDenied);
        }
        let mut files = self.files.lock().map_err(|_| StatusCode::Failure)?;
        let file = files.get_mut(&handle).ok_or(StatusCode::NoSuchFile)?;
        let start = offset as usize;
        file.data.resize(file.data.len().max(start + data.len()), 0);
        file.data[start..start + data.len()].copy_from_slice(&data);
        Ok(ok(id))
    }

    async fn fsetstat(
        &mut self,
        id: u32,
        handle: String,
        attrs: FileAttributes,
    ) -> std::result::Result<Status, Self::Error> {
        self.files
            .lock()
            .map_err(|_| StatusCode::Failure)?
            .get_mut(&handle)
            .ok_or(StatusCode::NoSuchFile)?
            .permissions = 0o100000 | attrs.permissions.ok_or(StatusCode::BadMessage)?;
        Ok(ok(id))
    }

    async fn close(&mut self, id: u32, _: String) -> std::result::Result<Status, Self::Error> {
        Ok(ok(id))
    }

    async fn remove(
        &mut self,
        id: u32,
        filename: String,
    ) -> std::result::Result<Status, Self::Error> {
        self.files
            .lock()
            .map_err(|_| StatusCode::Failure)?
            .remove(&filename);
        Ok(ok(id))
    }

    async fn extended(
        &mut self,
        id: u32,
        request: String,
        _: Vec<u8>,
    ) -> std::result::Result<Packet, Self::Error> {
        assert_eq!(request, POSIX_RENAME);
        let mut files = self.files.lock().map_err(|_| StatusCode::Failure)?;
        let temporary = files
            .keys()
            .find(|path| path.starts_with("/srv/.axssh-edit-"))
            .cloned()
            .ok_or(StatusCode::NoSuchFile)?;
        let file = files.remove(&temporary).ok_or(StatusCode::NoSuchFile)?;
        assert_eq!(file.permissions, files[PATH].permissions & 0o100777);
        files.insert(PATH.into(), file);
        Ok(Packet::Status(ok(id)))
    }
}

fn fixture(data: &[u8]) -> Server {
    Server {
        files: Arc::new(Mutex::new(HashMap::from([(
            PATH.into(),
            RemoteFile {
                data: data.to_vec(),
                permissions: 0o100640,
            },
        )]))),
        atomic: true,
        fail_write: false,
        change_before_publish: false,
        change_after_read: false,
        truncate: false,
        lstats: 0,
        fstats: 0,
    }
}

async fn execute(server: Server, operation: SftpWriteOperation) -> Result<SftpWriteResult> {
    let (client, stream) = tokio::io::duplex(128 * 1024);
    russh_sftp::server::run(stream, server).await;
    timeout(
        Duration::from_secs(5),
        execute_sftp_write(client, operation),
    )
    .await
    .context("test deadline")?
}

fn save(data: &[u8], size: u64, modified: u32) -> SftpWriteOperation {
    SftpWriteOperation::WriteEditFile {
        path: PATH.into(),
        max_bytes: default_edit_limit(),
        data: data.to_vec(),
        expected_size: size,
        expected_modified: Some(modified),
    }
}

#[tokio::test]
async fn external_edit_download_preserves_binary_empty_and_short_reads() {
    for data in [
        Vec::new(),
        vec![0, 255, 128, b'\r', b'\n'],
        vec![171; 1024 * 1024 + 7],
    ] {
        let result = execute(
            fixture(&data),
            SftpWriteOperation::ReadEditFile {
                path: PATH.into(),
                max_bytes: default_edit_limit(),
            },
        )
        .await
        .unwrap();
        assert!(
            matches!(result, SftpWriteResult::EditFile { data: actual, expected_size, .. } if actual == data && expected_size == data.len() as u64)
        );
    }
}

#[tokio::test]
async fn external_edit_download_rejects_truncation_and_final_changes() {
    for change in [false, true] {
        let mut server = fixture(b"original");
        server.truncate = !change;
        server.change_after_read = change;
        assert!(
            execute(
                server,
                SftpWriteOperation::ReadEditFile {
                    path: PATH.into(),
                    max_bytes: default_edit_limit()
                }
            )
            .await
            .is_err()
        );
    }
}

#[tokio::test]
async fn external_edit_atomic_upload_preserves_bytes_empty_files_and_permissions() {
    for data in [Vec::new(), vec![0, 255, 128], vec![171; 1024 * 1024 + 7]] {
        let server = fixture(b"original");
        let files = server.files.clone();
        let result = execute(server, save(&data, 8, 123)).await.unwrap();
        assert!(
            matches!(result, SftpWriteResult::Updated { size, .. } if size == data.len() as u64)
        );
        let files = files.lock().unwrap();
        assert_eq!(files[PATH].data, data);
        assert_eq!(files[PATH].permissions, 0o100640);
        assert_eq!(files.len(), 1);
    }
}

#[tokio::test]
async fn external_edit_conflict_before_and_after_upload_preserves_original() {
    for during_upload in [false, true] {
        let mut server = fixture(b"original");
        server.change_before_publish = during_upload;
        let files = server.files.clone();
        assert!(
            execute(
                server,
                save(b"new bytes", 8, if during_upload { 123 } else { 122 })
            )
            .await
            .is_err()
        );
        let files = files.lock().unwrap();
        assert_eq!(files[PATH].data, b"original");
        assert_eq!(files.len(), 1);
    }
}

#[tokio::test]
async fn external_edit_failed_write_cleans_temporary_and_preserves_original() {
    let mut server = fixture(b"original");
    server.fail_write = true;
    let files = server.files.clone();
    assert!(execute(server, save(b"new bytes", 8, 123)).await.is_err());
    let files = files.lock().unwrap();
    assert_eq!(files[PATH].data, b"original");
    assert_eq!(files.len(), 1);
}

#[tokio::test]
async fn external_edit_requires_atomic_overwrite_and_rejects_links_and_oversize() {
    let mut server = fixture(b"original");
    server.atomic = false;
    let files = server.files.clone();
    assert!(execute(server, save(b"new bytes", 8, 123)).await.is_err());
    assert_eq!(files.lock().unwrap().len(), 1);
    let server = fixture(b"original");
    server
        .files
        .lock()
        .unwrap()
        .get_mut(PATH)
        .unwrap()
        .permissions = 0o120777;
    assert!(
        execute(
            server,
            SftpWriteOperation::ReadEditFile {
                path: PATH.into(),
                max_bytes: default_edit_limit()
            }
        )
        .await
        .is_err()
    );
    let server = fixture(&vec![0; default_edit_limit() as usize + 1]);
    assert!(
        execute(
            server,
            SftpWriteOperation::ReadEditFile {
                path: PATH.into(),
                max_bytes: default_edit_limit()
            }
        )
        .await
        .is_err()
    );
    assert!(
        execute(
            fixture(b"original"),
            save(&vec![0; default_edit_limit() as usize + 1], 8, 123)
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn external_edit_configured_limit_allows_larger_files_and_rejects_excess() {
    let data = vec![171; default_edit_limit() as usize + 1];
    assert!(
        execute(
            fixture(&data),
            SftpWriteOperation::ReadEditFile {
                path: PATH.into(),
                max_bytes: default_edit_limit(),
            }
        )
        .await
        .is_err()
    );
    let raised = 8 * 1024 * 1024;
    let downloaded = execute(
        fixture(&data),
        SftpWriteOperation::ReadEditFile {
            path: PATH.into(),
            max_bytes: raised,
        },
    )
    .await
    .unwrap();
    assert!(matches!(downloaded, SftpWriteResult::EditFile { data: actual, .. } if actual == data));
    let server = fixture(b"original");
    let files = server.files.clone();
    let mut operation = save(&data, 8, 123);
    if let SftpWriteOperation::WriteEditFile { max_bytes, .. } = &mut operation {
        *max_bytes = raised;
    }
    execute(server, operation).await.unwrap();
    assert_eq!(files.lock().unwrap().get(PATH).unwrap().data, data);
    // A lower setting limits the new payload; an already-downloaded larger
    // original can still be replaced by a smaller, valid saved file.
    execute(
        fixture(&data),
        SftpWriteOperation::WriteEditFile {
            path: PATH.into(),
            data: b"small".to_vec(),
            max_bytes: default_edit_limit(),
            expected_size: data.len() as u64,
            expected_modified: Some(123),
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn external_edit_rejects_invalid_operation_limits_before_file_access() {
    for limit in [0, u64::MAX] {
        let error = execute(
            fixture(b"original"),
            SftpWriteOperation::ReadEditFile {
                path: PATH.into(),
                max_bytes: limit,
            },
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("supported 1–64 MiB"));
        let error = execute(
            fixture(b"original"),
            SftpWriteOperation::WriteEditFile {
                path: PATH.into(),
                data: b"new".to_vec(),
                max_bytes: limit,
                expected_size: 8,
                expected_modified: Some(123),
            },
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("supported 1–64 MiB"));
    }
}
