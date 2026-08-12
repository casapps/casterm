//! Serial port transport — connects a `SerialConfig` to a real `serialport`
//! device and pipes its byte stream into the same `PtyMsg`-shaped channel
//! consumer local PTY and SSH panes use (see `ui::tui::PaneBackend`).

use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::app::serial::{DataBits, FlowControl, NewlineMode, Parity, SerialConfig, StopBits};
use crate::support::error::{CastermError, Result};

/// A message delivered from the serial reader thread.
pub enum SerialMsg {
    /// Bytes read from the device (already hex-formatted if `hex_mode` is
    /// enabled — see `format_incoming`).
    Data(Vec<u8>),
    /// The device was disconnected or the reader hit an unrecoverable I/O
    /// error.
    Disconnected,
}

/// A live serial connection: an open port handle for writes plus a
/// background reader thread streaming `SerialMsg`s back over a channel.
pub struct SerialTransport {
    port: Box<dyn serialport::SerialPort>,
    rx: mpsc::Receiver<SerialMsg>,
    newline: NewlineMode,
    hex_mode: bool,
}

impl SerialTransport {
    /// Open the device described by `config`, apply its line settings, and
    /// spawn a background thread that streams incoming bytes back via a
    /// channel. Blocks only for the (fast) local device-open syscall — there
    /// is no handshake to await, unlike SSH.
    pub fn connect(config: &SerialConfig) -> Result<Self> {
        let data_bits = map_data_bits(config.data_bits);
        let parity = map_parity(config.parity)?;
        let stop_bits = map_stop_bits(config.stop_bits);
        let flow_control = map_flow_control(config.flow_control);

        let port = serialport::new(&config.device, config.baud_rate)
            .data_bits(data_bits)
            .parity(parity)
            .stop_bits(stop_bits)
            .flow_control(flow_control)
            .timeout(Duration::from_millis(100))
            .open()
            .map_err(|e| CastermError::Serial(format!("failed to open {}: {e}", config.device)))?;

        let mut reader = port
            .try_clone()
            .map_err(|e| CastermError::Serial(format!("failed to clone port handle: {e}")))?;

        let (tx, rx) = mpsc::channel::<SerialMsg>();
        let hex_mode = config.hex_mode;
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => continue,
                    Ok(n) => {
                        let formatted = format_incoming(&buf[..n], hex_mode);
                        if tx.send(SerialMsg::Data(formatted)).is_err() {
                            break;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                    Err(_) => {
                        let _ = tx.send(SerialMsg::Disconnected);
                        break;
                    }
                }
            }
        });

        Ok(Self {
            port,
            rx,
            newline: config.newline,
            hex_mode,
        })
    }

    /// Write keystrokes to the device, applying the configured newline
    /// conversion first.
    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        let converted = apply_newline_mode(data, self.newline);
        self.port
            .write_all(&converted)
            .map_err(|e| CastermError::Serial(format!("write failed: {e}")))
    }

    /// Non-blocking poll for the next chunk of incoming data.
    pub fn try_recv(&self) -> Option<SerialMsg> {
        self.rx.try_recv().ok()
    }

    /// Whether incoming bytes are currently being hex-formatted for
    /// display.
    pub fn hex_mode(&self) -> bool {
        self.hex_mode
    }
}

fn map_data_bits(bits: DataBits) -> serialport::DataBits {
    match bits {
        DataBits::Five => serialport::DataBits::Five,
        DataBits::Six => serialport::DataBits::Six,
        DataBits::Seven => serialport::DataBits::Seven,
        DataBits::Eight => serialport::DataBits::Eight,
    }
}

fn map_parity(parity: Parity) -> Result<serialport::Parity> {
    match parity {
        Parity::None => Ok(serialport::Parity::None),
        Parity::Odd => Ok(serialport::Parity::Odd),
        Parity::Even => Ok(serialport::Parity::Even),
        // The underlying `serialport` crate has no mark/space parity
        // support on any backend; reject rather than silently downgrade.
        Parity::Mark => Err(CastermError::Serial(
            "mark parity is not supported by the serial backend".to_string(),
        )),
        Parity::Space => Err(CastermError::Serial(
            "space parity is not supported by the serial backend".to_string(),
        )),
    }
}

