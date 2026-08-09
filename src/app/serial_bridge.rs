use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

const SERIAL_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(5);
static SERIAL_DISCOVERY_GENERATION: AtomicU64 = AtomicU64::new(0);

pub(super) fn wire_serial_port_discovery(
    ui: &AppWindow,
    state: Arc<Mutex<AppState>>,
    runtime: Handle,
) {
    let ui_for_refresh = ui.as_weak();
    let state_for_refresh = state.clone();
    let runtime_for_refresh = runtime.clone();
    ui.on_refresh_serial_ports(move || {
        log_ui_action("serial.refresh-ports");
        refresh_serial_ports(
            &runtime_for_refresh,
            state_for_refresh.clone(),
            ui_for_refresh.clone(),
        );
    });

    let ui_for_mode = ui.as_weak();
    ui.on_serial_mode_changed(move |enabled| {
        if enabled {
            log_ui_action("serial.enter-mode");
            refresh_serial_ports(&runtime, state.clone(), ui_for_mode.clone());
            return;
        }
        if let Ok(mut app) = state.lock() {
            app.clear_serial_ports();
        }
        invalidate_serial_port_discovery();
        dispatch_ui(&ui_for_mode, move |ui| {
            ui.set_serial_port_options(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
        });
    });
}

fn refresh_serial_ports(runtime: &Handle, state: Arc<Mutex<AppState>>, ui: slint::Weak<AppWindow>) {
    let generation = SERIAL_DISCOVERY_GENERATION
        .fetch_add(1, Ordering::AcqRel)
        .wrapping_add(1);
    runtime.spawn(async move {
        let discovery = tokio::task::spawn_blocking(discover_serial_ports);
        let ports = match tokio::time::timeout(SERIAL_DISCOVERY_TIMEOUT, discovery).await {
            Ok(Ok(Ok(ports))) => ports,
            Ok(Ok(Err(error))) => {
                set_status(&ui, &format!("Cannot list serial ports: {error}"));
                return;
            }
            Ok(Err(error)) => {
                set_status(&ui, &format!("Serial port scan failed: {error}"));
                return;
            }
            Err(_) => {
                set_status(&ui, "Serial port scan timed out");
                return;
            }
        };
        let names = ports
            .iter()
            .map(|port| SharedString::from(port.port_name.clone()))
            .collect::<Vec<_>>();
        if SERIAL_DISCOVERY_GENERATION.load(Ordering::Acquire) != generation {
            return;
        }
        match state.lock() {
            Ok(mut app) if app.has_session_editor_tab() => app.replace_serial_ports(ports),
            Ok(_) => return,
            Err(_) => {
                set_status(&ui, "Cannot update serial port list");
                return;
            }
        }
        dispatch_ui(&ui, move |ui| {
            if SERIAL_DISCOVERY_GENERATION.load(Ordering::Acquire) != generation
                || !state.lock().is_ok_and(|app| app.has_session_editor_tab())
            {
                return;
            }
            ui.set_serial_port_options(ModelRc::new(VecModel::from(names)));
        });
    });
}

pub(super) fn invalidate_serial_port_discovery() {
    SERIAL_DISCOVERY_GENERATION.fetch_add(1, Ordering::AcqRel);
}
