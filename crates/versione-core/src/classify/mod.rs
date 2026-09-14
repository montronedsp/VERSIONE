//! Configurable small-vs-large file classification.
//!
//! Large media generally uses external object storage even when below GitHub limits.
//! Classification must not hard-code GitHub quotas into the architecture.

use serde::{Deserialize, Serialize};

use crate::filesystem::ProjectRelativePath;

/// Where file bytes should be persisted for a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileClass {
    /// Eligible to live in the Git-tracked project tree when policy allows.
    GitEligible,
    /// Must use VERSIONE object storage (never normal Git blobs for media).
    LargeObject,
}

/// Policy inputs for classification (extensible; not GitHub-limit driven).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassifyPolicy {
    /// Files at or above this size (bytes) default to LargeObject.
    pub size_threshold_bytes: u64,
    /// Extensions (lowercase, with dot) forced to LargeObject.
    pub large_extensions: Vec<String>,
}

impl Default for ClassifyPolicy {
    fn default() -> Self {
        Self {
            // Conservative default for music projects; not a GitHub API limit.
            size_threshold_bytes: 1_048_576,
            large_extensions: vec![
                ".wav".into(),
                ".wave".into(),
                ".aiff".into(),
                ".aif".into(),
                ".flac".into(),
                ".bwf".into(),
                ".caf".into(),
                ".rf64".into(),
            ],
        }
    }
}

impl ClassifyPolicy {
    pub fn classify(&self, path: &ProjectRelativePath, size: u64) -> FileClass {
        let lower = path.as_str().to_ascii_lowercase();
        if self
            .large_extensions
            .iter()
            .any(|ext| lower.ends_with(ext.as_str()))
        {
            return FileClass::LargeObject;
        }
        if size >= self.size_threshold_bytes {
            return FileClass::LargeObject;
        }
        FileClass::GitEligible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_is_large_even_when_small() {
        let policy = ClassifyPolicy::default();
        let path = ProjectRelativePath::new("Samples/kick.wav").unwrap();
        assert_eq!(policy.classify(&path, 100), FileClass::LargeObject);
    }

    #[test]
    fn small_text_is_git_eligible() {
        let policy = ClassifyPolicy::default();
        let path = ProjectRelativePath::new("notes.txt").unwrap();
        assert_eq!(policy.classify(&path, 200), FileClass::GitEligible);
    }
}
