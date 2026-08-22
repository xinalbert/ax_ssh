use super::*;

pub(super) fn sftp_entry_rows(
    entries: Vec<SftpEntry>,
    selected: &std::collections::HashSet<String>,
) -> Vec<SftpEntryRow> {
    entries
        .into_iter()
        .map(|entry| {
            let hidden = entry.name.starts_with('.');
            let is_selected = selected.contains(&entry.path);
            let icon_key = FileIconKey::for_entry(&entry.name, entry.is_dir, entry.is_symlink);
            SftpEntryRow {
                icon: slint_icon(&icon_key),
                has_icon: true,
                name: entry.name.into(),
                path: entry.path.into(),
                kind: if entry.is_dir {
                    "folder"
                } else if entry.is_symlink {
                    "link"
                } else {
                    "file"
                }
                .into(),
                size: format_file_size(entry.size, entry.is_dir).into(),
                modified: entry
                    .modified
                    .map(format_timestamp)
                    .unwrap_or_default()
                    .into(),
                hidden,
                selected: is_selected,
            }
        })
        .collect()
}

pub(super) fn local_entry_rows(
    entries: Vec<LocalDirectoryEntry>,
    selected: &std::collections::HashSet<String>,
) -> Vec<SftpEntryRow> {
    entries
        .into_iter()
        .map(|entry| {
            let hidden = entry.name.starts_with('.');
            let is_selected = selected.contains(&entry.path);
            let icon_key = FileIconKey::for_entry(&entry.name, entry.is_dir, entry.is_symlink);
            SftpEntryRow {
                icon: slint_icon(&icon_key),
                has_icon: true,
                name: entry.name.into(),
                path: entry.path.into(),
                kind: if entry.is_dir {
                    "folder"
                } else if entry.is_symlink {
                    "link"
                } else {
                    "file"
                }
                .into(),
                size: format_file_size(entry.size, entry.is_dir).into(),
                modified: entry
                    .modified
                    .map(format_local_timestamp)
                    .unwrap_or_default()
                    .into(),
                hidden,
                selected: is_selected,
            }
        })
        .collect()
}

pub(super) fn slint_icon(key: &FileIconKey) -> slint::Image {
    let icon = global_provider().cached_icon(key);
    let pixels = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        icon.rgba(),
        icon.width(),
        icon.height(),
    );
    slint::Image::from_rgba8(pixels)
}

pub(in crate::app) fn prewarm_file_icons(
    runtime: &tokio::runtime::Handle,
    keys: Vec<FileIconKey>,
    ui: &slint::Weak<AppWindow>,
    state: &std::sync::Arc<std::sync::Mutex<AppState>>,
) {
    if keys.is_empty() {
        return;
    }
    let coordinator = icon_prewarm_coordinator();
    let should_start = {
        let Ok(mut pending) = coordinator.lock() else {
            return;
        };
        for key in keys {
            if pending.queued.contains(&key) {
                continue;
            }
            if pending.keys.len() >= ICON_PREWARM_PENDING_KEY_LIMIT {
                break;
            }
            if pending.queued.insert(key.clone()) {
                pending.keys.push_back(key);
            }
        }
        pending.target = Some(IconPrewarmTarget {
            ui: ui.clone(),
            state: Arc::downgrade(state),
        });
        if pending.running {
            false
        } else {
            pending.running = true;
            true
        }
    };
    if should_start {
        let runtime = runtime.clone();
        drop(runtime.clone().spawn(run_icon_prewarm_worker(
            runtime,
            coordinator,
            ICON_PREWARM_BATCH_KEY_LIMIT,
        )));
    }
}

struct IconPrewarmTarget {
    ui: slint::Weak<AppWindow>,
    state: std::sync::Weak<Mutex<AppState>>,
}

struct IconPrewarmCoordinator {
    keys: VecDeque<FileIconKey>,
    queued: HashSet<FileIconKey>,
    target: Option<IconPrewarmTarget>,
    running: bool,
    generation: u64,
}

static ICON_PREWARM_COORDINATOR: OnceLock<Arc<Mutex<IconPrewarmCoordinator>>> = OnceLock::new();

fn icon_prewarm_coordinator() -> Arc<Mutex<IconPrewarmCoordinator>> {
    ICON_PREWARM_COORDINATOR
        .get_or_init(|| {
            Arc::new(Mutex::new(IconPrewarmCoordinator {
                keys: VecDeque::new(),
                queued: HashSet::new(),
                target: None,
                running: false,
                generation: 0,
            }))
        })
        .clone()
}

