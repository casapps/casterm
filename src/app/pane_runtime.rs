//! Shared pane I/O runtime: local PTY, SSH, and serial backends
//!
//! `ui::tui` (multi-pane, all three backends) and `ui::gui` (single-pane,
//! local-shell only for its MVP — see `TODO.AI.md` for SSH/serial-backed
//! GUI panes) both drive a pane's shell/session the same way: an I/O
//! backend feeding bytes into a `terminal`/VTE pipeline. This module owns
//! that shared plumbing so the two front ends never duplicate it.

use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use crate::app::pty::{Pty, PtyConfig};
use crate::app::serial::{SerialConfig, SerialConnection};
use crate::app::serial_transport::SerialMsg;
use crate::app::ssh::{SshConfig, SshConnection};
use crate::app::ssh_transport::SshMsg;
use crate::app::terminal::{Terminal as TerminalEmulator, TerminalSize};
use crate::app::vte_processor::VteProcessor;
use crate::config::Config;
use crate::support::error::{CastermError, Result};

/// Messages from a pane's PTY reader thread
pub enum PtyMsg {
    Data(Vec<u8>),
    Exit,
}

/// A pane's live I/O backend: a local PTY-backed shell, a remote SSH
/// session, or a serial device. All three feed the same `PtyMsg` shape into
/// the pane's terminal emulator/VTE parser, so the render/drain/write/
/// resize paths don't need to know which backend a given pane is running.
pub enum PaneBackend {
    Local {
        pty: Pty,
        rx: mpsc::Receiver<PtyMsg>,
    },
    Ssh {
        conn: Box<SshConnection>,
    },
    Serial {
        conn: Box<SerialConnection>,
    },
}

impl PaneBackend {
    pub fn try_recv(&self) -> Option<PtyMsg> {
        match self {
            PaneBackend::Local { rx, .. } => rx.try_recv().ok(),
            PaneBackend::Ssh { conn } => conn.try_recv().map(|msg| match msg {
                SshMsg::Data(data) => PtyMsg::Data(data),
                SshMsg::Exit => PtyMsg::Exit,
            }),
            PaneBackend::Serial { conn } => conn.try_recv().map(|msg| match msg {
                SerialMsg::Data(data) => PtyMsg::Data(data),
                SerialMsg::Disconnected => PtyMsg::Exit,
            }),
        }
    }

    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        match self {
            PaneBackend::Local { pty, .. } => {
                pty.write(data)?;
                pty.flush()
            }
            PaneBackend::Ssh { conn } => conn.write(data),
            PaneBackend::Serial { conn } => conn.write(data),
        }
    }

    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        match self {
            PaneBackend::Local { pty, .. } => pty.resize(rows, cols),
            PaneBackend::Ssh { conn } => conn.resize(cols, rows),
            // Serial devices have no concept of a terminal size — resize
            // is a PTY/SSH-only notion.
            PaneBackend::Serial { .. } => Ok(()),
        }
    }

    /// Gracefully tear down an SSH or serial session before the pane is
    /// dropped (local PTYs don't need this — their child process is killed
    /// by `Pty`'s own `Drop` impl).
    pub fn disconnect(&mut self) {
        match self {
            PaneBackend::Ssh { conn } => conn.disconnect(),
            PaneBackend::Serial { conn } => conn.disconnect(),
            PaneBackend::Local { .. } => {}
        }
    }
}

/// The live state backing a single pane: its I/O backend and its own
/// terminal emulator/VTE parser. Each pane runs an independent shell or SSH
/// session — splitting a window multiplies this, it doesn't share one
/// backend across panes.
pub struct PaneRuntime {
    pub backend: PaneBackend,
    pub emulator: TerminalEmulator,
    pub vte: VteProcessor,
    /// The working directory this pane's shell was spawned in — tracked so
    /// session save/restore (`state::PaneState.cwd`) can re-open a restored
    /// pane in the same place. Defaults to the process's own cwd when the
    /// caller doesn't request a specific one. SSH panes report the local
    /// process's own cwd here since they have no local shell directory of
    /// their own.
    pub cwd: PathBuf,
}

