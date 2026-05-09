mod app;
mod config;
mod diff;
mod i18n;
mod keys;
mod review;
mod review_session;
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
use source::{DiffSource, FilePair, GitSource};
use std::{
    io,
    path::PathBuf,
    time::{Duration, Instant},
};

/// lziff — TUI diff & review tool.
///
/// Default behaviour mirrors `git status` + `git diff` on the current
/// repository: shows the modified files in the working tree and lets you
/// browse each one. Use the flags below to pivot to staged / commit /
/// range mode, or pass two file paths to compare arbitrary files.
#[derive(Parser, Debug)]
#[command(
    version,
    about,
    long_about = None,
    after_help = "EXAMPLES:\n  \
        lziff                         working tree vs HEAD/index\n  \
        lziff --staged                staged changes (index vs HEAD)\n  \
        lziff --commit HEAD           what HEAD changed (HEAD~ vs HEAD)\n  \
        lziff --commit abc123         what abc123 changed (abc123~ vs abc123)\n  \
        lziff --range main..feature   what feature added on top of main\n  \
        lziff --review 42             open PR #42 from origin (gh CLI)\n  \
        lziff --review URL            open PR by URL\n  \
        lziff a.txt b.txt             arbitrary file pair"
)]
struct Cli {
    /// Compare two arbitrary files (left and right). If omitted, runs in
    /// one of the git modes selected by the flags below (default: working
    /// tree).
    #[arg(num_args = 0..=2)]
    paths: Vec<PathBuf>,

    /// Show staged changes only (index vs HEAD).
    #[arg(long, alias = "cached", conflicts_with_all = ["commit", "range", "review"])]
    staged: bool,

    /// Show what a commit changed: <REV> vs <REV>~.
    #[arg(short = 'c', long, value_name = "REV", conflicts_with_all = ["staged", "range", "review"])]
    commit: Option<String>,

    /// Diff between two refs, written as "<from>..<to>".
    #[arg(
        short = 'r',
        long,
        value_name = "FROM..TO",
        conflicts_with_all = ["staged", "commit", "review"]
    )]
    range: Option<String>,

    /// Review a pull request. Argument is a PR number (`42`), a branch
    /// name (`feature/foo`), or a full URL. Resolves through the
    /// configured review backend (default: GitHub via `gh` CLI). When
    /// the user is already on the PR's branch the diff runs in place;
    /// otherwise lziff fetches the head into a `git worktree` under
    /// `~/.cache/lziff/review/` and cleans it up on exit.
    #[arg(long, value_name = "PR", conflicts_with_all = ["staged", "commit", "range"])]
    review: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let prep = prepare(&cli)?;

    let cfg = config::Config::load();
    let strings = i18n::Strings::for_lang(&cfg.i18n.lang);
    let mut app = App::new_with_review(prep.source, cfg, strings, prep.review)?;

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

    // Drop the prep struct *after* terminal teardown so worktree
    // cleanup messages don't end up on the alternate screen.
    drop(prep.cleanup);

    res
}

/// Everything the host needs to start. The `cleanup` field, when present,
/// owns a `Drop` guard that tears down a temporary git worktree on exit.
struct Prep {
    source: Box<dyn DiffSource>,
    /// Held for the lifetime of the run; dropping it removes the worktree
    /// (only set in `--review` mode when we created one).
    cleanup: Option<review::WorktreeGuard>,
    /// Active review session when `--review` was used.
    review: Option<review_session::ReviewSession>,
}

fn prepare(cli: &Cli) -> Result<Prep> {
    if !cli.paths.is_empty() {
        if cli.paths.len() != 2 {
            anyhow::bail!("file-pair mode expects exactly 2 paths");
        }
        return Ok(Prep {
            source: Box::new(FilePair {
                left: cli.paths[0].clone(),
                right: cli.paths[1].clone(),
            }),
            cleanup: None,
            review: None,
        });
    }

    let cwd = std::env::current_dir()?;

    if let Some(spec) = &cli.review {
        let opened = review::open(spec)?;
        return Ok(Prep {
            source: opened.source,
            cleanup: Some(opened.guard),
            review: Some(opened.session),
        });
    }
    if cli.staged {
        return Ok(Prep {
            source: Box::new(GitSource::staged(&cwd)?),
            cleanup: None,
            review: None,
        });
    }
    if let Some(rev) = &cli.commit {
        return Ok(Prep {
            source: Box::new(GitSource::commit(&cwd, rev)?),
            cleanup: None,
            review: None,
        });
    }
    if let Some(range) = &cli.range {
        let (from, to) = range
            .split_once("..")
            .ok_or_else(|| anyhow::anyhow!("--range expects FROM..TO, got `{range}`"))?;
        if from.is_empty() || to.is_empty() {
            anyhow::bail!("--range needs both endpoints, got `{range}`");
        }
        return Ok(Prep {
            source: Box::new(GitSource::range(&cwd, from, to)?),
            cleanup: None,
            review: None,
        });
    }
    Ok(Prep {
        source: Box::new(GitSource::working_tree(&cwd)?),
        cleanup: None,
        review: None,
    })
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
