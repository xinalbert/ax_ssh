//! Serial-port discovery, stable device matching, and session worker lifetime.

use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{Duration, timeout};
use tokio_serial::{
    DataBits, FlowControl, Parity, SerialPortBuilderExt, SerialPortInfo, SerialPortType, StopBits,
};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::config::{
    SerialConfig, SerialDataBits, SerialFlowControl, SerialParity, SerialStopBits,
};

const WORKER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(7);
const COMMAND_CAPACITY: usize = 32;
const EVENT_CAPACITY: usize = 32;
const MAX_INPUT_BYTES: usize = 16 * 1024;
const MAX_OUTPUT_BATCH_BYTES: usize = 16 * 1024;
const MAX_ERROR_CHARS: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SerialPortDescriptor {
    pub port_name: String,
    pub usb_vendor_id: Option<u16>,
    pub usb_product_id: Option<u16>,
    pub usb_serial_number: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

impl SerialPortDescriptor {
    fn from_port_info(port: SerialPortInfo) -> Self {
        let mut descriptor = Self {
            port_name: port.port_name,
            usb_vendor_id: None,
            usb_product_id: None,
            usb_serial_number: None,
            manufacturer: None,
            product: None,
        };
        if let SerialPortType::UsbPort(usb) = port.port_type {
            descriptor.usb_vendor_id = Some(usb.vid);
            descriptor.usb_product_id = Some(usb.pid);
            descriptor.usb_serial_number = usb.serial_number.filter(|value| !value.is_empty());
            descriptor.manufacturer = usb.manufacturer.filter(|value| !value.is_empty());
            descriptor.product = usb.product.filter(|value| !value.is_empty());
        }
        descriptor
    }

    pub fn apply_identity_to(&self, config: &mut SerialConfig) {
        config.port_name.clone_from(&self.port_name);
        config.usb_vendor_id = self.usb_vendor_id;
        config.usb_product_id = self.usb_product_id;
        config.usb_serial_number.clone_from(&self.usb_serial_number);
    }
}

/// Lists available devices without opening a serial handle or toggling modem lines.
pub fn discover_serial_ports() -> Result<Vec<SerialPortDescriptor>> {
    let mut ports = tokio_serial::available_ports()
        .context("failed to enumerate serial ports")?
        .into_iter()
        .map(SerialPortDescriptor::from_port_info)
        .collect::<Vec<_>>();
    ports.sort_by(|left, right| {
        left.port_name
            .to_ascii_lowercase()
            .cmp(&right.port_name.to_ascii_lowercase())
            .then_with(|| left.port_name.cmp(&right.port_name))
    });
    ports.dedup_by(|left, right| left.port_name == right.port_name);
    Ok(ports)
}

/// Resolves a persisted device identity without opening any candidate port.
pub fn resolve_serial_port<'a>(
    config: &SerialConfig,
    ports: &'a [SerialPortDescriptor],
) -> Result<&'a SerialPortDescriptor> {
    if let (Some(vendor_id), Some(product_id)) = (config.usb_vendor_id, config.usb_product_id) {
        let matches = ports
            .iter()
            .filter(|port| {
                port.usb_vendor_id == Some(vendor_id)
                    && port.usb_product_id == Some(product_id)
                    && config
                        .usb_serial_number
                        .as_ref()
                        .is_none_or(|serial| port.usb_serial_number.as_ref() == Some(serial))
            })
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [port] => return Ok(port),
            [] => {}
            ports => {
                if let Some(port) = ports.iter().find(|port| port.port_name == config.port_name) {
                    return Ok(port);
                }
                anyhow::bail!(
                    "multiple serial devices match the saved USB identity; select a port again"
                );
            }
        }
    }
    ports
        .iter()
        .find(|port| port.port_name == config.port_name)
        .with_context(|| format!("serial port {} is not available", config.port_name))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SerialSessionEvent {
    Connected { port_name: String },
    Output(Vec<u8>),
    Disconnected,
    Failed(String),
}

