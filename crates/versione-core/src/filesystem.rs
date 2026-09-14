//! Project-relative path validation and filesystem helpers.

use std::path::{Component, Path, PathBuf};

use crate::error::{Error, ErrorKind, Result};

/// A path stored relative to a music project root. Never absolute.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectRelativePath(String);

impl ProjectRelativePath {
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let raw = value.as_ref().replace('\\', "/");
        validate_relative(&raw)?;
        Ok(Self(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(&self.0)
    }
}

impl std::fmt::Display for ProjectRelativePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn validate_relative(raw: &str) -> Result<()> {
    if raw.is_empty() {
        return Err(Error::new(
            ErrorKind::PathEscape,
            "project-relative path must not be empty",
        ));
    }
    if raw.starts_with('/') || raw.chars().nth(1) == Some(':') {
        return Err(Error::new(
            ErrorKind::PathEscape,
            "absolute paths are not allowed in project metadata",
        )
        .with_path(raw));
    }

    let path = Path::new(raw);
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(Error::new(
                    ErrorKind::PathEscape,
                    "parent-directory components are not allowed",
                )
                .with_path(raw));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(
                    Error::new(ErrorKind::PathEscape, "path escapes the project root")
                        .with_path(raw),
                );
            }
        }
    }
    Ok(())
}

/// Resolve `relative` under `root`, ensuring the result stays inside `root`.
pub fn safe_join(root: &Path, relative: &ProjectRelativePath) -> Result<PathBuf> {
    let root = root
        .canonicalize()
        .map_err(|e| Error::io("failed to canonicalize project root", root, e))?;
    let candidate = root.join(relative.to_path_buf());
    // For paths that do not exist yet, canonicalize the parent chain carefully.
    if candidate.exists() {
        let canonical = candidate
            .canonicalize()
            .map_err(|e| Error::io("failed to canonicalize candidate path", &candidate, e))?;
        if !canonical.starts_with(&root) {
            return Err(
                Error::new(ErrorKind::PathEscape, "resolved path escapes project root")
                    .with_path(canonical),
            );
        }
        return Ok(canonical);
    }

    // Non-existent path: validate components only (already done) and return joined path
    // without following links outside the root.
    let mut acc = root.clone();
    for component in Path::new(relative.as_str()).components() {
        match component {
            Component::Normal(part) => acc.push(part),
            Component::CurDir => {}
            _ => {
                return Err(Error::new(
                    ErrorKind::PathEscape,
                    "invalid component while joining path",
                )
                .with_path(relative.as_str()));
            }
        }
    }
    Ok(acc)
}

/// Placeholder for future stable-file detection (size/mtime quiescence).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StableFilePolicy {
    /// How many consecutive unchanged observations are required.
    pub quiet_observations: u32,
}

impl Default for StableFilePolicy {
    fn default() -> Self {
        Self {
            quiet_observations: 2,
        }
    }
}

/// Filesystem watch events abstracted for later native backends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEventKind {
    Create,
    Modify,
    Remove,
    Rename,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchEvent {
    pub kind: WatchEventKind,
    pub path: PathBuf,
}

/// Trait for project filesystem monitors. Implementations belong in later phases.
pub trait FilesystemMonitor {
    fn poll(&mut self) -> Result<Vec<WatchEvent>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal() {
        assert!(ProjectRelativePath::new("../secret").is_err());
        assert!(ProjectRelativePath::new("/etc/passwd").is_err());
    }

    #[test]
    fn accepts_nested_relative() {
        let p = ProjectRelativePath::new("Samples/Kick.wav").unwrap();
        assert_eq!(p.as_str(), "Samples/Kick.wav");
    }
}
