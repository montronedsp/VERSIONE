//! Typed errors with enough context for maintainers and CLI users.

use std::path::PathBuf;

use thiserror::Error;

/// Result alias for VERSIONE core operations.
pub type Result<T> = std::result::Result<T, Error>;

/// High-level failure category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    NotFound,
    AlreadyExists,
    PermissionDenied,
    InvalidProject,
    UnsupportedFormat,
    Corruption,
    PathEscape,
    InvalidObjectId,
    Io,
    Cancelled,
    Invariant,
    /// Git command/backend reported a failure.
    Git,
    /// Git executable or repository is unavailable (including offline tooling gaps).
    GitUnavailable,
    /// Object store backend failure distinct from generic I/O.
    Storage,
    /// Publication ordering / gate violation.
    Publication,
}

/// Rich error carrying category, message, and optional path context.
#[derive(Debug, Error)]
#[error("{kind:?}: {message}{}", format_path(.path))]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
    pub path: Option<PathBuf>,
    #[source]
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

fn format_path(path: &Option<PathBuf>) -> String {
    match path {
        Some(p) => format!(" ({})", p.display()),
        None => String::new(),
    }
}

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            path: None,
            source: None,
        }
    }

    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn with_source(mut self, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    pub fn io(
        message: impl Into<String>,
        path: impl Into<PathBuf>,
        source: std::io::Error,
    ) -> Self {
        let kind = match source.kind() {
            std::io::ErrorKind::NotFound => ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
            std::io::ErrorKind::AlreadyExists => ErrorKind::AlreadyExists,
            _ => ErrorKind::Io,
        };
        Self {
            kind,
            message: message.into(),
            path: Some(path.into()),
            source: Some(Box::new(source)),
        }
    }
}
