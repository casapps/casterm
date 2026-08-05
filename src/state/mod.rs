//! State persistence for sessions and configuration

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::platform::Platform;
use crate::support::error::{CastermError, Result};

/// Persistent session state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub name: String,
    pub created_at: i64,
    pub last_attached: i64,
    pub windows: Vec<WindowState>,
}

/// Persistent window state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    pub name: String,
    pub index: usize,
    pub panes: Vec<PaneState>,
    pub layout: String,
}

/// Persistent pane state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneState {
    pub index: usize,
    pub cwd: Option<PathBuf>,
    pub command: Option<String>,
}

/// State manager for persisting sessions
pub struct StateManager {
    state_dir: PathBuf,
    sessions: HashMap<String, SessionState>,
}

impl StateManager {
    /// Create a new state manager backed by the platform's standard data
    /// directory (`Platform::data_dir()/sessions`).
    pub fn new() -> Result<Self> {
        let state_dir = Platform::data_dir()
            .ok_or_else(|| CastermError::Config("Cannot determine data directory".into()))?
            .join("sessions");
        Self::with_dir(state_dir)
    }

    /// Create a state manager backed by an arbitrary directory. Production
    /// code should use `new()`; this exists so tests can exercise
    /// save/load round-trips without touching the real platform data
    /// directory.
    pub fn with_dir(state_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&state_dir)?;

        let mut manager = Self {
            state_dir,
            sessions: HashMap::new(),
        };

        manager.load_all()?;
        tracing::debug!(dir = %manager.state_dir().display(), "session state directory ready");
        Ok(manager)
    }

    /// Load all saved sessions
    fn load_all(&mut self) -> Result<()> {
        let dir = match std::fs::read_dir(&self.state_dir) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };

        for entry in dir {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|e| e == "json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(state) = serde_json::from_str::<SessionState>(&content) {
                        self.sessions.insert(state.name.clone(), state);
                    }
                }
            }
        }

        Ok(())
    }

    /// Save a session state
    pub fn save_session(&mut self, state: SessionState) -> Result<()> {
        let path = self.state_dir.join(format!("{}.json", &state.name));
        let content = serde_json::to_string_pretty(&state)?;
        std::fs::write(&path, content)?;
        self.sessions.insert(state.name.clone(), state);
        Ok(())
    }

    /// Load a session state
    pub fn load_session(&self, name: &str) -> Option<&SessionState> {
        self.sessions.get(name)
    }

    /// Remove a session state
    pub fn remove_session(&mut self, name: &str) -> Result<()> {
        let path = self.state_dir.join(format!("{}.json", name));
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        self.sessions.remove(name);
        Ok(())
    }

    /// List all saved sessions
    pub fn list_sessions(&self) -> impl Iterator<Item = &SessionState> {
        self.sessions.values()
    }

    /// Get the state directory path
    pub fn state_dir(&self) -> &PathBuf {
        &self.state_dir
    }
}

/// History state for commands
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HistoryState {
    pub commands: Vec<String>,
    pub max_size: usize,
}

impl HistoryState {
    pub fn new(max_size: usize) -> Self {
        Self {
            commands: Vec::new(),
            max_size,
        }
    }

    pub fn add(&mut self, command: impl Into<String>) {
        let command = command.into();
        // Remove duplicates
        self.commands.retain(|c| c != &command);
        self.commands.push(command);
        // Trim to max size
        while self.commands.len() > self.max_size {
            self.commands.remove(0);
        }
    }

    pub fn search<'a, 'b>(
        &'a self,
        prefix: &'b str,
    ) -> impl Iterator<Item = &'a String> + use<'a, 'b> {
        self.commands
            .iter()
            .rev()
            .filter(move |c| c.starts_with(prefix))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_state_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "casterm-state-test-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ))
    }

    #[test]
    fn save_load_round_trip_preserves_window_pane_tree_and_cwd() {
        let dir = temp_state_dir("round-trip");
        let mut manager = StateManager::with_dir(dir.clone()).expect("state dir is creatable");

        let state = SessionState {
            name: "work".to_string(),
            created_at: 1000,
            last_attached: 2000,
            windows: vec![WindowState {
                name: "main".to_string(),
                index: 0,
                layout: "H0.5(S0|S1)".to_string(),
                panes: vec![
                    PaneState {
                        index: 0,
                        cwd: Some(PathBuf::from("/tmp/one")),
                        command: None,
                    },
                    PaneState {
                        index: 1,
                        cwd: Some(PathBuf::from("/tmp/two")),
                        command: Some("htop".to_string()),
                    },
                ],
            }],
        };

        manager.save_session(state.clone()).expect("save succeeds");

        // A fresh manager pointed at the same directory picks the session
        // back up from disk, mirroring a process restart.
        let reloaded = StateManager::with_dir(dir.clone()).expect("state dir reopens");
        let loaded = reloaded
            .load_session("work")
            .expect("saved session round-trips");

        assert_eq!(loaded.name, state.name);
        assert_eq!(loaded.last_attached, state.last_attached);
        assert_eq!(loaded.windows.len(), 1);
        assert_eq!(loaded.windows[0].layout, "H0.5(S0|S1)");
        assert_eq!(loaded.windows[0].panes.len(), 2);
        assert_eq!(
            loaded.windows[0].panes[0].cwd,
            Some(PathBuf::from("/tmp/one"))
        );
        assert_eq!(loaded.windows[0].panes[1].command, Some("htop".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_session_deletes_from_disk_and_memory() {
        let dir = temp_state_dir("remove");
        let mut manager = StateManager::with_dir(dir.clone()).expect("state dir is creatable");
        manager
            .save_session(SessionState {
                name: "scratch".to_string(),
                created_at: 0,
                last_attached: 0,
                windows: vec![],
            })
            .expect("save succeeds");

        manager.remove_session("scratch").expect("remove succeeds");
        assert!(manager.load_session("scratch").is_none());
        assert!(!dir.join("scratch.json").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
