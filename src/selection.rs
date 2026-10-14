//! Native actionable candidate selection followed by atomic ownership attempts.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;

#[cfg(test)]
use tests::corpus_items;

use serde_json::{Value, json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    ItemDocument, OwnershipItem, OwnershipOptions, PmRustError, SelectionOptions, Workspace,
};

impl Workspace {
    /// Selects ranked actionable candidates and atomically claims the first available.
    ///
    /// # Errors
    /// Returns selection, storage or ownership errors. Only competing claims are retried.
    pub fn claim_next(
        &self,
        request: &OwnershipItem,
        options: &OwnershipOptions,
        selection: &SelectionOptions,
    ) -> Result<Value, PmRustError> {
        match selection.assignee_filter.as_deref() {
            None | Some("assigned" | "unassigned") => {}
            Some(filter) => {
                return Err(invalid_selection(&format!(
                    "Invalid assignee filter \"{filter}\". Allowed: assigned|unassigned"
                )));
            }
        }
        let max_attempts = selection.max_attempts.unwrap_or(10);
        if !(1..=100).contains(&max_attempts) {
            return Err(invalid_selection(
                "--max-attempts must be an integer from 1 to 100",
            ));
        }
        let settings = crate::ownership::runtime_settings(self.pm_root())?;
        let corpus = self.read_items()?;
        let candidates = candidates(self, &corpus, &settings, request, selection)?;
        // Published selection forwards the resolved display author as an asserted
        // author to each atomic claim, even when discovery detected a session.
        let mut atomic_options = options.clone();
        if let Some(identity) = &mut atomic_options.identity {
            identity.principal.clone_from(&request.author);
            identity.source = "asserted";
        }
        self.claim_ranked(request, &atomic_options, &candidates, max_attempts)
    }

    /// Retries stale ranked candidates through the same atomic ownership boundary.
    fn claim_ranked(
        &self,
        request: &OwnershipItem,
        options: &OwnershipOptions,
        candidates: &[ItemDocument],
        max_attempts: usize,
    ) -> Result<Value, PmRustError> {
        let mut attempts = 0;
        for candidate in candidates.iter().take(max_attempts) {
            attempts += 1;
            let mut candidate_request = request.clone();
            candidate_request.id.clone_from(&candidate.metadata.id);
            match self.claim_with_options(&candidate_request, options) {
                Ok(receipt) if receipt.skipped.is_none() => {
                    let mut value = serde_json::to_value(receipt).unwrap_or_default();
                    value["available"] = json!(true);
                    value["attempts"] = json!(attempts);
                    return Ok(value);
                }
                Ok(_) => {}
                Err(PmRustError::OwnershipRefusal { context, .. })
                    if context["code"] == "already_claimed_by" => {}
                Err(error) => return Err(error),
            }
        }
        if request.if_available {
            return Ok(
                json!({"available":false,"item":null,"claimed_by":request.author,"previous_assignee":null,"forced":request.force,"skipped":true,"recommendation":null,"attempts":attempts,"warnings":["no_available_next_item"]}),
            );
        }
        Err(refusal(
            if attempts == 0 {
                "No actionable items matched the selection filters"
            } else {
                "No actionable item remained available to claim"
            },
            if attempts == 0 { 3 } else { 4 },
            json!({"code":"no_available_next_item","why":if attempts == 0 {"No ready items matched the specified filters or selection criteria."}else{"Every ranked candidate was claimed by another agent before this atomic selection completed."},"next_steps":[if attempts==0{"Verify the filters or run pm next to inspect all ready work."}else{"Run pm claim --next again to refresh the ranked candidate set."}]}),
        ))
    }
}

/// Uses the published shared guidance for invalid selection flag values.
fn invalid_selection(detail: &str) -> PmRustError {
    refusal(
        detail,
        2,
        json!({"code":"invalid_argument_value","title":"Invalid argument value","required":"Use values that match documented command constraints.","next_steps":["Check allowed values in command help, then rerun with corrected input."],"examples":["pm claim --help","pm contracts --command <command> --flags-only --json"]}),
    )
}

