//! Core application logic

pub mod editor;
pub mod file_browser;
pub mod keybindings;
pub mod multiplexer;
pub mod pane_runtime;
pub mod pty;
pub mod serial;
pub mod serial_transport;
pub mod session;
pub mod ssh;
pub mod ssh_transport;
pub mod terminal;
pub mod vte_processor;

use crate::config::Config;
use crate::state::StateManager;
use crate::support::error::Result;

/// Core application state
pub struct App {
    config: Config,
    sessions: session::SessionManager,
    state: StateManager,
}

impl App {
    /// Create a new application instance
    pub fn new(config: Config) -> Result<Self> {
        Ok(Self {
            config,
            sessions: session::SessionManager::new(),
            state: StateManager::new()?,
        })
    }

    /// Get the configuration
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Get the session manager
    pub fn sessions(&self) -> &session::SessionManager {
        &self.sessions
    }

    /// Get mutable session manager
    pub fn sessions_mut(&mut self) -> &mut session::SessionManager {
        &mut self.sessions
    }

    /// Get the persisted-session state manager
    pub fn state(&self) -> &StateManager {
        &self.state
    }

    /// Get the mutable persisted-session state manager
    pub fn state_mut(&mut self) -> &mut StateManager {
        &mut self.state
    }
}
