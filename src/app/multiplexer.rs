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

    /// Numeric value backing this id, used for a true numeric ordering
    /// (string-based sorting would put "10" before "9").
    pub(crate) fn value(self) -> u64 {
        self.0
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

impl Layout {
    /// Serialize into a compact string using pane *index* positions
    /// (matching `Window::pane_ids_sorted`'s order) rather than the live
    /// `PaneId`, since `PaneId`s are an in-process atomic counter and don't
    /// survive a restart. Format: `S<idx>` for a leaf, `H<ratio>(a|b)` /
    /// `V<ratio>(a|b)` for a horizontal/vertical split.
    pub fn encode(&self, index_of: &HashMap<PaneId, usize>) -> String {
        match self {
            Layout::Single(id) => format!("S{}", index_of.get(id).copied().unwrap_or(0)),
            Layout::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let d = match direction {
                    SplitDirection::Horizontal => 'H',
                    SplitDirection::Vertical => 'V',
                };
                format!(
                    "{d}{ratio}({}|{})",
                    first.encode(index_of),
                    second.encode(index_of)
                )
            }
        }
    }

    /// Reconstruct a `Layout` from `encode`'s output, mapping saved pane
    /// indices back onto freshly created `PaneId`s (`pane_ids[index]`).
    /// Returns `None` on any malformed input rather than panicking, since
    /// this parses a value read back from an on-disk session file.
    pub fn decode(s: &str, pane_ids: &[PaneId]) -> Option<Layout> {
        let (layout, rest) = Self::decode_inner(s, pane_ids)?;
        if rest.is_empty() {
            Some(layout)
        } else {
            None
        }
    }

    fn decode_inner<'a>(s: &'a str, pane_ids: &[PaneId]) -> Option<(Layout, &'a str)> {
        let tag = s.as_bytes().first().copied()?;
        match tag {
            b'S' => {
                let digits_end = s[1..]
                    .find(|c: char| !c.is_ascii_digit())
                    .map(|i| i + 1)
                    .unwrap_or(s.len());
                let idx: usize = s[1..digits_end].parse().ok()?;
                let pane_id = *pane_ids.get(idx)?;
                Some((Layout::Single(pane_id), &s[digits_end..]))
            }
            b'H' | b'V' => {
                let direction = if tag == b'H' {
                    SplitDirection::Horizontal
                } else {
                    SplitDirection::Vertical
                };
                let rest = &s[1..];
                let paren = rest.find('(')?;
                let ratio: f32 = rest[..paren].parse().ok()?;
                let rest = &rest[paren + 1..];
                let (first, rest) = Self::decode_inner(rest, pane_ids)?;
                let rest = rest.strip_prefix('|')?;
                let (second, rest) = Self::decode_inner(rest, pane_ids)?;
                let rest = rest.strip_prefix(')')?;
                Some((
                    Layout::Split {
                        direction,
                        ratio,
                        first: Box::new(first),
                        second: Box::new(second),
                    },
                    rest,
                ))
            }
            _ => None,
        }
    }
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

    /// Pane IDs in a stable, deterministic order. `PaneId`s are assigned by
    /// an in-process atomic counter, so this order is only stable within a
    /// single run — it exists so save/restore (`state::SessionState`) and
    /// the TUI's pane-cycling can agree on "pane index N" without either
    /// side needing to invent its own ordering.
    pub fn pane_ids_sorted(&self) -> Vec<PaneId> {
        let mut ids: Vec<PaneId> = self.panes.keys().copied().collect();
        ids.sort_by_key(|id| id.value());
        ids
    }

    /// Number of panes currently in this window
    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }

    /// Insert `count` fresh, blank panes without touching the layout tree.
    /// `create_pane`/`split_pane` both mutate `self.layout` on every call,
    /// which is wrong when restoring a saved session: the on-disk
    /// `Layout::encode`d tree (decoded via `Layout::decode`) already
    /// describes the arrangement, and `set_layout` installs it directly.
    /// Returns the new pane IDs in creation order, matching the order
    /// `state::WindowState.panes` was saved in.
    pub fn restore_panes(&mut self, count: usize) -> Vec<PaneId> {
        (0..count)
            .map(|_| {
                let pane = Pane::new();
                let id = pane.id();
                self.panes.insert(id, pane);
                id
            })
            .collect()
    }

    /// Install a layout tree directly (used when restoring a saved
    /// session). `active` becomes the active pane if it names a pane that
    /// actually exists in this window; otherwise an arbitrary existing pane
    /// (if any) is chosen.
    pub fn set_layout(&mut self, layout: Layout, active: Option<PaneId>) {
        self.layout = Some(layout);
        self.active_pane = active
            .filter(|id| self.panes.contains_key(id))
            .or_else(|| self.panes.keys().next().copied());
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

    #[test]
    fn layout_encode_decode_round_trips_a_single_pane() {
        let mut window = Window::new("main");
        window.create_pane();
        let ordered = window.pane_ids_sorted();
        let index_of: HashMap<PaneId, usize> =
            ordered.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        let encoded = window.layout().unwrap().encode(&index_of);
        let decoded = Layout::decode(&encoded, &ordered).expect("valid encoding decodes");
        assert!(matches!(decoded, Layout::Single(id) if id == ordered[0]));
    }

    #[test]
    fn layout_encode_decode_round_trips_a_split_tree() {
        let mut window = Window::new("main");
        let first = window.create_pane();
        window.split_pane(first, SplitDirection::Vertical);
        let ordered = window.pane_ids_sorted();
        let index_of: HashMap<PaneId, usize> =
            ordered.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        let encoded = window.layout().unwrap().encode(&index_of);

        // Decoding maps saved indices onto a *fresh* set of PaneIds (as a
        // restart would produce), not the original ones.
        let restored_ids: Vec<PaneId> = (0..ordered.len()).map(|_| Pane::new().id()).collect();
        let decoded = Layout::decode(&encoded, &restored_ids).expect("valid encoding decodes");
        match decoded {
            Layout::Split {
                direction,
                first,
                second,
                ..
            } => {
                assert_eq!(direction, SplitDirection::Vertical);
                assert!(matches!(*first, Layout::Single(id) if id == restored_ids[0]));
                assert!(matches!(*second, Layout::Single(id) if id == restored_ids[1]));
            }
            other => panic!("expected a Split layout, got {other:?}"),
        }
    }

    #[test]
    fn layout_decode_rejects_malformed_input() {
        let ids = [PaneId::new()];
        assert!(Layout::decode("garbage", &ids).is_none());
        assert!(Layout::decode("H0.5(S0|S1", &ids).is_none());
        assert!(Layout::decode("S99", &ids).is_none());
    }
}
