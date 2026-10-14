//! Ordering fallback and canonical-equivalence regressions.
use super::*;

/// Missing ICU data retains a deterministic lexical order.
#[test]
fn absent_locale_data_uses_lexical_order() {
    assert_eq!(compare_using(None, "+1", "-0"), Ordering::Less);
    assert_eq!(compare("+1", "-0"), Ordering::Greater);
}

/// Canonically equivalent Unicode tags remain distinct when their bytes differ.
#[test]
fn tags_trim_and_deduplicate_without_erasing_distinct_unicode() {
    let mut tags = [" ", " A ", "a", "é", "e\u{301}"]
        .map(str::to_owned)
        .to_vec();
    normalize_tags(&mut tags);
    assert_eq!(tags, ["a", "é", "e\u{301}"]);
}