async fn run_icon_prewarm_worker(
    runtime: tokio::runtime::Handle,
    coordinator: Arc<Mutex<IconPrewarmCoordinator>>,
    batch_limit: usize,
) {
    loop {
        let (keys, target, generation) = {
            let Ok(mut pending) = coordinator.lock() else {
                return;
            };
            if pending.keys.is_empty() {
                pending.running = false;
                pending.target = None;
                return;
            }
            let mut keys = Vec::with_capacity(batch_limit);
            while keys.len() < batch_limit {
                let Some(key) = pending.keys.pop_front() else {
                    break;
                };
                pending.queued.remove(&key);
                keys.push(key);
            }
            (
                keys,
                pending.target.as_ref().map(|target| IconPrewarmTarget {
                    ui: target.ui.clone(),
                    state: target.state.clone(),
                }),
                pending.generation,
            )
        };

        let prewarm = prewarm_async(&runtime, keys);
        if let Err(error) = prewarm.await {
            tracing::debug!(%error, "file icon prewarm task stopped");
        }
        let still_current = coordinator
            .lock()
            .is_ok_and(|pending| pending.generation == generation);
        if !still_current {
            clear_global_cache();
            continue;
        }
        if let Some(target) = target
            && let Some(state) = target.state.upgrade()
        {
            dispatch_active_snapshot(&target.ui, &state);
        }
    }
}

pub(in crate::app) fn clear_file_icon_cache() {
    if let Some(coordinator) = ICON_PREWARM_COORDINATOR.get()
        && let Ok(mut pending) = coordinator.lock()
    {
        pending.keys.clear();
        pending.queued.clear();
        pending.target = None;
        pending.generation = pending.generation.wrapping_add(1);
    }
    let released = clear_global_cache();
    tracing::debug!(
        released_extension_icons = released,
        "cleared SFTP file-icon cache"
    );
}

pub(in crate::app) fn sftp_icon_keys(entries: &[SftpEntry]) -> Vec<FileIconKey> {
    entries
        .iter()
        .map(|entry| FileIconKey::for_entry(&entry.name, entry.is_dir, entry.is_symlink))
        .collect()
}

pub(in crate::app) fn local_icon_keys(entries: &[LocalDirectoryEntry]) -> Vec<FileIconKey> {
    entries
        .iter()
        .map(|entry| FileIconKey::for_entry(&entry.name, entry.is_dir, entry.is_symlink))
        .collect()
}

pub(super) fn sftp_transfer_rows(transfers: Vec<SftpTransferSnapshot>) -> Vec<SftpTransferRow> {
    transfers
        .into_iter()
        .map(|transfer| {
            let progress = if transfer.total_bytes == 0 {
                0.0
            } else {
                (transfer.downloaded_bytes as f64 / transfer.total_bytes as f64).clamp(0.0, 1.0)
                    as f32
            };
            let size = if transfer.total_bytes == 0 {
                format_file_size(transfer.downloaded_bytes, false)
            } else if transfer.downloaded_bytes >= transfer.total_bytes {
                format_file_size(transfer.total_bytes, false)
            } else {
                format!(
                    "{} / {}",
                    format_file_size(transfer.downloaded_bytes, false),
                    format_file_size(transfer.total_bytes, false)
                )
            };
            let speed = if transfer.phase.cancellable() && transfer.bytes_per_second > 0 {
                format!("{}/s", format_file_size(transfer.bytes_per_second, false))
            } else {
                String::new()
            };
            SftpTransferRow {
                id: transfer.id.to_string().into(),
                name: transfer.name.into(),
                state: transfer.phase.as_str().into(),
                status: transfer.status.into(),
                progress,
                size: size.into(),
                speed: speed.into(),
                selected: transfer.selected,
                pausable: transfer.phase == SftpTransferPhase::Downloading,
                resumable: transfer.phase == SftpTransferPhase::Paused,
                cancellable: transfer.phase.cancellable(),
            }
        })
        .collect()
}

pub(super) fn format_file_size(bytes: u64, is_dir: bool) -> String {
    if is_dir {
        return "-".to_owned();
    }
    if bytes < 1_024 {
        format!("{bytes} B")
    } else if bytes < 1_048_576 {
        format!("{:.1} KB", bytes as f64 / 1_024.0)
    } else if bytes < 1_073_741_824 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    }
}

pub(super) fn format_timestamp(timestamp: u32) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(i64::from(timestamp), 0)
        .map(|time| time.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

pub(super) fn format_local_timestamp(timestamp: std::time::SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(timestamp)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}
