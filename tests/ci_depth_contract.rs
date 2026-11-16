//! Prove quick-depth prefixes without changing the process environment or full defaults.
#[allow(dead_code)]
#[path = "support/depth.rs"]
mod test_depth;

/// Quick depth is bounded and deterministic; full retains every case.
#[test]
fn seed_prefix_counts_preserve_full_and_small_boundary_matrices() {
    for (full, quick) in [
        (0, 0),
        (8, 8),
        (25, 25),
        (30, 25),
        (96, 25),
        (500, 50),
        (768, 77),
    ] {
        assert_eq!(test_depth::cases_for(full, true), quick);
        assert_eq!(test_depth::cases_for(full, false), full);
    }
    assert_eq!(
        test_depth::property_cases(500),
        u32::try_from(test_depth::depth(500)).unwrap_or(0)
    );
}
