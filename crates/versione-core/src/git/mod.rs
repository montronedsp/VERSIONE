//! Narrow Git abstraction used by VERSIONE.
//!
//! Git is infrastructure for lightweight history and collaboration.
//! Musician-facing APIs must not expose plumbing vocabulary.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorKind, Result};

/// Path to a Git working tree used by a VERSIONE project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRepository {
    root: PathBuf,
}

/// Opaque commit identifier (Git object name). Not shown as UX vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GitCommit(String);

impl GitCommit {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitBranch {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitRemote {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitStatus {
    pub initialized: bool,
    pub clean: bool,
    pub branch: Option<String>,
    pub remotes: Vec<GitRemote>,
}

impl GitRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Initialize a Git repository at the project root when one is not present.
    pub fn ensure_initialized(&self) -> Result<()> {
        let status = self.detect_status()?;
        if status.initialized {
            return Ok(());
        }
        self.git_run(&["init"])?;
        // Local-only identity for automated VERSIONE metadata commits.
        self.git_run(&["config", "user.name", "VERSIONE"])?;
        self.git_run(&["config", "user.email", "versione@local"])?;
        Ok(())
    }

    /// Stage and commit only intended `.versione` metadata (never object bytes).
    pub fn commit_versione_metadata(&self, message: &str) -> Result<Option<GitCommit>> {
        self.ensure_initialized()?;
        // Explicit paths: do not `git add -A` the music project tree.
        // Skip pathspecs that do not exist yet (optional Music Memory files).
        const METADATA_PATHS: &[&str] = &[
            ".versione/config.toml",
            ".versione/storage.toml",
            ".versione/state.toml",
            ".versione/lifecycle.toml",
            ".versione/collection.toml",
            ".versione/content_overrides.toml",
            ".versione/staging",
            ".versione/profile.toml",
            ".versione/notes.toml",
            ".versione/tasks.toml",
            ".versione/media.toml",
            ".versione/metrics.toml",
            ".versione/manifests",
            ".versione/refs",
            ".versione/metadata",
        ];
        for relative in METADATA_PATHS {
            if self.root.join(relative).exists() {
                let _ = self.git_run(&["add", "-f", "--", relative]);
            }
        }

        let porcelain = self.git_output(&["status", "--porcelain", "--", ".versione"])?;
        if porcelain.trim().is_empty() {
            return Ok(None);
        }

        self.git_run(&["commit", "-m", message])?;
        let sha = self.git_output(&["rev-parse", "HEAD"])?;
        Ok(Some(GitCommit::new(sha.trim())))
    }

    /// Push the current branch to its configured upstream (fast-forward only; never force).
    pub fn push_upstream(&self) -> Result<()> {
        self.ensure_initialized()?;
        let status = self.detect_status()?;
        if status.remotes.is_empty() {
            return Err(Error::new(
                ErrorKind::Git,
                "no Git remote configured; add a remote before publishing",
            )
            .with_path(&self.root));
        }
        // Explicitly forbid force pushes in the VERSIONE Git abstraction.
        self.git_run(&["push", "--porcelain"])?;
        Ok(())
    }

    /// Probe whether `root` is inside a Git work tree (requires `git` on PATH).
    pub fn detect_status(&self) -> Result<GitStatus> {
        if !self.root.exists() {
            return Err(
                Error::new(ErrorKind::NotFound, "Git repository path does not exist")
                    .with_path(&self.root),
            );
        }

        let inside = self.git_output(&["rev-parse", "--is-inside-work-tree"]);
        match inside {
            Ok(text) if text.trim() == "true" => {
                let branch = self
                    .git_output(&["branch", "--show-current"])
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let clean = self
                    .git_output(&["status", "--porcelain"])
                    .map(|s| s.trim().is_empty())
                    .unwrap_or(false);
                let remotes = self.list_remotes().unwrap_or_default();
                Ok(GitStatus {
                    initialized: true,
                    clean,
                    branch,
                    remotes,
                })
            }
            Ok(_) | Err(_) => Ok(GitStatus {
                initialized: false,
                ..GitStatus::default()
            }),
        }
    }

    pub fn list_remotes(&self) -> Result<Vec<GitRemote>> {
        let output = self.git_output(&["remote", "-v"])?;
        let mut remotes = Vec::new();
        for line in output.lines() {
            let mut parts = line.split_whitespace();
            let Some(name) = parts.next() else { continue };
            let Some(url) = parts.next() else { continue };
            let Some(kind) = parts.next() else { continue };
            if kind == "(fetch)" {
                remotes.push(GitRemote {
                    name: name.to_string(),
                    url: url.to_string(),
                });
            }
        }
        Ok(remotes)
    }

    fn git_run(&self, args: &[&str]) -> Result<()> {
        let _ = self.git_output(args)?;
        Ok(())
    }

    fn git_output(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .env("GIT_AUTHOR_NAME", "VERSIONE")
            .env("GIT_AUTHOR_EMAIL", "versione@local")
            .env("GIT_COMMITTER_NAME", "VERSIONE")
            .env("GIT_COMMITTER_EMAIL", "versione@local")
            .output()
            .map_err(|e| {
                Error::new(
                    ErrorKind::GitUnavailable,
                    "failed to execute git; offline Git tooling may be missing",
                )
                .with_source(e)
                .with_path(&self.root)
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(Error::new(
                ErrorKind::Git,
                if stderr.is_empty() {
                    format!("git {} failed", args.join(" "))
                } else {
                    stderr
                },
            )
            .with_path(&self.root));
        }

        String::from_utf8(output.stdout).map_err(|e| {
            Error::new(ErrorKind::Git, "git output was not valid UTF-8").with_source(e)
        })
    }
}

/// Capabilities VERSIONE will eventually require from a Git backend.
pub trait GitBackend {
    fn status(&self) -> Result<GitStatus>;
    fn remotes(&self) -> Result<Vec<GitRemote>>;
}

impl GitBackend for GitRepository {
    fn status(&self) -> Result<GitStatus> {
        self.detect_status()
    }

    fn remotes(&self) -> Result<Vec<GitRemote>> {
        if !self.detect_status()?.initialized {
            return Ok(Vec::new());
        }
        self.list_remotes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn non_git_directory_reports_uninitialized() {
        let dir = tempdir().unwrap();
        let repo = GitRepository::new(dir.path());
        let status = repo.detect_status().unwrap();
        assert!(!status.initialized);
    }

    #[test]
    fn ensure_initialized_creates_repo() {
        let dir = tempdir().unwrap();
        let repo = GitRepository::new(dir.path());
        repo.ensure_initialized().unwrap();
        assert!(repo.detect_status().unwrap().initialized);
    }
}
