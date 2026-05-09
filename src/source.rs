//! Diff sources: where the left/right text comes from.
//!
//! `DiffSource` is the abstraction that the future plugin protocol will speak to.
//! For now we ship two in-process implementations:
//! - [`GitWorkingTree`]: lists modified files in the working tree and produces
//!   index-vs-worktree (or HEAD-vs-worktree for untracked) diffs.
//! - [`FilePair`]: ad-hoc comparison of two arbitrary files.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: String,
    pub display: String,
    pub status: String,
}

pub struct DiffPayload {
    pub left_label: String,
    pub right_label: String,
    pub left: String,
    pub right: String,
}

pub trait DiffSource {
    fn list(&self) -> Result<Vec<Entry>>;
    fn load(&self, id: &str) -> Result<DiffPayload>;
    /// Cheap fingerprint used to detect external changes between ticks.
    /// Implementations should return a different value whenever the data
    /// returned by `list()` or `load(current_id)` could have changed.
    fn signature(&self, current_id: Option<&str>) -> u64 {
        let _ = current_id;
        0
    }
}

fn mtime_ms(p: &Path) -> u64 {
    std::fs::metadata(p)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub struct FilePair {
    pub left: PathBuf,
    pub right: PathBuf,
}

impl DiffSource for FilePair {
    fn list(&self) -> Result<Vec<Entry>> {
        Ok(vec![Entry {
            id: "pair".into(),
            display: format!(
                "{}  ↔  {}",
                self.left.display(),
                self.right.display()
            ),
            status: "  ".into(),
        }])
    }

    fn load(&self, _id: &str) -> Result<DiffPayload> {
        let left = read_file(&self.left)?;
        let right = read_file(&self.right)?;
        Ok(DiffPayload {
            left_label: self.left.display().to_string(),
            right_label: self.right.display().to_string(),
            left,
            right,
        })
    }

    fn signature(&self, _: Option<&str>) -> u64 {
        mtime_ms(&self.left).wrapping_mul(1469598103934665603).wrapping_add(mtime_ms(&self.right))
    }
}

pub struct GitWorkingTree {
    pub root: PathBuf,
}

impl GitWorkingTree {
    pub fn discover(start: &Path) -> Result<Self> {
        let out = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(start)
            .output()
            .context("failed to invoke git")?;
        if !out.status.success() {
            anyhow::bail!("not inside a git repository");
        }
        let root = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(Self { root: PathBuf::from(root) })
    }
}

impl DiffSource for GitWorkingTree {
    fn list(&self) -> Result<Vec<Entry>> {
        let out = Command::new("git")
            .args(["status", "--porcelain=v1", "-z", "--untracked-files=all"])
            .current_dir(&self.root)
            .output()
            .context("git status failed")?;
        if !out.status.success() {
            anyhow::bail!(
                "git status: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        // porcelain v1 -z format: XY␠path[\0orig\0]?...
        let mut entries = Vec::new();
        let mut iter = out.stdout.split(|&b| b == 0).peekable();
        while let Some(rec) = iter.next() {
            if rec.is_empty() {
                continue;
            }
            if rec.len() < 4 {
                continue;
            }
            let xy = std::str::from_utf8(&rec[..2]).unwrap_or("??");
            let path = std::str::from_utf8(&rec[3..]).unwrap_or("").to_string();
            // Renames have a second NUL-separated original path; consume it.
            if xy.starts_with('R') || xy.starts_with('C') {
                let _ = iter.next();
            }
            entries.push(Entry {
                id: path.clone(),
                display: path,
                status: xy.to_string(),
            });
        }
        Ok(entries)
    }

    fn load(&self, id: &str) -> Result<DiffPayload> {
        let path = self.root.join(id);
        let untracked = git_is_untracked(&self.root, id)?;

        let left = if untracked {
            String::new()
        } else {
            git_show_index_or_head(&self.root, id).unwrap_or_default()
        };
        let right = if path.exists() {
            read_file(&path).unwrap_or_default()
        } else {
            String::new()
        };

        Ok(DiffPayload {
            left_label: if untracked {
                format!("(untracked)  {}", id)
            } else {
                format!("HEAD:{}", id)
            },
            right_label: format!("worktree:{}", id),
            left,
            right,
        })
    }

    fn signature(&self, current_id: Option<&str>) -> u64 {
        let mut h = mtime_ms(&self.root.join(".git").join("index"));
        h = h.wrapping_mul(1469598103934665603);
        h ^= mtime_ms(&self.root.join(".git").join("HEAD")).rotate_left(7);
        if let Some(id) = current_id {
            h ^= mtime_ms(&self.root.join(id)).rotate_left(17);
        }
        h
    }
}

fn git_is_untracked(root: &Path, path: &str) -> Result<bool> {
    let out = Command::new("git")
        .args(["ls-files", "--error-unmatch", "--", path])
        .current_dir(root)
        .output()?;
    Ok(!out.status.success())
}

fn git_show_index_or_head(root: &Path, path: &str) -> Result<String> {
    // Try index first, fall back to HEAD.
    let spec_index = format!(":{}", path);
    if let Ok(s) = git_show(root, &spec_index) {
        return Ok(s);
    }
    let spec_head = format!("HEAD:{}", path);
    git_show(root, &spec_head)
}

fn git_show(root: &Path, spec: &str) -> Result<String> {
    let out = Command::new("git")
        .args(["show", spec])
        .current_dir(root)
        .output()?;
    if !out.status.success() {
        anyhow::bail!("git show {} failed", spec);
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn read_file(p: &Path) -> Result<String> {
    let bytes = std::fs::read(p).with_context(|| format!("read {}", p.display()))?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}
