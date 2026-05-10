//! PR picker — a tiny TUI shown when the user runs `lziff --review`
//! without an argument. Fetches `review-requested:@me` open PRs through
//! the active review provider and lets the user select one with
//! Up/Down/k/j and Enter (or cancel with Esc/q).
//!
//! Lives in its own module because the picker session is short-lived and
//! has nothing to do with the diff renderer — it owns a mini terminal
//! lifecycle (raw mode + alternate screen) of its own, and the main app
//! enters its own session afterwards. The two TUI sessions are
//! deliberately decoupled so the picker can be skipped entirely when the
//! user passes a PR number on the command line.

use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use review_protocol::{ListQuery, PrState, PrSummary, ReviewProvider};
use std::io;
use std::time::Duration;

/// Run the picker. Returns the selected PR number, or `None` if the user
/// cancelled. Errors come from either the backend list call or terminal
/// setup.
///
/// `assigned_to_me` — when `true` (default) only lists PRs where the
/// current user is a requested reviewer; when `false` lists all open PRs.
pub fn pick_pr(provider: &dyn ReviewProvider, assigned_to_me: bool) -> Result<Option<u64>> {
    let prs = provider
        .list_pull_requests(ListQuery {
            assigned_to_me,
            state: Some(PrState::Open),
        })
        .map_err(|e| anyhow::anyhow!("{e}"))
        .context("list pull requests")?;

    if prs.is_empty() {
        let filter = if assigned_to_me {
            "`review-requested:@me`"
        } else {
            "open"
        };
        eprintln!("lziff: no {filter} pull requests found.");
        return Ok(None);
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_picker_loop(&mut terminal, &prs, assigned_to_me);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn run_picker_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    prs: &[PrSummary],
    assigned_to_me: bool,
) -> Result<Option<u64>> {
    let mut selected = 0usize;
    loop {
        terminal.draw(|f| render(f, prs, selected, assigned_to_me))?;
        if event::poll(Duration::from_millis(200))? {
            match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press => match k.code {
                    KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                    KeyCode::Enter => {
                        return Ok(prs.get(selected).map(|p| p.number));
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if selected + 1 < prs.len() {
                            selected += 1;
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if selected > 0 {
                            selected -= 1;
                        }
                    }
                    KeyCode::Home | KeyCode::Char('g') => selected = 0,
                    KeyCode::End | KeyCode::Char('G') => {
                        selected = prs.len().saturating_sub(1)
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
}

fn render(f: &mut Frame, prs: &[PrSummary], selected: usize, assigned_to_me: bool) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    render_title(f, chunks[0], prs.len(), assigned_to_me);
    render_list(f, chunks[1], prs, selected);
    render_hint(f, chunks[2]);
}

fn render_title(f: &mut Frame, area: Rect, count: usize, assigned_to_me: bool) {
    let filter_label = if assigned_to_me {
        "review-requested:@me"
    } else {
        "all open PRs"
    };
    let title = Line::from(vec![
        Span::styled(
            " lziff ",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::Rgb(60, 90, 140))
                .fg(Color::White),
        ),
        Span::raw(" "),
        Span::styled(
            filter_label,
            Style::default().fg(Color::Rgb(225, 200, 130)),
        ),
        Span::raw("   "),
        Span::styled(
            format!("{count} open"),
            Style::default().fg(Color::Rgb(180, 180, 180)),
        ),
    ]);
    f.render_widget(Paragraph::new(title), area);
}

fn render_list(f: &mut Frame, area: Rect, prs: &[PrSummary], selected: usize) {
    let items: Vec<ListItem> = prs
        .iter()
        .map(|p| {
            // Two-line layout per PR: the title row plus a meta row with
            // the author and branch flow. Keeps the list scannable on a
            // wide terminal without trying to be a full table.
            let head = Line::from(vec![
                Span::styled(
                    format!("#{} ", p.number),
                    Style::default()
                        .fg(Color::Rgb(170, 210, 245))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    p.title.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]);
            let meta = Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    format!("@{}", p.author),
                    Style::default().fg(Color::Rgb(180, 180, 180)),
                ),
                Span::raw("  "),
                Span::styled(p.branch.clone(), Style::default().fg(Color::Rgb(120, 200, 140))),
                Span::styled(
                    "  →  ",
                    Style::default().fg(Color::Rgb(120, 120, 120)),
                ),
                Span::styled(p.base.clone(), Style::default().fg(Color::Rgb(200, 120, 120))),
            ]);
            ListItem::new(vec![head, meta])
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Pull requests ")
        .border_style(Style::default().fg(Color::Rgb(120, 160, 220)));
    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(Color::Rgb(50, 70, 100))
            .add_modifier(Modifier::BOLD),
    );

    let mut state = ListState::default();
    state.select(Some(selected));
    f.render_stateful_widget(list, area, &mut state);
}

fn render_hint(f: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled(" ↑/↓ j/k ", Style::default().fg(Color::Rgb(170, 210, 245))),
        Span::styled("move    ", Style::default().fg(Color::Rgb(210, 215, 220))),
        Span::styled(" Enter ", Style::default().fg(Color::Rgb(170, 210, 245))),
        Span::styled("open    ", Style::default().fg(Color::Rgb(210, 215, 220))),
        Span::styled(" Esc/q ", Style::default().fg(Color::Rgb(170, 210, 245))),
        Span::styled("cancel", Style::default().fg(Color::Rgb(210, 215, 220))),
    ]);
    f.render_widget(Paragraph::new(line), area);
}
