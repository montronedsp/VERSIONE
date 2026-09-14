//! Staged RPP rewrite: only replace positively identified FILE path strings.

use crate::daw::reaper::parser::ReaperReference;
use crate::error::{Error, ErrorKind, Result};

/// Rewrite FILE path tokens for references that have a `replacement_relative` mapping.
///
/// `replacements` maps original `source_path` → new project-relative path (slash-separated).
/// Only exact matches of the original path text inside FILE directives are rewritten.
pub fn rewrite_rpp_references(original: &str, replacements: &[(String, String)]) -> Result<String> {
    if replacements.is_empty() {
        return Ok(original.to_string());
    }
    let mut out = String::with_capacity(original.len());
    for line in original.split_inclusive('\n') {
        out.push_str(&rewrite_line(line, replacements)?);
    }
    Ok(out)
}

fn rewrite_line(line: &str, replacements: &[(String, String)]) -> Result<String> {
    let trimmed = line.trim_start();
    if !is_file_directive(trimmed) {
        return Ok(line.to_string());
    }
    for (old, new) in replacements {
        if let Some(rewritten) = try_replace_path(line, old, new)? {
            return Ok(rewritten);
        }
    }
    Ok(line.to_string())
}

fn is_file_directive(trimmed: &str) -> bool {
    trimmed.len() >= 4
        && trimmed.as_bytes()[..4].eq_ignore_ascii_case(b"FILE")
        && (trimmed.len() == 4
            || trimmed.as_bytes()[4].is_ascii_whitespace()
            || trimmed.as_bytes()[4] == b':')
}

fn try_replace_path(line: &str, old: &str, new: &str) -> Result<Option<String>> {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];
    let after_file = {
        let rest = &trimmed[4..];
        let rest = rest.strip_prefix(':').unwrap_or(rest);
        rest.trim_start()
    };
    if let Some(rest) = after_file.strip_prefix('"') {
        // Quoted path
        let (path, after_path) = split_quoted(rest)?;
        if path != old {
            return Ok(None);
        }
        let mut s = String::new();
        s.push_str(indent);
        s.push_str("FILE \"");
        s.push_str(&escape_rpp_path(new));
        s.push('"');
        s.push_str(after_path);
        return Ok(Some(s));
    }
    // Unquoted
    let end = after_file
        .find(|c: char| c.is_whitespace())
        .unwrap_or(after_file.len());
    let path = &after_file[..end];
    if path != old {
        return Ok(None);
    }
    let after = &after_file[end..];
    let mut s = String::new();
    s.push_str(indent);
    s.push_str("FILE ");
    // Prefer quoting rewritten paths (spaces / unicode).
    s.push('"');
    s.push_str(&escape_rpp_path(new));
    s.push('"');
    s.push_str(after);
    Ok(Some(s))
}

fn split_quoted(rest: &str) -> Result<(String, &str)> {
    let mut out = String::new();
    let mut it = rest.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        match c {
            '"' => return Ok((out, &rest[i + c.len_utf8()..])),
            '\\' => match it.next() {
                Some((_, '"')) => out.push('"'),
                Some((_, '\\')) => out.push('\\'),
                Some((_, other)) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            },
            other => out.push(other),
        }
    }
    Err(Error::new(
        ErrorKind::UnsupportedFormat,
        "malformed quoted FILE path while rewriting RPP",
    ))
}

fn escape_rpp_path(path: &str) -> String {
    path.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Validate that every replacement target was applied at least once for collected refs.
pub fn assert_rewrites_applied(
    original: &str,
    rewritten: &str,
    collected: &[ReaperReference],
    replacements: &[(String, String)],
) -> Result<()> {
    for (old, new) in replacements {
        if !rewritten.contains(new) {
            return Err(Error::new(
                ErrorKind::Invariant,
                format!("staged RPP rewrite missing expected path {new}"),
            ));
        }
        // Original absolute/external form should not remain for collected refs.
        if collected.iter().any(|r| r.source_path == *old) && rewritten.contains(old) && old != new
        {
            // Same path text could theoretically appear elsewhere as NAME — only fail if FILE still points to it.
            if file_directive_still_points_to(rewritten, old) {
                return Err(Error::new(
                    ErrorKind::Invariant,
                    format!("staged RPP still references unrecollected path {old}"),
                ));
            }
        }
    }
    let _ = original;
    Ok(())
}

fn file_directive_still_points_to(text: &str, old: &str) -> bool {
    for line in text.lines() {
        let t = line.trim_start();
        if !is_file_directive(t) {
            continue;
        }
        if t.contains(old) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_quoted_file_path() {
        let original =
            "<REAPER_PROJECT\n  <SOURCE WAVE\n    FILE \"C:\\Samples\\kick.wav\"\n  >\n>\n";
        let out = rewrite_rpp_references(
            original,
            &[(
                "C:\\Samples\\kick.wav".into(),
                "Media/imported/kick.wav".into(),
            )],
        )
        .unwrap();
        assert!(out.contains("Media/imported/kick.wav"));
        assert!(!file_directive_still_points_to(
            &out,
            "C:\\Samples\\kick.wav"
        ));
    }
}
