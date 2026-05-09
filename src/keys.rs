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

    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), _) => return Action::Quit,
        (KeyCode::Char('?'), _) => app.show_help = true,
        (KeyCode::Tab, _) => app.toggle_focus(),
        (KeyCode::Char('r'), _) => app.reload_entries(),
        (KeyCode::Char('n'), _) => app.select_next(),
        (KeyCode::Char('p'), _) => app.select_prev(),
        (KeyCode::Char('='), _) => app.resnap(),

        (KeyCode::Char('j') | KeyCode::Down, _) => app.cursor_down(1),
        (KeyCode::Char('k') | KeyCode::Up, _) => app.cursor_up(1),
        (KeyCode::Char('J'), _) => app.next_hunk(),
        (KeyCode::Char('K'), _) => app.prev_hunk(),
        (KeyCode::Char('d'), KeyModifiers::CONTROL) => app.cursor_down(15),
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => app.cursor_up(15),
        (KeyCode::Char('g'), _) => {
            app.left_top = 0;
            app.right_top = 0;
        }
        (KeyCode::Char('G'), _) => {
            app.left_top = app.diff.left_render.len().saturating_sub(1);
            app.right_top = app.diff.right_render.len().saturating_sub(1);
        }

        (KeyCode::Enter, _) => {
            if matches!(app.focus, Focus::Files) {
                app.focus = Focus::Diff;
            }
        }
        (KeyCode::Left | KeyCode::Char('h'), _) => app.focus = Focus::Files,
        (KeyCode::Right | KeyCode::Char('l'), _) => app.focus = Focus::Diff,
        _ => {}
    }
    Action::Continue
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
