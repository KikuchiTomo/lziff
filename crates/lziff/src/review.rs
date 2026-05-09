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

use crate::review_session::ReviewSession;
use crate::source::{DiffSource, GitSource};
use anyhow::{Context, Result};
use review_protocol::{PrRef, ReviewProvider};
use std::path::PathBuf;
use std::process::Command;

/// What `prepare()` gets back when the user asked for `--review <spec>`.
pub struct OpenedReview {
    pub source: Box<dyn DiffSource>,
    pub guard: WorktreeGuard,
    pub session: ReviewSession,
}

/// Resolve a `--review` spec into the host-ready bundle: a diff source
/// pointed at the PR's worktree, the cleanup guard for that worktree,
/// and the review session that owns drafts/modal state and the
/// provider used at submit time.
///
/// The spec is whatever the user typed: `42`, `feature/x`, or a full
/// PR URL. We pick the right backend (currently always GitHub), look
/// up the PR, ensure a worktree, and hand the host a `GitSource::range`
/// that diffs `base_sha..head_sha`.
pub fn open(spec: &str) -> Result<OpenedReview> {
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
    // `gh pr view --json` doesn't expose the base SHA, so the provider
    // leaves it blank and we resolve it here against the worktree (or the
    // cwd repo, if we're reusing an existing checkout). We fetch the base
    // branch from origin and take the merge-base with the PR head — that
    // matches GitHub's "Files changed" semantics rather than the raw tip
    // of the base branch (which would include unrelated upstream commits
    // landed after the PR diverged).
    let mut pr = pr;
    if pr.base_sha.is_empty() {
        pr.base_sha = resolve_base_sha(&workdir, &pr.base, &pr.head_sha)
            .with_context(|| format!("resolve base sha for {}", pr.base))?;
    }
    let source = GitSource::range(&workdir, &pr.base_sha, &pr.head_sha)?;

    let guard = WorktreeGuard {
        path: if handle.cleanup_on_drop {
            Some(workdir)
        } else {
            None
        },
    };
    // The session keeps its own provider handle so submitting from the
    // TUI doesn't need to thread anything back through main.rs.
    let session_provider = make_provider("github").expect("backend already validated above");
    let session = ReviewSession::new(pr, session_provider);
    Ok(OpenedReview {
        source: Box::new(source),
        guard,
        session,
    })
}

/// Owns the temporary worktree lifetime. Dropping it removes the
/// worktree from disk via `git worktree remove --force`. When the
/// review session was run in-place (the user was already on the PR's
/// branch), `path` is `None` and Drop is a no-op.
pub struct WorktreeGuard {
    path: Option<PathBuf>,
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

/// Fetch `base_ref` from origin and return the merge-base with `head_sha`.
/// Falls back to the tip of the fetched ref if `merge-base` fails (e.g. the
/// PR head and base have no common ancestor — rare, but possible for force-
/// pushed branches).
fn resolve_base_sha(workdir: &std::path::Path, base_ref: &str, head_sha: &str) -> Result<String> {
    // Best-effort fetch; if origin already has the ref this is a fast no-op,
    // and if the fetch fails (e.g. offline) we still try to resolve from
    // whatever's already in the local refs.
    let _ = Command::new("git")
        .current_dir(workdir)
        .args(["fetch", "--no-tags", "origin", base_ref])
        .output();
    // Try merge-base FETCH_HEAD..head_sha; if FETCH_HEAD isn't set (no
    // fetch), fall back to origin/<base_ref>.
    for base_spec in ["FETCH_HEAD", &format!("origin/{base_ref}"), base_ref] {
        let out = Command::new("git")
            .current_dir(workdir)
            .args(["merge-base", base_spec, head_sha])
            .output();
        if let Ok(out) = out {
            if out.status.success() {
                let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !sha.is_empty() {
                    return Ok(sha);
                }
            }
        }
    }
    // Last resort: just resolve the base ref to its tip.
    for base_spec in ["FETCH_HEAD", &format!("origin/{base_ref}"), base_ref] {
        let out = Command::new("git")
            .current_dir(workdir)
            .args(["rev-parse", base_spec])
            .output();
        if let Ok(out) = out {
            if out.status.success() {
                let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !sha.is_empty() {
                    return Ok(sha);
                }
            }
        }
    }
    anyhow::bail!("could not resolve base ref `{base_ref}` to a sha")
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
