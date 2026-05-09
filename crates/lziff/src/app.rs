use crate::config::Config;
use crate::diff::{Diff, LineKind};
use crate::i18n::Strings;
use crate::review_session::ReviewSession;
use crate::source::{DiffPayload, DiffSource, Entry};
use anyhow::Result;
use ratatui::layout::Rect;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Files,
    Diff,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AnchorSide {
    Left,
    Right,
}

#[derive(Default, Clone, Copy)]
pub struct LayoutCache {
    pub files_inner: Rect,
    pub diff_left_inner: Rect,
    pub diff_right_inner: Rect,
    pub viewport_h: u16,
}

pub struct App {
    pub source: Box<dyn DiffSource>,
    pub entries: Vec<Entry>,
    pub selected: usize,
    pub diff: Diff,
    pub payload: Option<DiffPayload>,
    /// Independent scroll positions per pane. The "selected line stays put"
    /// click UX requires that we can adjust one pane's scroll without
    /// touching the other's.
    pub left_top: usize,
    pub right_top: usize,
    /// Screen row of the alignment band. Set by clicks (becomes the click
    /// row) and otherwise stays where the user left it. Equal anchors land
    /// on this row when both panes show their counterparts there.
    pub cursor_y: usize,
    /// Which side was clicked last — used to decide which pane scrolls
    /// "naturally" and which one re-snaps to its counterpart on j/k.
    pub anchor_side: AnchorSide,
    pub focus: Focus,
    pub show_help: bool,
    pub status: String,
    pub layout: LayoutCache,
    pub show_files_panel: bool,
    pub config: Config,
    pub strings: Strings,
    /// Active review session when the host was started with `--review`.
    /// Owns the buffered draft comments and the active modal, if any.
    pub review: Option<ReviewSession>,
    last_signature: u64,
}

impl App {
    pub fn new_with_review(
        source: Box<dyn DiffSource>,
        config: Config,
        strings: Strings,
        review: Option<ReviewSession>,
    ) -> Result<Self> {
        let entries = source.list()?;
        let signature = source.signature(entries.first().map(|e| e.id.as_str()));
        let show_files = source.show_files_panel();
        let mut app = Self {
            source,
            entries,
            selected: 0,
            diff: Diff::default(),
            payload: None,
            left_top: 0,
            right_top: 0,
            cursor_y: 0,
            anchor_side: AnchorSide::Left,
            focus: if show_files { Focus::Files } else { Focus::Diff },
            show_help: false,
            status: String::new(),
            layout: LayoutCache::default(),
            show_files_panel: show_files,
            config,
            strings,
            review,
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
        self.left_top = 0;
        self.right_top = 0;
        self.cursor_y = 0;
        self.anchor_side = AnchorSide::Left;
        self.reload_diff_inner();
    }

    fn reload_diff_keep_view(&mut self) {
        self.reload_diff_inner();
        self.left_top = self
            .left_top
            .min(self.diff.left_render.len().saturating_sub(1).max(0));
        self.right_top = self
            .right_top
            .min(self.diff.right_render.len().saturating_sub(1).max(0));
    }

    fn reload_diff_inner(&mut self) {
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            self.diff = Diff::default();
            self.payload = None;
            return;
        };
        match self.source.load(&entry.id) {
            Ok(payload) => {
                self.diff = Diff::compute(
                    &payload.left,
                    &payload.right,
                    self.config.behavior.similarity_threshold,
                );
                self.payload = Some(payload);
            }
            Err(e) => {
                self.diff = Diff::default();
                self.payload = None;
                self.status = format!("load failed: {e}");
            }
        }
    }