enum SerialCommand {
    Send(Vec<u8>),
    Disconnect,
}

/// UI-adjacent controller for one worker-owned serial device.
pub struct SerialSessionHandle {
    command_tx: mpsc::Sender<SerialCommand>,
    task: JoinHandle<()>,
}

impl SerialSessionHandle {
    pub fn spawn(
        runtime: &Handle,
        session_id: Uuid,
        config: SerialConfig,
    ) -> (Self, mpsc::Receiver<SerialSessionEvent>) {
        let (command_tx, command_rx) = mpsc::channel(COMMAND_CAPACITY);
        let (event_tx, event_rx) = mpsc::channel(EVENT_CAPACITY);
        let task = runtime.spawn(run_serial_session(session_id, config, command_rx, event_tx));
        (Self { command_tx, task }, event_rx)
    }

    pub fn request_disconnect(&self) -> Result<()> {
        self.command_tx
            .try_send(SerialCommand::Disconnect)
            .map_err(|error| anyhow::anyhow!("cannot request serial disconnect: {error}"))
    }

    pub fn request_send(&self, data: Vec<u8>) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        if data.len() > MAX_INPUT_BYTES {
            anyhow::bail!("terminal input cannot exceed {MAX_INPUT_BYTES} bytes");
        }
        self.command_tx
            .try_send(SerialCommand::Send(data))
            .map_err(|error| anyhow::anyhow!("cannot queue serial input: {error}"))
    }

    pub async fn shutdown(mut self) -> Result<()> {
        if !self.task.is_finished()
            && self
                .command_tx
                .send(SerialCommand::Disconnect)
                .await
                .is_err()
        {
            debug!("serial worker command receiver already closed during shutdown");
        }
        match timeout(WORKER_SHUTDOWN_TIMEOUT, &mut self.task).await {
            Ok(joined) => joined.context("serial worker task failed during shutdown"),
            Err(_) => {
                self.task.abort();
                match self.task.await {
                    Err(error) if error.is_cancelled() => {
                        warn!("serial worker exceeded shutdown timeout and was aborted");
                        Ok(())
                    }
                    Err(error) => Err(error).context("failed to abort serial worker task"),
                    Ok(()) => Ok(()),
                }
            }
        }
    }
}

async fn run_serial_session(
    session_id: Uuid,
    config: SerialConfig,
    mut command_rx: mpsc::Receiver<SerialCommand>,
    event_tx: mpsc::Sender<SerialSessionEvent>,
) {
    let builder = tokio_serial::new(&config.port_name, config.baud_rate)
        .data_bits(data_bits(config.data_bits))
        .stop_bits(stop_bits(config.stop_bits))
        .parity(parity(config.parity))
        .flow_control(flow_control(config.flow_control));
    let mut port = match builder.open_native_async() {
        Ok(port) => port,
        Err(error) => {
            send_serial_event(
                &event_tx,
                SerialSessionEvent::Failed(bounded_error(&error)),
                session_id,
            )
            .await;
            return;
        }
    };
    #[cfg(unix)]
    if let Err(error) = port.set_exclusive(true) {
        send_serial_event(
            &event_tx,
            SerialSessionEvent::Failed(bounded_error(&error)),
            session_id,
        )
        .await;
        return;
    }
    if !send_serial_event(
        &event_tx,
        SerialSessionEvent::Connected {
            port_name: config.port_name.clone(),
        },
        session_id,
    )
    .await
    {
        return;
    }
    info!(session_id = %session_id, port = %config.port_name, "serial port connected");

    let mut buffer = vec![0; MAX_OUTPUT_BATCH_BYTES];
    let mut failed = false;
    loop {
        tokio::select! {
            command = command_rx.recv() => {
                match command {
                    Some(SerialCommand::Send(data)) => {
                        if let Err(error) = port.write_all(&data).await {
                            send_serial_event(
                                &event_tx,
                                SerialSessionEvent::Failed(bounded_error(&error)),
                                session_id,
                            ).await;
                            failed = true;
                            break;
                        }
                    }
                    Some(SerialCommand::Disconnect) | None => break,
                }
            }
            read = port.read(&mut buffer) => {
                match read {
                    Ok(0) => break,
                    Ok(read) => {
                        if !send_serial_event(
                            &event_tx,
                            SerialSessionEvent::Output(buffer[..read].to_vec()),
                            session_id,
                        ).await {
                            break;
                        }
                    }
                    Err(error) => {
                        send_serial_event(
                            &event_tx,
                            SerialSessionEvent::Failed(bounded_error(&error)),
                            session_id,
                        ).await;
                        failed = true;
                        break;
                    }
                }
            }
        }
    }
    let _ = port.shutdown().await;
    if !failed {
        send_serial_event(&event_tx, SerialSessionEvent::Disconnected, session_id).await;
    }
}

