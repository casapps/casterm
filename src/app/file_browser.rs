//! UI-agnostic state for the built-in local file browser (tree panel).
//!
//! This is deliberately front-end-independent: both the TUI
//! (`ui::tui::file_browser`) and GUI (`ui::gui::file_browser`) render the
//! same `FileBrowserState`, mirroring the existing `ui::render_model`
//! pattern of "shared core, front-end-specific rendering". See
//! `.claude/plans/inherited-painting-lark.md` for the full feature plan.
//!
//! Rendering/consumption is wired in Phase 2 (TUI, `ui::tui::file_browser`)
//! and Phase 3 (GUI). `ViewerContent::Editor` and `open_for_edit` remain
//! unused until Phase 4 wires the built-in editor in; see the `dead_code`
//! allow below on just that part.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::support::error::Result;

/// Classification of a file, used to decide what opening it should do:
/// directories expand/collapse in the tree, text opens the built-in editor,
/// images open the built-in viewer (GUI only — see the plan's TUI/GUI
/// asymmetry note), and everything else hands off to the OS default app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Directory,
    Text,
    Image,
    Other,
}

/// Extensions treated as images, matching the `image` crate's currently
/// enabled Cargo features (`png`, `jpeg`, `gif`).
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif"];

/// Number of leading bytes sniffed to decide text vs. binary when the
/// extension alone doesn't already say "image". Extension-only detection is
/// unreliable for arbitrary files (no extension, misleading extension), so
/// content is sniffed as a fallback.
const SNIFF_BYTES: usize = 512;

/// Classify a path for file-browser "open" behavior. Directories are
/// classified without touching the filesystem beyond `is_dir`; files are
/// classified by extension first (images), then by a content sniff (text
/// vs. other).
pub fn classify_path(path: &Path) -> FileKind {
    if path.is_dir() {
        return FileKind::Directory;
    }

    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_ascii_lowercase();
        if IMAGE_EXTENSIONS.contains(&ext_lower.as_str()) {
            return FileKind::Image;
        }
    }

    match std::fs::read(path) {
        Ok(bytes) => {
            let sample = &bytes[..bytes.len().min(SNIFF_BYTES)];
            if is_probably_text(sample) {
                FileKind::Text
            } else {
                FileKind::Other
            }
        }
        Err(_) => FileKind::Other,
    }
}

/// Heuristic: a null byte anywhere in the sample means binary; otherwise
/// the sample must be valid UTF-8 to count as text.
fn is_probably_text(sample: &[u8]) -> bool {
    if sample.contains(&0) {
        return false;
    }
    std::str::from_utf8(sample).is_ok()
}

/// One row in the flattened, currently-visible tree listing.
#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub depth: usize,
}

/// State for the tree-style file-browser panel: root directory, which
/// directories are expanded, the flattened visible-row list (rebuilt on
/// every expand/collapse/navigate), and the current selection/scroll
/// position.
pub struct FileBrowserState {
    root: PathBuf,
    expanded: HashSet<PathBuf>,
    show_hidden: bool,
    entries: Vec<TreeEntry>,
    selected: usize,
    scroll_offset: usize,
}

impl FileBrowserState {
    /// Create a new browser rooted at `root` (typically the session's
    /// current working directory), with the root itself expanded so the
    /// panel isn't empty on open.
    pub fn new(root: PathBuf, show_hidden: bool) -> Self {
        let mut state = Self {
            root: root.clone(),
            expanded: HashSet::new(),
            show_hidden,
            entries: Vec::new(),
            selected: 0,
            scroll_offset: 0,
        };
        state.expanded.insert(root);
        state.rebuild();
        state
    }

    /// Unused until the Phase 3 GUI panel (which needs the root to render a
    /// breadcrumb/header) lands; the TUI panel doesn't display it.
    #[allow(dead_code)]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn entries(&self) -> &[TreeEntry] {
        &self.entries
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn selected_entry(&self) -> Option<&TreeEntry> {
        self.entries.get(self.selected)
    }

    /// Whether `path` (a directory) is currently expanded in the tree.
    pub fn is_expanded(&self, path: &Path) -> bool {
        self.expanded.contains(path)
    }

    /// Rebuild the flattened visible-row list from `root` and `expanded`.
    /// Only one directory level is walked per expanded directory (not an
    /// eager recursive walk), so large trees stay responsive.
    fn rebuild(&mut self) {
        self.entries.clear();
        let root = self.root.clone();
        self.walk_dir(&root, 0);
        if self.selected >= self.entries.len() {
            self.selected = self.entries.len().saturating_sub(1);
        }
    }

