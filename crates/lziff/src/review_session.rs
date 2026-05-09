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
}

pub struct CommentModal {
    pub path: String,
    pub line: u32,
    pub side: CommentSide,
    pub textarea: TextArea<'static>,
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
        }
    }

    pub fn body(&self) -> String {
        self.textarea.lines().join("\n")
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

    /// True if the cursor row currently has at least one draft comment.
    /// Reserved for the gutter-marker UI; not wired yet.
    #[allow(dead_code)]
    pub fn has_draft_at(&self, path: &str, line: u32) -> bool {
        self.drafts
            .iter()
            .any(|d| d.path == path && d.line == line)
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

    /// Persist the open comment modal as a draft.
    pub fn save_comment_draft(&mut self) {
        let Some(Modal::Comment(m)) = self.modal.take() else {
            return;
        };
        let body = m.body();
        if body.trim().is_empty() {
            return;
        }
        self.drafts.push(DraftComment {
            path: m.path,
            line: m.line,
            side: m.side,
            body,
        });
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
