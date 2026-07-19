//! Terminal emulation core

/// Terminal dimensions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self { cols: 80, rows: 24 }
    }
}

/// Cursor position
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CursorPos {
    pub row: u16,
    pub col: u16,
}

/// Cursor style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorStyle {
    #[default]
    Block,
    Underline,
    Bar,
}

/// Terminal color: default, 256-color indexed, or 24-bit RGB
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TermColor {
    #[default]
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// Cell attributes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellAttrs {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub blink: bool,
    pub reverse: bool,
    pub hidden: bool,
    pub strikethrough: bool,
    pub fg: TermColor,
    pub bg: TermColor,
}

/// A single terminal cell
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cell {
    pub char: char,
    pub attrs: CellAttrs,
}

impl Cell {
    pub fn new(char: char) -> Self {
        Self {
            char,
            attrs: CellAttrs::default(),
        }
    }

    pub fn with_attrs(char: char, attrs: CellAttrs) -> Self {
        Self { char, attrs }
    }
}

/// Terminal grid (screen buffer)
pub struct Grid {
    cells: Vec<Cell>,
    size: TerminalSize,
}

impl Grid {
    pub fn new(size: TerminalSize) -> Self {
        let capacity = size.cols as usize * size.rows as usize;
        Self {
            cells: vec![Cell::default(); capacity],
            size,
        }
    }

    pub fn size(&self) -> TerminalSize {
        self.size
    }

    pub fn resize(&mut self, new_size: TerminalSize) {
        let capacity = new_size.cols as usize * new_size.rows as usize;
        self.cells.resize(capacity, Cell::default());
        self.size = new_size;
    }

    pub fn get(&self, row: u16, col: u16) -> Option<&Cell> {
        if row < self.size.rows && col < self.size.cols {
            let idx = row as usize * self.size.cols as usize + col as usize;
            self.cells.get(idx)
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, row: u16, col: u16) -> Option<&mut Cell> {
        if row < self.size.rows && col < self.size.cols {
            let idx = row as usize * self.size.cols as usize + col as usize;
            self.cells.get_mut(idx)
        } else {
            None
        }
    }

    pub fn set(&mut self, row: u16, col: u16, cell: Cell) {
        if let Some(c) = self.get_mut(row, col) {
            *c = cell;
        }
    }

    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            *cell = Cell::default();
        }
    }

    pub fn clear_row(&mut self, row: u16) {
        for col in 0..self.size.cols {
            if let Some(cell) = self.get_mut(row, col) {
                *cell = Cell::default();
            }
        }
    }

    /// Scroll the grid up by n lines
    pub fn scroll_up(&mut self, n: u16) {
        let n = n.min(self.size.rows) as usize;
        let row_size = self.size.cols as usize;

        // Shift cells up
        self.cells.copy_within(n * row_size.., 0);

        // Clear the bottom n rows
        let start = (self.size.rows as usize - n) * row_size;
        for cell in &mut self.cells[start..] {
            *cell = Cell::default();
        }
    }
}

/// Terminal state machine
pub struct Terminal {
    grid: Grid,
    alt_grid: Option<Grid>,
    cursor: CursorPos,
    cursor_style: CursorStyle,
    cursor_visible: bool,
    attrs: CellAttrs,
    title: String,
    // Scrollback buffer
    scrollback: Vec<Vec<Cell>>,
    scrollback_limit: usize,
}

impl Terminal {
    pub fn new(size: TerminalSize) -> Self {
        Self {
            grid: Grid::new(size),
            alt_grid: None,
            cursor: CursorPos::default(),
            cursor_style: CursorStyle::default(),
            cursor_visible: true,
            attrs: CellAttrs::default(),
            title: String::new(),
            scrollback: Vec::new(),
            scrollback_limit: 10000,
        }
    }

    pub fn size(&self) -> TerminalSize {
        self.grid.size()
    }

