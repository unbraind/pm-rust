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
}

/// Explicit inputs keep the policy contract independent of ambient property caps.
#[test]
fn property_caps_preserve_depth_and_honor_smaller_positive_limits() {
    for (quick, uncapped) in [(false, 500), (true, 50)] {
        for cap in [
            None,
            Some("0"),
            Some("invalid"),
            Some("-1"),
            Some("4294967296"),
        ] {
            assert_eq!(test_depth::property_cases_for(500, quick, cap), uncapped);
        }
        assert_eq!(test_depth::property_cases_for(500, quick, Some("10")), 10);
        assert_eq!(
            test_depth::property_cases_for(500, quick, Some("1000")),
            uncapped
        );
    }
    let cap = std::env::var("PROPTEST_CASES").ok();
    let quick = std::env::var("PM_TEST_DEPTH").as_deref() == Ok("quick");
    assert_eq!(
        test_depth::property_cases(500),
        test_depth::property_cases_for(500, quick, cap.as_deref())
    );
}
