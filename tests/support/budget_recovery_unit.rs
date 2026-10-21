//! Feasibility search proves minima without assuming monotone rendered output.
use super::*;

/// Exhaustive verification finds an earlier feasible island and never repeats a probe.
#[test]
fn nonmonotone_minimum_is_proved_and_cached() {
    let mut calls = BTreeMap::<usize, usize>::new();
    let result = find(1, 1, &mut |tokens| {
        *calls.entry(tokens).or_default() += 1;
        if tokens == 7 || tokens >= 300 {
            Probe::Fits
        } else {
            Probe::Floor
        }
    });
    assert_eq!(result["minimum_status"], "exact");
    assert_eq!(result["minimum_feasible_tokens"], 7);
    assert!(calls.values().all(|calls| *calls == 1));
    assert!((1..7).all(|tokens| calls.contains_key(&tokens)));
}

/// Cost and search ceilings never report an unverified candidate as feasible.
#[test]
fn unavailable_search_bounds_remain_explicit() {
    let mut calls = 0;
    let result = find(1, 1_000_001, &mut |_| {
        calls += 1;
        Probe::Fits
    });
    assert_eq!(calls, 0);
    assert_eq!(result["reason"], "query_cost_limit");
    for requested in [1, 9_007_199_254_740_991] {
        let result = find(requested, 1, &mut |_| Probe::Floor);
        assert_eq!(result["reason"], "search_limit");
        assert!(result["feasible_tokens"].is_null());
    }
    let result = find(1, 0, &mut |_| Probe::OutputConstraints);
    assert_eq!(result["reason"], "output_constraints");
}

/// A bounded proof discloses feasibility separately from an exact global minimum.
#[test]
fn partial_verification_never_claims_a_minimum() {
    for (requested, work, threshold) in [
        (1, 100_000, 300),
        (1, 1, 10_000),
        (9_000_000_000_000_000, 100_000, 4_000_000_000_000_000),
    ] {
        let result = find(requested, work, &mut |tokens| {
            if tokens >= threshold {
                Probe::Fits
            } else {
                Probe::Floor
            }
        });
        assert_eq!(result["minimum_status"], "feasible_only");
        assert!(
            result["feasible_tokens"]
                .as_u64()
                .is_some_and(|value| value >= threshold as u64)
        );
        assert!(result["minimum_feasible_tokens"].is_null());
    }
}
