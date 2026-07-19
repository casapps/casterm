//! Core application logic

pub mod multiplexer;
pub mod pty;
pub mod serial;
pub mod session;
pub mod ssh;
pub mod terminal;
pub mod vte_processor;

use crate::config::Config;
use crate::support::error::Result;

/// Core application state
pub struct App {
    config: Config,
    sessions: session::SessionManager,
}

impl App {
    /// Create a new application instance
    pub fn new(config: Config) -> Result<Self> {
        Ok(Self {
            config,
            sessions: session::SessionManager::new(),
        })
    }

    /// Get the configuration
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Get mutable configuration
    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    /// Get the session manager
    pub fn sessions(&self) -> &session::SessionManager {
        &self.sessions
    }

    /// Get mutable session manager
    pub fn sessions_mut(&mut self) -> &mut session::SessionManager {
        &mut self.sessions
    }
}
