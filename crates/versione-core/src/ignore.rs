//! Deterministic ignore / exclusion rules for project walks.

use std::path::{Path, PathBuf};

use crate::filesystem::ProjectRelativePath;

/// Rule set applied when enumerating project files for snapshotting.
#[derive(Debug, Clone)]
pub struct IgnoreRules {
    exact_names: Vec<String>,
    suffixes: Vec<String>,
}

impl Default for IgnoreRules {
    fn default() -> Self {
        Self {
            exact_names: vec![
                ".versione".into(),
                ".DS_Store".into(),
                "Thumbs.db".into(),
                "desktop.ini".into(),
            ],
            suffixes: vec![
                "~".into(),
                ".tmp".into(),
                ".temp".into(),
                ".versione-partial".into(),
            ],
        }
    }
}

impl IgnoreRules {
    pub fn ignores_relative(&self, path: &ProjectRelativePath) -> bool {
        self.ignores_path_str(path.as_str())
    }

    pub fn ignores_path(&self, path: &Path) -> bool {
        let as_str = path
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        self.ignores_path_str(&as_str)
    }

    /// Forward-slash path string form used by the scanner.
    pub fn ignores_path_str(&self, path: &str) -> bool {
        let normalized = path.replace('\\', "/");
        for part in normalized.split('/') {
            if self.exact_names.iter().any(|n| n == part) {
                return true;
            }
        }
        let file_name = PathBuf::from(&normalized)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.suffixes
            .iter()
            .any(|suffix| file_name.ends_with(suffix))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_versione_dir_and_os_junk() {
        let rules = IgnoreRules::default();
        assert!(rules.ignores_relative(&ProjectRelativePath::new(".versione/config.toml").unwrap()));
        assert!(rules.ignores_relative(&ProjectRelativePath::new("Thumbs.db").unwrap()));
        assert!(!rules.ignores_relative(&ProjectRelativePath::new("Song.als").unwrap()));
    }
}
