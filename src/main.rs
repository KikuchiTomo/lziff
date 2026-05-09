mod app;
mod config;
mod diff;
mod i18n;
mod keys;
mod source;
mod ui;

use anyhow::Result;
use app::App;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use source::{DiffSource, FilePair, GitWorkingTree};
use std::{
    io,
    path::PathBuf,
    time::{Duration, Instant},
};

/// lziff — TUI diff & review tool
#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Compare two arbitrary files (left and right). If omitted, opens the git working tree.
    #[arg(num_args = 0..=2)]
    paths: Vec<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let source: Box<dyn DiffSource> = match cli.paths.len() {
        0 => Box::new(GitWorkingTree::discover(&std::env::current_dir()?)?),
        2 => Box::new(FilePair {
            left: cli.paths[0].clone(),
            right: cli.paths[1].clone(),
        }),
        _ => anyhow::bail!("expected 0 args (git mode) or 2 args (file pair)"),
    };

    let cfg = config::Config::load();
    let strings = i18n::Strings::for_lang(&cfg.i18n.lang);
    let mut app = App::new(source, cfg, strings)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    let tick = Duration::from_millis(app.config.behavior.tick_ms);
    let mut last_tick = Instant::now();
    loop {
        terminal.draw(|f| ui::render(f, app))?;
        let timeout = tick.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind != crossterm::event::KeyEventKind::Press {
                        continue;
                    }
                    match keys::handle_key(app, key) {
                        keys::Action::Quit => return Ok(()),
                        keys::Action::Continue => {}
                    }
                }
                Event::Mouse(m) => keys::handle_mouse(app, m),
                _ => {}
            }
        }
        if last_tick.elapsed() >= tick {
            app.poll_changes();
            last_tick = Instant::now();
        }
    }
}
