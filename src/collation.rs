//! Locale collation shared by published metadata normalization and selection.

use std::cmp::Ordering;
use std::sync::OnceLock;

use icu_collator::{Collator, CollatorBorrowed, options::CollatorOptions};
use icu_locale::locale;

/// Compares strings with the published CLI's default English locale collation.
pub(crate) fn compare(left: &str, right: &str) -> Ordering {
    static COLLATOR: OnceLock<Option<CollatorBorrowed<'static>>> = OnceLock::new();
    let collator = COLLATOR
        .get_or_init(|| Collator::try_new(locale!("en").into(), CollatorOptions::default()).ok())
        .as_ref();
    compare_using(collator, left, right)
}

/// Keeps deterministic lexical ordering when compiled locale data is unavailable.
fn compare_using(collator: Option<&CollatorBorrowed<'_>>, left: &str, right: &str) -> Ordering {
    match collator {
        Some(collator) => collator.compare(left, right),
        None => left.cmp(right),
    }
}

#[cfg(test)]
#[path = "../tests/support/collation_unit.rs"]
mod tests;

/// Trims, removes empty tags, deduplicates and applies locale ordering.
pub(crate) fn normalize_tags(tags: &mut Vec<String>) {
    *tags = tags
        .iter()
        .map(|tag| tag.trim().to_lowercase())
        .filter(|tag| !tag.is_empty())
        .collect();
    tags.sort_by(|left, right| compare(left, right));
    // Collation-equivalent strings need not be byte-identical.
    let mut seen = std::collections::HashSet::new();
    tags.retain(|tag| seen.insert(tag.clone()));
}

/// Orders equal-time annotation rows by text, then by author.
pub(crate) fn normalize_comments(rows: &mut [serde_json::Value]) {
    rows.sort_by(|left, right| {
        left["created_at"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["created_at"].as_str().unwrap_or_default())
            .then_with(|| {
                compare(
                    left["text"].as_str().unwrap_or_default(),
                    right["text"].as_str().unwrap_or_default(),
                )
            })
            .then_with(|| {
                compare(
                    left["author"].as_str().unwrap_or_default(),
                    right["author"].as_str().unwrap_or_default(),
                )
            })
    });
}