    pub fn poll_changes(&mut self) {
        let id = self.entries.get(self.selected).map(|e| e.id.clone());
        let sig = self.source.signature(id.as_deref());
        if sig == self.last_signature {
            return;
        }
        self.last_signature = sig;
        self.reload_entries();
        self.status = self.strings.status_auto_reloaded.clone();
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

    /// Snap-aware scroll: at each step, if advancing both panes by 1 would
    /// move them into different segments (one side reaches the next equal
    /// anchor while the other's still inside the change), pause the side
    /// that exited and only advance the side that's still catching up.
    /// When both sides finally reach their respective segment boundary,
    /// they advance together — the snap.
    pub fn cursor_down(&mut self, n: usize) {
        if matches!(self.focus, Focus::Files) {
            for _ in 0..n {
                self.select_next();
            }
            return;
        }
        for _ in 0..n {
            self.scroll_one(true);
        }
    }

    pub fn cursor_up(&mut self, n: usize) {
        if matches!(self.focus, Focus::Files) {
            for _ in 0..n {
                self.select_prev();
            }
            return;
        }
        for _ in 0..n {
            self.scroll_one(false);
        }
    }

    fn scroll_one(&mut self, down: bool) {
        let l_idx = self.left_top + self.cursor_y;
        let r_idx = self.right_top + self.cursor_y;
        if down {
            let l_at_end = l_idx + 1 >= self.diff.left_render.len();
            let r_at_end = r_idx + 1 >= self.diff.right_render.len();
            if l_at_end && r_at_end {
                return;
            }
            let new_l = l_idx.saturating_add(1);
            let new_r = r_idx.saturating_add(1);
            let (adv_l, adv_r) = self.decide_advance(l_idx, r_idx, new_l, new_r);
            if adv_l && !l_at_end {
                self.left_top = self.left_top.saturating_add(1);
            }
            if adv_r && !r_at_end {
                self.right_top = self.right_top.saturating_add(1);
            }
        } else {
            if l_idx == 0 && r_idx == 0 {
                return;
            }
            let new_l = l_idx.saturating_sub(1);
            let new_r = r_idx.saturating_sub(1);
            let (adv_l, adv_r) = self.decide_advance(l_idx, r_idx, new_l, new_r);
            if adv_l && self.left_top > 0 {
                self.left_top -= 1;
            }
            if adv_r && self.right_top > 0 {
                self.right_top -= 1;
            }
        }
    }

    fn decide_advance(
        &self,
        l_idx: usize,
        r_idx: usize,
        new_l: usize,
        new_r: usize,
    ) -> (bool, bool) {
        let l_seg = self.diff.segment_for_left_render(l_idx);
        let r_seg = self.diff.segment_for_right_render(r_idx);
        let new_l_seg = self.diff.segment_for_left_render(new_l);
        let new_r_seg = self.diff.segment_for_right_render(new_r);
        if new_l_seg == new_r_seg {
            // Both will land in the same segment after the step → fine,
            // both advance together (this includes the "snap" moment when
            // they reach the next equal anchor simultaneously).
            return (true, true);
        }
        // Diverge: only advance the side that *stays* inside its current
        // segment. The side that would jump ahead is paused until the
        // other catches up.
        let l_stays = new_l_seg == l_seg;
        let r_stays = new_r_seg == r_seg;
        (l_stays, r_stays)
    }

    /// Move the click-set snap anchor by one render row in the anchored
    /// pane and resnap the other side to match. This is *not* the same as
    /// j/k (which scrolls both panes); `[`/`]` re-binds the alignment to a
    /// neighbouring line on the anchored side without changing where on
    /// screen the alignment row sits, so the user can fine-tune the snap
    /// after a click.
    pub fn snap_step(&mut self, forward: bool) {
        match self.anchor_side {
            AnchorSide::Left => {
                let cur = self.left_top + self.cursor_y;
                let new_l = if forward {
                    if cur + 1 >= self.diff.left_render.len() {
                        return;
                    }
                    cur + 1
                } else {
                    if cur == 0 {
                        return;
                    }
                    cur - 1
                };
                let r = self.diff.corresponding_right_for_left(new_l);
                let cy = self.cursor_y.min(r);
                self.cursor_y = cy;
                self.left_top = new_l.saturating_sub(cy);
                self.right_top = r.saturating_sub(cy);
            }
            AnchorSide::Right => {
                let cur = self.right_top + self.cursor_y;
                let new_r = if forward {
                    if cur + 1 >= self.diff.right_render.len() {
                        return;
                    }
                    cur + 1
                } else {
                    if cur == 0 {
                        return;
                    }
                    cur - 1
                };
                let l = self.diff.corresponding_left_for_right(new_r);
                let cy = self.cursor_y.min(l);
                self.cursor_y = cy;
                self.right_top = new_r.saturating_sub(cy);
                self.left_top = l.saturating_sub(cy);
            }
        }
    }

    /// Snap the non-anchor pane to whatever the anchor pane currently shows
    /// at the cursor row. Used by "=" to re-align after the panes drift.
    pub fn resnap(&mut self) {
        match self.anchor_side {
            AnchorSide::Left => {
                let l = self.left_top + self.cursor_y;
                if l < self.diff.left_render.len() {
                    let r = self.diff.corresponding_right_for_left(l);
                    let cy = self.cursor_y.min(r);
                    self.cursor_y = cy;
                    self.left_top = l.saturating_sub(cy);
                    self.right_top = r.saturating_sub(cy);
                }
            }
            AnchorSide::Right => {
                let r = self.right_top + self.cursor_y;
                if r < self.diff.right_render.len() {
                    let l = self.diff.corresponding_left_for_right(r);
                    let cy = self.cursor_y.min(l);
                    self.cursor_y = cy;
                    self.right_top = r.saturating_sub(cy);
                    self.left_top = l.saturating_sub(cy);
                }
            }
        }
    }

    /// Jump to the next change segment's start, snapping both panes to its
    /// render-row anchors at the cursor row.
    pub fn next_hunk(&mut self) {
        let l_at_cursor = self.left_top + self.cursor_y;
        for seg in &self.diff.segments {
            if seg.is_change && seg.l_render_start > l_at_cursor {
                self.left_top = seg.l_render_start.saturating_sub(self.cursor_y);
                self.right_top = seg.r_render_start.saturating_sub(self.cursor_y);
                self.anchor_side = AnchorSide::Left;
                return;
            }
        }
    }

    pub fn prev_hunk(&mut self) {
        let l_at_cursor = self.left_top + self.cursor_y;
        let mut last: Option<(usize, usize)> = None;
        for seg in &self.diff.segments {
            if seg.is_change && seg.l_render_start < l_at_cursor {
                last = Some((seg.l_render_start, seg.r_render_start));
            }
        }
        if let Some((l, r)) = last {
            self.left_top = l.saturating_sub(self.cursor_y);
            self.right_top = r.saturating_sub(self.cursor_y);
            self.anchor_side = AnchorSide::Left;
        }
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Files => Focus::Diff,
            Focus::Diff => {
                if self.show_files_panel {
                    Focus::Files
                } else {
                    Focus::Diff
                }
            }
        };
    }