/// Creates a published structured selection refusal.
fn refusal(detail: &str, exit_code: u8, context: Value) -> PmRustError {
    PmRustError::CommandRefusal {
        detail: detail.to_owned(),
        exit_code,
        context,
    }
}

/// Returns a metadata extension string or an empty sentinel.
fn text<'a>(item: &'a ItemDocument, key: &str) -> &'a str {
    item.metadata
        .extra
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// Resolves runtime lifecycle roles from the tracker schema.
fn role(item: &ItemDocument, settings: &Value, wanted: &str) -> bool {
    if let Some(statuses) = settings["schema"]["statuses"].as_array() {
        return statuses.iter().any(|status| {
            status["id"] == item.metadata.status
                && status["roles"]
                    .as_array()
                    .is_some_and(|roles| roles.iter().any(|role| role == wanted))
        });
    }
    match wanted {
        "active" => matches!(item.metadata.status.as_str(), "open" | "in_progress"),
        "terminal" => matches!(item.metadata.status.as_str(), "closed" | "canceled"),
        _ => false,
    }
}

/// Iterates a whole corpus; tests observe each pass over the actual slice.
#[cfg(not(test))]
fn corpus_items(corpus: &[ItemDocument]) -> std::slice::Iter<'_, ItemDocument> {
    corpus.iter()
}

/// Shared readiness lookups, retaining each original comparison's case semantics.
struct CorpusIndex<'a> {
    /// ASCII-insensitive IDs retain all matching documents for existential lookups.
    by_id: BTreeMap<String, Vec<&'a ItemDocument>>,
    /// Unicode-lowercase parent edges match the existing descendant traversal.
    children: BTreeMap<String, Vec<String>>,
    /// Inverse dependency targets use ASCII-insensitive, untrimmed comparisons.
    inverse_blocks: BTreeMap<String, BTreeSet<String>>,
    /// Leaf selection deliberately retains the existing exact parent comparison.
    exact_parents: BTreeSet<&'a str>,
    /// At most two distinct nonterminal witnesses per subtree suffice to exclude self.
    nonterminal_witnesses: BTreeMap<String, BTreeSet<String>>,
}

impl<'a> CorpusIndex<'a> {
    /// Indexes the corpus once, then propagates bounded witnesses through parent edges.
    fn new(corpus: &'a [ItemDocument], settings: &Value) -> Self {
        let mut index = Self {
            by_id: BTreeMap::new(),
            children: BTreeMap::new(),
            inverse_blocks: BTreeMap::new(),
            exact_parents: BTreeSet::new(),
            nonterminal_witnesses: BTreeMap::new(),
        };
        let mut nodes = BTreeSet::new();
        let mut parents: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for item in corpus_items(corpus) {
            let id = item.metadata.id.to_lowercase();
            nodes.insert(id.clone());
            index
                .by_id
                .entry(item.metadata.id.to_ascii_lowercase())
                .or_default()
                .push(item);
            if let Some(parent) = &item.metadata.parent {
                let folded_parent = parent.to_lowercase();
                index
                    .children
                    .entry(folded_parent.clone())
                    .or_default()
                    .push(id.clone());
                parents.entry(id).or_default().push(folded_parent);
                index.exact_parents.insert(parent);
            }
            if let Some(dependencies) = item
                .metadata
                .extra
                .get("dependencies")
                .and_then(Value::as_array)
            {
                for dep in dependencies {
                    if dep["kind"] == "blocks"
                        && let Some(target) = dep["id"].as_str()
                    {
                        index
                            .inverse_blocks
                            .entry(target.to_ascii_lowercase())
                            .or_default()
                            .insert(item.metadata.id.to_lowercase());
                    }
                }
            }
        }
        let mut pending = VecDeque::new();
        for id in nodes {
            if index
                .by_id
                .get(&id.to_ascii_lowercase())
                .is_some_and(|items| items.iter().any(|item| !role(item, settings, "terminal")))
            {
                pending.push_back((id.clone(), id));
            }
        }
        // Each node forwards at most two witnesses. Cycles terminate and a lone
        // nonterminal node cannot count itself as its own active descendant.
        while let Some((id, witness)) = pending.pop_front() {
            let witnesses = index.nonterminal_witnesses.entry(id.clone()).or_default();
            if witnesses.len() >= 2 || !witnesses.insert(witness.clone()) {
                continue;
            }
            if let Some(ancestors) = parents.get(&id) {
                pending.extend(
                    ancestors
                        .iter()
                        .map(|parent| (parent.clone(), witness.clone())),
                );
            }
        }
        index
    }

