//! Diff sources: where the left/right text comes from.
//!
//! `DiffSource` is the abstraction that the future plugin protocol will speak
//! to. We ship two in-process implementations:
//!
//! - [`GitSource`] — git-backed, with several modes (working tree vs index,
//!   index vs HEAD, a single commit vs its parent, two arbitrary refs).
//! - [`FilePair`] — ad-hoc comparison of two arbitrary files.

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
    /// Whether to render the Files panel. Sources that only ever produce a
    /// single fixed entry (file pair) can hide it for a roomier diff.
    fn show_files_panel(&self) -> bool {
        true
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

    fn show_files_panel(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone)]
pub enum GitMode {
    /// `git status` working tree vs (index ∪ HEAD).
    WorkingTree,
    /// `git diff --cached`: index vs HEAD.
    Staged,
    /// What this commit changed: `<rev>~` vs `<rev>`.
    Commit { rev: String },
    /// Diff between two arbitrary refs: `<from>` vs `<to>`.
    Range { from: String, to: String },
}

pub struct GitSource {
    pub root: PathBuf,
    pub mode: GitMode,
}

impl GitSource {
    pub fn working_tree(start: &Path) -> Result<Self> {
        Ok(Self {
            root: discover_root(start)?,
            mode: GitMode::WorkingTree,
        })
    }

    pub fn staged(start: &Path) -> Result<Self> {
        Ok(Self {
            root: discover_root(start)?,
            mode: GitMode::Staged,
        })
    }

    pub fn commit(start: &Path, rev: impl Into<String>) -> Result<Self> {
        Ok(Self {
            root: discover_root(start)?,
            mode: GitMode::Commit { rev: rev.into() },
        })
    }

    pub fn range(start: &Path, from: impl Into<String>, to: impl Into<String>) -> Result<Self> {
        Ok(Self {
            root: discover_root(start)?,
            mode: GitMode::Range {
                from: from.into(),
                to: to.into(),
            },
        })
    }
}

fn discover_root(start: &Path) -> Result<PathBuf> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(start)
        .output()
        .context("failed to invoke git")?;
    if !out.status.success() {
        anyhow::bail!("not inside a git repository");
    }
    let root = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(PathBuf::from(root))
}

impl DiffSource for GitSource {
    fn list(&self) -> Result<Vec<Entry>> {
        match &self.mode {
            GitMode::WorkingTree => list_working_tree(&self.root),
            GitMode::Staged => list_diff_name_status(
                &self.root,
                &["diff", "--cached", "--name-status", "-z", "--no-renames"],
            ),
            GitMode::Commit { rev } => list_diff_name_status(
                &self.root,
                &[
                    "diff",
                    "--name-status",
                    "-z",
                    "--no-renames",
                    &format!("{rev}~"),
                    rev,
                ],
            ),
            GitMode::Range { from, to } => list_diff_name_status(
                &self.root,
                &[
                    "diff",
                    "--name-status",
                    "-z",
                    "--no-renames",
                    from,
                    to,
                ],
            ),
        }
    }

    fn load(&self, id: &str) -> Result<DiffPayload> {
        match &self.mode {
            GitMode::WorkingTree => load_working_tree(&self.root, id),
            GitMode::Staged => Ok(DiffPayload {
                left_label: format!("HEAD:{id}"),
                right_label: format!("index:{id}"),
                left: git_show(&self.root, &format!("HEAD:{id}")).unwrap_or_default(),
                right: git_show(&self.root, &format!(":{id}")).unwrap_or_default(),
            }),
            GitMode::Commit { rev } => {
                let parent = format!("{rev}~");
                Ok(DiffPayload {
                    left_label: format!("{parent}:{id}"),
                    right_label: format!("{rev}:{id}"),
                    left: git_show(&self.root, &format!("{parent}:{id}")).unwrap_or_default(),
                    right: git_show(&self.root, &format!("{rev}:{id}")).unwrap_or_default(),
                })
            }
            GitMode::Range { from, to } => Ok(DiffPayload {
                left_label: format!("{from}:{id}"),
                right_label: format!("{to}:{id}"),
                left: git_show(&self.root, &format!("{from}:{id}")).unwrap_or_default(),
                right: git_show(&self.root, &format!("{to}:{id}")).unwrap_or_default(),
            }),
        }
    }

    fn signature(&self, current_id: Option<&str>) -> u64 {
        match &self.mode {
            GitMode::WorkingTree => {
                let mut h = mtime_ms(&self.root.join(".git").join("index"));
                h = h.wrapping_mul(1469598103934665603);
                h ^= mtime_ms(&self.root.join(".git").join("HEAD")).rotate_left(7);
                if let Some(id) = current_id {
                    h ^= mtime_ms(&self.root.join(id)).rotate_left(17);
                }
                h
            }
            GitMode::Staged => {
                let mut h = mtime_ms(&self.root.join(".git").join("index"));
                h = h.wrapping_mul(1469598103934665603);
                h ^= mtime_ms(&self.root.join(".git").join("HEAD")).rotate_left(7);
                h
            }
            // Commit/Range refer to immutable snapshots — nothing to poll.
            GitMode::Commit { .. } | GitMode::Range { .. } => 0,
        }
    }
}

fn list_working_tree(root: &Path) -> Result<Vec<Entry>> {
    let out = Command::new("git")
        .args(["status", "--porcelain=v1", "-z", "--untracked-files=all"])
        .current_dir(root)
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

/// Parse `git diff --name-status -z` output into entries.
/// Each record is `<status>\t<path>\0`. With `--no-renames` we don't need to
/// worry about the two-path rename form.
fn list_diff_name_status(root: &Path, args: &[&str]) -> Result<Vec<Entry>> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .context("git diff failed")?;
    if !out.status.success() {
        anyhow::bail!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let mut entries = Vec::new();
    for rec in out.stdout.split(|&b| b == 0) {
        if rec.is_empty() {
            continue;
        }
        let s = std::str::from_utf8(rec).unwrap_or("");
        let mut split = s.splitn(2, '\t');
        let status = split.next().unwrap_or("?").to_string();
        let path = split.next().unwrap_or("").to_string();
        if path.is_empty() {
            continue;
        }
        // Pad status to 2 chars so the renderer's `.chars().next()` check
        // and existing color mapping behave the same as porcelain output.
        let status_padded = if status.len() == 1 {
            format!("{status} ")
        } else {
            status
        };
        entries.push(Entry {
            id: path.clone(),
            display: path,
            status: status_padded,
        });
    }
    Ok(entries)
}

fn load_working_tree(root: &Path, id: &str) -> Result<DiffPayload> {
    let path = root.join(id);
    let untracked = git_is_untracked(root, id)?;

    let left = if untracked {
        String::new()
    } else {
        git_show_index_or_head(root, id).unwrap_or_default()
    };
    let right = if path.exists() {
        read_file(&path).unwrap_or_default()
    } else {
        String::new()
    };

    Ok(DiffPayload {
        left_label: if untracked {
            format!("(untracked)  {id}")
        } else {
            format!("HEAD:{id}")
        },
        right_label: format!("worktree:{id}"),
        left,
        right,
    })
}

fn git_is_untracked(root: &Path, path: &str) -> Result<bool> {
    let out = Command::new("git")
        .args(["ls-files", "--error-unmatch", "--", path])
        .current_dir(root)
        .output()?;
    Ok(!out.status.success())
}

fn git_show_index_or_head(root: &Path, path: &str) -> Result<String> {
    let spec_index = format!(":{path}");
    if let Ok(s) = git_show(root, &spec_index) {
        return Ok(s);
    }
    let spec_head = format!("HEAD:{path}");
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
