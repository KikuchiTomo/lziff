use crate::diff::{Diff, RowKind};
use crate::source::{DiffPayload, DiffSource, Entry};
use anyhow::Result;
use ratatui::layout::Rect;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Files,
    Diff,
}

/// Layout rectangles captured during the last render so input handlers
/// can hit-test mouse events without re-running the layout calculation.
#[derive(Default, Clone, Copy)]
pub struct LayoutCache {
    pub files_inner: Rect,
    pub diff_inner: Rect,
    pub diff_top_row: usize,
}

pub struct App {
    pub source: Box<dyn DiffSource>,
    pub entries: Vec<Entry>,
    pub selected: usize,
    pub diff: Diff,
    pub payload: Option<DiffPayload>,
    pub cursor_row: usize,
    pub focus: Focus,
    pub show_help: bool,
    pub status: String,
    pub layout: LayoutCache,
    last_signature: u64,
}

impl App {
    pub fn new(source: Box<dyn DiffSource>) -> Result<Self> {
        let entries = source.list()?;
        let signature = source.signature(entries.first().map(|e| e.id.as_str()));
        let mut app = Self {
            source,
            entries,
            selected: 0,
            diff: Diff::default(),
            payload: None,
            cursor_row: 0,
            focus: Focus::Files,
            show_help: false,
            status: String::new(),
            layout: LayoutCache::default(),
            last_signature: signature,
        };
        app.reload_diff();
        Ok(app)
    }

    pub fn reload_entries(&mut self) {
        let prev_id = self.entries.get(self.selected).map(|e| e.id.clone());
        match self.source.list() {
            Ok(entries) => {
                self.entries = entries;
                self.selected = prev_id
                    .as_deref()
                    .and_then(|id| self.entries.iter().position(|e| e.id == id))
                    .unwrap_or(0);
                if self.selected >= self.entries.len() && !self.entries.is_empty() {
                    self.selected = self.entries.len() - 1;
                }
                self.reload_diff_keep_view();
            }
            Err(e) => self.status = format!("reload failed: {e}"),
        }
    }

    pub fn reload_diff(&mut self) {
        self.cursor_row = 0;
        self.reload_diff_inner();
    }

    fn reload_diff_keep_view(&mut self) {
        let prev = self.cursor_row;
        self.reload_diff_inner();
        let max = self.diff.rows.len().saturating_sub(1);
        self.cursor_row = prev.min(max);
    }

    fn reload_diff_inner(&mut self) {
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            self.diff = Diff::default();
            self.payload = None;
            return;
        };
        match self.source.load(&entry.id) {
            Ok(payload) => {
                self.diff = Diff::compute(&payload.left, &payload.right);
                self.payload = Some(payload);
            }
            Err(e) => {
                self.diff = Diff::default();
                self.payload = None;
                self.status = format!("load failed: {e}");
            }
        }
    }

    /// Called every tick. If the underlying source changed (file mtime, git index, etc.),
    /// reload entries and the current diff while preserving cursor position.
    pub fn poll_changes(&mut self) {
        let id = self.entries.get(self.selected).map(|e| e.id.clone());
        let sig = self.source.signature(id.as_deref());
        if sig == self.last_signature {
            return;
        }
        self.last_signature = sig;
        self.reload_entries();
        self.status = "auto-reloaded".into();
    }

    pub fn select_next(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.entries.len();
        self.reload_diff();
    }

    pub fn select_prev(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected = if self.selected == 0 {
            self.entries.len() - 1
        } else {
            self.selected - 1
        };
        self.reload_diff();
    }

    pub fn select_index(&mut self, idx: usize) {
        if idx < self.entries.len() && idx != self.selected {
            self.selected = idx;
            self.reload_diff();
        }
    }

    pub fn cursor_down(&mut self, n: usize) {
        let max = self.diff.rows.len().saturating_sub(1);
        self.cursor_row = (self.cursor_row + n).min(max);
    }

    pub fn cursor_up(&mut self, n: usize) {
        self.cursor_row = self.cursor_row.saturating_sub(n);
    }

    pub fn next_hunk(&mut self) {
        if let Some(&(s, _)) = self
            .diff
            .hunks
            .iter()
            .find(|(s, _)| *s > self.cursor_row)
        {
            self.cursor_row = s;
        }
    }

    pub fn prev_hunk(&mut self) {
        if let Some(&(s, _)) = self
            .diff
            .hunks
            .iter()
            .rev()
            .find(|(_, e)| *e <= self.cursor_row)
        {
            self.cursor_row = s;
        }
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Files => Focus::Diff,
            Focus::Diff => Focus::Files,
        };
    }

    pub fn header_label(&self) -> (String, String) {
        if let Some(p) = &self.payload {
            (p.left_label.clone(), p.right_label.clone())
        } else {
            (String::new(), String::new())
        }
    }

    pub fn change_summary(&self) -> (usize, usize, usize) {
        let mut add = 0;
        let mut del = 0;
        let mut mods = 0;
        for r in &self.diff.rows {
            match r.kind {
                RowKind::Insert => add += 1,
                RowKind::Delete => del += 1,
                RowKind::Replace => mods += 1,
                RowKind::Equal => {}
            }
        }
        (add, del, mods)
    }

    pub fn handle_click(&mut self, x: u16, y: u16) {
        if rect_contains(self.layout.files_inner, x, y) {
            let row = (y - self.layout.files_inner.y) as usize;
            self.select_index(row);
            self.focus = Focus::Files;
        } else if rect_contains(self.layout.diff_inner, x, y) {
            let row = (y - self.layout.diff_inner.y) as usize;
            let target = self.layout.diff_top_row + row;
            if target < self.diff.rows.len() {
                self.cursor_row = target;
            }
            self.focus = Focus::Diff;
        }
    }

    pub fn handle_scroll(&mut self, x: u16, y: u16, down: bool) {
        const STEP: usize = 3;
        if rect_contains(self.layout.files_inner, x, y) {
            for _ in 0..STEP {
                if down {
                    self.select_next();
                } else {
                    self.select_prev();
                }
            }
        } else if rect_contains(self.layout.diff_inner, x, y) || matches!(self.focus, Focus::Diff) {
            if down {
                self.cursor_down(STEP);
            } else {
                self.cursor_up(STEP);
            }
        }
    }
}

fn rect_contains(r: Rect, x: u16, y: u16) -> bool {
    r.width > 0
        && r.height > 0
        && x >= r.x
        && x < r.x + r.width
        && y >= r.y
        && y < r.y + r.height
}
