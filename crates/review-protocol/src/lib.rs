//! lziff review provider protocol.
//!
//! ════════════════════════════════════════════════════════════════════════
//!  PLUGIN BOUNDARY — TREAT THIS CRATE AS THE WIRE FORMAT
//! ════════════════════════════════════════════════════════════════════════
//!
//! This crate exists so that the body of `lziff` never depends on any
//! particular review backend (GitHub, GitLab, Gitea, …). Concrete backends
//! live in their own crates (`lziff-github`, future `lziff-gitlab`, etc.)
//! and implement the [`ReviewProvider`] trait defined here.
//!
//! Today every backend runs in-process and is linked into the main
//! binary via Cargo features. Tomorrow we will spawn each backend as a
//! child process and speak this same protocol over JSON-RPC on stdio.
//! To keep that migration painless, every type and function in this
//! crate is constrained to be:
//!
//! 1. **Owned, serde-able data only.** No references to host state, no
//!    file handles, no closures, no callbacks. If it can't survive a
//!    round-trip through `serde_json`, it does not belong here.
//! 2. **Coarse-grained.** One method per logical user action. A wire
//!    protocol can't tolerate chatty fine-grained calls.
//! 3. **Errors as values.** Methods return [`ReviewError`], an enum with
//!    serializable variants. Don't surface `anyhow::Error` across this
//!    boundary.
//! 4. **No host imports.** This crate must compile with zero knowledge of
//!    `lziff` internals. Adding `lziff = ...` to its `Cargo.toml` is a
//!    bug.
//!
//! When you implement a new method or type here, write it as if you were
//! shipping it over a network in the next sprint — because we eventually
//! will.

use serde::{Deserialize, Serialize};

/// How a user referred to a pull/merge request on the command line.
///
/// `lziff --review 42`         → `Number(42)`
/// `lziff --review feature/x`  → `Branch("feature/x")`
/// `lziff --review https://…`  → `Url("https://…")`
/// `lziff --review`            → caller asks for the "list mine" flow,
///                                no `PrRef` involved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PrRef {
    Number(u64),
    Branch(String),
    Url(String),
}

/// Filter shape for [`ReviewProvider::list_pull_requests`]. Reserved
/// fields anticipate filters we know we'll want; backends are free to
/// ignore unknown variants of `state`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListQuery {
    /// Pull only PRs the current user is requested as a reviewer on.
    /// This is the default for `lziff --review` (no argument).
    #[serde(default)]
    pub assigned_to_me: bool,
    /// Filter by state. `None` = backend default.
    #[serde(default)]
    pub state: Option<PrState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrState {
    Open,
    Closed,
    Merged,
    All,
}

/// Lightweight summary used in the picker UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrSummary {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub branch: String,
    pub base: String,
    pub state: PrState,
    pub url: String,
}

/// Full PR detail returned by [`ReviewProvider::get_pull_request`]. The
/// SHAs anchor the diff: `base_sha` and `head_sha` are the two ends of
/// the range that the renderer will display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub author: String,
    pub branch: String,
    pub base: String,
    pub head_sha: String,
    pub base_sha: String,
    pub state: PrState,
    pub url: String,
    pub repo_owner: String,
    pub repo_name: String,
}

/// Outcome of [`ReviewProvider::ensure_worktree`]. The host gets a
/// directory it can `cd` into and a flag telling it whether the
/// directory should be cleaned up on exit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeHandle {
    /// Absolute filesystem path the host should treat as the repo root
    /// for the duration of the review session.
    pub path: String,
    /// `true` if the host should `git worktree remove` (or similar) on
    /// exit; `false` when the user was already on the PR's branch and
    /// we reused the existing checkout in place.
    pub cleanup_on_drop: bool,
}

/// A single review comment (PR-level or line-level). Optional fields
/// allow backends that don't carry the data to omit it; the renderer
/// degrades gracefully.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewComment {
    pub author: String,
    pub body: String,
    /// File path the comment is anchored to, if any.
    pub path: Option<String>,
    /// Line number (1-based) on the *new* side of the diff, if any.
    pub line: Option<u32>,
    pub url: Option<String>,
    /// ISO-8601 timestamp; left as a string to keep this crate
    /// dependency-free and round-trip stable through JSON-RPC.
    pub created_at: String,
}

/// Errors crossing the plugin boundary. Variants are coarse on purpose —
/// each maps cleanly to a JSON-RPC error code in the eventual wire
/// version.
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "code", content = "message", rename_all = "snake_case")]
pub enum ReviewError {
    #[error("not authenticated: {0}")]
    NotAuthenticated(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("backend error: {0}")]
    Backend(String),
    #[error("bad input: {0}")]
    BadInput(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
}

pub type ProviderResult<T> = Result<T, ReviewError>;

/// The actual plugin interface.
///
/// ════════════════════════════════════════════════════════════════════════
///  PLUGIN BOUNDARY — every method here is a future JSON-RPC call.
/// ════════════════════════════════════════════════════════════════════════
///
/// When you add a method, ask: would this make sense as a single
/// network round-trip? If you find yourself wanting to pass closures,
/// references, file handles, or fine-grained polling — pull back and
/// design a coarser API.
pub trait ReviewProvider: Send + Sync {
    /// Stable identifier for the backend (`"github"`, `"gitlab"`, …).
    /// Used for routing user input like `lziff --review github:42`.
    fn id(&self) -> &'static str;

    /// Cheap precondition check: is the backend ready to use?
    /// Typically wraps `gh auth status` etc. The default
    /// implementation always succeeds; override if your backend needs
    /// auth that can't be verified lazily.
    fn check_ready(&self) -> ProviderResult<()> {
        Ok(())
    }

    /// List PRs matching the query. May be a long-ish call (a single
    /// HTTP request), but never streaming — return the whole vector.
    fn list_pull_requests(&self, query: ListQuery) -> ProviderResult<Vec<PrSummary>>;

    /// Resolve a user-supplied `PrRef` to a fully-populated [`PullRequest`].
    fn get_pull_request(&self, r: PrRef) -> ProviderResult<PullRequest>;

    /// Make sure the PR's `head` branch is checked out somewhere we can
    /// diff. Implementations should:
    ///
    /// 1. If the user is already on the PR's branch in the cwd repo,
    ///    return a [`WorktreeHandle`] pointing at the cwd with
    ///    `cleanup_on_drop = false`.
    /// 2. Otherwise create a `git worktree` under the host-supplied
    ///    `cache_root`, fetch `head_sha`, and return that path with
    ///    `cleanup_on_drop = true`.
    fn ensure_worktree(
        &self,
        pr: &PullRequest,
        cache_root: &str,
    ) -> ProviderResult<WorktreeHandle>;

    /// Load all review comments on the PR. Optional; default returns an
    /// empty list so a backend that doesn't surface comments still
    /// satisfies the trait.
    fn list_review_comments(&self, _pr: &PullRequest) -> ProviderResult<Vec<ReviewComment>> {
        Ok(Vec::new())
    }
}

// thiserror is used purely for the Display impl on ReviewError; everything
// else here is hand-written so the crate stays trivially serializable.
//
// We deliberately do NOT re-export thiserror — it's a build-time concern
// of this crate alone.
