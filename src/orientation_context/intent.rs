//! Core context intent defaults and complete, bounded projection receipts.
use crate::{
    ContextOptions, ListOutput, PmRustError, Workspace, collections::refusal, read_output,
};
use serde_json::json;

/// Positive decimal controls accepted by the published intent layer.
fn positive(raw: Option<&str>) -> Option<usize> {
    let raw = crate::orientation::trim_control(raw?);
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    raw.parse::<usize>()
        .ok()
        .filter(|n| *n > 0 && *n <= 9_007_199_254_740_991)
}

/// Closed set of built-in context projections.
#[derive(Clone, Copy)]
enum CoreIntent {
    /// Hierarchy and blockers for initial orientation.
    Orient,
    /// Progress and decisions for a handoff.
    Handoff,
}

/// Static producer defaults and receipt fields for one core projection.
struct Declaration {
    /// Declared token budget.
    budget: usize,
    /// Fixed receipt overhead.
    overhead: usize,
    /// Estimated cost of one producer row.
    row_cost: usize,
    /// Default projection depth.
    depth: &'static str,
    /// Default selected sections.
    sections: &'static [&'static str],
    /// Receipt field groups.
    field_groups: &'static [&'static str],
}

impl CoreIntent {
    /// Parses an already normalized name at the native boundary.
    fn parse(name: &str) -> Option<Self> {
        match name {
            "orient" => Some(Self::Orient),
            "handoff" => Some(Self::Handoff),
            _ => None,
        }
    }

    /// Returns the published wire spelling.
    fn name(self) -> &'static str {
        match self {
            Self::Orient => "orient",
            Self::Handoff => "handoff",
        }
    }

    /// Resolves the complete static declaration without a string fallback.
    fn declaration(self) -> Declaration {
        match self {
            Self::Orient => Declaration {
                budget: 3000,
                overhead: 1200,
                row_cost: 600,
                depth: "standard",
                sections: &["hierarchy", "blockers", "activity"],
                field_groups: &["summary", "focus", "hierarchy", "blockers", "activity"],
            },
            Self::Handoff => Declaration {
                budget: 2200,
                overhead: 800,
                row_cost: 500,
                depth: "deep",
                sections: &["activity", "progress", "blockers"],
                field_groups: &["summary", "focus", "in_progress", "decisions", "activity"],
            },
        }
    }
}

/// Validated built-in declaration and its effective row ceiling.
pub(super) struct Intent {
    /// Validated core projection.
    core: CoreIntent,
    /// Built-in token declaration.
    declared: usize,
    /// Effective token budget.
    budget: usize,
    /// Effective maximum producer rows.
    derived: usize,
    /// Fixed receipt overhead used to derive rows.
    overhead: usize,
    /// Per-row cost used to derive rows.
    row_cost: usize,
}

/// Rejects runtime declarations before invoking the static native core contract.
fn boundary(workspace: &Workspace, global: Option<&std::path::Path>) -> Result<(), PmRustError> {
    let declarations = workspace.pm_root().join("context-intents.json");
    if declarations
        .try_exists()
        .map_err(|source| PmRustError::Io {
            path: declarations.clone(),
            source,
        })?
    {
        return Err(refusal(
            "Native context intents do not evaluate workspace declarations; use the published pm CLI.",
            2,
            json!({"code":"unsupported_native_context","field":"context-intents"}),
        ));
    }
    for root in global
        .into_iter()
        .chain(std::iter::once(workspace.pm_root()))
    {
        let directory = root.join("extensions");
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => {
                return Err(PmRustError::Io {
                    path: directory,
                    source,
                });
            }
        };
        check_extension_entries(&mut entries.into_iter(), &directory)?;
    }
    Ok(())
}