    pub fn toggle_files_panel(&mut self) {
        self.show_files_panel = !self.show_files_panel;
        // Don't strand the focus on a hidden panel.
        if !self.show_files_panel && matches!(self.focus, Focus::Files) {
            self.focus = Focus::Diff;
        }
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
        for l in &self.diff.right {
            match l.kind {
                LineKind::Standalone => add += 1,
                LineKind::Modified => mods += 1,
                LineKind::Equal => {}
            }
        }
        for l in &self.diff.left {
            if l.kind == LineKind::Standalone {
                del += 1;
            }
        }
        (add, del, mods)
    }

    /// Click handler implementing the "selected side stays" snap UX:
    /// the clicked pane's `*_top` is left untouched, the cursor row is set
    /// to where the user clicked, and the *other* pane is snapped so its
    /// corresponding line lands at that same row.
    pub fn handle_click(&mut self, x: u16, y: u16) {
        if rect_contains(self.layout.files_inner, x, y) {
            let row = (y - self.layout.files_inner.y) as usize;
            self.select_index(row);
            self.focus = Focus::Files;
            return;
        }
        if rect_contains(self.layout.diff_left_inner, x, y) {
            let row = (y - self.layout.diff_left_inner.y) as usize;
            let l_idx = self.left_top + row;
            if l_idx >= self.diff.left_render.len() {
                return;
            }
            // Snap reliability: if the corresponding row would land above the
            // file (negative scroll), pull cursor_y up instead of letting the
            // snap silently fail.
            let r = self.diff.corresponding_right_for_left(l_idx);
            let cy = row.min(r);
            self.cursor_y = cy;
            self.anchor_side = AnchorSide::Left;
            self.left_top = l_idx.saturating_sub(cy);
            self.right_top = r.saturating_sub(cy);
            self.focus = Focus::Diff;
        } else if rect_contains(self.layout.diff_right_inner, x, y) {
            let row = (y - self.layout.diff_right_inner.y) as usize;
            let r_idx = self.right_top + row;
            if r_idx >= self.diff.right_render.len() {
                return;
            }
            let l = self.diff.corresponding_left_for_right(r_idx);
            let cy = row.min(l);
            self.cursor_y = cy;
            self.anchor_side = AnchorSide::Right;
            self.right_top = r_idx.saturating_sub(cy);
            self.left_top = l.saturating_sub(cy);
            self.focus = Focus::Diff;
        }
    }

    /// Open the comment modal for the cursor row's currently-anchored
    /// pane. Path comes from the active diff payload; line number and
    /// side come from the alignment row.
    pub fn open_comment_at_cursor(&mut self) {
        let Some(review) = self.review.as_mut() else {
            return;
        };
        // Determine line and side from the current cursor position.
        let (line, side) = match self.anchor_side {
            AnchorSide::Left => {
                let l_render_idx = self.left_top + self.cursor_y;
                let Some(Some(line_idx)) = self.diff.left_render.get(l_render_idx) else {
                    return;
                };
                let Some(line) = self.diff.left.get(*line_idx) else {
                    return;
                };
                (line.line_no as u32, review_protocol::CommentSide::Old)
            }
            AnchorSide::Right => {
                let r_render_idx = self.right_top + self.cursor_y;
                let Some(Some(line_idx)) = self.diff.right_render.get(r_render_idx) else {
                    return;
                };
                let Some(line) = self.diff.right.get(*line_idx) else {
                    return;
                };
                (line.line_no as u32, review_protocol::CommentSide::New)
            }
        };
        // Path is the currently-selected entry's id (= file path on
        // either side; for git it's the same on both).
        let Some(entry) = self.entries.get(self.selected) else {
            return;
        };
        review.open_comment(entry.id.clone(), line, side);
    }

    pub fn open_submit(&mut self) {
        if let Some(review) = self.review.as_mut() {
            review.open_submit();
        }
    }

    pub fn handle_scroll(&mut self, x: u16, y: u16, down: bool) {
        let step = self.config.behavior.scroll_step;
        // Files panel deliberately ignores wheel events: scrolling there
        // felt sluggish on long file lists and made it too easy to lose
        // your place. j/k or click are the way to move the selection.
        if rect_contains(self.layout.diff_left_inner, x, y)
            || rect_contains(self.layout.diff_right_inner, x, y)
        {
            if down {
                self.cursor_down(step);
            } else {
                self.cursor_up(step);
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
