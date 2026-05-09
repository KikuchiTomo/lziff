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
//! - `make_provider` returns a trait object so the rest of the host
//!   never sees the concrete type. When we eventually swap an
//!   in-process backend for a JSON-RPC subprocess, the only file that
//!   has to change is this one.
//! - Don't fan out: a future `make_gitlab_provider` lives next to
//!   `make_github_provider`, both produce `Box<dyn ReviewProvider>`.
//!
//! The function is currently dead — no code path reaches it yet. The
//! `--review` CLI flow will land in a follow-up commit; this scaffolding
//! exists so that flow has somewhere to go without touching the rest of
//! the binary.

use review_protocol::ReviewProvider;

/// Pick the right review backend by id. Returns `None` if the requested
/// backend isn't compiled in (e.g., `lziff` built without the `github`
/// feature) or isn't recognized.
#[allow(dead_code)]
pub fn make_provider(id: &str) -> Option<Box<dyn ReviewProvider>> {
    match id {
        #[cfg(feature = "github")]
        "github" => Some(lziff_github::make_provider()),
        _ => None,
    }
}
