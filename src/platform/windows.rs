//! Windows-specific platform code

use std::path::PathBuf;

/// Windows-specific platform utilities
pub struct Windows;

impl Windows {
    /// Check if running in Windows Terminal
    pub fn is_windows_terminal() -> bool {
        std::env::var("WT_SESSION").is_ok()
    }

    /// Check if running in ConPTY environment
    pub fn has_conpty() -> bool {
        // ConPTY is available on Windows 10 1809+
        // For now, assume it's available on modern Windows
        true
    }

    /// Get Git Bash path if installed
    pub fn git_bash() -> Option<PathBuf> {
        let paths = [
            PathBuf::from(r"C:\Program Files\Git\bin\bash.exe"),
            PathBuf::from(r"C:\Program Files (x86)\Git\bin\bash.exe"),
        ];

        for path in &paths {
            if path.exists() {
                return Some(path.clone());
            }
        }

        // Check PROGRAMFILES env var
        if let Ok(pf) = std::env::var("PROGRAMFILES") {
            let path = PathBuf::from(pf).join(r"Git\bin\bash.exe");
            if path.exists() {
                return Some(path);
            }
        }

        None
    }

    /// Get PowerShell Core path if installed
    pub fn pwsh() -> Option<PathBuf> {
        let paths = [
            PathBuf::from(r"C:\Program Files\PowerShell\7\pwsh.exe"),
            PathBuf::from(r"C:\Program Files\PowerShell\pwsh.exe"),
        ];

        for path in &paths {
            if path.exists() {
                return Some(path.clone());
            }
        }

        // Check if pwsh is in PATH
        which::which("pwsh").ok()
    }

    /// Get Windows PowerShell path
    pub fn powershell() -> PathBuf {
        PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe")
    }

    /// Get cmd.exe path
    pub fn cmd() -> PathBuf {
        PathBuf::from(r"C:\Windows\System32\cmd.exe")
    }

    /// Check if WSL is available
    pub fn has_wsl() -> bool {
        which::which("wsl").is_ok()
    }

    /// Get the Windows version (build number)
    pub fn build_number() -> Option<u32> {
        // Could use winapi/windows-sys here for proper detection
        // For now, return None
        None
    }
}