/// Spawn a shell PTY plus its background reader thread and terminal
/// emulator for one pane, starting the shell in `cwd` (falling back to the
/// process's own working directory when `None`).
pub fn spawn_pane_runtime(
    config: &Config,
    size: TerminalSize,
    cwd: Option<PathBuf>,
) -> Result<PaneRuntime> {
    if size.cols == 0 || size.rows == 0 {
        return Err(CastermError::Terminal(
            "terminal size must be non-zero".to_string(),
        ));
    }
    let shell = config
        .shell
        .path
        .clone()
        .or_else(crate::config::detect_shell)
        .unwrap_or_else(|| {
            #[cfg(windows)]
            {
                std::path::PathBuf::from("cmd.exe")
            }
            #[cfg(not(windows))]
            {
                std::path::PathBuf::from("/bin/sh")
            }
        });

    let resolved_cwd = cwd
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let mut pty_config = PtyConfig {
        shell,
        rows: size.rows,
        cols: size.cols,
        cwd,
        ..Default::default()
    };
    // Advertise true-color support so shells and editors use it. Prefer
    // casterm's own embedded terminfo entry (extracted to a per-user
    // cache dir, never installed system-wide); fall back to the
    // universally-available xterm-256color identity if it's missing.
    match crate::support::terminfo::install() {
        Some(terminfo_dir) => {
            pty_config.env.push((
                "TERM".to_string(),
                crate::support::terminfo::TERM_NAME.to_string(),
            ));
            pty_config
                .env
                .push(("TERMINFO".to_string(), terminfo_dir.display().to_string()));
        }
        None => {
            pty_config
                .env
                .push(("TERM".to_string(), "xterm-256color".to_string()));
        }
    }
    pty_config
        .env
        .push(("COLORTERM".to_string(), "truecolor".to_string()));

    let mut pty = Pty::spawn(pty_config)?;

    // Move reader into a background thread; send bytes back via channel
    let (tx, pty_rx) = mpsc::channel::<PtyMsg>();
    let mut reader = pty
        .take_reader()
        .ok_or_else(|| CastermError::Pty("PTY reader not available".to_string()))?;

    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    let _ = tx.send(PtyMsg::Exit);
                    break;
                }
                Ok(n) => {
                    if tx.send(PtyMsg::Data(buf[..n].to_vec())).is_err() {
                        break;
                    }
                }
                Err(_) => {
                    let _ = tx.send(PtyMsg::Exit);
                    break;
                }
            }
        }
    });

    let emulator = TerminalEmulator::new(size);
    let vte = VteProcessor::new();

    Ok(PaneRuntime {
        backend: PaneBackend::Local { pty, rx: pty_rx },
        emulator,
        vte,
        cwd: resolved_cwd,
    })
}

/// Connect an SSH-backed pane and open an interactive remote shell,
/// blocking until the connection either succeeds or fails (same contract as
/// `spawn_pane_runtime` for a local shell).
pub fn spawn_ssh_pane_runtime(ssh_config: &SshConfig, size: TerminalSize) -> Result<PaneRuntime> {
    if size.cols == 0 || size.rows == 0 {
        return Err(CastermError::Terminal(
            "terminal size must be non-zero".to_string(),
        ));
    }
    let mut conn = SshConnection::new(ssh_config.clone());
    conn.connect(size.cols, size.rows)?;

    let emulator = TerminalEmulator::new(size);
    let vte = VteProcessor::new();

    Ok(PaneRuntime {
        backend: PaneBackend::Ssh {
            conn: Box::new(conn),
        },
        emulator,
        vte,
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    })
}

/// Connect a serial-backed pane and open the device, blocking only for the
/// local open syscall (same contract as `spawn_pane_runtime`/
/// `spawn_ssh_pane_runtime`, but there's no remote handshake to await).
pub fn spawn_serial_pane_runtime(
    serial_config: &SerialConfig,
    size: TerminalSize,
) -> Result<PaneRuntime> {
    if size.cols == 0 || size.rows == 0 {
        return Err(CastermError::Terminal(
            "terminal size must be non-zero".to_string(),
        ));
    }
    let mut conn = SerialConnection::new(serial_config.clone());
    conn.connect()?;

    let emulator = TerminalEmulator::new(size);
    let vte = VteProcessor::new();

    Ok(PaneRuntime {
        backend: PaneBackend::Serial {
            conn: Box::new(conn),
        },
        emulator,
        vte,
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A zero `cols` or `rows` must be rejected before any backend resource
    /// (shell process, network socket, serial device) is touched — a 0x0
    /// terminal has no cell grid to render into.
    #[test]
    fn spawn_pane_runtime_rejects_zero_size() {
        let config = Config::default();
        let zero_cols = TerminalSize { cols: 0, rows: 24 };
        let zero_rows = TerminalSize { cols: 80, rows: 0 };

        assert!(matches!(
            spawn_pane_runtime(&config, zero_cols, None),
            Err(CastermError::Terminal(_))
        ));
        assert!(matches!(
            spawn_pane_runtime(&config, zero_rows, None),
            Err(CastermError::Terminal(_))
        ));
    }

    #[test]
    fn spawn_ssh_pane_runtime_rejects_zero_size() {
        let ssh_config = SshConfig::default();
        let zero = TerminalSize { cols: 0, rows: 0 };

        assert!(matches!(
            spawn_ssh_pane_runtime(&ssh_config, zero),
            Err(CastermError::Terminal(_))
        ));
    }

    #[test]
    fn spawn_serial_pane_runtime_rejects_zero_size() {
        let serial_config = SerialConfig::default();
        let zero_cols = TerminalSize { cols: 0, rows: 24 };

        assert!(matches!(
            spawn_serial_pane_runtime(&serial_config, zero_cols),
            Err(CastermError::Terminal(_))
        ));
    }
}
