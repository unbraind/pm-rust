//! Built-in triage projection and token-budget receipts.

use crate::{ListOptions, ListOutput, PmRustError, list::integer, read_output};
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
    let budget = if let Some(canonical) = options
        .output_budget
        .as_deref()
        .filter(|value| *value != "unbounded")
    {
        integer(canonical, 1)?
    } else {
        options
            .token_budget
            .as_deref()
            .map(|raw| integer(raw, 256))
            .transpose()?
            .unwrap_or(3200)
    };
    let derived = row_limit(budget, explicit_budget(options));
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

/// Canonical numeric cost controls and legacy token controls both make row limits explicit.
fn explicit_budget(options: &ListOptions) -> bool {
    options.token_budget.is_some()
        || options
            .output_budget
            .as_deref()
            .is_some_and(|value| value != "unbounded")
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
pub(crate) fn attach(
    result: impl Into<ListOutput>,
    options: &ListOptions,
    budget: usize,
) -> ListOutput {
    attach_with_overhead(result.into(), options, budget, 0)
}

/// Reserves finalized envelope overhead before applying the same intent projection.
fn attach_with_overhead(
    result: ListOutput,
    options: &ListOptions,
    reported_budget: usize,
    overhead: usize,
) -> ListOutput {
    let mut result = result;
    let budget = i64::try_from(reported_budget).unwrap_or(i64::MAX)
        - i64::try_from(overhead).unwrap_or(i64::MAX);
    let exceeds = |measured: usize| i64::try_from(measured).unwrap_or(i64::MAX) > budget;
    // Intent selection is measured before the discovery row contract is hidden.
    result.value["row_contract"] = json!({"command":"list","row_kind":"collection","row_keys":["items"],"fields":"supported","jq_selector":".row_contract.row_keys[] as $key | getpath($key | split(\".\")) | if type == \"array\" then .[] else if type == \"object\" then to_entries[] else empty end end","toon_encoding":"tabular_when_uniform"});
    let derived = row_limit(reported_budget, explicit_budget(options));
    let explicit = options
        .limit
        .as_deref()
        .and_then(|s| s.parse::<usize>().ok())
        .is_some_and(|n| n < derived);
    result.value["budget_derived_limit"] = json!(derived);
    result.value["context_intent"] = json!({"command":"list","intent":"triage","source":"core","included_field_groups":["identity","governance","ownership","dependencies"],"token_budget":budget,"declared_token_budget":3200,"token_budget_override":i64::try_from(reported_budget).unwrap_or(i64::MAX)-3200,"estimated_tokens":0,"within_budget":true,"degradation":"bounded_fields_and_rows","declaration_feasible":true,"result_omitted":false,"budget_derived_limit":derived,"binding_constraint":if explicit {"explicit_limit"} else {"token_budget"},"limit_reason":if explicit {"The caller supplied a smaller row limit than the budget-derived ceiling."} else {"The selected intent token budget constrains the effective row ceiling."}});
    update_intent(&mut result, options.toon);
    let unbounded = options.output_budget.as_deref() == Some("unbounded");
    if !unbounded && exceeds(measure_intent(&result, options.toon)) {
        read_output::compact_strings(&mut result.value);
        result.value["context_intent"]["degradation"] = json!("recursive_budget_compaction");
        update_intent(&mut result, options.toon);
    }
    let existing = result["next_cursor"].is_string();
    let raw = result["next_cursor"].as_str().or(options.after.as_deref());
    let cursor = raw.and_then(|raw| {
        let bytes =
            base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, raw).ok()?;
        let cursor = crate::strict_json::from_slice::<Value>(&bytes).ok()?;
        // A legacy cursor may omit after_index. Treating the omission as zero
        // rebases a final page onto the first rows; the published CLI skips
        // compaction unless the index is present.
        cursor["after_index"].as_u64()?;
        Some(cursor)
    });
    if let Some(mut cursor) = cursor.filter(|_| !unbounded) {
        let original_count = result["items"].as_array().map_or(0, Vec::len);
        let source_index = usize::try_from(cursor["after_index"].as_u64().unwrap_or_default())
            .unwrap_or(usize::MAX);
        let mut shrunk = false;
        loop {
            let count = result["items"].as_array().map_or(0, Vec::len);
            let measured = measure_intent(&result, options.toon);
            if !exceeds(measured) || count <= 1 {
                break;
            }
            // Size rows with the renderer the estimate uses: serde writes 1e20 in
            // 4 bytes, JSON.stringify in 21, and the difference changes how many
            // rows a compaction removes.
            let row_bytes = crate::stringify_json(&result.view("/items"), false).len();
            let excess = measured
                .saturating_add(overhead)
                .saturating_sub(reported_budget)
                * 4;
            let remove = excess
                .div_ceil(row_bytes.div_ceil(count).max(1))
                .max(1)
                .min(count - 1);
            result.value["items"] = json!(
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
            result.value["next_cursor"] = json!(crate::pagination::encode(&cursor));
            shrunk = true;
            update_intent(&mut result, options.toon);
        }
        if shrunk {
            result.value["count"] = json!(result["items"].as_array().map_or(0, Vec::len));
            if result.get("applied_limit").is_some() {
                result.value["applied_limit"] = result["count"].clone();
            }
            result.value["context_intent"]["degradation"] = json!("budget_row_compaction");
            update_intent(&mut result, options.toon);
        }
    }
    let measured = measure_intent(&result, options.toon);
    result.value["context_intent"]["declaration_feasible"] = json!(measured <= 3200);
    if exceeds(measured) && !unbounded {
        result = omitted(
            result["context_intent"].clone(),
            options,
            reported_budget,
            measured.saturating_add(overhead),
        );
    } else {
        if exceeds(measured) {
            result.value["context_intent"]["within_budget"] = json!(false);
        }
        let mut map = result.as_object().cloned().unwrap_or_default();
        map.shift_remove("row_contract");
        result.value = Value::Object(map);
        update_intent(&mut result, options.toon);
        result.value["context_intent"]["token_budget"] = json!(reported_budget);
    }
    result
}

/// Measures the actual selected encoding without hidden row discovery metadata.
fn measure_intent(result: &ListOutput, toon: bool) -> usize {
    let mut delivered = result.clone();
    if let Some(map) = delivered.value.as_object_mut() {
        map.shift_remove("row_contract");
    }
    read_output::measure(&delivered, toon)
}

/// Stabilizes the intent receipt against exactly the bytes the caller receives.
fn update_intent(result: &mut ListOutput, toon: bool) {
    for _ in 0..8 {
        let measured = measure_intent(result, toon);
        if result.value["context_intent"]["estimated_tokens"] == measured {
            break;
        }
        result.value["context_intent"]["estimated_tokens"] = json!(measured);
    }
}

/// Reconciles intent and read-output estimates after attaching both receipts.
pub(crate) fn stabilize(result: &mut ListOutput, toon: bool) {
    for _ in 0..8 {
        let before = (
            result["context_intent"]["estimated_tokens"].clone(),
            result["read_output"]["estimated_tokens"].clone(),
        );
        update_intent(result, toon);
        // Compaction may lower the delivered estimate, but it cannot make an
        // already-infeasible full declaration feasible again.
        result.value["context_intent"]["declaration_feasible"] = json!(
            result["context_intent"]["declaration_feasible"] != false
                && result["context_intent"]["estimated_tokens"]
                    .as_u64()
                    .unwrap_or_default()
                    <= 3200
        );
        read_output::update_output(result, toon);
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

/// Produces the shared omission outcome after measuring the preserved query.
fn omitted(
    mut receipt: Value,
    options: &ListOptions,
    budget: usize,
    measured: usize,
) -> ListOutput {
    receipt["token_budget"] = json!(budget);
    receipt["estimated_tokens"] = json!(measured);
    receipt["declaration_feasible"] = json!(measured <= 3200);
    receipt["degradation"] = json!("budget_receipt_only");
    receipt["result_omitted"] = json!(true);
    receipt["within_budget"] = json!(false);
    if options
        .output_budget
        .as_deref()
        .is_some_and(|value| value != "unbounded")
    {
        return json!({"output_budget_exceeded":{"omitted_result":true,"reason":"requested_budget_infeasible","restore_with":"Unbounded","recovery":{"outputBudget":"unbounded"}},"context_intent":receipt}).into();
    }
    let recovery = (measured.max(budget.saturating_add(1)) * 5)
        .div_ceil(4)
        .div_ceil(100)
        * 100;
    let derived = receipt["budget_derived_limit"].as_u64().unwrap_or(1);
    json!({"budget_exceeded":{"omitted_result":true,"reason":if budget==3200 {"declared_budget_infeasible"} else {"effective_budget_infeasible"},"restore_with":format!("pm list --for triage --token-budget {recovery} --limit {derived}")},"context_intent":receipt}).into()
}

/// Finalizes delivery through the same continuation and output-budget pipeline on every pass.
fn finalize(
    mut result: ListOutput,
    requested: &ListOptions,
    continuation: Option<&str>,
) -> Result<ListOutput, PmRustError> {
    if let Some(fingerprint) = continuation.filter(|_| {
        result.get("budget_exceeded").is_none() && result.get("output_budget_exceeded").is_none()
    }) {
        let mut map = result.as_object().cloned().unwrap_or_default();
        for key in [
            "applied_limit",
            "completeness",
            "context_intent",
            "count",
            "filters",
            "has_more",
            "now",
            "omission_receipt",
            "projection",
            "row_contract",
            "sorting",
            "total",
            "truncated",
        ] {
            map.shift_remove(key);
        }
        map.insert(
            "continuation_contract".to_owned(),
            json!({"fingerprint":fingerprint,"metadata":"reference","restore_with":"omit --after"}),
        );
        result.value = Value::Object(map);
    }
    read_output::apply(result, requested)
}

/// Finds an over-budget finalized intent without charging receipts already omitted.
fn settled(
    result: &ListOutput,
    receipt: &Value,
    requested: &ListOptions,
) -> Option<(Value, usize)> {
    let delivered = result.get("context_intent").is_some();
    let receipt = if delivered {
        &result["context_intent"]
    } else {
        receipt
    };
    if !receipt.is_object()
        || receipt["result_omitted"] == true
        || result.get("output_budget_exceeded").is_some()
        || requested.output_budget.as_deref() == Some("unbounded")
    {
        return None;
    }
    let estimated = if delivered {
        usize::try_from(receipt["estimated_tokens"].as_u64()?).unwrap_or(usize::MAX)
    } else {
        read_output::measure(result, requested.toon)
    };
    (estimated > usize::try_from(receipt["token_budget"].as_u64()?).unwrap_or(usize::MAX))
        .then(|| (receipt.clone(), estimated))
}

/// Recharges intent projection for finalized envelope overhead, then verifies its delivered fit.
pub(crate) fn deliver(
    source: ListOutput,
    options: &ListOptions,
    requested: &ListOptions,
    budget: Option<usize>,
    continuation: Option<&str>,
) -> Result<ListOutput, PmRustError> {
    let Some(budget) = budget else {
        return finalize(source, requested, continuation);
    };
    let project = |overhead| -> Result<(ListOutput, Value), PmRustError> {
        let projected = if overhead == 0 {
            attach(source.clone(), options, budget)
        } else {
            attach_with_overhead(source.clone(), options, budget, overhead)
        };
        let receipt = projected["context_intent"].clone();
        Ok((finalize(projected, requested, continuation)?, receipt))
    };
    let (first, receipt) = project(0)?;
    let Some((_, estimated)) = settled(&first, &receipt, requested) else {
        return Ok(first);
    };
    let initial =
        usize::try_from(receipt["estimated_tokens"].as_u64().unwrap_or(0)).unwrap_or(usize::MAX);
    let (second, receipt) = project(estimated.saturating_sub(initial))?;
    let Some((receipt, estimated)) = settled(&second, &receipt, requested) else {
        return Ok(second);
    };
    finalize(
        omitted(receipt, options, budget, estimated),
        requested,
        continuation,
    )
}