    pub fn resize(&mut self, new_size: TerminalSize) {
        self.grid.resize(new_size);
        if let Some(ref mut alt) = self.alt_grid {
            alt.resize(new_size);
        }
    }

    pub fn cursor(&self) -> CursorPos {
        self.cursor
    }

    pub fn cursor_style(&self) -> CursorStyle {
        self.cursor_style
    }

    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn grid(&self) -> &Grid {
        &self.grid
    }

    pub fn grid_mut(&mut self) -> &mut Grid {
        &mut self.grid
    }

    /// Get current cell attributes
    pub fn current_attrs(&self) -> &CellAttrs {
        &self.attrs
    }

    /// Write a character at the current cursor position
    pub fn write_char(&mut self, c: char) {
        match c {
            '\n' => self.newline(),
            '\r' => self.cursor.col = 0,
            '\t' => {
                // Move to next tab stop (every 8 columns)
                let next_tab = ((self.cursor.col / 8) + 1) * 8;
                self.cursor.col = next_tab.min(self.grid.size().cols - 1);
            }
            '\x08' => {
                // Backspace
                if self.cursor.col > 0 {
                    self.cursor.col -= 1;
                }
            }
            _ if !c.is_control() => {
                self.grid.set(
                    self.cursor.row,
                    self.cursor.col,
                    Cell::with_attrs(c, self.attrs),
                );
                self.cursor.col += 1;
                if self.cursor.col >= self.grid.size().cols {
                    self.newline();
                }
            }
            _ => {}
        }
    }

    /// Write a string
    pub fn write_str(&mut self, s: &str) {
        for c in s.chars() {
            self.write_char(c);
        }
    }

    fn newline(&mut self) {
        self.cursor.col = 0;
        if self.cursor.row + 1 >= self.grid.size().rows {
            self.scroll();
        } else {
            self.cursor.row += 1;
        }
    }

    fn scroll(&mut self) {
        // Save top line to scrollback
        let row_size = self.grid.size().cols as usize;
        let top_row: Vec<Cell> = (0..row_size)
            .map(|col| {
                self.grid
                    .get(0, col as u16)
                    .cloned()
                    .unwrap_or_default()
            })
            .collect();

        self.scrollback.push(top_row);

        // Trim scrollback if needed
        while self.scrollback.len() > self.scrollback_limit {
            self.scrollback.remove(0);
        }

        self.grid.scroll_up(1);
    }

    /// Enter alternate screen buffer
    pub fn enter_alt_screen(&mut self) {
        if self.alt_grid.is_none() {
            let mut alt = Grid::new(self.grid.size());
            std::mem::swap(&mut self.grid, &mut alt);
            self.alt_grid = Some(alt);
        }
    }

    /// Leave alternate screen buffer
    pub fn leave_alt_screen(&mut self) {
        if let Some(mut alt) = self.alt_grid.take() {
            std::mem::swap(&mut self.grid, &mut alt);
        }
    }

    /// Set cursor position (0-indexed)
    pub fn set_cursor(&mut self, row: u16, col: u16) {
        self.cursor.row = row.min(self.grid.size().rows.saturating_sub(1));
        self.cursor.col = col.min(self.grid.size().cols.saturating_sub(1));
    }

    /// Set cursor visibility
    pub fn set_cursor_visible(&mut self, visible: bool) {
        self.cursor_visible = visible;
    }

    /// Set cursor style
    pub fn set_cursor_style(&mut self, style: CursorStyle) {
        self.cursor_style = style;
    }

    /// Set terminal title
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    /// Clear the screen
    pub fn clear(&mut self) {
        self.grid.clear();
        self.cursor = CursorPos::default();
    }

    /// Set current cell attributes
    pub fn set_attrs(&mut self, attrs: CellAttrs) {
        self.attrs = attrs;
    }

    /// Reset cell attributes to default
    pub fn reset_attrs(&mut self) {
        self.attrs = CellAttrs::default();
    }
}
