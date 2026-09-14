//! Deterministic project tree scanning for checkpoints.

use std::fs;
use std::path::{Path, PathBuf};

use crate::classify::{ClassifyPolicy, FileClass};
use crate::error::{Error, ErrorKind, Result};
use crate::filesystem::ProjectRelativePath;
use crate::ignore::IgnoreRules;

/// A single scanned project file ready for hashing/ingestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedFile {
    pub relative: ProjectRelativePath,
    pub absolute: PathBuf,
    pub size: u64,
    pub class: FileClass,
}

/// Walk `root`, applying ignore rules and classification. Results are sorted by path.
pub fn scan_project(
    root: &Path,
    ignore: &IgnoreRules,
    policy: &ClassifyPolicy,
) -> Result<Vec<ScannedFile>> {
    if !root.is_dir() {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            "project root must be a directory",
        )
        .with_path(root));
    }

    let mut files = Vec::new();
    walk(root, root, ignore, policy, &mut files)?;
    files.sort_by(|a, b| a.relative.as_str().cmp(b.relative.as_str()));
    Ok(files)
}

fn walk(
    root: &Path,
    dir: &Path,
    ignore: &IgnoreRules,
    policy: &ClassifyPolicy,
    out: &mut Vec<ScannedFile>,
) -> Result<()> {
    let entries =
        fs::read_dir(dir).map_err(|e| Error::io("failed to read project directory", dir, e))?;
    let mut collected = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| Error::io("failed to read directory entry", dir, e))?;
        collected.push(entry);
    }
    collected.sort_by_key(|e| e.file_name());

    for entry in collected {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        let rel = path.strip_prefix(root).map_err(|_| {
            Error::new(ErrorKind::PathEscape, "scanned path escaped project root").with_path(&path)
        })?;
        let rel_str = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");

        if ignore.ignores_path_str(&rel_str) || name_str == ".git" {
            continue;
        }

        let file_type = entry
            .file_type()
            .map_err(|e| Error::io("failed to read file type", &path, e))?;
        if file_type.is_symlink() {
            return Err(Error::new(
                ErrorKind::InvalidProject,
                "symbolic links are not supported in Phase 1 project scans",
            )
            .with_path(path));
        }
        if file_type.is_dir() {
            walk(root, &path, ignore, policy, out)?;
            continue;
        }
        if !file_type.is_file() {
            return Err(Error::new(
                ErrorKind::InvalidProject,
                "unsupported filesystem entry type during project scan",
            )
            .with_path(path));
        }

        let relative = ProjectRelativePath::new(&rel_str)?;
        let meta = fs::metadata(&path)
            .map_err(|e| Error::io("failed to read file metadata during scan", &path, e))?;
        let size = meta.len();
        let class = policy.classify(&relative, size);
        out.push(ScannedFile {
            relative,
            absolute: path,
            size,
            class,
        });
    }
    Ok(())
}
