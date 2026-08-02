use super::*;

const SERIAL_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(5);

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
    refresh_serial_ports(&runtime, state, ui.as_weak());
}

fn refresh_serial_ports(runtime: &Handle, state: Arc<Mutex<AppState>>, ui: slint::Weak<AppWindow>) {
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
        match state.lock() {
            Ok(mut app) => app.replace_serial_ports(ports),
            Err(_) => {
                set_status(&ui, "Cannot update serial port list");
                return;
            }
        }
        dispatch_ui(&ui, move |ui| {
            ui.set_serial_port_options(ModelRc::new(VecModel::from(names)));
        });
    });
}
