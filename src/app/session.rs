//! Session management

use std::collections::HashMap;

use crate::support::error::Result;

/// Unique session identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId(u64);

impl SessionId {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Session state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Active,
    Detached,
    Dead,
}

/// A terminal session
pub struct Session {
    id: SessionId,
    name: String,
    state: SessionState,
    windows: Vec<super::multiplexer::WindowId>,
    active_window: Option<super::multiplexer::WindowId>,
}

impl Session {
    /// Create a new session
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: SessionId::new(),
            name: name.into(),
            state: SessionState::Active,
            windows: Vec::new(),
            active_window: None,
        }
    }

    pub fn id(&self) -> SessionId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    pub fn set_state(&mut self, state: SessionState) {
        self.state = state;
    }

    pub fn windows(&self) -> &[super::multiplexer::WindowId] {
        &self.windows
    }

    pub fn active_window(&self) -> Option<super::multiplexer::WindowId> {
        self.active_window
    }

    pub fn add_window(&mut self, window_id: super::multiplexer::WindowId) {
        self.windows.push(window_id);
        if self.active_window.is_none() {
            self.active_window = Some(window_id);
        }
    }

    pub fn set_active_window(&mut self, window_id: super::multiplexer::WindowId) {
        if self.windows.contains(&window_id) {
            self.active_window = Some(window_id);
        }
    }
}

/// Session manager
pub struct SessionManager {
    sessions: HashMap<SessionId, Session>,
    active_session: Option<SessionId>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            active_session: None,
        }
    }

    /// Create a new session
    pub fn create(&mut self, name: impl Into<String>) -> SessionId {
        let session = Session::new(name);
        let id = session.id();
        self.sessions.insert(id, session);
        if self.active_session.is_none() {
            self.active_session = Some(id);
        }
        id
    }

    /// Get a session by ID
    pub fn get(&self, id: SessionId) -> Option<&Session> {
        self.sessions.get(&id)
    }

    /// Get a mutable session by ID
    pub fn get_mut(&mut self, id: SessionId) -> Option<&mut Session> {
        self.sessions.get_mut(&id)
    }

    /// Get session by name
    pub fn find_by_name(&self, name: &str) -> Option<&Session> {
        self.sessions.values().find(|s| s.name() == name)
    }

    /// List all sessions
    pub fn list(&self) -> impl Iterator<Item = &Session> {
        self.sessions.values()
    }

    /// Get the active session
    pub fn active(&self) -> Option<&Session> {
        self.active_session.and_then(|id| self.sessions.get(&id))
    }

    /// Set the active session
    pub fn set_active(&mut self, id: SessionId) -> Result<()> {
        if self.sessions.contains_key(&id) {
            self.active_session = Some(id);
            Ok(())
        } else {
            Err(crate::support::error::CastermError::Session(
                format!("Session {} not found", id),
            ))
        }
    }

    /// Remove a session
    pub fn remove(&mut self, id: SessionId) -> Option<Session> {
        let session = self.sessions.remove(&id);
        if self.active_session == Some(id) {
            self.active_session = self.sessions.keys().next().copied();
        }
        session
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
