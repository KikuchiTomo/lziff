//! The single, deliberately tiny crossing point between the lziff body
//! and the review-provider plugins.
//!
//! ════════════════════════════════════════════════════════════════════════
//!  PLUGIN BOUNDARY — KEEP THIS FILE THIN
//! ════════════════════════════════════════════════════════════════════════
//!
//! Rules of engagement:
//!
//! - This module is the **only** place inside `lziff` allowed to name a
//!   concrete provider crate (`lziff_github`, future `lziff_gitlab`, …).
//!   Every other module in `crates/lziff/` works against
//!   [`review_protocol::ReviewProvider`] only.
//! - All `make_provider*` factory functions return a trait object so
//!   the rest of the host never sees the concrete type. When we
//!   eventually swap an in-process backend for a JSON-RPC subprocess,
//!   the only file that has to change is this one.
//! - Don't fan out: a future `make_gitlab_provider` lives next to
//!   `make_github_provider`, both produce `Box<dyn ReviewProvider>`.

use crate::source::{DiffSource, GitSource};
use anyhow::{Context, Result};
use review_protocol::{PrRef, PullRequest, ReviewProvider};
use std::path::PathBuf;
use std::process::Command;

/// Resolve a `--review` spec into (source, cleanup-guard).
///
/// The spec is whatever the user typed: `42`, `feature/x`, or a full
/// PR URL. We pick the right backend (currently always GitHub), look
/// up the PR, ensure a worktree, and hand the host a `GitSource::range`
/// that diffs `base_sha..head_sha`.
pub fn open(spec: &str) -> Result<(Box<dyn DiffSource>, WorktreeGuard)> {
    // Pick the backend. Today there's only `github`; a future
    // GitLab/Gitea backend just adds a branch above this.
    let provider = make_provider("github").context("no review backend available")?;
    provider.check_ready().map_err(|e| anyhow::anyhow!("{e}"))?;

    let pr_ref = parse_spec(spec);
    let pr = provider
        .get_pull_request(pr_ref)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let cache_root = cache_root_for_review()?;
    let handle = provider
        .ensure_worktree(&pr, cache_root.to_string_lossy().as_ref())
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let workdir = PathBuf::from(&handle.path);
    // Build a GitSource::range against the worktree root (or cwd, when
    // we're reusing the existing checkout).
    let source = GitSource::range(&workdir, &pr.base_sha, &pr.head_sha)?;

    let guard = WorktreeGuard {
        path: if handle.cleanup_on_drop {
            Some(workdir)
        } else {
            None
        },
        // Stash the PR so the host can later show it in the title bar /
        // status bar; reserved for a follow-up commit.
        _pr: pr,
    };
    Ok((Box::new(source), guard))
}

/// Owns the temporary worktree lifetime. Dropping it removes the
/// worktree from disk via `git worktree remove --force`. When the
/// review session was run in-place (the user was already on the PR's
/// branch), `path` is `None` and Drop is a no-op.
pub struct WorktreeGuard {
    path: Option<PathBuf>,
    _pr: PullRequest,
}

impl Drop for WorktreeGuard {
    fn drop(&mut self) {
        if let Some(p) = self.path.take() {
            // Best-effort: ignore errors. The worst case is a stale
            // directory the user can clean up manually.
            let _ = Command::new("git")
                .args(["worktree", "remove", "--force"])
                .arg(&p)
                .status();
            // If `git worktree remove` failed (e.g. parent repo gone),
            // try to nuke the directory ourselves.
            let _ = std::fs::remove_dir_all(&p);
        }
    }
}

fn parse_spec(spec: &str) -> PrRef {
    if spec.starts_with("http://") || spec.starts_with("https://") {
        return PrRef::Url(spec.to_string());
    }
    if let Ok(n) = spec.parse::<u64>() {
        return PrRef::Number(n);
    }
    PrRef::Branch(spec.to_string())
}

fn cache_root_for_review() -> Result<PathBuf> {
    let dir = dirs::cache_dir()
        .ok_or_else(|| anyhow::anyhow!("no cache dir available on this platform"))?
        .join("lziff")
        .join("review");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("create cache dir {}", dir.display()))?;
    Ok(dir)
}

/// Pick a backend by id. Returns `None` if the requested backend isn't
/// compiled in (e.g., the `github` cargo feature was disabled) or isn't
/// recognized.
fn make_provider(id: &str) -> Option<Box<dyn ReviewProvider>> {
    match id {
        #[cfg(feature = "github")]
        "github" => Some(lziff_github::make_provider()),
        _ => None,
    }
}