    fn walk_dir(&mut self, dir: &Path, depth: usize) {
        let mut children: Vec<(PathBuf, bool)> = match std::fs::read_dir(dir) {
            Ok(read) => read
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let path = e.path();
                    let is_dir = e.file_type().ok()?.is_dir();
                    Some((path, is_dir))
                })
                .collect(),
            Err(_) => Vec::new(),
        };

        if !self.show_hidden {
            children.retain(|(path, _)| {
                !path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with('.'))
            });
        }

        // Directories first, then files, both alphabetically — matches the
        // conventional NERDTree/tmux-sidebar tree ordering.
        children.sort_by(|(a_path, a_dir), (b_path, b_dir)| {
            b_dir
                .cmp(a_dir)
                .then_with(|| a_path.file_name().cmp(&b_path.file_name()))
        });

        for (path, is_dir) in children {
            self.entries.push(TreeEntry {
                path: path.clone(),
                is_dir,
                depth,
            });
            if is_dir && self.expanded.contains(&path) {
                self.walk_dir(&path, depth + 1);
            }
        }
    }

    /// Move the selection down/up by `delta` rows, clamped to the visible
    /// row list.
    pub fn move_selection(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        let len = self.entries.len() as isize;
        let mut idx = self.selected as isize + delta;
        idx = idx.clamp(0, len - 1);
        self.selected = idx as usize;
    }

    /// Toggle expand/collapse on the currently selected directory. No-op on
    /// a file selection.
    pub fn toggle_selected(&mut self) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        if !entry.is_dir {
            return;
        }
        let path = entry.path.clone();
        if self.expanded.contains(&path) {
            self.expanded.remove(&path);
        } else {
            self.expanded.insert(path);
        }
        self.rebuild();
    }

    /// Reload the tree from disk (e.g. after an external filesystem change),
    /// preserving the current expand set and selection index.
    pub fn refresh(&mut self) {
        self.rebuild();
    }

    pub fn set_scroll_offset(&mut self, offset: usize) {
        self.scroll_offset = offset;
    }
}

/// Content currently shown in the file-browser panel: the tree itself, or —
/// once a file has been opened — the built-in editor/viewer for it.
///
/// Unused until Phase 4 (`.claude/plans/inherited-painting-lark.md`) wires
/// the built-in editor into the TUI; both front ends currently hand every
/// non-directory entry off to the OS default application (Phase 2/3).
#[allow(dead_code)]
pub enum ViewerContent {
    Tree,
    Editor(super::editor::EditorState),
}

#[allow(dead_code)]
impl ViewerContent {
    pub fn is_tree(&self) -> bool {
        matches!(self, ViewerContent::Tree)
    }
}

/// Open `path` for editing, loading it into a fresh `EditorState`. Unused
/// until Phase 4/5 wire the built-in editor into a front end.
#[allow(dead_code)]
pub fn open_for_edit(path: &Path) -> Result<super::editor::EditorState> {
    super::editor::EditorState::load(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn classify_text_file() {
        let dir = tempfile_dir();
        let path = dir.join("a.txt");
        fs::write(&path, b"hello world\n").unwrap();
        assert_eq!(classify_path(&path), FileKind::Text);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn classify_image_by_extension() {
        let dir = tempfile_dir();
        let path = dir.join("a.png");
        // Real PNG magic bytes, no need for a full valid PNG for classification.
        fs::write(&path, [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]).unwrap();
        assert_eq!(classify_path(&path), FileKind::Image);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn classify_binary_as_other() {
        let dir = tempfile_dir();
        let path = dir.join("a.bin");
        fs::write(&path, [0u8, 1, 2, 3, 255, 254]).unwrap();
        assert_eq!(classify_path(&path), FileKind::Other);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn classify_directory() {
        let dir = tempfile_dir();
        assert_eq!(classify_path(&dir), FileKind::Directory);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn expand_collapse_updates_visible_entries() {
        let dir = tempfile_dir();
        let sub = dir.join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("inner.txt"), b"x").unwrap();
        fs::write(dir.join("top.txt"), b"x").unwrap();

        let mut state = FileBrowserState::new(dir.clone(), false);
        // Root starts expanded: "sub" dir + "top.txt" file, sub not expanded yet.
        assert_eq!(state.entries().len(), 2);

        // Select "sub" (directories sort first) and expand it.
        state.selected = 0;
        assert!(state.selected_entry().unwrap().is_dir);
        state.toggle_selected();
        assert_eq!(state.entries().len(), 3);

        state.toggle_selected();
        assert_eq!(state.entries().len(), 2);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn move_selection_clamps_to_bounds() {
        let dir = tempfile_dir();
        fs::write(dir.join("a.txt"), b"x").unwrap();
        fs::write(dir.join("b.txt"), b"x").unwrap();
        let mut state = FileBrowserState::new(dir.clone(), false);

        state.move_selection(-5);
        assert_eq!(state.selected_index(), 0);
        state.move_selection(5);
        assert_eq!(state.selected_index(), state.entries().len() - 1);

        fs::remove_dir_all(&dir).unwrap();
    }

    fn tempfile_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "casterm-file-browser-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