/// Refuses package sources, retaining iteration and metadata I/O errors.
fn check_extension_entries(
    entries: &mut dyn Iterator<Item = std::io::Result<std::fs::DirEntry>>,
    directory: &std::path::Path,
) -> Result<(), PmRustError> {
    for entry in entries {
        let entry = entry.map_err(|source| PmRustError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let kind =
            std::fs::symlink_metadata(&path).map_err(|source| PmRustError::Io { path, source })?;
        if kind.is_dir() {
            return Err(refusal(
                "Native context intents cannot certify installed package declarations; use the published pm CLI.",
                2,
                json!({"code":"unsupported_native_context","field":"context-intents"}),
            ));
        }
    }
    Ok(())
}

/// Resolves a core declaration without guessing unknown or custom intent names.
pub(super) fn prepare(
    workspace: &Workspace,
    options: &mut ContextOptions,
    global: Option<&std::path::Path>,
) -> Result<Option<Intent>, PmRustError> {
    let Some(name) = options.intent.as_deref() else {
        return Ok(None);
    };
    boundary(workspace, global)?;
    let name = crate::orientation::trim_control(name).to_lowercase();
    let core = CoreIntent::parse(&name).ok_or_else(|| refusal(
                "Native context supports only core orient and handoff intents; use the published pm CLI for other declarations.",
                2,
                json!({"code":"unsupported_native_context","field":"for"}),
            ))?;
    let Declaration {
        budget: declared,
        overhead,
        row_cost,
        depth,
        sections,
        ..
    } = core.declaration();
    let explicit = positive(options.token_budget.as_deref());
    let budget = explicit.unwrap_or(declared);
    if budget < 256 {
        return Err(refusal(
            "Context intent token budget must be at least 256",
            2,
            json!({"code":"invalid_argument_value","reason":"below_minimum","field":"tokenBudget","flag":"--token-budget","value":options.token_budget,"required":"Use an integer token budget of at least 256.","why":"Smaller ceilings cannot contain the minimum machine-readable intent receipt.","next_steps":["Retry with --token-budget 256 or omit the override."]}),
        ));
    }
    let calculated = budget.saturating_sub(overhead).div_euclid(row_cost).max(1);
    let derived = if explicit.is_some() {
        calculated
    } else {
        calculated.min(20)
    };
    let effective = positive(options.limit.as_deref()).map_or(derived, |n| n.min(derived));
    if options.limit.is_none() || positive(options.limit.as_deref()).is_some() {
        options.limit = Some(effective.to_string());
    }
    if options.sections.is_empty() && options.depth.is_none() {
        options.sections = sections
            .iter()
            .map(|section| (*section).to_owned())
            .collect();
    }
    options.depth.get_or_insert_with(|| depth.to_owned());
    options
        .activity_limit
        .get_or_insert_with(|| effective.to_string());
    options
        .token_budget
        .get_or_insert_with(|| declared.to_string());
    Ok(Some(Intent {
        core,
        declared,
        budget,
        derived,
        overhead,
        row_cost,
    }))
}

impl Intent {
    /// Attaches the oracle receipt before continuation and general output budgeting.
    pub(super) fn attach(&self, mut result: ListOutput, original: &ContextOptions) -> ListOutput {
        let Self {
            core,
            declared,
            budget,
            derived,
            overhead,
            row_cost,
        } = self;
        let name = core.name();
        let field_groups = core.declaration().field_groups;
        result.value["row_contract"] = json!({"command":"context","row_kind":"collection","row_keys":["high_level","low_level"],"fields":"supported","jq_selector":".row_contract.row_keys[] as $key | getpath($key | split(\".\")) | if type == \"array\" then .[] else if type == \"object\" then to_entries[] else empty end end","toon_encoding":"tabular_when_uniform"});
        let requested = positive(original.limit.as_deref());
        let explicit = requested.is_some_and(|n| n < *derived);
        result.value["budget_derived_limit"] = json!(derived);
        let mut receipt = json!({"command":"context","intent":name,"source":"core","included_field_groups":field_groups,"token_budget":budget,"declared_token_budget":declared,"token_budget_override":i64::try_from(*budget).unwrap_or(i64::MAX)-i64::try_from(*declared).unwrap_or_default(),"estimated_tokens":0,"within_budget":true,"degradation":"bounded_sections","declaration_feasible":true,"result_omitted":false,"budget_derived_limit":derived,"binding_constraint":if explicit {"explicit_limit"} else {"token_budget"},"limit_reason":if explicit {"The caller supplied a smaller row limit than the budget-derived ceiling."} else {"The selected intent token budget constrains the effective row ceiling."}});
        if let Some(requested) = requested.filter(|n| n > derived) {
            receipt["limit_reason"] = json!(format!(
                "The {budget}-token {name} budget carries {derived} rows, so it reduced the caller's --limit {requested}."
            ));
            let sufficient = (*budget).max(overhead + row_cost * requested);
            receipt["limit_override"] =
                json!({"flag":"--limit","requested":requested,"effective":derived});
            if sufficient <= 9_007_199_254_740_991 {
                receipt["limit_override"]["token_budget_for_requested"] = json!(sufficient);
                receipt["limit_override"]["restore_with"] =
                    json!(format!("--token-budget {sufficient}"));
            }
            result.value["warnings"] = json!([format!(
                "limit_overridden_by_intent:--limit {requested}->{derived}"
            )]);
            result.value["filters"]["requested_limit"] = json!(requested.to_string());
        }
        result.value["context_intent"] = receipt;
        read_output::update(&mut result, "context_intent", false);
        if read_output::estimate(&result, false) > *budget {
            read_output::compact_strings(&mut result.value, true);
            result.value["context_intent"]["degradation"] = json!("recursive_budget_compaction");
            read_output::update(&mut result, "context_intent", false);
        }
        let measured = read_output::estimate(&result, false);
        result.value["context_intent"]["declaration_feasible"] = json!(measured <= *declared);
        if measured > *budget {
            result.value["context_intent"]["degradation"] = json!("budget_receipt_only");
            result.value["context_intent"]["result_omitted"] = json!(true);
            result.value["context_intent"]["within_budget"] = json!(false);
            let recovery = measured.div_ceil(100) * 100;
            result = json!({"budget_exceeded":{"omitted_result":true,"reason":if budget==declared {"declared_budget_infeasible"} else {"effective_budget_infeasible"},"restore_with":format!("pm context --for {name} --token-budget {recovery} --limit {derived}")},"context_intent":result["context_intent"]}).into();
        } else {
            result
                .value
                .as_object_mut()
                .map(|map| map.shift_remove("row_contract"));
            read_output::stabilize_intent(&mut result);
        }
        result
    }
}

#[cfg(test)]
#[path = "../../tests/support/orientation_intent_unit.rs"]
mod tests;
