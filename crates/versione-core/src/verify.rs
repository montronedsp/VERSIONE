//! Structured integrity verification — the gate before a valid snapshot.

use std::fs;
use std::path::Path;

use crate::classify::ClassifyPolicy;
use crate::error::{Error, ErrorKind, Result};
use crate::ignore::IgnoreRules;
use crate::manifest::SnapshotManifest;
use crate::objects::{hash_file, ObjectId};
use crate::scan::scan_project;

/// Outcome category for one required object / path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerifyIssueKind {
    Verified,
    Missing,
    SizeMismatch,
    HashMismatch,
    Unreadable,
    Unsupported,
    ExternalUncollected,
    PathViolation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyFinding {
    pub path: String,
    pub kind: VerifyIssueKind,
    pub detail: Option<String>,
    pub expected_size: Option<u64>,
    pub actual_size: Option<u64>,
    pub expected_hash: Option<ObjectId>,
    pub actual_hash: Option<ObjectId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReport {
    pub findings: Vec<VerifyFinding>,
    pub verified: usize,
    pub failed: usize,
}

impl VerificationReport {
    pub fn ok(&self) -> bool {
        self.failed == 0
    }

    pub fn require_ok(&self) -> Result<()> {
        if self.ok() {
            return Ok(());
        }
        let mut lines = Vec::new();
        for f in self
            .findings
            .iter()
            .filter(|f| f.kind != VerifyIssueKind::Verified)
        {
            let detail = f.detail.as_deref().unwrap_or(f.kind_label());
            lines.push(format!("  {}: {}", f.path, detail));
        }
        Err(Error::new(
            ErrorKind::Invariant,
            format!(
                "Snapshot blocked\n\n{} required object(s) are unresolved:\n{}",
                self.failed,
                lines.join("\n")
            ),
        ))
    }
}

impl VerifyFinding {
    fn kind_label(&self) -> &'static str {
        match self.kind {
            VerifyIssueKind::Verified => "verified",
            VerifyIssueKind::Missing => "missing",
            VerifyIssueKind::SizeMismatch => "size mismatch",
            VerifyIssueKind::HashMismatch => "hash mismatch",
            VerifyIssueKind::Unreadable => "unreadable",
            VerifyIssueKind::Unsupported => "unsupported",
            VerifyIssueKind::ExternalUncollected => "external/uncollected reference",
            VerifyIssueKind::PathViolation => "path violation",
        }
    }
}

/// Verify every scanned project file is readable and hashable (pre-snapshot gate).
///
/// When content overrides exist (e.g. staged self-contained REAPER `.rpp`), those
/// bytes are verified instead of the working-tree original for that path.
pub fn verify_working_tree(root: &Path) -> Result<VerificationReport> {
    let ignore = IgnoreRules::default();
    let policy = ClassifyPolicy::default();
    let scanned = scan_project(root, &ignore, &policy)?;
    let paths = crate::project::ProjectPaths::from_root(root);
    let mut findings = Vec::with_capacity(scanned.len());
    let mut verified = 0usize;
    let mut failed = 0usize;

    for file in scanned {
        let path = file.relative.as_str().to_string();
        let content_path = if paths.config_path().exists() {
            crate::daw::reaper::resolve_content_path(&paths, file.relative.as_str())?
        } else {
            file.absolute.clone()
        };
        if !content_path.is_file() {
            failed += 1;
            findings.push(VerifyFinding {
                path,
                kind: VerifyIssueKind::Missing,
                detail: Some("expected project file is missing".into()),
                expected_size: Some(file.size),
                actual_size: None,
                expected_hash: None,
                actual_hash: None,
            });
            continue;
        }
        let meta = match fs::metadata(&content_path) {
            Ok(m) => m,
            Err(e) => {
                failed += 1;
                findings.push(VerifyFinding {
                    path,
                    kind: VerifyIssueKind::Unreadable,
                    detail: Some(format!("cannot stat: {e}")),
                    expected_size: Some(file.size),
                    actual_size: None,
                    expected_hash: None,
                    actual_hash: None,
                });
                continue;
            }
        };
        let actual_size = meta.len();
        match hash_file(&content_path) {
            Ok(hash) => {
                verified += 1;
                findings.push(VerifyFinding {
                    path,
                    kind: VerifyIssueKind::Verified,
                    detail: None,
                    expected_size: Some(actual_size),
                    actual_size: Some(actual_size),
                    expected_hash: Some(hash.clone()),
                    actual_hash: Some(hash),
                });
            }
            Err(e) => {
                failed += 1;
                findings.push(VerifyFinding {
                    path,
                    kind: VerifyIssueKind::Unreadable,
                    detail: Some(e.to_string()),
                    expected_size: Some(actual_size),
                    actual_size: Some(actual_size),
                    expected_hash: None,
                    actual_hash: None,
                });
            }
        }
    }

    Ok(VerificationReport {
        findings,
        verified,
        failed,
    })
}

