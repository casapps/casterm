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
    /// Create a new state manager
    pub fn new() -> Result<Self> {
        let state_dir = Platform::data_dir()
            .ok_or_else(|| CastermError::Config("Cannot determine data directory".into()))?
            .join("sessions");

        std::fs::create_dir_all(&state_dir)?;

        let mut manager = Self {
            state_dir,
            sessions: HashMap::new(),
        };

        manager.load_all()?;
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

    pub fn search<'a, 'b>(&'a self, prefix: &'b str) -> impl Iterator<Item = &'a String> + use<'a, 'b> {
        self.commands
            .iter()
            .rev()
            .filter(move |c| c.starts_with(prefix))
    }
}
