use crate::app::{App, Focus};
use crate::config::Action as ConfigAction;
use crate::review_session::{Modal, SubmitFocus};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use review_protocol::ReviewVerdict;
use tui_textarea::{Input, Key};

pub enum Action {
    Continue,
    Quit,
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    // Modal capture comes first — when a comment / submit modal is open
    // every keystroke (except escape and the explicit save shortcut)
    // belongs to its text area.
    if app.review.as_ref().and_then(|r| r.modal.as_ref()).is_some() {
        return handle_modal_key(app, key);
    }

    let mods = key.modifiers
        & (KeyModifiers::SHIFT | KeyModifiers::CONTROL | KeyModifiers::ALT);
    let cfg_action = app.config.keymap.lookup(key.code, mods);

    if app.show_help {
        match cfg_action {
            Some(ConfigAction::ToggleHelp)
            | Some(ConfigAction::CloseHelp)
            | Some(ConfigAction::Quit) => app.show_help = false,
            _ => {}
        }
        return Action::Continue;
    }

    let Some(action) = cfg_action else {
        if matches!(key.code, KeyCode::Enter) && matches!(app.focus, Focus::Files) {
            app.focus = Focus::Diff;
        }
        return Action::Continue;
    };

    let half_step = app.config.behavior.half_page_step;
    match action {
        ConfigAction::Quit => return Action::Quit,
        ConfigAction::ToggleHelp => app.show_help = true,
        ConfigAction::CloseHelp => {}
        ConfigAction::ToggleFocus => app.toggle_focus(),
        ConfigAction::ToggleFilesPanel => app.toggle_files_panel(),
        ConfigAction::Reload => app.reload_entries(),
        ConfigAction::NextFile => app.select_next(),
        ConfigAction::PrevFile => app.select_prev(),
        ConfigAction::Resnap => app.resnap(),
        ConfigAction::ScrollDown => app.cursor_down(1),
        ConfigAction::ScrollUp => app.cursor_up(1),
        ConfigAction::NextHunk => app.next_hunk(),
        ConfigAction::PrevHunk => app.prev_hunk(),
        ConfigAction::HalfPageDown => app.cursor_down(half_step),
        ConfigAction::HalfPageUp => app.cursor_up(half_step),
        ConfigAction::Top => {
            app.left_top = 0;
            app.right_top = 0;
        }
        ConfigAction::Bottom => {
            app.left_top = app.diff.left_render.len().saturating_sub(1);
            app.right_top = app.diff.right_render.len().saturating_sub(1);
        }
        ConfigAction::EnterDiff => {
            if matches!(app.focus, Focus::Files) {
                app.focus = Focus::Diff;
            }
        }
        ConfigAction::FocusFiles => app.focus = Focus::Files,
        ConfigAction::FocusDiff => app.focus = Focus::Diff,
        ConfigAction::OpenComment => {
            if app.review.is_some() && matches!(app.focus, Focus::Diff) {
                app.open_comment_at_cursor();
            }
        }
        ConfigAction::OpenSubmit => {
            if app.review.is_some() {
                app.open_submit();
            }
        }
        ConfigAction::SnapUp => app.snap_step(false),
        ConfigAction::SnapDown => app.snap_step(true),
        ConfigAction::OpenDrafts => {
            if let Some(r) = app.review.as_mut() {
                r.open_drafts();
            }
        }
    }
    Action::Continue
}

