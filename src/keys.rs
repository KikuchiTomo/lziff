use crate::app::{App, Focus};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

pub enum Action {
    Continue,
    Quit,
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    if app.show_help {
        match key.code {
            KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => app.show_help = false,
            _ => {}
        }
        return Action::Continue;
    }

    match key.code {
        KeyCode::Char('q') => return Action::Quit,
        KeyCode::Char('?') => app.show_help = true,
        KeyCode::Tab => app.toggle_focus(),
        KeyCode::Char('r') => app.reload_entries(),
        KeyCode::Char('n') => app.select_next(),
        KeyCode::Char('p') => app.select_prev(),
        _ => match app.focus {
            Focus::Files => handle_files(app, key),
            Focus::Diff => handle_diff(app, key),
        },
    }
    Action::Continue
}

fn handle_files(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.select_next(),
        KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
        KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => app.focus = Focus::Diff,
        _ => {}
    }
}

fn handle_diff(app: &mut App, key: KeyEvent) {
    match (key.code, key.modifiers) {
        (KeyCode::Char('j') | KeyCode::Down, _) => app.cursor_down(1),
        (KeyCode::Char('k') | KeyCode::Up, _) => app.cursor_up(1),
        (KeyCode::Char('J'), _) => app.next_hunk(),
        (KeyCode::Char('K'), _) => app.prev_hunk(),
        (KeyCode::Char('d'), KeyModifiers::CONTROL) => app.cursor_down(15),
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => app.cursor_up(15),
        (KeyCode::Char('g'), _) => app.cursor_row = 0,
        (KeyCode::Char('G'), _) => {
            if !app.diff.rows.is_empty() {
                app.cursor_row = app.diff.rows.len() - 1;
            }
        }
        (KeyCode::Char('h') | KeyCode::Left, _) => app.focus = Focus::Files,
        _ => {}
    }
}

pub fn handle_mouse(app: &mut App, m: MouseEvent) {
    if app.show_help {
        return;
    }
    match m.kind {
        MouseEventKind::Down(_) => app.handle_click(m.column, m.row),
        MouseEventKind::ScrollDown => app.handle_scroll(m.column, m.row, true),
        MouseEventKind::ScrollUp => app.handle_scroll(m.column, m.row, false),
        _ => {}
    }
}