    /// Resolves terminal prerequisites without revisiting the corpus.
    fn is_terminal(&self, id: &str, settings: &Value) -> bool {
        self.by_id
            .get(&id.to_ascii_lowercase())
            .is_some_and(|items| items.iter().any(|item| role(item, settings, "terminal")))
    }

    /// Tests for any reachable nonterminal item other than the candidate itself.
    fn has_active_descendants(&self, id: &str) -> bool {
        self.nonterminal_witnesses
            .get(&id.to_lowercase())
            .is_some_and(|witnesses| {
                witnesses
                    .iter()
                    .any(|witness| !witness.eq_ignore_ascii_case(id))
            })
    }
}

/// Collects prerequisite IDs, including indexed inverse blocks relationships.
fn blockers(item: &ItemDocument, index: &CorpusIndex<'_>) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let blocked_by = text(item, "blocked_by").trim();
    if !blocked_by.is_empty() && blocked_by != "no-active-blocker" {
        ids.insert(blocked_by.to_lowercase());
    }
    if let Some(dependencies) = item
        .metadata
        .extra
        .get("dependencies")
        .and_then(Value::as_array)
    {
        for dep in dependencies {
            if dep["kind"] == "blocked_by"
                && let Some(id) = dep["id"].as_str()
            {
                ids.insert(id.trim().to_lowercase());
            }
        }
    }
    if let Some(inverse) = index
        .inverse_blocks
        .get(&item.metadata.id.to_ascii_lowercase())
    {
        ids.extend(inverse.iter().cloned());
    }
    ids
}

/// Finds all descendants with an indexed, cycle-safe traversal for parent filters.
fn descendants(parent: &str, index: &CorpusIndex<'_>) -> BTreeSet<String> {
    let mut ids = BTreeSet::from([parent.to_lowercase()]);
    let mut pending = vec![parent.to_lowercase()];
    while let Some(id) = pending.pop() {
        if let Some(children) = index.children.get(&id) {
            for child in children {
                if ids.insert(child.clone()) {
                    pending.push(child.clone());
                }
            }
        }
    }
    ids
}

/// Checks supplied filters independently of readiness and ownership.
fn matches(item: &ItemDocument, selection: &SelectionOptions) -> bool {
    selection
        .item_type
        .as_ref()
        .is_none_or(|wanted| item.metadata.item_type.eq_ignore_ascii_case(wanted))
        && selection
            .tag
            .as_ref()
            .is_none_or(|wanted| item.metadata.tags.contains(wanted))
        && selection
            .priority
            .is_none_or(|wanted| item.metadata.priority == wanted)
        && selection
            .sprint
            .as_ref()
            .is_none_or(|wanted| text(item, "sprint") == wanted)
        && selection
            .release
            .as_ref()
            .is_none_or(|wanted| text(item, "release") == wanted)
        && match selection.assignee_filter.as_deref() {
            Some("unassigned") => text(item, "assignee").trim().is_empty(),
            Some("assigned") => !text(item, "assignee").trim().is_empty(),
            _ => true,
        }
}

