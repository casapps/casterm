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

    /// Create a new pane in this window. If a layout already exists, the
    /// new pane is split in next to the currently active pane (defaulting
    /// to a horizontal split) rather than being silently dropped from the
    /// layout tree.
    pub fn create_pane(&mut self) -> PaneId {
        let pane = Pane::new();
        let id = pane.id();
        self.panes.insert(id, pane);

        match self.layout.take() {
            None => {
                self.layout = Some(Layout::Single(id));
            }
            Some(layout) => {
                let target = self.active_pane.unwrap_or(id);
                self.layout =
                    Some(self.insert_split(layout, target, id, SplitDirection::Horizontal));
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

    /// Number of panes currently in this window
    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }

    /// Remove a pane, collapsing the layout tree around it. If the removed
    /// pane was active, an arbitrary remaining pane (if any) becomes active.
    pub fn remove_pane(&mut self, id: PaneId) {
        self.panes.remove(&id);
        if let Some(layout) = self.layout.take() {
            self.layout = Self::remove_from_layout(layout, id);
        }
        if self.active_pane == Some(id) {
            self.active_pane = self.panes.keys().next().copied();
        }
    }

    fn remove_from_layout(layout: Layout, target: PaneId) -> Option<Layout> {
        match layout {
            Layout::Single(id) if id == target => None,
            Layout::Single(id) => Some(Layout::Single(id)),
            Layout::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let first = Self::remove_from_layout(*first, target);
                let second = Self::remove_from_layout(*second, target);
                match (first, second) {
                    (Some(f), Some(s)) => Some(Layout::Split {
                        direction,
                        ratio,
                        first: Box::new(f),
                        second: Box::new(s),
                    }),
                    (Some(f), None) => Some(f),
                    (None, Some(s)) => Some(s),
                    (None, None) => None,
                }
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_create_pane_produces_single_layout() {
        let mut window = Window::new("main");
        let id = window.create_pane();
        assert!(matches!(window.layout(), Some(Layout::Single(pane)) if *pane == id));
        assert_eq!(window.active_pane(), Some(id));
        assert_eq!(window.pane_count(), 1);
    }

    #[test]
    fn second_create_pane_splits_existing_layout_instead_of_dropping_it() {
        let mut window = Window::new("main");
        let first = window.create_pane();
        let second = window.create_pane();
        assert_eq!(window.pane_count(), 2);
        match window.layout() {
            Some(Layout::Split {
                first: f,
                second: s,
                ..
            }) => {
                assert!(matches!(**f, Layout::Single(id) if id == first));
                assert!(matches!(**s, Layout::Single(id) if id == second));
            }
            other => panic!("expected a Split layout, got {other:?}"),
        }
    }

    #[test]
    fn split_pane_creates_split_layout_with_requested_direction() {
        let mut window = Window::new("main");
        let first = window.create_pane();
        let second = window
            .split_pane(first, SplitDirection::Vertical)
            .expect("split against an existing pane succeeds");
        match window.layout() {
            Some(Layout::Split { direction, .. }) => {
                assert_eq!(*direction, SplitDirection::Vertical);
            }
            other => panic!("expected a Split layout, got {other:?}"),
        }
        assert_eq!(window.pane_count(), 2);
        assert!(window.get_pane(second).is_some());
    }

    #[test]
    fn remove_pane_collapses_layout_to_surviving_sibling() {
        let mut window = Window::new("main");
        let first = window.create_pane();
        let second = window.create_pane();
        window.remove_pane(second);
        assert!(matches!(window.layout(), Some(Layout::Single(id)) if *id == first));
        assert_eq!(window.pane_count(), 1);
    }

    #[test]
    fn removing_active_pane_promotes_a_remaining_pane_to_active() {
        let mut window = Window::new("main");
        let first = window.create_pane();
        let second = window.create_pane();
        window.set_active_pane(second);
        window.remove_pane(second);
        assert_eq!(window.active_pane(), Some(first));
    }

    #[test]
    fn removing_last_pane_leaves_an_empty_layout() {
        let mut window = Window::new("main");
        let id = window.create_pane();
        window.remove_pane(id);
        assert!(window.layout().is_none());
        assert_eq!(window.active_pane(), None);
        assert_eq!(window.pane_count(), 0);
    }
}
