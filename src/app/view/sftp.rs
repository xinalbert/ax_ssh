use super::*;

const SLINT_ICON_CACHE_CAPACITY: usize = 128;
type SlintIconCache =
    std::collections::HashMap<FileIconKey, (super::super::file_icons::FileIcon, slint::Image)>;

thread_local! {
    static SLINT_FILE_ICONS: std::cell::RefCell<SlintIconCache> = std::cell::RefCell::new(SlintIconCache::new());
}

struct FileRowSource<E> {
    entries: Vec<E>,
    selected: HashSet<String>,
    icon_revision: u64,
}

struct FileRowModel<E> {
    rows: VecModel<SftpEntryRow>,
    source: std::cell::RefCell<FileRowSource<E>>,
}

impl<E: 'static> slint::Model for FileRowModel<E> {
    type Data = SftpEntryRow;
    fn row_count(&self) -> usize {
        self.rows.row_count()
    }
    fn row_data(&self, row: usize) -> Option<Self::Data> {
        self.rows.row_data(row)
    }
    fn set_row_data(&self, row: usize, data: Self::Data) {
        self.rows.set_row_data(row, data);
    }
    fn model_tracker(&self) -> &dyn slint::ModelTracker {
        self.rows.model_tracker()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub(super) fn update_file_rows<E: Clone + PartialEq + 'static>(
    current: ModelRc<SftpEntryRow>,
    entries: Vec<E>,
    selected: &HashSet<String>,
    icon_revision: u64,
    build: impl FnOnce(Vec<E>, &HashSet<String>) -> Vec<SftpEntryRow>,
) -> ModelRc<SftpEntryRow> {
    let existing = current.as_any().downcast_ref::<FileRowModel<E>>();
    if let Some(model) = existing {
        let source = model.source.borrow();
        if source.entries == entries
            && source.selected == *selected
            && source.icon_revision == icon_revision
        {
            return current.clone();
        }
    }
    let rows = build(entries.clone(), selected);
    let source = FileRowSource {
        entries,
        selected: selected.clone(),
        icon_revision,
    };
    if let Some(model) = existing
        && row_ids_match(&model.rows, &rows, |old, new| old.path == new.path)
    {
        sync_rows(&model.rows, rows);
        *model.source.borrow_mut() = source;
        return current;
    }
    ModelRc::new(FileRowModel {
        rows: VecModel::from(rows),
        source: std::cell::RefCell::new(source),
    })
}

fn row_ids_match<T: Clone + 'static>(
    model: &VecModel<T>,
    rows: &[T],
    same_identity: impl Fn(&T, &T) -> bool,
) -> bool {
    let common = model.row_count().min(rows.len());
    (0..common).all(|index| {
        model
            .row_data(index)
            .is_some_and(|row| same_identity(&row, &rows[index]))
    })
}

fn sync_rows<T: Clone + PartialEq + 'static>(model: &VecModel<T>, rows: Vec<T>) {
    for index in (rows.len()..model.row_count()).rev() {
        model.remove(index);
    }
    for (index, row) in rows.into_iter().enumerate() {
        match model.row_data(index) {
            Some(existing) if existing == row => {}
            Some(_) => model.set_row_data(index, row),
            None => model.push(row),
        }
    }
}

pub(super) fn update_sftp_rows<T: Clone + PartialEq + 'static>(
    current: ModelRc<T>,
    rows: Vec<T>,
    same_identity: impl Fn(&T, &T) -> bool,
) -> ModelRc<T> {
    let Some(model) = current.as_any().downcast_ref::<VecModel<T>>() else {
        return ModelRc::new(VecModel::from(rows));
    };
    if !row_ids_match(model, &rows, same_identity) {
        return ModelRc::new(VecModel::from(rows));
    }
    sync_rows(model, rows);
    current
}

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
    SLINT_FILE_ICONS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((cached, image)) = cache.get(key)
            && cached.shares_pixels(&icon)
        {
            return image.clone();
        }
        let pixels = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
            icon.rgba(),
            icon.width(),
            icon.height(),
        );
        let image = slint::Image::from_rgba8(pixels);
        if cache.len() >= SLINT_ICON_CACHE_CAPACITY && !cache.contains_key(key) {
            let oldest = cache.keys().next().cloned();
            if let Some(oldest) = oldest {
                cache.remove(&oldest);
            }
        }
        cache.insert(key.clone(), (icon, image.clone()));
        image
    })
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
        let changed = match prewarm.await {
            Ok(report) => report.identity_changed || report.system_resolved > 0,
            Err(error) => {
                tracing::debug!(%error, "file icon prewarm task stopped");
                false
            }
        };
        let still_current = coordinator
            .lock()
            .is_ok_and(|pending| pending.generation == generation);
        if !still_current {
            clear_global_cache();
            continue;
        }
        if changed
            && let Some(target) = target
            && let Some(state) = target.state.upgrade()
        {
            dispatch_active_snapshot(&target.ui, &state);
        }
    }
}

pub(in crate::app) fn clear_file_icon_cache() {
    SLINT_FILE_ICONS.with(|cache| cache.borrow_mut().clear());
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
                direction: transfer.direction.as_str().into(),
                state: transfer.phase.as_str().into(),
                status: transfer.status.into(),
                progress,
                size: size.into(),
                speed: speed.into(),
                selected: transfer.selected,
                pausable: transfer.pausable && transfer.phase == SftpTransferPhase::Downloading,
                resumable: transfer.pausable && transfer.phase == SftpTransferPhase::Paused,
                cancellable: transfer.phase.cancellable(),
                has_local_path: transfer.has_local_path,
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
