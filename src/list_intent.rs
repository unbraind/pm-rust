//! Built-in triage projection and token-budget receipts.

use crate::{ListOptions, PmRustError, list::integer, read_output};
use serde_json::{Value, json};

/// Fields declared by the built-in list triage intent, in presentation order.
pub(crate) const FIELDS: [&str; 9] = [
    "id",
    "title",
    "status",
    "type",
    "priority",
    "parent",
    "assignee",
    "risk",
    "blocked_by",
];

/// Validates the intent override and derives the producer row ceiling.
pub(crate) fn prepare(options: &mut ListOptions) -> Result<Option<usize>, PmRustError> {
    if options.intent.is_none() {
        if options.token_budget.is_some() {
            return Err(PmRustError::InvalidReadRequest {
                reason: "--token-budget requires a declared context intent selected with --for"
                    .to_owned(),
            });
        }
        return Ok(None);
    }
    if options.intent.as_deref() != Some("triage") {
        return Err(PmRustError::InvalidReadRequest {
            reason: "Unknown context intent for list; use triage.".to_owned(),
        });
    }
    let budget = options
        .token_budget
        .as_deref()
        .map(|raw| integer(raw, 256))
        .transpose()?
        .unwrap_or(3200);
    let derived = row_limit(budget, options.token_budget.is_some());
    let limit = options
        .limit
        .as_deref()
        .map(|raw| integer(raw, 1))
        .transpose()?
        .unwrap_or(derived)
        .min(derived);
    options.limit = Some(limit.to_string());
    Ok(Some(budget))
}

/// Derives the declared intent's maximum number of rows.
fn row_limit(budget: usize, explicit: bool) -> usize {
    let calculated = budget.saturating_sub(520).div_euclid(16).max(2);
    if explicit {
        calculated
    } else {
        calculated.min(100)
    }
}

/// Projects complete metadata into triage rows with explicit missing-field nulls.
pub(crate) fn project(result: &mut Value) {
    result["items"] = json!(
        result["items"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|row| Value::Object(
                FIELDS
                    .into_iter()
                    .map(|field| (field.to_owned(), row[field].clone()))
                    .collect()
            ))
            .collect::<Vec<_>>()
    );
    result["projection"] = json!({"mode":"fields","fields":FIELDS});
    result["omission_receipt"] = json!({"has_omissions":true,"omitted_field_group_count":1,"omitted_field_groups":[{"name":"full_item_fields","restore_with":"--full"}]});
}

/// Attaches intent diagnostics and compacts rows while rebasing producer cursors.
pub(crate) fn attach(mut result: Value, options: &ListOptions, budget: usize) -> Value {
    // Intent selection is measured before the discovery row contract is hidden.
    result["row_contract"] = json!({"command":"list","row_kind":"collection","row_keys":["items"],"fields":"supported","jq_selector":".row_contract.row_keys[] as $key | getpath($key | split(\".\")) | if type == \"array\" then .[] else if type == \"object\" then to_entries[] else empty end end","toon_encoding":"tabular_when_uniform"});
    let derived = row_limit(budget, options.token_budget.is_some());
    let explicit = options
        .limit
        .as_deref()
        .and_then(|s| s.parse::<usize>().ok())
        .is_some_and(|n| n < derived);
    result["budget_derived_limit"] = json!(derived);
    result["context_intent"] = json!({"command":"list","intent":"triage","source":"core","included_field_groups":["identity","governance","ownership","dependencies"],"token_budget":budget,"declared_token_budget":3200,"token_budget_override":i64::try_from(budget).unwrap_or(i64::MAX)-3200,"estimated_tokens":0,"within_budget":true,"degradation":"bounded_fields_and_rows","declaration_feasible":true,"result_omitted":false,"budget_derived_limit":derived,"binding_constraint":if explicit {"explicit_limit"} else {"token_budget"},"limit_reason":if explicit {"The caller supplied a smaller row limit than the budget-derived ceiling."} else {"The selected intent token budget constrains the effective row ceiling."}});
    read_output::update(&mut result, "context_intent", false);
    if read_output::estimate(&result, false) > budget {
        read_output::compact_strings(&mut result);
        result["context_intent"]["degradation"] = json!("recursive_budget_compaction");
        read_output::update(&mut result, "context_intent", false);
    }
    let existing = result["next_cursor"].is_string();
    let raw = result["next_cursor"].as_str().or(options.after.as_deref());
    let cursor = raw.and_then(|raw| {
        let bytes =
            base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, raw).ok()?;
        serde_json::from_slice::<Value>(&bytes).ok()
    });
    if let Some(mut cursor) = cursor {
        let original_count = result["items"].as_array().map_or(0, Vec::len);
        let source_index = usize::try_from(cursor["after_index"].as_u64().unwrap_or_default())
            .unwrap_or(usize::MAX);
        let mut shrunk = false;
        loop {
            let count = result["items"].as_array().map_or(0, Vec::len);
            let measured = read_output::estimate(&result, false);
            if measured <= budget || count <= 1 {
                break;
            }
            let row_bytes = result["items"].to_string().len();
            let excess = (measured - budget) * 4;
            let remove = excess
                .div_ceil(row_bytes.div_ceil(count).max(1))
                .max(1)
                .min(count - 1);
            result["items"] = json!(
                result["items"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(count - remove)
                    .cloned()
                    .collect::<Vec<_>>()
            );
            let retained = count - remove;
            cursor["after_id"] = result["items"][retained - 1]["id"].clone();
            cursor["after_index"] = json!(if existing {
                source_index - (original_count - retained)
            } else {
                source_index + retained
            });
            result["next_cursor"] = json!(crate::pagination::encode(&cursor));
            shrunk = true;
            read_output::update(&mut result, "context_intent", false);
        }
        if shrunk {
            result["count"] = json!(result["items"].as_array().map_or(0, Vec::len));
            if result.get("applied_limit").is_some() {
                result["applied_limit"] = result["count"].clone();
            }
            result["context_intent"]["degradation"] = json!("budget_row_compaction");
            read_output::update(&mut result, "context_intent", false);
        }
    }
    let measured = read_output::estimate(&result, false);
    result["context_intent"]["declaration_feasible"] = json!(measured <= 3200);
    if measured > budget {
        result["context_intent"]["degradation"] = json!("budget_receipt_only");
        result["context_intent"]["result_omitted"] = json!(true);
        result["context_intent"]["within_budget"] = json!(false);
        let recovery = measured.div_ceil(100) * 100;
        result = json!({"budget_exceeded":{"omitted_result":true,"reason":if budget==3200 {"declared_budget_infeasible"} else {"effective_budget_infeasible"},"restore_with":format!("pm list --for triage --token-budget {recovery} --limit {derived}")},"context_intent":result["context_intent"]});
    } else {
        let mut map = result.as_object().cloned().unwrap_or_default();
        map.shift_remove("row_contract");
        result = Value::Object(map);
        read_output::update(&mut result, "context_intent", false);
    }
    result
}

/// Reconciles intent and read-output estimates after attaching both receipts.
pub(crate) fn stabilize(result: &mut Value) {
    for _ in 0..8 {
        let before = (
            result["context_intent"]["estimated_tokens"].clone(),
            result["read_output"]["estimated_tokens"].clone(),
        );
        read_output::update(result, "context_intent", false);
        read_output::update(result, "read_output", true);
        if before
            == (
                result["context_intent"]["estimated_tokens"].clone(),
                result["read_output"]["estimated_tokens"].clone(),
            )
        {
            break;
        }
    }
}

#[cfg(test)]
#[path = "../tests/support/list_intent_unit.rs"]
mod tests;
