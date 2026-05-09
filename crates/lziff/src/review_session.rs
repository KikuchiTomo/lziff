//! Review-session state: the in-tool comment buffer + modal state.
//!
//! Lives entirely on the host side. The `ReviewProvider` trait only
//! sees the finished payload at submit time — drafts are an
//! implementation detail of the lziff TUI.

use review_protocol::{
    CommentSide, NewComment, PullRequest, ReviewProvider, ReviewVerdict,
};
use tui_textarea::TextArea;

/// Per-PR review state. Created when the host enters `--review` mode.
pub struct ReviewSession {
    pub pr: PullRequest,
    pub provider: Box<dyn ReviewProvider>,
    pub drafts: Vec<DraftComment>,
    pub modal: Option<Modal>,
    /// Last submission outcome, surfaced briefly in the status bar.
    pub last_status: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DraftComment {
    pub path: String,
    pub line: u32,
    pub side: CommentSide,
    pub body: String,
}

impl DraftComment {
    pub fn into_protocol(self) -> NewComment {
        NewComment {
            path: self.path,
            line: self.line,
            side: self.side,
            body: self.body,
        }
    }
}

pub enum Modal {
    Comment(CommentModal),
    Submit(SubmitModal),
    Drafts(DraftsModal),
}

pub struct CommentModal {
    pub path: String,
    pub line: u32,
    pub side: CommentSide,
    pub textarea: TextArea<'static>,
    /// When the modal was opened from the drafts list to edit an existing
    /// draft, this holds its index so saving replaces the draft in place
    /// instead of appending a new one.
    pub editing_index: Option<usize>,
}

impl CommentModal {
    pub fn new(path: String, line: u32, side: CommentSide) -> Self {
        // The modal's outer block already shows " Comment · <path>:<line> "
        // as its title — give the textarea a borderless block so we don't
        // end up with two nested frames carrying the same caption.
        let mut textarea = TextArea::default();
        textarea.set_cursor_line_style(ratatui::style::Style::default());
        Self {
            path,
            line,
            side,
            textarea,
            editing_index: None,
        }
    }

    pub fn editing(
        index: usize,
        path: String,
        line: u32,
        side: CommentSide,
        body: &str,
    ) -> Self {
        let mut m = Self::new(path, line, side);
        m.editing_index = Some(index);
        // Pre-fill the textarea with the existing draft body.
        for (i, l) in body.split('\n').enumerate() {
            if i > 0 {
                m.textarea.insert_newline();
            }
            m.textarea.insert_str(l);
        }
        m
    }

    pub fn body(&self) -> String {
        self.textarea.lines().join("\n")
    }
}

/// A list-only modal that shows the buffered drafts and lets the user
/// edit (Enter), delete (`x`), or jump to (`o`) each. No textarea — it's
/// just navigation.
pub struct DraftsModal {
    pub selected: usize,
}

impl DraftsModal {
    pub fn new() -> Self {
        Self { selected: 0 }
    }
}

pub struct SubmitModal {
    pub verdict: ReviewVerdict,
    pub textarea: TextArea<'static>,
    /// Toggles focus between the verdict selector and the body textarea.
    pub focus: SubmitFocus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitFocus {
    Verdict,
    Body,
}

impl SubmitModal {
    pub fn new(default: ReviewVerdict) -> Self {
        let mut textarea = TextArea::default();
        textarea.set_cursor_line_style(ratatui::style::Style::default());
        textarea.set_block(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .title(" Overall body "),
        );
        Self {
            verdict: default,
            textarea,
            focus: SubmitFocus::Verdict,
        }
    }

    pub fn body(&self) -> String {
        self.textarea.lines().join("\n")
    }
}

impl ReviewSession {
    pub fn new(pr: PullRequest, provider: Box<dyn ReviewProvider>) -> Self {
        Self {
            pr,
            provider,
            drafts: Vec::new(),
            modal: None,
            last_status: None,
        }
    }

    /// True if `(path, line)` has at least one draft on the given side.
    /// Drives the diff gutter marker.
    pub fn has_draft_at(&self, path: &str, line: u32, side: CommentSide) -> bool {
        self.drafts
            .iter()
            .any(|d| d.path == path && d.line == line && d.side == side)
    }

    pub fn open_drafts(&mut self) {
        if self.drafts.is_empty() {
            self.last_status = Some("no drafts to show".into());
            return;
        }
        self.modal = Some(Modal::Drafts(DraftsModal::new()));
    }

    /// Begin editing the draft at `index`. Replaces the open modal with a
    /// CommentModal pre-filled with the draft body. The draft itself
    /// stays in `self.drafts`; on save it's replaced in place, on cancel
    /// it's left untouched.
    pub fn edit_draft(&mut self, index: usize) {
        let Some(d) = self.drafts.get(index) else {
            return;
        };
        self.modal = Some(Modal::Comment(CommentModal::editing(
            index,
            d.path.clone(),
            d.line,
            d.side,
            &d.body,
        )));
    }

    pub fn delete_draft(&mut self, index: usize) {
        if index < self.drafts.len() {
            self.drafts.remove(index);
        }
        // Keep the modal open if there are still drafts; close otherwise.
        if self.drafts.is_empty() {
            self.modal = None;
        } else if let Some(Modal::Drafts(m)) = &mut self.modal {
            if m.selected >= self.drafts.len() {
                m.selected = self.drafts.len() - 1;
            }
        }
    }

    pub fn open_comment(&mut self, path: String, line: u32, side: CommentSide) {
        self.modal = Some(Modal::Comment(CommentModal::new(path, line, side)));
    }

    pub fn open_submit(&mut self) {
        self.modal = Some(Modal::Submit(SubmitModal::new(ReviewVerdict::Comment)));
    }

    pub fn close_modal(&mut self) {
        self.modal = None;
    }

    /// Persist the open comment modal as a draft. If the modal was
    /// opened from the drafts list (i.e. has `editing_index = Some`),
    /// the existing draft is replaced in place instead of appending.
    pub fn save_comment_draft(&mut self) {
        let Some(Modal::Comment(m)) = self.modal.take() else {
            return;
        };
        let body = m.body();
        if body.trim().is_empty() {
            return;
        }
        let entry = DraftComment {
            path: m.path,
            line: m.line,
            side: m.side,
            body,
        };
        match m.editing_index {
            Some(i) if i < self.drafts.len() => {
                self.drafts[i] = entry;
            }
            _ => self.drafts.push(entry),
        }
    }

    /// Submit the buffered drafts + verdict + overall body.
    pub fn submit(&mut self) {
        let Some(Modal::Submit(m)) = self.modal.take() else {
            return;
        };
        let body = m.body();
        let verdict = m.verdict;
        let comments: Vec<NewComment> = std::mem::take(&mut self.drafts)
            .into_iter()
            .map(DraftComment::into_protocol)
            .collect();
        match self.provider.submit_review(&self.pr, &body, verdict, comments) {
            Ok(()) => self.last_status = Some(format!("review submitted ({:?})", verdict)),
            Err(e) => {
                self.last_status = Some(format!("submit failed: {e}"));
            }
        }
    }
}
