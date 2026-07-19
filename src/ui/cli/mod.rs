//! CLI mode for non-interactive use

use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::support::error::Result;

fn default_shell_fallback() -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from("cmd.exe")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/bin/sh")
    }
}

/// Run in CLI mode (non-interactive)
pub fn run(_config: &Config, command: &Option<String>, directory: Option<&Path>) -> Result<()> {
    tracing::info!("Starting CLI mode");

    // If a command is specified, execute it
    if let Some(cmd) = command {
        return execute_command(cmd, directory);
    }

    // Otherwise show help/status
    println!("CASTERM {}", crate::build_info::VERSION);
    println!();
    println!("Modern terminal emulator, multiplexer, and connection manager");
    println!();
    println!("Usage:");
    println!("  casterm                   Start terminal (auto-detects GUI/TUI)");
    println!("  casterm -e <cmd>          Execute command");
    println!("  casterm -s <name>         Create or attach to session");
    println!("  casterm --attach          Attach to existing session");
    println!();
    println!("Run 'casterm --help' for full options.");

    Ok(())
}

fn execute_command(command: &str, directory: Option<&Path>) -> Result<()> {
    use std::process::Command;

    let fallback_shell = default_shell_fallback();
    let shell = crate::config::detect_shell().unwrap_or(fallback_shell);

    let mut cmd = Command::new(&shell);

    #[cfg(unix)]
    cmd.arg("-c").arg(command);
    #[cfg(windows)]
    cmd.arg("/C").arg(command);
    #[cfg(not(any(unix, windows)))]
    cmd.arg("-c").arg(command);

    if let Some(dir) = directory {
        cmd.current_dir(dir);
    }

    let status = cmd.status()?;

    std::process::exit(status.code().unwrap_or(1));
}