async fn send_serial_event(
    event_tx: &mpsc::Sender<SerialSessionEvent>,
    event: SerialSessionEvent,
    session_id: Uuid,
) -> bool {
    if event_tx.send(event).await.is_err() {
        debug!(session_id = %session_id, "serial event receiver dropped");
        false
    } else {
        true
    }
}

const fn data_bits(value: SerialDataBits) -> DataBits {
    match value {
        SerialDataBits::Five => DataBits::Five,
        SerialDataBits::Six => DataBits::Six,
        SerialDataBits::Seven => DataBits::Seven,
        SerialDataBits::Eight => DataBits::Eight,
    }
}

const fn stop_bits(value: SerialStopBits) -> StopBits {
    match value {
        SerialStopBits::One => StopBits::One,
        SerialStopBits::Two => StopBits::Two,
    }
}

const fn parity(value: SerialParity) -> Parity {
    match value {
        SerialParity::None => Parity::None,
        SerialParity::Odd => Parity::Odd,
        SerialParity::Even => Parity::Even,
    }
}

const fn flow_control(value: SerialFlowControl) -> FlowControl {
    match value {
        SerialFlowControl::None => FlowControl::None,
        SerialFlowControl::Software => FlowControl::Software,
        SerialFlowControl::Hardware => FlowControl::Hardware,
    }
}

fn bounded_error(error: &impl std::fmt::Display) -> String {
    error.to_string().chars().take(MAX_ERROR_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usb(port_name: &str, serial: Option<&str>) -> SerialPortDescriptor {
        SerialPortDescriptor {
            port_name: port_name.into(),
            usb_vendor_id: Some(0x0403),
            usb_product_id: Some(0x6001),
            usb_serial_number: serial.map(str::to_owned),
            manufacturer: None,
            product: None,
        }
    }

    #[test]
    fn stable_usb_identity_survives_a_port_name_change() {
        let mut config = SerialConfig {
            port_name: "/dev/cu.old".into(),
            usb_vendor_id: Some(0x0403),
            usb_product_id: Some(0x6001),
            usb_serial_number: Some("FT123".into()),
            ..SerialConfig::default()
        };
        let ports = [usb("/dev/cu.new", Some("FT123"))];
        let resolved = resolve_serial_port(&config, &ports).expect("device should resolve");
        resolved.apply_identity_to(&mut config);

        assert_eq!(config.port_name, "/dev/cu.new");
    }

    #[test]
    fn ambiguous_usb_identity_requires_an_explicit_selection() {
        let config = SerialConfig {
            port_name: "/dev/cu.missing".into(),
            usb_vendor_id: Some(0x0403),
            usb_product_id: Some(0x6001),
            ..SerialConfig::default()
        };
        let ports = [usb("/dev/cu.one", None), usb("/dev/cu.two", None)];

        assert!(resolve_serial_port(&config, &ports).is_err());
    }
}
