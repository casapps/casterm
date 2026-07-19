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
    Reconnecting,
    Failed,
}

/// A serial connection entry in the manager
pub struct SerialConnection {
    pub config: SerialConfig,
    pub state: SerialState,
    /// Error message if state is Failed
    pub error: Option<String>,
}

impl SerialConnection {
    pub fn new(config: SerialConfig) -> Self {
        Self {
            config,
            state: SerialState::Disconnected,
            error: None,
        }
    }
}

/// Named serial port preset — saved baud/data/parity/stop/flow settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerialPreset {
    pub name: String,
    pub baud_rate: u32,
    pub data_bits: DataBits,
    pub parity: Parity,
    pub stop_bits: StopBits,
    pub flow_control: FlowControl,
}

impl SerialPreset {
    /// Common presets
    pub fn common_presets() -> Vec<Self> {
        vec![
            Self {
                name: "115200-8N1".to_string(),
                baud_rate: 115200,
                data_bits: DataBits::Eight,
                parity: Parity::None,
                stop_bits: StopBits::One,
                flow_control: FlowControl::None,
            },
            Self {
                name: "9600-8N1".to_string(),
                baud_rate: 9600,
                data_bits: DataBits::Eight,
                parity: Parity::None,
                stop_bits: StopBits::One,
                flow_control: FlowControl::None,
            },
            Self {
                name: "57600-8N1".to_string(),
                baud_rate: 57600,
                data_bits: DataBits::Eight,
                parity: Parity::None,
                stop_bits: StopBits::One,
                flow_control: FlowControl::None,
            },
            Self {
                name: "4800-7E1".to_string(),
                baud_rate: 4800,
                data_bits: DataBits::Seven,
                parity: Parity::Even,
                stop_bits: StopBits::One,
                flow_control: FlowControl::None,
            },
        ]
    }
}

/// Serial connection manager
pub struct SerialManager {
    connections: Vec<SerialConnection>,
    presets: Vec<SerialPreset>,
}

impl SerialManager {
    pub fn new() -> Self {
        Self {
            connections: Vec::new(),
            presets: SerialPreset::common_presets(),
        }
    }

    /// Add a new serial connection
    pub fn add(&mut self, config: SerialConfig) -> SerialId {
        let id = config.id;
        self.connections.push(SerialConnection::new(config));
        id
    }

    /// Remove a serial connection
    pub fn remove(&mut self, id: SerialId) {
        self.connections.retain(|c| c.config.id != id);
    }

    /// Get all serial connections
    pub fn list(&self) -> &[SerialConnection] {
        &self.connections
    }

    /// Find a connection by ID
    pub fn get(&self, id: SerialId) -> Option<&SerialConnection> {
        self.connections.iter().find(|c| c.config.id == id)
    }

    /// Find a connection by ID (mutable)
    pub fn get_mut(&mut self, id: SerialId) -> Option<&mut SerialConnection> {
        self.connections.iter_mut().find(|c| c.config.id == id)
    }

    /// List available serial presets
    pub fn presets(&self) -> &[SerialPreset] {
        &self.presets
    }

    /// Add a named preset
    pub fn add_preset(&mut self, preset: SerialPreset) {
        self.presets.push(preset);
    }

    /// Validate a serial config
    pub fn validate(config: &SerialConfig) -> Result<()> {
        if config.device.is_empty() {
            return Err(CastermError::Config("Serial device path cannot be empty".into()));
        }
        if config.baud_rate == 0 {
            return Err(CastermError::Config("Serial baud rate must be non-zero".into()));
        }
        Ok(())
    }

    /// List available serial ports on the current system
    #[cfg(not(target_os = "windows"))]
    pub fn list_ports() -> Vec<String> {
        use std::path::Path;

        let prefixes = [
            "/dev/ttyUSB",
            "/dev/ttyACM",
            "/dev/ttyS",
            "/dev/tty.",
            "/dev/cu.",
        ];

        let mut ports = Vec::new();

        for prefix in &prefixes {
            for i in 0..32 {
                let path = format!("{}{}", prefix, i);
                if Path::new(&path).exists() {
                    ports.push(path);
                }
            }
        }

        ports.sort();
        ports
    }

    #[cfg(target_os = "windows")]
    pub fn list_ports() -> Vec<String> {
        (1..=32).map(|i| format!("COM{}", i)).collect()
    }
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
