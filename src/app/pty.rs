//! PTY (pseudo-terminal) handling

use std::io::{Read, Write};
use std::path::PathBuf;

use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};

use crate::support::error::{CastermError, Result};

/// PTY configuration
#[derive(Debug, Clone)]
pub struct PtyConfig {
    pub shell: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: Option<PathBuf>,
    pub rows: u16,
    pub cols: u16,
}

fn default_shell() -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from("cmd.exe")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/bin/sh")
    }
}

impl Default for PtyConfig {
    fn default() -> Self {
        let shell = crate::config::detect_shell().unwrap_or_else(default_shell);

        Self {
            shell,
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            rows: 24,
            cols: 80,
        }
    }
}

/// A running PTY process
pub struct Pty {
    pair: PtyPair,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    /// Reader is stored as Option so it can be moved into a background thread
    reader: Option<Box<dyn Read + Send>>,
    writer: Box<dyn Write + Send>,
}

impl Pty {
    /// Spawn a new PTY with the given configuration
    pub fn spawn(config: PtyConfig) -> Result<Self> {
        let pty_system = native_pty_system();

        let pair = pty_system
            .openpty(PtySize {
                rows: config.rows,
                cols: config.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| CastermError::Pty(e.to_string()))?;

        let mut cmd = CommandBuilder::new(&config.shell);
        cmd.args(&config.args);

        if let Some(cwd) = &config.cwd {
            cmd.cwd(cwd);
        }

        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| CastermError::Pty(e.to_string()))?;

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| CastermError::Pty(e.to_string()))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| CastermError::Pty(e.to_string()))?;

        Ok(Self {
            pair,
            child,
            reader: Some(reader),
            writer,
        })
    }

    /// Resize the PTY
    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        self.pair
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| CastermError::Pty(e.to_string()))
    }

    /// Take the reader out of this Pty, transferring ownership (e.g. to a background thread).
    /// After calling this, `read()` will return an error.
    pub fn take_reader(&mut self) -> Option<Box<dyn Read + Send>> {
        self.reader.take()
    }

    /// Read data from the PTY. Returns an error if the reader has been taken.
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        match &mut self.reader {
            Some(r) => r.read(buf).map_err(|e| CastermError::Pty(e.to_string())),
            None => Err(CastermError::Pty("PTY reader has been moved to background thread".to_string())),
        }
    }

    /// Write data to the PTY
    pub fn write(&mut self, data: &[u8]) -> Result<usize> {
        self.writer
            .write(data)
            .map_err(|e| CastermError::Pty(e.to_string()))
    }

    /// Flush the PTY writer
    pub fn flush(&mut self) -> Result<()> {
        self.writer
            .flush()
            .map_err(|e| CastermError::Pty(e.to_string()))
    }

    /// Check if the child process has exited
    pub fn try_wait(&mut self) -> Result<Option<portable_pty::ExitStatus>> {
        self.child
            .try_wait()
            .map_err(|e| CastermError::Pty(e.to_string()))
    }

    /// Wait for the child process to exit
    pub fn wait(&mut self) -> Result<portable_pty::ExitStatus> {
        self.child
            .wait()
            .map_err(|e| CastermError::Pty(e.to_string()))
    }

    /// Kill the child process
    pub fn kill(&mut self) -> Result<()> {
        self.child
            .kill()
            .map_err(|e| CastermError::Pty(e.to_string()))
    }
}