fn handle_modal_key(app: &mut App, key: KeyEvent) -> Action {
    let Some(review) = app.review.as_mut() else {
        return Action::Continue;
    };

    // Esc always closes (cancels) the modal.
    if matches!(key.code, KeyCode::Esc) {
        review.close_modal();
        return Action::Continue;
    }

    // Ctrl-S short-circuits: it acts on the *whole* session (saving a
    // draft or submitting), so the modal borrow has to be released
    // first via `review.modal.take()`-style helpers.
    let is_ctrl_s =
        key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL);
    if is_ctrl_s {
        match review.modal {
            Some(Modal::Comment(_)) => review.save_comment_draft(),
            Some(Modal::Submit(_)) => review.submit(),
            // Ctrl-S in the drafts list is a no-op — there's nothing to
            // save from there, only navigation/edit/delete.
            Some(Modal::Drafts(_)) => {}
            None => {}
        }
        return Action::Continue;
    }

    // The drafts modal has its own non-textarea key handling — handle it
    // before grabbing a mutable borrow on the modal.
    if matches!(review.modal, Some(Modal::Drafts(_))) {
        return handle_drafts_modal_key(review, key);
    }

    // Otherwise route into the open modal.
    let Some(modal) = review.modal.as_mut() else {
        return Action::Continue;
    };
    match modal {
        Modal::Comment(m) => {
            m.textarea.input(crossterm_to_input(key));
        }
        Modal::Drafts(_) => unreachable!("handled above"),
        Modal::Submit(m) => {
            if matches!(key.code, KeyCode::Tab) {
                m.focus = match m.focus {
                    SubmitFocus::Verdict => SubmitFocus::Body,
                    SubmitFocus::Body => SubmitFocus::Verdict,
                };
                return Action::Continue;
            }
            if m.focus == SubmitFocus::Verdict {
                match key.code {
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        m.verdict = ReviewVerdict::Approve
                    }
                    KeyCode::Char('c') | KeyCode::Char('C') => {
                        m.verdict = ReviewVerdict::Comment
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        m.verdict = ReviewVerdict::RequestChanges
                    }
                    KeyCode::Left => m.verdict = prev_verdict(m.verdict),
                    KeyCode::Right => m.verdict = next_verdict(m.verdict),
                    _ => {}
                }
                return Action::Continue;
            }
            m.textarea.input(crossterm_to_input(key));
        }
    }
    Action::Continue
}

fn handle_drafts_modal_key(
    review: &mut crate::review_session::ReviewSession,
    key: KeyEvent,
) -> Action {
    let count = review.drafts.len();
    let Some(Modal::Drafts(m)) = &mut review.modal else {
        return Action::Continue;
    };
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            if m.selected + 1 < count {
                m.selected += 1;
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if m.selected > 0 {
                m.selected -= 1;
            }
        }
        KeyCode::Home | KeyCode::Char('g') => m.selected = 0,
        KeyCode::End | KeyCode::Char('G') => m.selected = count.saturating_sub(1),
        KeyCode::Enter => {
            let i = m.selected;
            review.edit_draft(i);
        }
        KeyCode::Char('x') | KeyCode::Delete => {
            let i = m.selected;
            review.delete_draft(i);
        }
        _ => {}
    }
    Action::Continue
}

fn prev_verdict(v: ReviewVerdict) -> ReviewVerdict {
    match v {
        ReviewVerdict::Comment => ReviewVerdict::RequestChanges,
        ReviewVerdict::Approve => ReviewVerdict::Comment,
        ReviewVerdict::RequestChanges => ReviewVerdict::Approve,
    }
}

fn next_verdict(v: ReviewVerdict) -> ReviewVerdict {
    match v {
        ReviewVerdict::Comment => ReviewVerdict::Approve,
        ReviewVerdict::Approve => ReviewVerdict::RequestChanges,
        ReviewVerdict::RequestChanges => ReviewVerdict::Comment,
    }
}

fn crossterm_to_input(key: KeyEvent) -> Input {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let k = match key.code {
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter => Key::Enter,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Tab => Key::Tab,
        KeyCode::Delete => Key::Delete,
        KeyCode::Esc => Key::Esc,
        KeyCode::F(n) => Key::F(n),
        _ => Key::Null,
    };
    Input {
        key: k,
        ctrl,
        alt,
        shift,
    }
}

pub fn handle_mouse(app: &mut App, m: MouseEvent) {
    if app.show_help {
        return;
    }
    if app.review.as_ref().and_then(|r| r.modal.as_ref()).is_some() {
        return;
    }
    match m.kind {
        MouseEventKind::Down(_) => app.handle_click(m.column, m.row),
        MouseEventKind::ScrollDown => app.handle_scroll(m.column, m.row, true),
        MouseEventKind::ScrollUp => app.handle_scroll(m.column, m.row, false),
        _ => {}
    }
}
