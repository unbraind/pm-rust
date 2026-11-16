//! Test-only deterministic quick-depth policy; unset and unknown depths stay exhaustive.

/// Return the same seed prefix at one tenth depth, with at least 25 cases.
pub fn depth(full: usize) -> usize {
    cases_for(
        full,
        std::env::var("PM_TEST_DEPTH").as_deref() == Ok("quick"),
    )
}

/// Pure policy shared by the environment reader and regression fixtures.
pub fn cases_for(full: usize, quick: bool) -> usize {
    if quick {
        full.min(25.max(full.div_ceil(10)))
    } else {
        full
    }
}

/// Preserve each suite's original full count while honoring an explicit property case cap.
pub fn property_cases(full: u32) -> u32 {
    let requested = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|count| *count > 0)
        .unwrap_or(full);
    u32::try_from(depth(full as usize))
        .unwrap_or(full)
        .min(requested)
}
