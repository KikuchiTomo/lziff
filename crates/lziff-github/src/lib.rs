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
//! - The crate's `Cargo.toml` depends on `review-protocol` and
//!   `serde_json` only. **Adding `lziff = ...` here is a hard policy
//!   violation** — nothing in the GitHub plugin should know what's
//!   inside the host.
//! - All public types and methods are reachable through the
//!   [`ReviewProvider`] trait. The host
//!   (`crates/lziff/src/main.rs` via `crates/lziff/src/review.rs`)
//!   only ever obtains a `Box<dyn ReviewProvider>` from
//!   [`make_provider`] and never names `GithubProvider` directly.
//! - We shell out to the user's `gh` CLI (already authenticated).
//!   Going through HTTP+OAuth is left for the eventual stand-alone
//!   plugin once we hit a perf/feature wall.

use review_protocol::{
    CommentSide, ListQuery, NewComment, PrRef, PrState, PrSummary, ProviderResult, PullRequest,
    ReviewComment, ReviewError, ReviewProvider, ReviewVerdict, WorktreeHandle,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

pub fn make_provider() -> Box<dyn ReviewProvider> {
    Box::new(GithubProvider)
}

struct GithubProvider;

impl ReviewProvider for GithubProvider {
    fn id(&self) -> &'static str {
        "github"
    }

    fn check_ready(&self) -> ProviderResult<()> {
        let out = Command::new("gh")
            .args(["auth", "status"])
            .output()
            .map_err(|e| {
                ReviewError::NotAuthenticated(format!(
                    "could not run `gh` (is the GitHub CLI installed?): {e}"
                ))
            })?;
        if !out.status.success() {
            return Err(ReviewError::NotAuthenticated(
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            ));
        }
        Ok(())
    }

    fn list_pull_requests(&self, query: ListQuery) -> ProviderResult<Vec<PrSummary>> {
        let fields = "number,title,author,headRefName,baseRefName,state,url";
        let mut args = vec!["pr", "list", "--json", fields];
        if query.assigned_to_me {
            args.extend_from_slice(&["--search", "review-requested:@me state:open"]);
        }
        if let Some(state) = query.state {
            // gh accepts open|closed|merged|all — map our enum.
            let s = match state {
                PrState::Open => "open",
                PrState::Closed => "closed",
                PrState::Merged => "merged",
                PrState::All => "all",
            };
            args.extend_from_slice(&["--state", s]);
        }
        let raw = run_gh(&args)?;
        let parsed: Vec<RawPrSummary> = serde_json::from_slice(&raw)
            .map_err(|e| ReviewError::Backend(format!("parse gh pr list: {e}")))?;
        Ok(parsed.into_iter().map(RawPrSummary::into_protocol).collect())
    }

    fn get_pull_request(&self, r: PrRef) -> ProviderResult<PullRequest> {
        let arg = match &r {
            PrRef::Number(n) => n.to_string(),
            PrRef::Branch(b) => b.clone(),
            PrRef::Url(u) => u.clone(),
        };
        let fields = "number,title,body,author,headRefName,baseRefName,headRefOid,baseRefOid,state,url,headRepository,baseRepository";
        let raw = run_gh(&["pr", "view", &arg, "--json", fields])?;
        let parsed: RawPrFull = serde_json::from_slice(&raw)
            .map_err(|e| ReviewError::Backend(format!("parse gh pr view: {e}")))?;
        Ok(parsed.into_protocol())
    }

    fn ensure_worktree(
        &self,
        pr: &PullRequest,
        cache_root: &str,
    ) -> ProviderResult<WorktreeHandle> {
        // If the user is already on the PR's branch in the cwd repo, work
        // there. Cheap check: `git rev-parse --abbrev-ref HEAD`.
        if let Ok(cur) = git_current_branch() {
            if cur == pr.branch {
                let cwd = std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| ".".into());
                return Ok(WorktreeHandle {
                    path: cwd,
                    cleanup_on_drop: false,
                });
            }
        }

        // Otherwise: fetch the PR head into a refspec we own and add a
        // worktree at <cache_root>/<owner>-<repo>-<num>. Using the
        // `pull/<n>/head` refspec works on github.com without us needing
        // to know whether the PR comes from a fork.
        let dest_dir =
            format!("{}-{}-{}", pr.repo_owner, pr.repo_name, pr.number);
        let dest = Path::new(cache_root).join(&dest_dir);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ReviewError::Backend(format!("create cache dir {}: {}", parent.display(), e))
            })?;
        }

        let pr_ref = format!("pull/{}/head", pr.number);
        let local_ref = format!("refs/lziff/review/{}", pr.number);
        run_git(&[
            "fetch",
            "origin",
            &format!("+{pr_ref}:{local_ref}"),
        ])?;
        // If a stale worktree exists at this path, remove it first.
        let _ = run_git(&["worktree", "remove", "--force", dest.to_str().unwrap_or("")]);
        run_git(&[
            "worktree",
            "add",
            "--detach",
            dest.to_str().unwrap_or_default(),
            &pr.head_sha,
        ])?;
        Ok(WorktreeHandle {
            path: dest.to_string_lossy().into_owned(),
            cleanup_on_drop: true,
        })
    }

    fn list_review_comments(&self, _pr: &PullRequest) -> ProviderResult<Vec<ReviewComment>> {
        // Wire-up plan: combine
        //   `gh api repos/{owner}/{repo}/pulls/{n}/comments` (line-anchored)
        //   `gh api repos/{owner}/{repo}/issues/{n}/comments`  (PR-level)
        // Skipping for the first cut — the diff UX works without comments.
        Ok(Vec::new())
    }

    fn submit_review(
        &self,
        pr: &PullRequest,
        body: &str,
        verdict: ReviewVerdict,
        comments: Vec<NewComment>,
    ) -> ProviderResult<()> {
        if pr.repo_owner.is_empty() || pr.repo_name.is_empty() {
            return Err(ReviewError::Backend(
                "PR is missing repo owner/name (cannot submit)".into(),
            ));
        }
        // Build the JSON body matching GitHub's
        //   POST /repos/{owner}/{repo}/pulls/{n}/reviews
        let payload = ReviewPayload {
            commit_id: pr.head_sha.clone(),
            body: body.to_string(),
            event: match verdict {
                ReviewVerdict::Comment => "COMMENT",
                ReviewVerdict::Approve => "APPROVE",
                ReviewVerdict::RequestChanges => "REQUEST_CHANGES",
            }
            .to_string(),
            comments: comments.into_iter().map(NewCommentJson::from).collect(),
        };
        let body_json = serde_json::to_string(&payload)
            .map_err(|e| ReviewError::Backend(format!("encode review body: {e}")))?;
        let endpoint =
            format!("repos/{}/{}/pulls/{}/reviews", pr.repo_owner, pr.repo_name, pr.number);
        // `gh api -X POST <endpoint> --input -` reads JSON from stdin.
        let mut child = Command::new("gh")
            .args(["api", "-X", "POST", &endpoint, "--input", "-"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ReviewError::Backend(format!("spawn gh: {e}")))?;
        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin
                .write_all(body_json.as_bytes())
                .map_err(|e| ReviewError::Backend(format!("write gh stdin: {e}")))?;
        }
        let out = child
            .wait_with_output()
            .map_err(|e| ReviewError::Backend(format!("wait gh: {e}")))?;
        if !out.status.success() {
            let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(classify_gh_error(&msg));
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct ReviewPayload {
    commit_id: String,
    body: String,
    event: String,
    comments: Vec<NewCommentJson>,
}

#[derive(Serialize)]
struct NewCommentJson {
    path: String,
    line: u32,
    side: &'static str,
    body: String,
}

impl From<NewComment> for NewCommentJson {
    fn from(c: NewComment) -> Self {
        Self {
            path: c.path,
            line: c.line,
            side: match c.side {
                CommentSide::Old => "LEFT",
                CommentSide::New => "RIGHT",
            },
            body: c.body,
        }
    }
}

// ---------------------------------------------------------------------------
// Subprocess helpers

fn run_gh(args: &[&str]) -> ProviderResult<Vec<u8>> {
    let out = Command::new("gh")
        .args(args)
        .output()
        .map_err(|e| ReviewError::Backend(format!("spawn gh: {e}")))?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(classify_gh_error(&msg));
    }
    Ok(out.stdout)
}

fn run_git(args: &[&str]) -> ProviderResult<Vec<u8>> {
    let out = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| ReviewError::Backend(format!("spawn git: {e}")))?;
    if !out.status.success() {
        return Err(ReviewError::Backend(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(out.stdout)
}

fn git_current_branch() -> Result<String, ReviewError> {
    let out = run_git(&["rev-parse", "--abbrev-ref", "HEAD"])?;
    Ok(String::from_utf8_lossy(&out).trim().to_string())
}

fn classify_gh_error(msg: &str) -> ReviewError {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("not authenticated") || lower.contains("authenticate") {
        ReviewError::NotAuthenticated(msg.into())
    } else if lower.contains("no pull requests found")
        || lower.contains("not found")
        || lower.contains("could not find")
    {
        ReviewError::NotFound(msg.into())
    } else if lower.contains("network") || lower.contains("timeout") {
        ReviewError::Network(msg.into())
    } else {
        ReviewError::Backend(msg.into())
    }
}

// ---------------------------------------------------------------------------
// gh JSON shapes

#[derive(Deserialize, Default)]
struct RawAuthor {
    #[serde(default)]
    login: String,
}

#[derive(Deserialize, Default)]
struct RawRepoOwner {
    #[serde(default)]
    login: String,
}

#[derive(Deserialize, Default)]
struct RawRepo {
    #[serde(default)]
    name: String,
    #[serde(default)]
    owner: RawRepoOwner,
}

#[derive(Deserialize)]
struct RawPrSummary {
    number: u64,
    title: String,
    #[serde(default)]
    author: RawAuthor,
    #[serde(rename = "headRefName", default)]
    head_ref_name: String,
    #[serde(rename = "baseRefName", default)]
    base_ref_name: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    url: String,
}

impl RawPrSummary {
    fn into_protocol(self) -> PrSummary {
        PrSummary {
            number: self.number,
            title: self.title,
            author: self.author.login,
            branch: self.head_ref_name,
            base: self.base_ref_name,
            state: parse_state(&self.state),
            url: self.url,
        }
    }
}

#[derive(Deserialize)]
struct RawPrFull {
    number: u64,
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    author: RawAuthor,
    #[serde(rename = "headRefName", default)]
    head_ref_name: String,
    #[serde(rename = "baseRefName", default)]
    base_ref_name: String,
    #[serde(rename = "headRefOid", default)]
    head_ref_oid: String,
    #[serde(rename = "baseRefOid", default)]
    base_ref_oid: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    url: String,
    // headRepository is asked for so the JSON shape is symmetric with
    // baseRepository; we don't currently consume it because the URL
    // already gives us enough to disambiguate forks.
    #[serde(rename = "headRepository", default)]
    _head_repo: RawRepo,
    #[serde(rename = "baseRepository", default)]
    base_repo: RawRepo,
}

impl RawPrFull {
    fn into_protocol(self) -> PullRequest {
        // Prefer base repository (the PR's home repo) for owner/name.
        // gh sometimes returns blanks for forks; fall back to URL parse.
        let mut owner = self.base_repo.owner.login.clone();
        let mut name = self.base_repo.name.clone();
        if owner.is_empty() || name.is_empty() {
            if let Some((o, n)) = parse_owner_name_from_url(&self.url) {
                if owner.is_empty() {
                    owner = o;
                }
                if name.is_empty() {
                    name = n;
                }
            }
        }
        PullRequest {
            number: self.number,
            title: self.title,
            body: self.body,
            author: self.author.login,
            branch: self.head_ref_name,
            base: self.base_ref_name,
            head_sha: self.head_ref_oid,
            base_sha: self.base_ref_oid,
            state: parse_state(&self.state),
            url: self.url,
            repo_owner: owner,
            repo_name: name,
        }
    }
}

fn parse_state(s: &str) -> PrState {
    match s.to_ascii_uppercase().as_str() {
        "OPEN" => PrState::Open,
        "CLOSED" => PrState::Closed,
        "MERGED" => PrState::Merged,
        _ => PrState::All,
    }
}

/// Parse owner/repo out of a typical PR URL:
///   https://github.com/<owner>/<repo>/pull/<n>
/// Used as a fallback when `gh` doesn't surface the repository fields.
fn parse_owner_name_from_url(url: &str) -> Option<(String, String)> {
    let stripped = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let parts: Vec<&str> = stripped.split('/').collect();
    if parts.len() >= 3 && parts.first().map(|h| h.contains("github")).unwrap_or(false) {
        return Some((parts[1].to_string(), parts[2].to_string()));
    }
    None
}
