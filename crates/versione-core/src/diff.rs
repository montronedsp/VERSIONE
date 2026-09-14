//! Generic manifest-level diff.
//!
//! DAW-specific semantic interpretation (tempo, tracks, clips) belongs in adapters.

use std::collections::{BTreeMap, BTreeSet};

use crate::manifest::SnapshotManifest;
use crate::objects::ObjectId;

/// Difference for a single project-relative path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathDiff {
    Added { object_id: ObjectId },
    Removed { object_id: ObjectId },
    Changed { from: ObjectId, to: ObjectId },
}

/// Compare two manifests by path → object id without DAW semantics.
pub fn diff_manifests(
    before: &SnapshotManifest,
    after: &SnapshotManifest,
) -> BTreeMap<String, PathDiff> {
    let before_map: BTreeMap<_, _> = before
        .entries
        .iter()
        .map(|e| (e.path.clone(), e.object_id.clone()))
        .collect();
    let after_map: BTreeMap<_, _> = after
        .entries
        .iter()
        .map(|e| (e.path.clone(), e.object_id.clone()))
        .collect();

    let mut keys = BTreeSet::new();
    keys.extend(before_map.keys().cloned());
    keys.extend(after_map.keys().cloned());

    let mut out = BTreeMap::new();
    for key in keys {
        match (before_map.get(&key), after_map.get(&key)) {
            (None, Some(to)) => {
                out.insert(
                    key,
                    PathDiff::Added {
                        object_id: to.clone(),
                    },
                );
            }
            (Some(from), None) => {
                out.insert(
                    key,
                    PathDiff::Removed {
                        object_id: from.clone(),
                    },
                );
            }
            (Some(from), Some(to)) if from != to => {
                out.insert(
                    key,
                    PathDiff::Changed {
                        from: from.clone(),
                        to: to.clone(),
                    },
                );
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::FileClass;
    use crate::manifest::{ManifestEntry, SnapshotManifest};
    use crate::objects::ObjectId;
    use crate::snapshot::SnapshotId;

    fn oid(hex64: &str) -> ObjectId {
        ObjectId::parse(hex64).unwrap()
    }

    #[test]
    fn detects_add_remove_change() {
        let a = oid(&"a".repeat(64));
        let b = oid(&"b".repeat(64));
        let c = oid(&"c".repeat(64));

        let before = SnapshotManifest::from_sorted_entries(
            SnapshotId::new(),
            vec![],
            vec![
                ManifestEntry {
                    path: "keep.wav".into(),
                    object_id: a.clone(),
                    size: 1,
                    class: FileClass::LargeObject,
                },
                ManifestEntry {
                    path: "old.wav".into(),
                    object_id: b,
                    size: 1,
                    class: FileClass::LargeObject,
                },
                ManifestEntry {
                    path: "changed.wav".into(),
                    object_id: a.clone(),
                    size: 1,
                    class: FileClass::LargeObject,
                },
            ],
        )
        .unwrap();

        let after = SnapshotManifest::from_sorted_entries(
            SnapshotId::new(),
            vec![],
            vec![
                ManifestEntry {
                    path: "keep.wav".into(),
                    object_id: a,
                    size: 1,
                    class: FileClass::LargeObject,
                },
                ManifestEntry {
                    path: "new.wav".into(),
                    object_id: c.clone(),
                    size: 1,
                    class: FileClass::LargeObject,
                },
                ManifestEntry {
                    path: "changed.wav".into(),
                    object_id: c,
                    size: 1,
                    class: FileClass::LargeObject,
                },
            ],
        )
        .unwrap();

        let diff = diff_manifests(&before, &after);
        assert!(matches!(
            diff.get("old.wav"),
            Some(PathDiff::Removed { .. })
        ));
        assert!(matches!(diff.get("new.wav"), Some(PathDiff::Added { .. })));
        assert!(matches!(
            diff.get("changed.wav"),
            Some(PathDiff::Changed { .. })
        ));
        assert!(!diff.contains_key("keep.wav"));
    }
}
