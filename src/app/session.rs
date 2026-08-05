//! Session management

use std::collections::HashMap;

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

/// A terminal session.
///
/// Phase 2 scoped the multiplexer to a single window per session (MVP); this
/// owns that `Window` directly rather than the earlier dangling
/// `Vec<WindowId>` design, which had no registry anywhere mapping a
/// `WindowId` back to a live `Window`. Real multi-window sessions are a
/// follow-up once the multiplexer grows window collections.
pub struct Session {
    id: SessionId,
    name: String,
    state: SessionState,
    window: super::multiplexer::Window,
}

impl Session {
    /// Create a session wrapping an already-constructed window (used when
    /// restoring a saved session).
    pub fn with_window(name: impl Into<String>, window: super::multiplexer::Window) -> Self {
        Self {
            id: SessionId::new(),
            name: name.into(),
            state: SessionState::Active,
            window,
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

    pub fn window(&self) -> &super::multiplexer::Window {
        &self.window
    }

    pub fn window_mut(&mut self) -> &mut super::multiplexer::Window {
        &mut self.window
    }
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("state", &self.state)
            .finish_non_exhaustive()
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

    /// Insert an already-constructed session (e.g. one restored from
    /// `state::SessionState` with its window/pane tree pre-populated, or a
    /// fresh `Session::with_window(..)`).
    pub fn insert(&mut self, session: Session) -> SessionId {
        let id = session.id();
        self.sessions.insert(id, session);
        if self.active_session.is_none() {
            self.active_session = Some(id);
        }
        id
    }

    /// Get the active session
    pub fn active(&self) -> Option<&Session> {
        self.active_session.and_then(|id| self.sessions.get(&id))
    }

    /// Get the active session, mutably
    pub fn active_mut(&mut self) -> Option<&mut Session> {
        let id = self.active_session?;
        self.sessions.get_mut(&id)
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
