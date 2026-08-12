//! Serial port connection manager

use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::support::error::{CastermError, Result};

static NEXT_SERIAL_ID: AtomicU64 = AtomicU64::new(1);

/// Unique serial connection identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SerialId(u64);

impl SerialId {
    pub fn next() -> Self {
        Self(NEXT_SERIAL_ID.fetch_add(1, Ordering::Relaxed))
    }
}

impl std::fmt::Display for SerialId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "serial-{}", self.0)
    }
}

/// Serial port data bits
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DataBits {
    Five,
    Six,
    Seven,
    #[default]
    Eight,
}

/// Serial port parity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Parity {
    #[default]
    None,
    Odd,
    Even,
    Mark,
    Space,
}

/// Serial port stop bits
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StopBits {
    #[default]
    One,
    OneAndHalf,
    Two,
}

/// Flow control mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FlowControl {
    #[default]
    None,
    Hardware,
    Software,
}

/// Newline conversion mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum NewlineMode {
    #[default]
    PassThrough,
    CrLf,
    Lf,
    Cr,
}

/// Serial connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerialConfig {
    /// Connection identifier
    pub id: SerialId,
    /// Display name for this connection
    pub name: String,
    /// Device path (e.g. /dev/ttyUSB0 on Linux, COM3 on Windows)
    pub device: String,
    /// Baud rate
    pub baud_rate: u32,
    /// Data bits
    pub data_bits: DataBits,
    /// Parity
    pub parity: Parity,
    /// Stop bits
    pub stop_bits: StopBits,
    /// Flow control
    pub flow_control: FlowControl,
    /// Newline conversion
    pub newline: NewlineMode,
    /// Enable hex view mode
    pub hex_mode: bool,
    /// Auto-reconnect on device disconnect
    pub auto_reconnect: bool,
    /// Seconds between reconnect attempts
    pub reconnect_delay: u32,
    /// Optional color label for visual identification
    pub color: Option<String>,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            id: SerialId::next(),
            name: String::new(),
            device: default_serial_device(),
            baud_rate: 115200,
            data_bits: DataBits::Eight,
            parity: Parity::None,
            stop_bits: StopBits::One,
            flow_control: FlowControl::None,
            newline: NewlineMode::PassThrough,
            hex_mode: false,
            auto_reconnect: true,
            reconnect_delay: 5,
            color: None,
        }
    }
}

/// Serial connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialState {
    Disconnected,
    Connecting,
    Connected,
    Failed,
}

/// A live serial connection: configuration, state, and (once connected) the
/// open transport streaming device I/O.
pub struct SerialConnection {
    pub config: SerialConfig,
    pub state: SerialState,
    /// Error message if state is Failed
    pub error: Option<String>,
    transport: Option<super::serial_transport::SerialTransport>,
}

impl SerialConnection {
    pub fn new(config: SerialConfig) -> Self {
        Self {
            config,
            state: SerialState::Disconnected,
            error: None,
            transport: None,
        }
    }

    /// Open the device, blocking only for the local open syscall (there is
    /// no remote handshake, unlike SSH). On success `state` becomes
    /// `Connected`; on failure it becomes `Failed` with `error` set and the
    /// error is also returned to the caller.
    pub fn connect(&mut self) -> Result<()> {
        self.state = SerialState::Connecting;
        match super::serial_transport::SerialTransport::connect(&self.config) {
            Ok(transport) => {
                self.transport = Some(transport);
                self.state = SerialState::Connected;
                self.error = None;
                Ok(())
            }
            Err(e) => {
                self.state = SerialState::Failed;
                // Surface the ports that *are* available so a typo'd or
                // unplugged device path is easy to diagnose from the error
                // alone.
                let available = list_ports();
                let message = if available.is_empty() {
                    format!("{e} (no serial ports detected)")
                } else {
                    format!("{e} (available ports: {})", available.join(", "))
                };
                self.error = Some(message.clone());
                Err(CastermError::Serial(message))
            }
        }
    }

    /// Close the port, discarding the transport.
    pub fn disconnect(&mut self) {
        self.transport = None;
        self.state = SerialState::Disconnected;
    }

    /// Write keystrokes to the device.
    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        match &mut self.transport {
            Some(t) => t.write(data),
            None => Err(CastermError::Serial("serial port is not open".to_string())),
        }
    }

    /// Non-blocking poll for the next byte chunk (or disconnect notice)
    /// from the device, matching the drain pattern `ui::tui` uses for local
    /// PTY and SSH panes.
    pub fn try_recv(&self) -> Option<super::serial_transport::SerialMsg> {
        self.transport.as_ref().and_then(|t| t.try_recv())
    }

    /// Whether incoming bytes are currently being hex-formatted for
    /// display — used by the TUI to mark hex-mode panes in their title.
    pub fn hex_mode(&self) -> bool {
        self.transport.as_ref().is_some_and(|t| t.hex_mode())
    }
}

/// Validate a serial config — returns Ok(()) if all required fields are
/// present.
pub fn validate(config: &SerialConfig) -> Result<()> {
    if config.device.is_empty() {
        return Err(CastermError::Config(
            "Serial device path cannot be empty".into(),
        ));
    }
    if config.baud_rate == 0 {
        return Err(CastermError::Config(
            "Serial baud rate must be non-zero".into(),
        ));
    }
    Ok(())
}

/// List available serial ports on the current system via the `serialport`
/// crate, which already handles per-platform enumeration correctly (USB
/// CDC-ACM, PCI, Bluetooth-serial, and platform-native device naming)
/// instead of hand-rolled `/dev/ttyUSB*`-style directory probing.
pub fn list_ports() -> Vec<String> {
    serialport::available_ports()
        .map(|ports| ports.into_iter().map(|p| p.port_name).collect())
        .unwrap_or_default()
}

/// Get the platform default serial device path
fn default_serial_device() -> String {
    #[cfg(target_os = "linux")]
    {
        "/dev/ttyUSB0".to_string()
    }
    #[cfg(target_os = "macos")]
    {
        "/dev/tty.usbserial".to_string()
    }
    #[cfg(target_os = "windows")]
    {
        "COM1".to_string()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        "/dev/ttyU0".to_string()
    }
}