/// Compares optional numeric order and deadline values, putting absent values last.
fn optional_order(left: Option<f64>, right: Option<f64>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.total_cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Reproduces the structural ordering before weighted relevance ranking.
fn baseline(left: &ItemDocument, right: &ItemDocument, settings: &Value) -> Ordering {
    let progress = settings["schema"]["workflow"]["in_progress_status"]
        .as_str()
        .unwrap_or("in_progress");
    (left.metadata.status != progress)
        .cmp(&(right.metadata.status != progress))
        .then_with(|| left.metadata.priority.cmp(&right.metadata.priority))
        .then_with(|| {
            optional_order(
                left.metadata.extra.get("order").and_then(Value::as_f64),
                right.metadata.extra.get("order").and_then(Value::as_f64),
            )
        })
        .then_with(|| {
            match (
                nonempty(text(left, "deadline")),
                nonempty(text(right, "deadline")),
            ) {
                (Some(left), Some(right)) => left.cmp(right),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            }
        })
        .then_with(|| right.metadata.updated_at.cmp(&left.metadata.updated_at))
        .then_with(|| crate::collation::compare(&left.metadata.id, &right.metadata.id))
}

/// Distinguishes absent metadata from a nonempty deadline.
fn nonempty(value: &str) -> Option<&str> {
    if value.is_empty() { None } else { Some(value) }
}

/// Reads the last substantive history timestamp rather than maintenance `updated_at`.
fn recency(workspace: &Workspace, item: &ItemDocument) -> String {
    fs::read_to_string(
        workspace
            .pm_root()
            .join("history")
            .join(format!("{}.jsonl", item.metadata.id)),
    )
    .ok()
    .and_then(|history| {
        history
            .lines()
            .rev()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .find(|entry| entry["event_class"] == "substantive")
            .and_then(|entry| entry["ts"].as_str().map(str::to_owned))
    })
    .unwrap_or_else(|| item.metadata.created_at.clone())
}

/// Counts annotation and test activity used by the default ranking signals.
fn activity(item: &ItemDocument, keys: &[&str]) -> u32 {
    keys.iter()
        .map(|key| {
            item.metadata
                .extra
                .get(*key)
                .and_then(Value::as_array)
                .map_or(0, |rows| u32::try_from(rows.len()).unwrap_or(u32::MAX))
        })
        .sum()
}

/// Computes normalized graph degree over the candidate set.
fn graph_degree(items: &[ItemDocument]) -> BTreeMap<String, u32> {
    let mut degree = BTreeMap::new();
    for item in items {
        let mut targets = Vec::new();
        if let Some(parent) = &item.metadata.parent {
            targets.push(parent.clone());
        }
        if let Some(deps) = item
            .metadata
            .extra
            .get("dependencies")
            .and_then(Value::as_array)
        {
            targets.extend(
                deps.iter()
                    .filter_map(|dep| dep["id"].as_str().map(str::to_owned)),
            );
        }
        for target in targets {
            *degree.entry(item.metadata.id.clone()).or_default() += 1;
            *degree.entry(target).or_default() += 1;
        }
    }
    degree
}

/// Selects ready, caller-owned candidates and ranks them with published default signals.
#[allow(clippy::too_many_lines)]
fn candidates(
    workspace: &Workspace,
    corpus: &[ItemDocument],
    settings: &Value,
    request: &OwnershipItem,
    selection: &SelectionOptions,
) -> Result<Vec<ItemDocument>, PmRustError> {
    let index = CorpusIndex::new(corpus, settings);
    let family = if let Some(parent) = &selection.parent {
        if !index.by_id.contains_key(&parent.to_ascii_lowercase()) {
            return Err(refusal(
                &format!("Next --parent item not found: {parent}"),
                3,
                json!({}),
            ));
        }
        Some(descendants(parent, &index))
    } else {
        None
    };
    let mut ready = Vec::new();
    for item in corpus_items(corpus) {
        if !role(item, settings, "active")
            || !matches(item, selection)
            || family
                .as_ref()
                .is_some_and(|family| !family.contains(&item.metadata.id.to_lowercase()))
        {
            continue;
        }
        let owner = text(item, "assignee").trim();
        if !owner.is_empty() && owner != request.author {
            continue;
        }
        if blockers(item, &index)
            .iter()
            .any(|id| !index.is_terminal(id, settings))
        {
            continue;
        }
        if index.has_active_descendants(&item.metadata.id) && !selection.include_containers {
            continue;
        }
        match item.metadata.item_type.to_lowercase().as_str() {
            "decision" if !selection.include_decisions => continue,
            "milestone" if !selection.include_gates => continue,
            _ => {}
        }
        ready.push(item.clone());
    }
    // Completed containers are considered only when no actionable leaf remains.
    if !selection.include_containers
        && ready
            .iter()
            .any(|item| !index.exact_parents.contains(item.metadata.id.as_str()))
    {
        ready.retain(|item| !index.exact_parents.contains(item.metadata.id.as_str()));
    }
    ready.sort_by(|left, right| baseline(left, right, settings));
    let mut recent: Vec<_> = ready
        .iter()
        .map(|item| (item.metadata.id.clone(), recency(workspace, item)))
        .collect();
    recent.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| crate::collation::compare(&left.0, &right.0))
    });
    let recent_ranks: BTreeMap<_, _> = recent
        .into_iter()
        .enumerate()
        .map(|(rank, (id, _))| (id, rank))
        .collect();
    let divisor =
        f64::from(u32::try_from(ready.len().saturating_sub(1).max(1)).unwrap_or(u32::MAX));
    let degrees = graph_degree(&ready);
    let max_degree = f64::from(degrees.values().copied().max().unwrap_or(0).max(1));
    let max_activity = f64::from(
        ready
            .iter()
            .map(|item| activity(item, &["comments", "notes", "learnings", "test_runs"]))
            .max()
            .unwrap_or(0)
            .max(1),
    );
    let now = request
        .timestamp
        .as_deref()
        .and_then(|now| OffsetDateTime::parse(now, &Rfc3339).ok())
        .unwrap_or_else(OffsetDateTime::now_utc);
    let progress = settings["schema"]["workflow"]["in_progress_status"]
        .as_str()
        .unwrap_or("in_progress");
    let mut scored: Vec<_> = ready
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let owned = text(&item, "assignee").trim() == request.author;
            let rank = recent_ranks.get(&item.metadata.id).copied().unwrap_or(0);
            let risk = match text(&item, "risk") {
                "critical" | "high" => 1.0,
                "medium" => 0.5,
                "low" => 0.1,
                _ => 0.0,
            };
            let deadline = OffsetDateTime::parse(text(&item, "deadline"), &Rfc3339)
                .ok()
                .map_or(0.0, |deadline| {
                    let days = (deadline - now).as_seconds_f64() / 86400.0;
                    if days <= 0.0 {
                        1.0
                    } else {
                        1.0 / (1.0 + days / 30.0)
                    }
                });
            let index_value = f64::from(u32::try_from(index).unwrap_or(u32::MAX));
            let rank_value = f64::from(u32::try_from(rank).unwrap_or(u32::MAX));
            let score = 0.4 * (1.0 - index_value / divisor)
                + 0.35 * (1.0 - rank_value / divisor)
                + 0.25
                    * f64::from(activity(
                        &item,
                        &["comments", "notes", "learnings", "test_runs"],
                    ))
                    / max_activity
                + 0.15 * f64::from(degrees.get(&item.metadata.id).copied().unwrap_or_default())
                    / max_degree
                + 0.6
                    * if item.metadata.status == progress {
                        1.0
                    } else if owned {
                        0.75
                    } else {
                        0.0
                    }
                + 0.35 * (1.0 - f64::from(item.metadata.priority) / 4.0)
                + 0.2 * risk
                + 0.45 * deadline
                + 0.25
                    * (f64::from(activity(&item, &["comments", "notes", "learnings"])) / 5.0)
                        .min(1.0)
                + 0.2 * f64::from(u8::from(owned));
            (score, index, item)
        })
        .collect();
    scored.sort_by(|left, right| right.0.total_cmp(&left.0).then(left.1.cmp(&right.1)));
    Ok(scored.into_iter().map(|(_, _, item)| item).collect())
}

#[cfg(test)]
#[path = "../tests/support/selection_unit.rs"]
mod tests;
