//! GitHub review backend for lziff.
//!
//! ════════════════════════════════════════════════════════════════════════
//!  FUTURE PLUGIN — DO NOT IMPORT FROM `lziff` INTERNALS
//! ════════════════════════════════════════════════════════════════════════
//!
//! This crate is the GitHub-specific [`ReviewProvider`] implementation.
//! It will eventually be split out into a separate process speaking the
//! `review-protocol` JSON-RPC over stdio. To keep that future viable:
//!
//! - The crate's `Cargo.toml` depends on `review-protocol` and `serde_json`
//!   only. **Adding `lziff = ...` here is a hard policy violation** —
//!   nothing in the GitHub plugin should know what's inside the host.
//! - All public types and methods are reachable through the
//!   [`ReviewProvider`] trait. The host (`crates/lziff/src/main.rs`) only
//!   ever obtains a `Box<dyn ReviewProvider>` from [`make_provider`] and
//!   never names `GithubProvider` directly.
//! - We shell out to the user's `gh` CLI (already authenticated). Going
//!   through HTTP+OAuth is left for the eventual stand-alone plugin.
//!
//! The current implementation is a skeleton. Methods return
//! [`ReviewError::Unsupported`] until the matching `gh` command is
//! wired up. Filling in the bodies is the next iteration.

use review_protocol::{
    ListQuery, PrRef, PrSummary, ProviderResult, PullRequest, ReviewComment, ReviewError,
    ReviewProvider, WorktreeHandle,
};
use std::process::Command;

/// Build the GitHub backend. The host calls this and stores the result
/// as `Box<dyn ReviewProvider>` — the only crossing point between
/// `lziff` and `lziff-github`.
pub fn make_provider() -> Box<dyn ReviewProvider> {
    Box::new(GithubProvider::new())
}

#[derive(Default)]
struct GithubProvider;

impl GithubProvider {
    fn new() -> Self {
        Self
    }
}

impl ReviewProvider for GithubProvider {
    fn id(&self) -> &'static str {
        "github"
    }

    fn check_ready(&self) -> ProviderResult<()> {
        // `gh auth status` exits non-zero when the user isn't logged in.
        let out = Command::new("gh")
            .args(["auth", "status"])
            .output()
            .map_err(|e| {
                ReviewError::NotAuthenticated(format!(
                    "couldn't run `gh` (is the GitHub CLI installed?): {e}"
                ))
            })?;
        if !out.status.success() {
            return Err(ReviewError::NotAuthenticated(
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            ));
        }
        Ok(())
    }

    fn list_pull_requests(&self, _query: ListQuery) -> ProviderResult<Vec<PrSummary>> {
        // Wire-up plan: `gh pr list --json number,title,author,headRefName,
        // baseRefName,state,url --search review-requested:@me`. Parse with
        // serde_json into PrSummary.
        Err(ReviewError::Unsupported(
            "GithubProvider::list_pull_requests not yet implemented".into(),
        ))
    }

    fn get_pull_request(&self, _r: PrRef) -> ProviderResult<PullRequest> {
        // Wire-up plan: `gh pr view <ref> --json number,title,body,
        // author,headRefName,baseRefName,headRefOid,baseRefOid,state,url,
        // headRepository,...`. URL → `gh pr view --repo <owner/name> <num>`.
        Err(ReviewError::Unsupported(
            "GithubProvider::get_pull_request not yet implemented".into(),
        ))
    }

    fn ensure_worktree(
        &self,
        _pr: &PullRequest,
        _cache_root: &str,
    ) -> ProviderResult<WorktreeHandle> {
        // Wire-up plan:
        //   if `git rev-parse --abbrev-ref HEAD` == pr.branch:
        //       reuse cwd, cleanup_on_drop = false
        //   else:
        //       fetch refs/pull/<num>/head into a tmp ref
        //       git worktree add <cache_root>/<owner>-<repo>-<num> <tmp ref>
        //       cleanup_on_drop = true
        Err(ReviewError::Unsupported(
            "GithubProvider::ensure_worktree not yet implemented".into(),
        ))
    }

    fn list_review_comments(&self, _pr: &PullRequest) -> ProviderResult<Vec<ReviewComment>> {
        // Wire-up plan: combine `gh api repos/{owner}/{repo}/pulls/{n}/comments`
        // (line-anchored) with `gh api repos/{owner}/{repo}/issues/{n}/comments`
        // (PR-level).
        Ok(Vec::new())
    }
}