/// Verify that files on disk match a snapshot manifest (existence, size, BLAKE3).
pub fn verify_manifest_on_disk(
    root: &Path,
    manifest: &SnapshotManifest,
) -> Result<VerificationReport> {
    let mut findings = Vec::with_capacity(manifest.entries.len());
    let mut verified = 0usize;
    let mut failed = 0usize;

    for entry in &manifest.entries {
        let abs = match crate::filesystem::ProjectRelativePath::new(&entry.path) {
            Ok(rel) => match crate::filesystem::safe_join(root, &rel) {
                Ok(p) => p,
                Err(e) => {
                    failed += 1;
                    findings.push(VerifyFinding {
                        path: entry.path.clone(),
                        kind: VerifyIssueKind::PathViolation,
                        detail: Some(e.to_string()),
                        expected_size: Some(entry.size),
                        actual_size: None,
                        expected_hash: Some(entry.object_id.clone()),
                        actual_hash: None,
                    });
                    continue;
                }
            },
            Err(e) => {
                failed += 1;
                findings.push(VerifyFinding {
                    path: entry.path.clone(),
                    kind: VerifyIssueKind::PathViolation,
                    detail: Some(e.to_string()),
                    expected_size: Some(entry.size),
                    actual_size: None,
                    expected_hash: Some(entry.object_id.clone()),
                    actual_hash: None,
                });
                continue;
            }
        };

        if !abs.is_file() {
            failed += 1;
            findings.push(VerifyFinding {
                path: entry.path.clone(),
                kind: VerifyIssueKind::Missing,
                detail: Some("required object file is missing".into()),
                expected_size: Some(entry.size),
                actual_size: None,
                expected_hash: Some(entry.object_id.clone()),
                actual_hash: None,
            });
            continue;
        }

        let actual_size = match fs::metadata(&abs) {
            Ok(m) => m.len(),
            Err(e) => {
                failed += 1;
                findings.push(VerifyFinding {
                    path: entry.path.clone(),
                    kind: VerifyIssueKind::Unreadable,
                    detail: Some(format!("cannot stat: {e}")),
                    expected_size: Some(entry.size),
                    actual_size: None,
                    expected_hash: Some(entry.object_id.clone()),
                    actual_hash: None,
                });
                continue;
            }
        };

        if actual_size != entry.size {
            failed += 1;
            findings.push(VerifyFinding {
                path: entry.path.clone(),
                kind: VerifyIssueKind::SizeMismatch,
                detail: Some(format!("expected size {}, found {actual_size}", entry.size)),
                expected_size: Some(entry.size),
                actual_size: Some(actual_size),
                expected_hash: Some(entry.object_id.clone()),
                actual_hash: None,
            });
            continue;
        }

        match hash_file(&abs) {
            Ok(hash) if hash == entry.object_id => {
                verified += 1;
                findings.push(VerifyFinding {
                    path: entry.path.clone(),
                    kind: VerifyIssueKind::Verified,
                    detail: None,
                    expected_size: Some(entry.size),
                    actual_size: Some(actual_size),
                    expected_hash: Some(entry.object_id.clone()),
                    actual_hash: Some(hash),
                });
            }
            Ok(hash) => {
                failed += 1;
                findings.push(VerifyFinding {
                    path: entry.path.clone(),
                    kind: VerifyIssueKind::HashMismatch,
                    detail: Some("BLAKE3 digest does not match manifest".into()),
                    expected_size: Some(entry.size),
                    actual_size: Some(actual_size),
                    expected_hash: Some(entry.object_id.clone()),
                    actual_hash: Some(hash),
                });
            }
            Err(e) => {
                failed += 1;
                findings.push(VerifyFinding {
                    path: entry.path.clone(),
                    kind: VerifyIssueKind::Unreadable,
                    detail: Some(e.to_string()),
                    expected_size: Some(entry.size),
                    actual_size: Some(actual_size),
                    expected_hash: Some(entry.object_id.clone()),
                    actual_hash: None,
                });
            }
        }
    }

    Ok(VerificationReport {
        findings,
        verified,
        failed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{init_project, InitOptions};
    use tempfile::tempdir;

    #[test]
    fn working_tree_all_valid() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("proj");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Track.als"), b"set").unwrap();
        init_project(&root, &InitOptions::default()).unwrap();
        let report = verify_working_tree(&root).unwrap();
        assert!(report.ok());
        assert!(report.verified >= 1);
    }

    #[test]
    fn missing_file_fails_manifest_verify() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("proj");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), b"hello").unwrap();
        init_project(&root, &InitOptions::default()).unwrap();
        let report = verify_working_tree(&root).unwrap();
        assert!(report.ok());
        // Build a fake manifest entry pointing at a missing path.
        let hash = hash_file(&root.join("a.txt")).unwrap();
        let entry = crate::manifest::ManifestEntry {
            path: "gone.txt".into(),
            object_id: hash,
            size: 5,
            class: crate::classify::FileClass::GitEligible,
        };
        let manifest = SnapshotManifest::from_sorted_entries(
            crate::snapshot::SnapshotId::new(),
            Vec::new(),
            vec![entry],
        )
        .unwrap();
        let bad = verify_manifest_on_disk(&root, &manifest).unwrap();
        assert!(!bad.ok());
        assert_eq!(bad.findings[0].kind, VerifyIssueKind::Missing);
    }
}
