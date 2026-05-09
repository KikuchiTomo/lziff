use crate::app::{App, Focus};
use crate::config::Action as ConfigAction;
use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};

pub enum Action {
    Continue,
    Quit,
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> Action {
    let mods = key.modifiers
        & (crossterm::event::KeyModifiers::SHIFT
            | crossterm::event::KeyModifiers::CONTROL
            | crossterm::event::KeyModifiers::ALT);
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
        // Fall through: nothing bound; ignore.
        if matches!(key.code, KeyCode::Enter) && matches!(app.focus, Focus::Files) {
            app.focus = Focus::Diff;
        }
        return Action::Continue;
    };

    let half_step = app.config.behavior.half_page_step;
    match action {
        ConfigAction::Quit => return Action::Quit,
        ConfigAction::ToggleHelp => app.show_help = true,
        ConfigAction::CloseHelp => {} // only meaningful while help is open
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
