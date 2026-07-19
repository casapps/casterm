//! Multiplexer: windows and panes

use std::collections::HashMap;

/// Unique window identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(u64);

impl WindowId {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

impl std::fmt::Display for WindowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique pane identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaneId(u64);

impl PaneId {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

impl std::fmt::Display for PaneId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Split direction for panes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

/// Layout for panes within a window
#[derive(Debug, Clone)]
pub enum Layout {
    Single(PaneId),
    Split {
        direction: SplitDirection,
        ratio: f32,
        first: Box<Layout>,
        second: Box<Layout>,
    },
}

/// A window containing panes
pub struct Window {
    id: WindowId,
    name: String,
    layout: Option<Layout>,
    panes: HashMap<PaneId, Pane>,
    active_pane: Option<PaneId>,
}

impl Window {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: WindowId::new(),
            name: name.into(),
            layout: None,
            panes: HashMap::new(),
            active_pane: None,
        }
    }

    pub fn id(&self) -> WindowId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    pub fn layout(&self) -> Option<&Layout> {
        self.layout.as_ref()
    }

    pub fn active_pane(&self) -> Option<PaneId> {
        self.active_pane
    }

    /// Create a new pane in this window
    pub fn create_pane(&mut self) -> PaneId {
        let pane = Pane::new();
        let id = pane.id();
        self.panes.insert(id, pane);

        // Update layout
        match &self.layout {
            None => {
                self.layout = Some(Layout::Single(id));
            }
            Some(_) => {
                // TODO: Split existing layout
            }
        }

        if self.active_pane.is_none() {
            self.active_pane = Some(id);
        }

        id
    }

    /// Split a pane
    pub fn split_pane(&mut self, pane_id: PaneId, direction: SplitDirection) -> Option<PaneId> {
        if !self.panes.contains_key(&pane_id) {
            return None;
        }

        let new_pane = Pane::new();
        let new_id = new_pane.id();
        self.panes.insert(new_id, new_pane);

        // Update layout to include the new split
        if let Some(layout) = self.layout.take() {
            self.layout = Some(self.insert_split(layout, pane_id, new_id, direction));
        }

        Some(new_id)
    }

    fn insert_split(
        &self,
        layout: Layout,
        target: PaneId,
        new_pane: PaneId,
        direction: SplitDirection,
    ) -> Layout {
        match layout {
            Layout::Single(id) if id == target => Layout::Split {
                direction,
                ratio: 0.5,
                first: Box::new(Layout::Single(id)),
                second: Box::new(Layout::Single(new_pane)),
            },
            Layout::Single(id) => Layout::Single(id),
            Layout::Split {
                direction: d,
                ratio,
                first,
                second,
            } => Layout::Split {
                direction: d,
                ratio,
                first: Box::new(self.insert_split(*first, target, new_pane, direction)),
                second: Box::new(self.insert_split(*second, target, new_pane, direction)),
            },
        }
    }

    /// Get a pane by ID
    pub fn get_pane(&self, id: PaneId) -> Option<&Pane> {
        self.panes.get(&id)
    }

    /// Get a mutable pane by ID
    pub fn get_pane_mut(&mut self, id: PaneId) -> Option<&mut Pane> {
        self.panes.get_mut(&id)
    }

    /// Set the active pane
    pub fn set_active_pane(&mut self, id: PaneId) {
        if self.panes.contains_key(&id) {
            self.active_pane = Some(id);
        }
    }

    /// Get all pane IDs
    pub fn pane_ids(&self) -> impl Iterator<Item = PaneId> + '_ {
        self.panes.keys().copied()
    }
}

/// A single pane within a window
pub struct Pane {
    id: PaneId,
    terminal: Option<super::terminal::Terminal>,
    title: String,
}

impl Pane {
    pub fn new() -> Self {
        Self {
            id: PaneId::new(),
            terminal: None,
            title: String::new(),
        }
    }

    pub fn id(&self) -> PaneId {
        self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    pub fn terminal(&self) -> Option<&super::terminal::Terminal> {
        self.terminal.as_ref()
    }

    pub fn terminal_mut(&mut self) -> Option<&mut super::terminal::Terminal> {
        self.terminal.as_mut()
    }

    pub fn set_terminal(&mut self, terminal: super::terminal::Terminal) {
        self.terminal = Some(terminal);
    }
}

impl Default for Pane {
    fn default() -> Self {
        Self::new()
    }
}
