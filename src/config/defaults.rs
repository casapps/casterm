//! Default values and shell detection

use std::path::PathBuf;

/// Detect the default shell for this platform
pub fn detect_shell() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        // macOS: prefer Homebrew bash, then system default
        let homebrew_bash = PathBuf::from("/opt/homebrew/bin/bash");
        if homebrew_bash.exists() {
            return Some(homebrew_bash);
        }

        let intel_homebrew = PathBuf::from("/usr/local/bin/bash");
        if intel_homebrew.exists() {
            return Some(intel_homebrew);
        }

        // Fall through to Unix detection
    }

    #[cfg(unix)]
    {
        // Check $SHELL first
        if let Ok(shell) = std::env::var("SHELL") {
            let path = PathBuf::from(&shell);
            if path.exists() {
                return Some(path);
            }
        }

        // Check passwd entry
        if let Some(shell) = passwd_shell() {
            if shell.exists() {
                return Some(shell);
            }
        }

        // Fallback list
        for shell in &[
            "/bin/bash",
            "/usr/bin/bash",
            "/bin/zsh",
            "/usr/bin/zsh",
            "/usr/bin/fish",
            "/bin/sh",
        ] {
            let path = PathBuf::from(shell);
            if path.exists() {
                return Some(path);
            }
        }
    }

    #[cfg(windows)]
    {
        // Windows: Git Bash, PowerShell, cmd
        let git_bash_paths = [
            "C:\\Program Files\\Git\\bin\\bash.exe",
            "C:\\Program Files (x86)\\Git\\bin\\bash.exe",
        ];

        for path in &git_bash_paths {
            let p = PathBuf::from(path);
            if p.exists() {
                return Some(p);
            }
        }

        // PowerShell Core
        if let Ok(output) = std::process::Command::new("where").arg("pwsh").output() {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout);
                let first = path.lines().next().map(PathBuf::from);
                if let Some(p) = first {
                    if p.exists() {
                        return Some(p);
                    }
                }
            }
        }

        // Windows PowerShell
        let powershell = PathBuf::from(
            "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
        );
        if powershell.exists() {
            return Some(powershell);
        }

        // cmd.exe fallback
        let cmd = PathBuf::from("C:\\Windows\\System32\\cmd.exe");
        if cmd.exists() {
            return Some(cmd);
        }
    }

    None
}

#[cfg(unix)]
fn passwd_shell() -> Option<PathBuf> {
    use std::ffi::CStr;

    unsafe {
        let uid = libc::getuid();
        let passwd = libc::getpwuid(uid);
        if passwd.is_null() {
            return None;
        }

        let shell = (*passwd).pw_shell;
        if shell.is_null() {
            return None;
        }

        let shell_str = CStr::from_ptr(shell).to_string_lossy();
        Some(PathBuf::from(shell_str.as_ref()))
    }
}