fn map_stop_bits(bits: StopBits) -> serialport::StopBits {
    match bits {
        StopBits::One => serialport::StopBits::One,
        // `serialport` has no distinct 1.5-stop-bit variant; it's a rare
        // legacy setting collapsed onto the nearest supported value.
        StopBits::OneAndHalf => serialport::StopBits::One,
        StopBits::Two => serialport::StopBits::Two,
    }
}

fn map_flow_control(flow: FlowControl) -> serialport::FlowControl {
    match flow {
        FlowControl::None => serialport::FlowControl::None,
        FlowControl::Hardware => serialport::FlowControl::Hardware,
        FlowControl::Software => serialport::FlowControl::Software,
    }
}

/// Apply the configured newline conversion to outgoing (keystroke) bytes.
fn apply_newline_mode(data: &[u8], mode: NewlineMode) -> Vec<u8> {
    match mode {
        NewlineMode::PassThrough => data.to_vec(),
        NewlineMode::CrLf => convert_newlines(data, b"\r\n"),
        NewlineMode::Lf => convert_newlines(data, b"\n"),
        NewlineMode::Cr => convert_newlines(data, b"\r"),
    }
}

/// Replace every `\r`, `\n`, or `\r\n` run in `data` with `replacement`.
fn convert_newlines(data: &[u8], replacement: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        match data[i] {
            b'\r' if data.get(i + 1) == Some(&b'\n') => {
                out.extend_from_slice(replacement);
                i += 2;
            }
            b'\r' | b'\n' => {
                out.extend_from_slice(replacement);
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    out
}

/// Format incoming device bytes for display. In hex mode, bytes are
/// rendered as a classic 16-columns-per-row hex dump (plain ASCII text, so
/// it can be fed straight through the pane's VTE processor like any other
/// terminal output); otherwise bytes pass through unchanged.
fn format_incoming(data: &[u8], hex_mode: bool) -> Vec<u8> {
    if !hex_mode {
        return data.to_vec();
    }

    let mut out = String::new();
    for chunk in data.chunks(16) {
        for byte in chunk {
            out.push_str(&format!("{byte:02x} "));
        }
        out.push_str("\r\n");
    }
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newline_mode_converts_bare_lf_to_crlf() {
        let out = apply_newline_mode(b"hello\nworld", NewlineMode::CrLf);
        assert_eq!(out, b"hello\r\nworld");
    }

    #[test]
    fn newline_mode_collapses_existing_crlf_to_single_replacement() {
        let out = apply_newline_mode(b"a\r\nb", NewlineMode::Lf);
        assert_eq!(out, b"a\nb");
    }

    #[test]
    fn newline_mode_pass_through_is_a_no_op() {
        let out = apply_newline_mode(b"a\r\nb\nc\rd", NewlineMode::PassThrough);
        assert_eq!(out, b"a\r\nb\nc\rd");
    }

    #[test]
    fn hex_mode_formats_bytes_as_a_hex_dump() {
        let out = format_incoming(&[0x00, 0x1f, 0xff], true);
        assert_eq!(out, b"00 1f ff \r\n");
    }

    #[test]
    fn non_hex_mode_passes_bytes_through_unchanged() {
        let out = format_incoming(b"raw data", false);
        assert_eq!(out, b"raw data");
    }

    #[test]
    fn loopback_round_trips_data_through_a_virtual_pty_pair() {
        // `TTYPort::pair()` creates two linked virtual serial ports (Unix
        // only) — write to one, read from the other. This exercises the
        // real `serialport` I/O path without requiring physical hardware.
        #[cfg(unix)]
        {
            use serialport::TTYPort;

            let (mut writer, mut reader) = TTYPort::pair().expect("open a virtual pty pair");
            writer.write_all(b"ping").expect("write to virtual port");

            let mut buf = [0u8; 4];
            let mut total = 0;
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while total < buf.len() && std::time::Instant::now() < deadline {
                match reader.read(&mut buf[total..]) {
                    Ok(n) => total += n,
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                    Err(e) => panic!("read from virtual port failed: {e}"),
                }
            }
            assert_eq!(&buf[..total], b"ping");
        }
    }
}
