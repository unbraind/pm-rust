//! Bounded verification of a feasible canonical relationship-context retry.
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Outcome of running the preserved query at a candidate token budget.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Probe {
    /// The native packet cannot deliver its required root, evidence and first row.
    Floor,
    /// The packet fits and the complete delivered output respects its constraints.
    Fits,
    /// A separate output constraint prevents delivery.
    OutputConstraints,
}

/// A cached search also retains the smallest candidate that actually delivered.
struct Search<'a> {
    /// Deterministic canonical-query probe.
    probe: &'a mut dyn FnMut(usize) -> Probe,
    /// Already observed candidate outcomes.
    cache: BTreeMap<usize, Probe>,
    /// Smallest observed feasible budget.
    feasible: Option<usize>,
}

impl Search<'_> {
    /// Evaluates each candidate once, retaining positive feasibility evidence.
    fn observe(&mut self, budget: usize) -> Probe {
        let outcome = *self
            .cache
            .entry(budget)
            .or_insert_with(|| (self.probe)(budget));
        if outcome == Probe::Fits {
            self.feasible = Some(self.feasible.map_or(budget, |old| old.min(budget)));
        }
        outcome
    }

    /// Finds a budget above the native floor within the oracle's bounded search.
    fn upper(&mut self, requested: usize) -> Option<usize> {
        const MAX: usize = 9_007_199_254_740_991;
        let mut upper = 1200.max(requested + 1).min(MAX);
        for _ in 0..16 {
            if self.observe(upper) != Probe::Floor {
                return Some(upper);
            }
            if upper == MAX {
                return None;
            }
            upper = (upper * 2).min(MAX);
        }
        None
    }
}

/// Discloses a bounded search refusal without inventing a feasible minimum.
fn unavailable(requested: usize, reason: &str) -> Value {
    json!({"requested_tokens":requested,"feasible_tokens":null,"minimum_feasible_tokens":null,"minimum_status":"unavailable","basis":"canonical_retry","reason":reason})
}

/// Proves exact minima only after checking every smaller positive candidate.
pub(super) fn find(requested: usize, work: usize, probe: &mut dyn FnMut(usize) -> Probe) -> Value {
    if work > 1_000_000 {
        return unavailable(requested, "query_cost_limit");
    }
    let mut search = Search {
        probe,
        cache: BTreeMap::new(),
        feasible: None,
    };
    let Some(upper) = search.upper(requested) else {
        return unavailable(requested, "search_limit");
    };
    let mut left = 0;
    let mut right = upper;
    for _ in 0..24 {
        if right - left <= 1 {
            break;
        }
        let middle = left + (right - left) / 2;
        if search.observe(middle) == Probe::Floor {
            left = middle;
        } else {
            right = middle;
        }
    }
    let limit = search
        .feasible
        .unwrap_or(upper)
        .min(4096)
        .min(100_000 / work.max(1));
    for tokens in 1..=limit {
        if search.observe(tokens) == Probe::Fits {
            return json!({"requested_tokens":requested,"feasible_tokens":tokens,"minimum_feasible_tokens":tokens,"minimum_status":"exact","basis":"canonical_retry"});
        }
    }
    match search.feasible {
        None => unavailable(requested, "output_constraints"),
        Some(feasible) => {
            json!({"requested_tokens":requested,"feasible_tokens":feasible,"minimum_feasible_tokens":null,"minimum_status":"feasible_only","basis":"canonical_retry"})
        }
    }
}

#[cfg(test)]
#[path = "../../tests/support/budget_recovery_unit.rs"]
mod tests;
