//! Read-only REAPER `.rpp` inspection, reference discovery, and staged rewrite.
//!
//! The user's canonical `.rpp` is never mutated. Self-contained snapshot bytes
//! are written under `.versione/staging/` and ingested via content overrides.

mod parser;
mod prepare;
mod rewrite;

pub use parser::{
    discover_references, inspect_rpp, parse_rpp_text, ReaperReference, ReaperReport,
    ReferenceClass, ReferenceKind,
};
pub use prepare::{
    load_content_overrides, prepare_reaper_self_contained, resolve_content_path,
    ContentOverrideDocument, ContentOverrideEntry, ReaperPrepareReport,
    CONTENT_OVERRIDES_FORMAT_VERSION,
};
pub use rewrite::rewrite_rpp_references;
