//! `pm-rust` command-line interface.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

mod invocation;
mod refusal;
use invocation::Invocation;

use clap::Parser;
use pm_rust::cli::{Cli, CollectionArgs, Command, OwnershipArgs};
#[cfg(test)]
use pm_rust::cli::{CloseArgs, SelectionArgs};
use pm_rust::{
    AuthorIdentity, CloseItem, CommentItem, CreateItem, CreateResult, ItemFilter, ListOptions,
    MutationResult, OwnershipItem, OwnershipOptions, SelectionOptions, UpdateItem, Workspace,
};
use serde::Serialize;

/// Writes one pretty JSON response to the process standard output stream.
fn write_json(value: &impl Serialize) -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = std::io::stdout().lock();
    write_json_to(&mut stdout, value)?;
    Ok(())
}

/// Serializes one response to a caller-supplied writer and flushes it.
fn write_json_to(
    writer: &mut dyn Write,
    value: &impl Serialize,
) -> Result<(), Box<dyn std::error::Error>> {
    pm_rust::write_pretty_json(&mut *writer, value)?;
    writer.write_all(b"\n")?;
    writer
        .flush()
        .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })
}

/// Writes a complete session frame and propagates codec, pipe and flush failures.
fn write_session_page_to(
    writer: &mut dyn std::io::Write,
    page: &pm_rust::SessionPage,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = if json {
        let mut bytes = pm_rust::stringify_json(page, false);
        bytes.push('\n');
        bytes
    } else {
        page.stringify_toon()?
    };
    writer.write_all(bytes.as_bytes())?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
#[path = "../tests/support/main_unit.rs"]
mod tests;

/// Checks stdin with bounded admission and renders a compact receipt or typed refusal.
fn run_adapter_check(
    reader: &mut dyn std::io::Read,
    writer: &mut dyn Write,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    reader
        .take((pm_rust::native_adapter::NATIVE_ADAPTER_MAX_TRANSCRIPT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    let result = pm_rust::native_adapter::check_adapter_transcript(&bytes);
    let value = match &result {
        Ok(value) => value.clone(),
        Err(error) => serde_json::json!({"ok": false, "refusal": error}),
    };
    let output = pm_rust::ListOutput::from(value);
    write_adapter_receipt_to(writer, &output, json)?;
    result?;
    Ok(())
}

/// Writes a compact ABI receipt, propagating codec, pipe and flush failures.
fn write_adapter_receipt_to(
    writer: &mut dyn Write,
    output: &pm_rust::ListOutput,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let rendered = if json {
        pm_rust::stringify_json(output, false)
    } else {
        pm_rust::stringify_toon(output)?
    };
    write_encoded_frame_to(writer, &rendered)
}

/// Writes one already-encoded page and propagates every pipe/newline/flush failure.
fn write_encoded_frame_to(
    writer: &mut dyn Write,
    rendered: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    writer.write_all(rendered.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

/// Renders an admitted response and propagates unexpected codec, bound and delivery failures.
fn write_discovery_to(
    writer: &mut dyn Write,
    response: Result<pm_rust::discovery::DiscoveryResponse, pm_rust::discovery::DiscoveryRefusal>,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let rendered = response?.render(json)?;
    write_encoded_frame_to(writer, &rendered)
}

/// Dispatches one parsed command using process-owned standard streams.
fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    run_with_io(
        cli,
        &mut std::io::stdin().lock(),
        &mut std::io::stdout().lock(),
    )
}

/// Dispatches workspace commands or an adapter check with caller-owned streams.
#[allow(clippy::too_many_lines)]
fn run_with_io(
    cli: Cli,
    reader: &mut dyn Read,
    writer: &mut dyn Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let discover = || Workspace::discover(&cli.workspace);
    match cli.command {
        Command::Capabilities {
            protocol,
            operation,
            output_bytes,
            adapter,
            json,
        } => {
            let request = pm_rust::discovery::DiscoveryRequest::from_cli(
                protocol,
                operation,
                output_bytes,
                adapter.as_deref(),
            )?;
            let response = pm_rust::discovery::discover_capabilities(&request);
            write_discovery_to(writer, response, json)?;
        }
        Command::PreviewItemMutations { json } => {
            let mut bytes = Vec::new();
            reader
                .take(
                    pm_rust::admission::ADMISSION_LIMITS.input_bytes as u64
                        + u64::from(pm_rust::admission::ADMISSION_LIMITS.over_limit_probe_bytes),
                )
                .read_to_end(&mut bytes)
                .map_err(|_| pm_rust::admission::AdmissionRefusal {
                    code: "admission_input",
                    required: "Provide readable bounded JSON on stdin and retry.",
                    limits: pm_rust::admission::ADMISSION_LIMITS,
                })?;
            let batch = pm_rust::admission::AdmissionBatch::from_json(&bytes)?;
            let workspace = discover().map_err(|_| pm_rust::admission::AdmissionRefusal {
                code: "admission_source",
                required: "Select a readable native tracker with settings.json.",
                limits: pm_rust::admission::ADMISSION_LIMITS,
            })?;
            // One refusal boundary: the stdin count is re-measured inside the same result.
            let preview = workspace
                .preview_item_mutations(&batch)
                .and_then(|preview| preview.with_input_bytes(bytes.len()))?;
            let output = pm_rust::ListOutput::from(serde_json::json!(preview));
            write_adapter_receipt_to(writer, &output, json)?;
        }
        Command::Explain {
            read: _,
            full,
            max_entries,
            max_bytes,
            json,
        } => {
            let workspace = discover().map_err(|_| pm_rust::QueryCostRefusal::Source)?;
            let plan = workspace.explain_list(&pm_rust::QueryCostRequest {
                projection: if full {
                    pm_rust::QueryProjection::Full
                } else {
                    pm_rust::QueryProjection::Brief
                },
                max_entries,
                max_bytes,
            })?;
            let output = pm_rust::ListOutput::from(serde_json::json!(plan));
            write_adapter_receipt_to(writer, &output, json)?;
        }
        Command::Changes {
            cursor,
            limit,
            output_bytes,
            cancel_file,
            json,
        } => {
            let request = pm_rust::changes::ChangeRequest {
                cursor,
                limit,
                output_bytes,
            };
            let cancellation = pm_rust::SessionCancellation::default();
            if cancel_file.as_ref().is_some_and(|path| path.exists()) {
                cancellation.cancel();
            }
            let adapter =
                pm_rust::changes::LocalChangeAdapter::new(discover()?, cancellation.clone());
            let page = adapter.pull(&request)?;
            let rendered = if json {
                pm_rust::stringify_json(&page, false)
            } else {
                page.stringify_toon()
            };
            write_encoded_frame_to(writer, &rendered)?;
        }
        Command::AdapterCheck { json } => run_adapter_check(reader, writer, json)?,
        Command::ReadSession {
            timestamp,
            memory_bytes,
            page_items,
            output_tokens,
            cancel_file,
            json,
        } => {
            let workspace = discover()?;
            let cancellation = pm_rust::SessionCancellation::default();
            if cancel_file.as_ref().is_some_and(|path| path.exists()) {
                cancellation.cancel();
            }
            let limits = pm_rust::ReadSessionLimits {
                memory_bytes,
                page_items,
                output_tokens,
                toon: !json,
            };
            let mut session = workspace.read_session(
                &limits,
                cancellation.clone(),
                &timestamp.unwrap_or_else(pm_rust::current_timestamp),
            )?;
            let mut stdout = std::io::stdout().lock();
            loop {
                if cancel_file.as_ref().is_some_and(|path| path.exists()) {
                    cancellation.cancel();
                }
                let Some(page) = session.next_page()? else {
                    break;
                };
                write_session_page_to(&mut stdout, &page, json)?;
            }
            return Ok(());
        }
        Command::Assurance {
            action,
            kind,
            id,
            output_budget,
            output_limit,
            output_cursor,
            json,
        } => {
            let workspace = discover()?;
            let id = if action == "show" {
                Some(id.ok_or_else(|| pm_rust::PmRustError::InvalidReadRequest {
                    reason: "assurance show requires an id".to_owned(),
                })?)
            } else {
                None
            };
            let result = workspace.assurance_read(
                &kind,
                id.as_deref(),
                &ListOptions {
                    output_budget,
                    output_limit,
                    output_cursor,
                    toon: !json,
                    ..ListOptions::default()
                },
            )?;
            return write_search_to(&mut std::io::stdout().lock(), &result, json);
        }
        Command::Context {
            limit,
            token_budget,
            depth,
            after,
            no_tags,
            no_extension_health,
            json,
            timestamp,
        } => {
            let workspace = discover()?;
            let now = timestamp.unwrap_or_else(pm_rust::current_timestamp);
            let result = workspace.context(
                &pm_rust::ContextOptions {
                    limit,
                    token_budget,
                    depth,
                    after,
                    no_tags,
                    no_extension_health,
                    toon: !json,
                },
                &now,
            )?;
            write_search_to(&mut std::io::stdout().lock(), &result, json)?;
        }
        Command::History {
            id,
            limit,
            full,
            compact,
            verify,
            strict_exit,
            json,
        } => {
            let workspace = discover()?;
            if full && compact {
                return Err(Box::new(pm_rust::PmRustError::CommandRefusal {detail:"History projection options are mutually exclusive. Use --compact, --provenance, or --full.".to_owned(),exit_code:2,context:serde_json::json!({})}));
            }
            if strict_exit && !verify {
                return Err(Box::new(pm_rust::PmRustError::CommandRefusal {
                    detail:
                        "--strict-exit requires --verify (it gates on the verification result)."
                            .to_owned(),
                    exit_code: 2,
                    context: serde_json::json!({}),
                }));
            }
            let result = workspace.history(
                &id,
                &pm_rust::HistoryOptions {
                    limit,
                    verify,
                    full,
                    compact,
                    toon: !json,
                },
            )?;
            write_search_to(&mut std::io::stdout().lock(), &result, json)?;
            if strict_exit && result["verification"]["ok"] == false {
                return Err(Box::new(pm_rust::PmRustError::HistoryVerificationFailed));
            }
        }
        Command::Deps {
            id,
            format,
            max_depth,
            direction,
            kind,
            cursor,
            collapse,
            summary,
            full,
            node_limit,
            edge_limit,
            token_budget,
            output_format,
            json,
        } => {
            let workspace = discover()?;
            let json = output_format
                .as_deref()
                .map_or(json, |format| format == "json");
            let result = workspace.deps(
                &id,
                &pm_rust::DepsOptions {
                    format,
                    max_depth,
                    direction,
                    kind,
                    cursor,
                    collapse,
                    summary,
                    full,
                    node_limit,
                    edge_limit,
                    token_budget,
                    toon: !json,
                },
            )?;
            return write_search_to(&mut std::io::stdout().lock(), &result, json);
        }
        Command::Search {
            query,
            limit,
            status,
            item_type,
            tag,
            after,
            output_cursor,
            output_budget,
            output_limit,
            json,
        } => {
            let workspace = discover()?;
            let result = workspace.search(
                &query.join(" "),
                &pm_rust::SearchOptions {
                    status,
                    item_type,
                    tag,
                    output: ListOptions {
                        limit,
                        after,
                        output_cursor,
                        output_budget,
                        output_limit,
                        toon: !json,
                        ..ListOptions::default()
                    },
                },
            )?;
            return write_search_to(&mut std::io::stdout().lock(), &result, json);
        }
        Command::Files(args) => run_collection(&discover()?, "files", args)?,
        Command::Docs(args) => run_collection(&discover()?, "docs", args)?,
        Command::Test(args) => run_collection(&discover()?, "tests", args)?,
        Command::List {
            json,
            output_budget,
            output_limit,
            brief,
            limit,
            offset,
            after,
            no_truncate,
            output_cursor,
            intent,
            token_budget,
            all,
            full,
            timestamp,
            status,
            item_type,
            id,
            parent,
            tag,
            sort,
            order,
        } => {
            let workspace = discover()?;
            let filters = ItemFilter {
                status,
                item_type,
                id,
            };
            {
                let now = timestamp.unwrap_or_else(pm_rust::current_timestamp);
                pm_rust::validate_timestamp(&now)?;
                let result = workspace.list_page(
                    &filters,
                    &ListOptions {
                        parent,
                        tag,
                        sort,
                        order,
                        toon: !json,
                        all,
                        full,
                        brief,
                        limit,
                        offset,
                        after,
                        no_truncate,
                        output_limit,
                        output_budget,
                        output_cursor,
                        intent,
                        token_budget,
                    },
                    &now,
                )?;
                write_list_to(&mut std::io::stdout().lock(), &result, json)?;
                if result.get("output_budget_exceeded").is_some()
                    || result.get("budget_exceeded").is_some()
                {
                    return Err(Box::new(pm_rust::PmRustError::OutputBudgetExceeded));
                }
            }
        }
        Command::Get { id } => write_json(&discover()?.get(&id)?)?,
        Command::Create {
            id,
            title,
            description,
            item_type,
            status,
            priority,
            tags,
            body,
            author,
            timestamp,
            message,
            force_stale_lock,
        } => {
            let workspace = discover()?;
            write_json(&create_payload(
                &workspace,
                CreateItem {
                    id,
                    title,
                    description,
                    item_type,
                    status,
                    priority,
                    tags,
                    body,
                    author,
                    timestamp,
                    message,
                    provenance_role: None,
                    force_stale_lock,
                },
            )?)?;
        }
        Command::Update {
            id,
            title,
            description,
            status,
            priority,
            tags_csv,
            body,
            author,
            timestamp,
            message,
            force_stale_lock,
        } => {
            let workspace = discover()?;
            let identity = scalar_identity(&workspace, author.as_deref())?;
            let author = author.unwrap_or_else(|| identity.author.clone());
            write_json(&update_payload(
                &workspace,
                &identity,
                UpdateItem {
                    id,
                    title,
                    description,
                    status,
                    priority,
                    // A provided CSV value expresses replacement intent even
                    // when it normalizes to an empty tag list. Each segment is
                    // trimmed before the empty filter so `--tags "alpha, beta"`
                    // stores `beta`, not `" beta"`, and `--tags "alpha, "`
                    // stores only `alpha` rather than keeping a whitespace-only
                    // segment.
                    tags: tags_csv.map(|csv| {
                        csv.split(',')
                            .map(str::trim)
                            .map(str::to_owned)
                            .filter(|tag| !tag.is_empty())
                            .collect()
                    }),
                    body,
                    author,
                    timestamp,
                    message,
                    provenance_role: None,
                    force_stale_lock,
                },
            )?)?;
        }
        Command::Comment {
            id,
            text,
            author,
            timestamp,
            message,
            force_stale_lock,
        } => {
            let workspace = discover()?;
            let identity = scalar_identity(&workspace, author.as_deref())?;
            let author = author.unwrap_or_else(|| identity.author.clone());
            write_json(&comment_payload(
                &workspace,
                &identity,
                &CommentItem {
                    id,
                    text,
                    author,
                    timestamp,
                    message,
                    provenance_role: None,
                    force_stale_lock,
                },
            )?)?;
        }
        Command::Claim {
            args,
            if_available,
            next,
            start,
            selection,
        } => {
            let workspace = discover()?;
            validate_claim_target(args.id.as_deref(), next, start, if_available)?;
            let (request, options) = ownership_request(&workspace, args, if_available)?;
            if next {
                return write_json(&workspace.claim_next(
                    &request,
                    &options,
                    &SelectionOptions {
                        item_type: selection.item_type,
                        tag: selection.tag,
                        priority: selection.priority,
                        parent: selection.parent,
                        sprint: selection.sprint,
                        release: selection.release,
                        assignee_filter: selection.assignee_filter,
                        include_decisions: selection.include_decisions,
                        include_gates: selection.include_gates,
                        include_containers: selection.include_containers,
                        max_attempts: selection.max_attempts,
                        start,
                    },
                )?);
            }
            if start {
                return write_json(&workspace.start_task(&request, &options)?);
            }
            return write_json(&workspace.claim_with_options(&request, &options)?);
        }
        Command::Release { args, pause } => {
            let workspace = discover()?;
            let (request, options) = ownership_request(&workspace, args, false)?;
            if pause {
                return write_json(&workspace.pause_task(&request, &options)?);
            }
            return write_json(&workspace.release_with_options(&request, &options)?);
        }
        Command::StartTask(args) => {
            let workspace = discover()?;
            let (mut request, options) = ownership_request(&workspace, args, false)?;
            request.provenance_role = None;
            return write_json(&workspace.start_task(&request, &options)?);
        }
        Command::PauseTask(args) => {
            let workspace = discover()?;
            let (mut request, options) = ownership_request(&workspace, args, false)?;
            request.provenance_role = None;
            return write_json(&workspace.pause_task(&request, &options)?);
        }
        Command::Close {
            id,
            text,
            reason,
            author,
            timestamp,
            force_stale_lock,
            fields,
        } => {
            let workspace = discover()?;
            let identity = scalar_identity(&workspace, author.as_deref())?;
            let author = author.unwrap_or_else(|| identity.author.clone());
            let close_author = author.clone();
            let result = close_payload(
                &workspace,
                &identity,
                CloseItem {
                    id,
                    reason: text
                        .filter(|text| !text.is_empty())
                        .or_else(|| {
                            reason
                                .filter(|reason| !reason.trim().is_empty())
                                .or(fields.close_reason)
                        })
                        .unwrap_or_default(),
                    author,
                    timestamp,
                    provenance_role: None,
                    fields: pm_rust::CloseFields {
                        resolution: fields.resolution,
                        expected_result: fields.expected_result,
                        actual_result: fields.actual_result,
                        completed_at: fields.completed_at,
                        duplicate_of: fields.duplicate_of,
                        message: fields.message.clone(),
                        validate_close: fields.validate_close,
                        force: fields.force,
                    },
                    force_stale_lock,
                },
            )?;
            let mut receipt = result.close_receipt.unwrap_or_default();
            project_close_receipt_scope(&mut receipt, &raw_invocation());
            if fields.release_assignment {
                let release = identity
                    .with(|| {
                        workspace.release(&OwnershipItem {
                            id: result.item.metadata.id.clone(),
                            author: close_author,
                            timestamp: Some(result.item.metadata.updated_at.clone()),
                            message: fields.message,
                            force: fields.force,
                            if_available: false,
                            provenance_role: Some("implementer".to_owned()),
                        })
                    })
                    .map_err(|error| {
                        pm_rust::completed_step_failure(error, &result.item.metadata.id, "close")
                    })?;
                // Another writer can claim between the separate close and release steps.
                let changed_count = close_release_changed_field_count(
                    &result.close_changed_fields,
                    release.changed_field_count,
                );
                receipt = serde_json::json!({"id":result.item.metadata.id,"action":"close_task","close":receipt,"release":release,"status":result.item.metadata.status,"changed_field_count":changed_count});
            }
            write_close_receipt(&receipt, fields.json)?;
        }
    }
    Ok(())
}

/// Resolves the collection author and renders the published receipt.
fn run_collection(
    workspace: &Workspace,
    key: &str,
    args: CollectionArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    run_collection_with_cwd(workspace, key, args, std::env::current_dir())
}

/// Propagates an unavailable invocation directory before a collection can mutate storage.
fn run_collection_with_cwd(
    workspace: &Workspace,
    key: &str,
    args: CollectionArgs,
    cwd: std::io::Result<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let settings: serde_json::Value = pm_rust::strict_json::from_str(&std::fs::read_to_string(
        workspace.pm_root().join("settings.json"),
    )?)?;
    let identity = mutation_identity(args.author.as_deref(), settings["author_default"].as_str());
    let result = workspace.collection(
        &args.id,
        key,
        &pm_rust::CollectionOptions {
            add: args.add,
            remove: args.remove,
            remove_index: args.remove_index,
            author: identity.author.clone(),
            identity: Some(identity),
            timestamp: args.timestamp,
            message: args.message,
            force: args.force,
            append_stable: args.append_stable,
            note: args.note,
        },
        &cwd?,
    )?;
    write_list_to(
        &mut std::io::stdout().lock(),
        &pm_rust::ListOutput::from(result),
        args.json,
    )
}

/// Writes search receipts before reporting an infeasible requested output budget.
fn write_search_to(
    mut writer: &mut dyn Write,
    result: &pm_rust::ListOutput,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    write_list_to(&mut writer, result, json)?;
    if result.get("output_budget_exceeded").is_some() {
        return Err(Box::new(pm_rust::PmRustError::OutputBudgetExceeded));
    }
    Ok(())
}

/// Projects successful closure guidance through the actual invocation scope.
fn project_close_receipt_scope(receipt: &mut serde_json::Value, raw: &[String]) {
    let scope = invocation_scope(raw);
    if let Some(values) = receipt["follow_up"]["suggested_command_args"].as_array() {
        let mut args = values
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        strip_bootstrap_scope(&mut args);
        args.splice(0..0, scope.clone());
        receipt["follow_up"]["suggested_command"] = serde_json::json!(render_recovery(&args));
        receipt["follow_up"]["suggested_command_args"] = serde_json::json!(args);
    }
    if receipt["completion"]["source"] == "close_time" {
        let mut args = scope;
        args.extend([
            "update".to_owned(),
            receipt["id"].as_str().unwrap_or_default().to_owned(),
            "--completed-at".to_owned(),
            "<ISO>".to_owned(),
        ]);
        receipt["completion"]["hint"] = serde_json::json!(format!(
            "completed_at defaults to the close time. If work finished earlier, correct it with {}.",
            render_recovery(&args)
        ));
    }
}

/// Writes published compact closure output in the selected wire format.
fn write_close_receipt(
    value: &serde_json::Value,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    write_close_receipt_to(&mut std::io::stdout().lock(), value, json)
}

/// Writes closure receipts to a caller-provided stream and propagates flush failures.
fn write_close_receipt_to(
    writer: &mut dyn Write,
    value: &serde_json::Value,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        return write_json_to(writer, value);
    }
    write_close_toon(writer, value, "")?;
    writer.flush()?;
    Ok(())
}

/// Renders compact mutation TOON scalars with the oracle's quoted string style.
fn write_close_toon(
    writer: &mut dyn Write,
    value: &serde_json::Value,
    indent: &str,
) -> std::io::Result<()> {
    if let Some(fields) = value.as_object() {
        for (key, value) in fields {
            if value.is_null() {
                continue;
            }
            if value.is_object() {
                writeln!(writer, "{indent}{key}:")?;
                write_close_toon(writer, value, &format!("{indent}  "))?;
            } else if value.is_array() {
                let mut field = serde_json::Map::new();
                field.insert(key.clone(), value.clone());
                // JSON values use infallible built-in serde serializers.
                let encoded = toon_format::encode_default(&serde_json::Value::Object(field))
                    .unwrap_or_default();
                for line in encoded.lines() {
                    writeln!(writer, "{indent}{line}")?;
                }
            } else {
                writeln!(
                    writer,
                    "{indent}{key}: {}",
                    pm_rust::stringify_json(value, false)
                )?;
            }
        }
    }
    Ok(())
}

/// Resolves bounded provenance once at the CLI mutation boundary.
fn mutation_identity(author: Option<&str>, configured: Option<&str>) -> AuthorIdentity {
    let environment = std::env::vars_os()
        .map(|(key, value)| {
            (
                key.to_string_lossy().into_owned(),
                value.to_string_lossy().into_owned(),
            )
        })
        .collect();
    AuthorIdentity::resolve(author, configured, &environment).for_collections(&environment)
}

/// Stamps explicit ownership requests with the published implementer role.
fn ownership_request(
    workspace: &Workspace,
    args: OwnershipArgs,
    if_available: bool,
) -> Result<(OwnershipItem, OwnershipOptions), Box<dyn std::error::Error>> {
    if let Some(author) = &args.author
        && author.trim().is_empty()
    {
        return Err(Box::new(pm_rust::PmRustError::InvalidMutationRequest {
            reason: "author must not be empty".to_owned(),
        }));
    }
    let settings: serde_json::Value = pm_rust::strict_json::from_slice(&std::fs::read(
        workspace.pm_root().join("settings.json"),
    )?)?;
    let identity = mutation_identity(args.author.as_deref(), settings["author_default"].as_str());
    let request = OwnershipItem {
        id: args.id.unwrap_or_default(),
        author: identity.author.clone(),
        timestamp: args.timestamp,
        message: args.message,
        force: args.force,
        if_available,
        provenance_role: Some("implementer".to_owned()),
    };
    Ok((
        request,
        OwnershipOptions {
            identity: Some(identity),
            release_bypass: false,
            start: false,
        },
    ))
}

/// Resolves scalar callers using exactly the ownership command's identity inputs.
fn scalar_identity(
    workspace: &Workspace,
    author: Option<&str>,
) -> Result<AuthorIdentity, Box<dyn std::error::Error>> {
    let settings: serde_json::Value = pm_rust::strict_json::from_slice(&std::fs::read(
        workspace.pm_root().join("settings.json"),
    )?)?;
    Ok(mutation_identity(
        author,
        settings["author_default"].as_str(),
    ))
}

/// Creates one item, stamping the argv-derived implementer role.
fn create_payload(
    workspace: &Workspace,
    mut request: CreateItem,
) -> Result<CreateResult, Box<dyn std::error::Error>> {
    request.provenance_role = Some("implementer".to_owned());
    mutation_identity(Some(&request.author), None)
        .with(|| workspace.create(request))
        .map_err(Into::into)
}

/// Applies one field update, stamping the argv-derived implementer role.
fn update_payload(
    workspace: &Workspace,
    identity: &AuthorIdentity,
    mut request: UpdateItem,
) -> Result<MutationResult, Box<dyn std::error::Error>> {
    request.provenance_role = Some("implementer".to_owned());
    identity
        .with(|| workspace.update(request))
        .map_err(Into::into)
}

/// Appends one comment and indexes its CLI receipt without changing durable metadata.
fn comment_payload(
    workspace: &Workspace,
    identity: &AuthorIdentity,
    request: &CommentItem,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let outcome = identity.with(|| workspace.comment(request))?;
    indexed_comment_receipt(serde_json::to_value(&outcome)).map_err(Into::into)
}

/// Closes one item, stamping the argv-derived implementer role.
fn close_payload(
    workspace: &Workspace,
    identity: &AuthorIdentity,
    mut request: CloseItem,
) -> Result<MutationResult, Box<dyn std::error::Error>> {
    request.provenance_role = Some("implementer".to_owned());
    identity
        .with(|| workspace.close(request))
        .map_err(Into::into)
}

/// Accepts canonical item namespaces emitted by verified retry commands.
fn native_arguments(mut args: Vec<std::ffi::OsString>) -> Vec<std::ffi::OsString> {
    // Ordinary commands need no namespace rewrite or second parser definition.
    if !args.iter().skip(1).any(|arg| arg == "item") {
        return args;
    }
    let raw = args
        .iter()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    if let Some(index) = Invocation::parse(&raw).command.map(|index| index + 1)
        && args.get(index).is_some_and(|arg| arg == "item")
    {
        match args.get(index + 1).and_then(|arg| arg.to_str()) {
            Some("deps") => {
                args.remove(index);
            }
            Some("comments") => {
                args.remove(index);
                args[index] = "comment".into();
            }
            _ => {}
        }
    }
    args
}

/// Rejects ambiguous or missing claim targets before any mutation.
fn validate_claim_target(
    id: Option<&str>,
    next: bool,
    start: bool,
    if_available: bool,
) -> Result<(), pm_rust::PmRustError> {
    let suffix = if start { " --start" } else { "" };
    let refusal = if id.is_none() && !next {
        Some((
            if start {
                "pm claim --start needs an item id, or --next to start the next available item"
            } else {
                "Specify an item id or pass --next"
            },
            serde_json::json!({"code":"missing_required_argument","required":"Pass an item id, or --next to select the next available item.","next_steps":[format!("Retry as pm claim <id>{suffix}, or pm claim --next{suffix}.")]}),
        ))
    } else if start && id.is_some() && (next || if_available) {
        Some((
            "--start cannot be combined with --next or --if-available for an explicit item",
            serde_json::json!({"code":"invalid_argument_value","required":"Remove --next and --if-available when starting an explicit item.","next_steps":["Remove --next and --if-available, keep the explicit item ID and other supplied arguments, then retry."]}),
        ))
    } else {
        id.filter(|_| next).map(|id| (
            "Specify either an item id or --next, not both",
            serde_json::json!({"code":"ambiguous_option_combination","required":"Pass either an item id or --next.","next_steps":[format!("Retry as pm claim {id}{suffix} to claim that item, or drop the id to select with --next.")]}),
        ))
    };
    match refusal {
        Some((detail, context)) => Err(pm_rust::PmRustError::CommandRefusal {
            detail: detail.to_owned(),
            exit_code: 2,
            context,
        }),
        None => Ok(()),
    }
}

/// Resolves explicit wire encoding before the legacy JSON switch for error output.
fn json_requested(args: &[String]) -> bool {
    let parsed = Invocation::parse(args);
    parsed
        .options
        .iter()
        .find(|option| option.name == "output-format")
        .map_or_else(
            || parsed.options.iter().any(|option| option.name == "json"),
            |option| option.value == Some("json"),
        )
}

/// Parses the command line and maps success or failure to the process exit code.
#[allow(clippy::too_many_lines)]
fn main() -> ExitCode {
    let cli = match Cli::try_parse_from(native_arguments(std::env::args_os().collect())) {
        Ok(cli) => cli,
        Err(error) => {
            let mut args = published_read_arguments(std::env::args_os().skip(1));
            let orientation = Invocation::parse(&args)
                .command
                .filter(|&index| matches!(args[index].as_str(), "context" | "history" | "deps"));
            if error.kind() == clap::error::ErrorKind::UnknownArgument
                && let Some(index) = orientation
            {
                let command = args.remove(index);
                args.insert(0, command);
                let allowed = match args[0].as_str() {
                    "context" => vec![
                        "--limit",
                        "--max-items",
                        "--token-budget",
                        "--token_budget",
                        "--depth",
                        "--after",
                        "--cursor",
                        "--no-tags",
                        "--no-extension-health",
                        "--json",
                    ],
                    "history" => vec![
                        "--limit",
                        "--full",
                        "--compact",
                        "--verify",
                        "--strict-exit",
                        "--fail-on-warn",
                        "--json",
                    ],
                    _ => vec![
                        "--format",
                        "--max-depth",
                        "--direction",
                        "--kind",
                        "--cursor",
                        "--collapse",
                        "--summary",
                        "--full",
                        "--node-limit",
                        "--edge-limit",
                        "--token-budget",
                        "--output-format",
                        "--json",
                    ],
                };
                let flag = rejected_orientation_argument(&error, &args, &allowed);
                let detail = format!(
                    "Native {} does not support {flag}; use the published pm CLI for this option.",
                    args[0]
                );
                let payload = orientation_error_json(
                    &detail,
                    &serde_json::json!({"code":"unsupported_native_option","required":"Use the published pm CLI for options outside the documented native slice.","title":"Unsupported native option","flag":flag,"value":flag}),
                    &args,
                    2,
                );
                let mut stderr = std::io::stderr().lock();
                let _ = write_command_refusal_to(
                    &mut stderr,
                    payload["detail"].as_str().unwrap_or_default(),
                    &serde_json::json!({}),
                    &args,
                    &payload,
                );
                return ExitCode::from(2);
            }
            // Keep Clap's bytes and statuses while allowing normal process teardown.
            // Immediate exit bypasses profile finalization on Windows.
            let diagnostic = error.render().to_string();
            let safe = refusal::diagnostic(&diagnostic, &raw_invocation());
            if safe == diagnostic {
                let _ = error.print();
            } else if error.use_stderr() {
                eprint!("{safe}");
            } else {
                print!("{safe}");
            }
            return if error.use_stderr() {
                ExitCode::from(2)
            } else {
                ExitCode::SUCCESS
            };
        }
    };
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if let Some(refusal) = error.downcast_ref::<pm_rust::discovery::DiscoveryRefusal>() {
                let payload = serde_json::json!({"ok":false,"refusal":refusal,"exit_code":2});
                let _ = write_json_to(&mut std::io::stderr().lock(), &payload);
                return ExitCode::from(2);
            }
            if let Some(refusal) = error.downcast_ref::<pm_rust::admission::AdmissionRefusal>() {
                let payload = serde_json::json!({"ok":false,"code":refusal.code,"required":refusal.required,"limits":refusal.limits,"exit_code":2});
                let _ = write_json_to(&mut std::io::stderr().lock(), &payload);
                return ExitCode::from(2);
            }
            if let Some(refusal) = error.downcast_ref::<pm_rust::changes::ChangeRefusal>() {
                let payload = serde_json::json!({"ok":false,"code":refusal.code(),"required":refusal.to_string(),"exit_code":2});
                let _ = write_json_to(&mut std::io::stderr().lock(), &payload);
                return ExitCode::from(2);
            }
            if let Some(refusal) = error.downcast_ref::<pm_rust::QueryCostRefusal>() {
                let payload = serde_json::json!({"ok":false,"code":refusal.code(),"required":refusal.to_string(),"exit_code":2});
                let _ = write_json_to(&mut std::io::stderr().lock(), &payload);
                return ExitCode::from(2);
            }
            if let Some(refusal) = error.downcast_ref::<pm_rust::SessionRefusal>() {
                let payload = serde_json::json!({"code":refusal.code(),"detail":refusal.to_string(),"required":refusal.to_string(),"exit_code":2});
                let _ = write_json_to(&mut std::io::stderr().lock(), &payload);
                return ExitCode::from(2);
            }
            if matches!(
                error.downcast_ref::<pm_rust::PmRustError>(),
                Some(pm_rust::PmRustError::HistoryVerificationFailed)
            ) {
                return ExitCode::from(1);
            }
            if let Some(pm_rust::PmRustError::CommandRefusal {
                detail,
                exit_code,
                context,
            }) = error.downcast_ref::<pm_rust::PmRustError>()
            {
                let args = published_read_arguments(std::env::args_os().skip(1));
                let payload = command_error_json(detail, context, &args, *exit_code);
                let mut stderr = std::io::stderr().lock();
                let _ = write_command_refusal_to(&mut stderr, detail, context, &args, &payload);
                return ExitCode::from(*exit_code);
            }
            if let Some(pm_rust::PmRustError::OwnershipRefusal { detail, context }) =
                error.downcast_ref::<pm_rust::PmRustError>()
            {
                let args = published_read_arguments(std::env::args_os().skip(1));
                let payload = ownership_error_json(detail, context, &args);
                let mut stderr = std::io::stderr().lock();
                let _ = write_command_refusal_to(&mut stderr, detail, context, &args, &payload);
                return ExitCode::from(4);
            }
            if let Some(pm_rust::PmRustError::LockConflict { .. }) =
                error.downcast_ref::<pm_rust::PmRustError>()
            {
                let args = published_read_arguments(std::env::args_os().skip(1));
                let payload = ownership_error_json(
                    &error.to_string(),
                    &serde_json::json!({"code":"lock_conflict"}),
                    &args,
                );
                let _ = write_json_to(
                    &mut std::io::stderr().lock(),
                    &complete_refusal(&payload, &raw_invocation()),
                );
                return ExitCode::from(4);
            }
            if matches!(
                error.downcast_ref::<pm_rust::PmRustError>(),
                Some(pm_rust::PmRustError::OutputBudgetExceeded)
            ) {
                return ExitCode::from(2);
            }
            if let Some(pm_rust::PmRustError::ReadCursor { code, detail }) =
                error.downcast_ref::<pm_rust::PmRustError>()
            {
                let args = published_read_arguments(std::env::args_os().skip(1));
                let payload = cursor_error_json(code, detail, &args);
                let mut stderr = std::io::stderr().lock();
                let _ = write_command_refusal_to(
                    &mut stderr,
                    payload["detail"].as_str().unwrap_or_default(),
                    &serde_json::json!({}),
                    &args,
                    &payload,
                );
            } else {
                let detail = refusal::diagnostic(&error.to_string(), &raw_invocation());
                eprintln!("pm-rust: {detail}");
            }
            ExitCode::from(2)
        }
    }
}

/// Retains Clap's rejected token, falling back only to unsupported option names.
fn rejected_orientation_argument<'a>(
    error: &'a clap::Error,
    args: &'a [String],
    allowed: &[&str],
) -> &'a str {
    let rejected = match error.get(clap::error::ContextKind::InvalidArg) {
        Some(clap::error::ContextValue::String(arg)) => Some(arg.as_str()),
        _ => None,
    };
    rejected
        .or_else(|| {
            Invocation::parse(args)
                .options
                .iter()
                .map(|option| args[option.start].as_str())
                .find(|arg| !allowed.contains(&arg.split('=').next().unwrap_or_default()))
        })
        .map_or("option", |arg| arg.split('=').next().unwrap_or_default())
}

/// Completes scope and credential projection once before any refusal rendering.
fn complete_refusal(payload: &serde_json::Value, raw: &[String]) -> serde_json::Value {
    let mut payload = payload.clone();
    preserve_refusal_scope(&mut payload, raw);
    refusal::redact(&mut payload, raw);
    if payload["title"]
        .as_str()
        .is_some_and(|title| title.chars().count() > 120)
    {
        payload["title"] = serde_json::json!(format!(
            "{}...",
            payload["title"]
                .as_str()
                .unwrap_or_default()
                .chars()
                .take(117)
                .collect::<String>()
        ));
    }
    payload
}

/// Writes a command refusal using its requested encoding and published human fallback.
fn write_command_refusal_to(
    writer: &mut dyn Write,
    detail: &str,
    context: &serde_json::Value,
    args: &[String],
    payload: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let structured = json_requested(args)
        || (context["code"] != "close_validation_missing_fields"
            && args.first().is_some_and(|command| {
                matches!(command.as_str(), "create" | "update" | "comment" | "close")
            }));
    let mut source = payload.clone();
    if !structured
        && let Some(why) = context["why"]
            .as_str()
            .filter(|why| why.contains("No rollback was performed."))
    {
        source["why"] = serde_json::json!(why);
    }
    let payload = complete_refusal(&source, &raw_invocation());
    if context["code"] == "unknown_error" && !json_requested(args) {
        writeln!(writer, "{}", payload["detail"].as_str().unwrap_or(detail))?;
        return Ok(());
    }
    if structured {
        write_json_to(writer, &payload)
    } else {
        write_cursor_refusal_to(writer, &payload)
    }
}

/// Preserves ordinary command refusals and the published unhandled-error envelope.
fn command_error_json(
    detail: &str,
    context: &serde_json::Value,
    args: &[String],
    exit_code: u8,
) -> serde_json::Value {
    if context["recovery"]["token_budget"].is_object() {
        return context.clone();
    }
    if context["code"] == "unknown_error" {
        return unhandled_error_json(detail, exit_code);
    }
    if let Some(recovery) = context["recovery"]
        .as_object()
        .filter(|_| context["code"] == "invalid_query_cursor")
    {
        let mut payload = cursor_error_json("invalid_query_cursor", detail, args);
        let retry = context["recovery"]["suggested_retry"].clone();
        let steps = payload["next_steps"]
            .as_array()
            .into_iter()
            .flatten()
            .cloned()
            .chain([retry])
            .collect::<Vec<_>>();
        payload["next_steps"] = serde_json::json!(steps);
        for (key, value) in recovery {
            payload["recovery"][key] = value.clone();
        }
        return payload;
    }
    if context["reason"] == "assurance_evaluation_refused" {
        let mut payload = ownership_error_json(detail, context, args);
        payload["exit_code"] = serde_json::json!(exit_code);
        payload["refusal"] = serde_json::json!({"surface":"assurance","rejected_value":context["value"],"exit_code":exit_code});
        payload["changed"] = context["changed"].clone();
        payload["value"] = context["value"].clone();
        for (key, value) in context["recovery"].as_object().into_iter().flatten() {
            payload["recovery"][key] = value.clone();
        }
        return payload;
    }
    if args
        .first()
        .is_some_and(|arg| matches!(arg.as_str(), "context" | "history" | "deps"))
    {
        return orientation_error_json(detail, context, args, exit_code);
    }
    if args.first().is_some_and(|arg| arg == "search") {
        return search_error_json(detail, context, args, exit_code);
    }
    if args.first().is_some_and(|arg| arg == "item") {
        return collection_error_json(detail, context, args, exit_code);
    }
    let mut payload = ownership_error_json(detail, context, args);
    payload["exit_code"] = serde_json::json!(exit_code);
    payload["refusal"] = if args.first().is_some_and(|command| command == "close") {
        close_error_refusal(detail, &payload, exit_code)
    } else {
        command_refusal_detail(detail, args, exit_code)
    };
    payload
}

/// Renders the published `TypeError` envelope without adding command-recovery metadata.
fn unhandled_error_json(detail: &str, exit_code: u8) -> serde_json::Value {
    serde_json::json!({"code":"unknown_error","required":"Inspect command input and runtime state, then retry.","exit_code":exit_code,"type":"urn:pm-cli:error:unknown_error","title":"Unhandled error","detail":detail,"why":"Unexpected runtime failures can occur from environment or extension-level issues.","examples":["pm --help","pm health --json"],"refusal":{"surface":"command","exit_code":exit_code}})
}

/// Preserves the published canonical dependency spelling in recovery arguments.
fn orientation_arguments(args: &[String]) -> Vec<String> {
    let mut args = args.to_vec();
    if args.first().is_some_and(|arg| arg == "deps") {
        args.insert(0, "item".to_owned());
    }
    args
}

/// Matches orientation command failures to the published problem envelope.
fn orientation_error_json(
    detail: &str,
    context: &serde_json::Value,
    args: &[String],
    exit_code: u8,
) -> serde_json::Value {
    let args = orientation_arguments(args);
    let command = &args[0];
    let mut context = context.clone();
    if detail.starts_with("--strict-exit requires --verify") {
        context["code"] = serde_json::json!("invalid_argument_value");
    }

    if context["code"] == "invalid_argument_value" {
        context["required"] =
            serde_json::json!("Use values that match documented command constraints.");
        context["title"] = serde_json::json!("Invalid argument value");
        context["next_steps"] = serde_json::json!([
            "Check allowed values in command help, then rerun with corrected input."
        ]);
        context["examples"] = serde_json::json!([
            format!("pm {command} --help"),
            "pm contracts --command <command> --flags-only --json".to_owned()
        ]);
    }
    let mut result = ownership_error_json(detail, &context, &args);
    if context["code"] == "item_not_found" {
        result["required"] =
            serde_json::json!("Use an existing item ID from current tracker data.");
        result["title"] = serde_json::json!("Item ID not found");
        result["why"] = serde_json::json!("Mutation and read commands operate only on known IDs.");
        result["examples"] = serde_json::json!([
            "pm list --status open --limit 20",
            "pm search \"<keyword>\" --limit 10"
        ]);
        result["next_steps"] = serde_json::json!([
            "Confirm the active --path/PM_PATH scope, then retry with a valid id."
        ]);
        result["refusal"] = serde_json::json!({"surface":"id","rejected_value":context["id"],"exit_code":exit_code});
        result["flag"] = serde_json::json!("id");
        result["value"] = context["id"].clone();
    } else {
        result["refusal"] = command_refusal_detail(detail, &args, exit_code);
    }
    if detail.starts_with("--strict-exit requires --verify") {
        result["recovery"]["missing"] = serde_json::json!(["--verify"]);
        result["recovery"]["suggested_retry"] = serde_json::json!(format!(
            "pm {} --verify",
            args.iter()
                .map(|arg| pm_rust::quote_command_arg(arg))
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    result["exit_code"] = serde_json::json!(exit_code);
    // Insertion order is part of the wire contract.
    let mut ordered = serde_json::Map::new();
    for key in [
        "code",
        "required",
        "recovery",
        "next_steps",
        "exit_code",
        "type",
        "title",
        "detail",
        "why",
        "examples",
        "refusal",
        "flag",
        "value",
    ] {
        if let Some(value) = result.get(key) {
            ordered.insert(key.to_owned(), value.clone());
        }
    }
    serde_json::Value::Object(ordered)
}

/// Preserves keyword-search usage classification and closed-domain recovery.
fn search_error_json(
    detail: &str,
    context: &serde_json::Value,
    args: &[String],
    exit_code: u8,
) -> serde_json::Value {
    let mut context = context.clone();
    if context.get("code").is_none()
        && (detail.starts_with("Invalid ")
            || detail.contains(" must be ")
            || detail.contains(" requires "))
    {
        context = serde_json::json!({"code":"invalid_argument_value"});
    }
    if context["code"] == "invalid_argument_value" {
        context["required"] =
            serde_json::json!("Use values that match documented command constraints.");
        context["title"] = serde_json::json!("Invalid argument value");
        if context.get("next_steps").is_none() {
            context["next_steps"] = serde_json::json!([
                "Check allowed values in command help, then rerun with corrected input."
            ]);
            context["examples"] = serde_json::json!([
                "pm search --help",
                "pm contracts --command <command> --flags-only --json"
            ]);
        }
    }
    let mut result = ownership_error_json(detail, &context, args);
    result["recovery"]["normalized_args"] = serde_json::json!(args);
    result["exit_code"] = serde_json::json!(exit_code);
    result["refusal"] = command_refusal_detail(detail, args, exit_code);
    if detail.starts_with("--output-budget ") || detail.starts_with("--output-limit ") {
        result["refusal"] = serde_json::json!({"surface":"search","exit_code":exit_code});
    }
    if let Some(recovery) = context["recovery"].as_object() {
        for (key, value) in recovery {
            result["recovery"][key] = value.clone();
        }
    }
    if let Some(domain) = context["recovery"].get("allowed_values") {
        let rejected = args.iter().enumerate().find_map(|(index, arg)| {
            arg.strip_prefix("--type=").or_else(|| {
                (arg == "--type")
                    .then(|| args.get(index + 1).map(String::as_str))
                    .flatten()
            })
        });
        result["refusal"] = serde_json::json!({"surface":"--type","rejected_value":rejected,"legal_domain":domain,"exit_code":exit_code});
        // The pinned oracle extracts equals-form values, but emits a corrected
        // retry only for the separate-token spelling. Preserve its exact bytes.
        if let Some(index) = args.iter().position(|arg| arg == "--type") {
            let mut retry = args.to_vec();
            if let Some(value) = retry.get_mut(index + 1) {
                domain[0].as_str().unwrap_or_default().clone_into(value);
            }
            result["examples"] = serde_json::json!([
                format!(
                    "pm {}",
                    retry
                        .iter()
                        .map(|arg| pm_rust::quote_command_arg(arg))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                "pm search --help"
            ]);
        } else {
            result["examples"] = serde_json::json!([
                "pm search --help",
                "pm contracts --command <command> --flags-only --json"
            ]);
        }
        let provided = result["recovery"]["provided_fields"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .map(|flag| flag.split('=').next().unwrap_or_default())
            .collect::<Vec<_>>();
        result["recovery"]["provided_fields"] = serde_json::json!(provided);
    }
    if let Ok(Cli {
        command: Command::Search { query, .. },
        ..
    }) = Cli::try_parse_from(std::iter::once("pm-rust".to_owned()).chain(args.iter().cloned()))
        && let Some(examples) = result["examples"].as_array_mut()
    {
        for example in examples {
            if let Some(text) = example.as_str() {
                *example = serde_json::json!(
                    text.replace("<query>", &pm_rust::quote_command_arg(&query.join(" ")))
                );
            }
        }
    }
    result
}

/// Removes native clock and discovery controls from published error recovery arguments.
///
/// Callers pass `args_os` so a non-UTF-8 argument cannot panic this refusal
/// path. Each argument is lossily owned, then published in the normal JSON
/// envelope with exit code 2.
fn published_read_arguments<I, S>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let raw = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let parsed = Invocation::parse(&raw);
    let mut excluded = vec![false; raw.len()];
    for option in &parsed.options {
        if option.name == "timestamp" || (option.bootstrap && option.name == "workspace") {
            excluded[option.start..option.end].fill(true);
        }
    }
    let mut arguments = raw
        .iter()
        .enumerate()
        .filter(|(index, _)| !excluded[*index])
        .map(|(_, argument)| argument.clone())
        .collect::<Vec<_>>();
    if arguments.starts_with(&["item".to_owned(), "deps".to_owned()]) {
        arguments.remove(0);
    }
    if arguments.first().is_some_and(|command| command == "close") {
        let aliases = Invocation::parse(&arguments)
            .options
            .iter()
            .filter_map(|option| match arguments[option.start].as_str() {
                "--expected" => Some((option.start, "--expected-result")),
                "--actual" => Some((option.start, "--actual-result")),
                _ => None,
            })
            .collect::<Vec<_>>();
        for (index, alias) in aliases {
            alias.clone_into(&mut arguments[index]);
        }
    }
    if arguments
        .first()
        .is_some_and(|command| matches!(command.as_str(), "files" | "docs" | "test"))
    {
        arguments.insert(0, "item".to_owned());
    }
    arguments
}

/// Applies the oracle's item namespace and error classification to collection refusals.
fn collection_error_json(
    detail: &str,
    context: &serde_json::Value,
    args: &[String],
    exit_code: u8,
) -> serde_json::Value {
    let mut context = context.clone();
    if context.get("code").is_none()
        && exit_code == 2
        && (detail.starts_with("Invalid ")
            || detail.contains(" must be ")
            || detail.contains(" requires "))
    {
        context = serde_json::json!({"code":"invalid_argument_value","required":"Use values that match documented command constraints.","next_steps":["Check allowed values in command help, then rerun with corrected input."],"title":"Invalid argument value","examples":["pm item --help","pm contracts --command <command> --flags-only --json"]});
    }
    let missing = detail.starts_with("Item ") && detail.ends_with(" not found");
    if missing {
        let steps = context.get("next_steps").cloned().unwrap_or_else(|| {
            serde_json::json!([
                "Confirm the active --path/PM_PATH scope, then retry with a valid id."
            ])
        });
        context = serde_json::json!({"code":"item_not_found","required":"Use an existing item ID from current tracker data.","next_steps":steps,"title":"Item ID not found","why":"Mutation and read commands operate only on known IDs.","examples":["pm list --status open --limit 20","pm search \"<keyword>\" --limit 10"]});
    }
    let mut result = ownership_error_json(detail, &context, args);
    if let Some(recovery) = context["recovery"].as_object() {
        for (key, value) in recovery {
            result["recovery"][key] = value.clone();
        }
    }
    result["exit_code"] = serde_json::json!(exit_code);
    result["refusal"] = command_refusal_detail(detail, &args[1..], exit_code);
    if missing {
        let id = &detail["Item ".len()..detail.len() - " not found".len()];
        result["refusal"] =
            serde_json::json!({"surface":"id","rejected_value":id,"exit_code":exit_code});
        result["flag"] = serde_json::json!("id");
        result["value"] = serde_json::json!(id);
    }
    if context["code"] == "unknown_option" {
        result.as_object_mut().map(|map| map.shift_remove("why"));
        result["refusal"] = serde_json::json!({"surface":"--remove-index","rejected_value":"--remove-index","exit_code":exit_code});
    }
    result
}

/// Captures the real invocation lossily so malformed OS strings cannot panic in diagnostics.
fn raw_invocation() -> Vec<String> {
    std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

/// Resolves an explicit native workspace into canonical published bootstrap tokens.
fn invocation_scope(raw: &[String]) -> Vec<String> {
    Invocation::parse(raw)
        .scope()
        .map_or_else(Vec::new, |path| {
            if path.starts_with('-') && path != "-" {
                vec![format!("--pm-path={path}")]
            } else {
                vec!["--pm-path".to_owned(), path.to_owned()]
            }
        })
}

/// Removes only bootstrap scope options while retaining every command value verbatim.
fn strip_bootstrap_scope(args: &mut Vec<String>) {
    let ranges = Invocation::parse(args)
        .options
        .iter()
        .filter(|option| option.bootstrap && option.name == "workspace")
        .map(|option| option.start..option.end)
        .collect::<Vec<_>>();
    for range in ranges.into_iter().rev() {
        args.drain(range);
    }
}

/// Renders canonical recovery arguments with the shared published shell renderer.
fn render_recovery(args: &[String]) -> String {
    format!(
        "pm {}",
        args.iter()
            .map(|arg| pm_rust::quote_command_arg(arg))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

/// Adds invocation scope to executable advice while respecting an explicit producer scope.
fn scope_recovery_command(command: &str, scope: &[String]) -> Option<String> {
    let command = command.strip_prefix("pm ")?;
    let prefix = command
        .split_whitespace()
        .take(2)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if Invocation::parse(&prefix).scope().is_some() {
        return None;
    }
    Some(format!("{} {command}", render_recovery(scope)))
}

/// Preserves explicit tracker scope for every recovery bundle, including close evidence.
fn preserve_refusal_scope(payload: &mut serde_json::Value, raw: &[String]) {
    let scope = invocation_scope(raw);
    if scope.is_empty() {
        return;
    }
    if let Some(steps) = payload
        .get_mut("next_steps")
        .and_then(serde_json::Value::as_array_mut)
    {
        for step in steps {
            if let Some(scoped) = step
                .as_str()
                .and_then(|text| scope_recovery_command(text, &scope))
            {
                *step = serde_json::json!(scoped);
            }
        }
    }
    let Some(recovery) = payload["recovery"].as_object_mut() else {
        return;
    };
    for (args_key, command_key) in [
        ("normalized_args", "attempted_command"),
        ("suggested_retry_args", "suggested_retry"),
    ] {
        let Some(values) = recovery.get(args_key).and_then(serde_json::Value::as_array) else {
            if let Some(scoped) = recovery
                .get(command_key)
                .and_then(serde_json::Value::as_str)
                .and_then(|text| scope_recovery_command(text, &scope))
            {
                recovery.insert(command_key.to_owned(), serde_json::json!(scoped));
            }
            continue;
        };
        let mut args = values
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if Invocation::parse(&args).scope().is_none() {
            args.splice(0..0, scope.clone());
        }
        recovery.insert(
            command_key.to_owned(),
            serde_json::json!(render_recovery(&args)),
        );
        recovery.insert(args_key.to_owned(), serde_json::json!(args));
    }
    if recovery
        .get("normalized_args")
        .is_some_and(serde_json::Value::is_array)
    {
        let mut fields = recovery
            .get("provided_fields")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        fields.retain(|flag| flag != "--pm-path");
        fields.insert(0, serde_json::json!("--pm-path"));
        recovery.insert("provided_fields".to_owned(), serde_json::json!(fields));
    }
    if payload["code"] == "lock_conflict" {
        payload["examples"] = serde_json::json!([payload["recovery"]["suggested_retry"]]);
    }
    if payload["code"] == "close_validation_missing_fields"
        && let Some(scoped) = payload["recovery"]["next_best_command"]
            .as_str()
            .and_then(|command| scope_recovery_command(command, &scope))
    {
        payload["recovery"]["next_best_command"] = serde_json::json!(scoped);
    }
}

/// Formats the published machine-readable refusal for a rejected list cursor.
fn cursor_error_json(code: &str, detail: &str, args: &[String]) -> serde_json::Value {
    let mut provided = Vec::new();
    for arg in args {
        let flag = arg.split('=').next().unwrap_or_default().to_owned();
        if flag.starts_with("--") && !provided.contains(&flag) {
            provided.push(flag);
        }
    }
    let recovery_args = orientation_arguments(args);
    let mut payload = serde_json::json!({"code":code,"required":"Adjust command input or tracker state and retry.","recovery":{"attempted_command":format!("pm {}",recovery_args.iter().map(|arg| pm_rust::quote_command_arg(arg)).collect::<Vec<_>>().join(" ")),"normalized_args":recovery_args,"provided_fields":provided}});
    if code == "invalid_query_cursor" {
        let flag = if args
            .iter()
            .any(|arg| arg == "--cursor" || arg.starts_with("--cursor="))
        {
            "--cursor"
        } else {
            "--after"
        };
        payload["next_steps"] = serde_json::json!([format!(
            "Repeat the original query without {flag} to obtain a fresh cursor."
        )]);
    }
    for (key, value) in [
        ("exit_code", serde_json::json!(2)),
        (
            "type",
            serde_json::json!(format!("urn:pm-cli:error:{code}")),
        ),
        ("title", serde_json::json!(detail)),
        ("detail", serde_json::json!(detail)),
        (
            "why",
            serde_json::json!(
                "pm enforces explicit, deterministic contracts for data and command semantics."
            ),
        ),
        (
            "examples",
            serde_json::json!(["pm --help", "pm <command> --help"]),
        ),
        (
            "refusal",
            serde_json::json!({"surface":args.first().filter(|command| matches!(command.as_str(),"context"|"deps"|"search"|"assurance")).map_or("list",String::as_str),"exit_code":2}),
        ),
    ] {
        payload[key] = value;
    }
    payload
}

/// Formats a published ownership conflict and command recovery receipt.
fn ownership_error_json(
    detail: &str,
    context: &serde_json::Value,
    args: &[String],
) -> serde_json::Value {
    let mut recovery_args = args.to_vec();
    if context["code"] == "lock_conflict" && args.first().is_some_and(|arg| arg == "comment") {
        "comments".clone_into(&mut recovery_args[0]);
        recovery_args.insert(0, "item".to_owned());
    }
    let mut payload = cursor_error_json(
        context["code"].as_str().unwrap_or("command_failed"),
        detail,
        &recovery_args,
    );
    apply_ownership_guidance(&mut payload, context);
    for key in ["required", "examples", "next_steps", "title"] {
        if let Some(value) = context.get(key) {
            payload[key] = value.clone();
        }
    }
    let include_why = context["code"].as_str().is_some_and(|code| {
        !matches!(
            code,
            "unknown_command"
                | "unknown_option"
                | "missing_required_option"
                | "missing_required_argument"
                | "invalid_argument_value"
                | "invalid_command_usage"
                | "command_failed"
        )
    });
    if include_why && let Some(why) = context.get("why") {
        payload["why"] = why.clone();
    }
    apply_close_error_recovery(&mut payload, context, args);
    let mut ordered = serde_json::Map::new();
    for key in [
        "code",
        "required",
        "recovery",
        "next_steps",
        "exit_code",
        "type",
        "title",
        "detail",
        "why",
        "examples",
        "refusal",
    ] {
        if key == "why" && !include_why {
            continue;
        }
        if let Some(value) = payload.get(key) {
            ordered.insert(key.to_owned(), value.clone());
        }
    }
    payload = serde_json::Value::Object(ordered);
    payload["exit_code"] = serde_json::json!(4);
    payload["refusal"] = command_refusal_detail(
        detail,
        if args.first().is_some_and(|s| s == "item") {
            &args[1..]
        } else {
            args
        },
        4,
    );
    for key in ["flag", "value", "claimed_by", "claimed_at", "claim_source"] {
        if let Some(value) = context.get(key) {
            payload[key] = value.clone();
        }
    }
    if context.get("claimed_by").is_some() {
        payload["item_id"] = context["item_id"].clone();
    }
    if let Some(details) = context.get("details") {
        payload["details"] = details.clone();
    }
    payload
}

/// Applies code-specific ownership and lock guidance at the common envelope boundary.
fn apply_ownership_guidance(payload: &mut serde_json::Value, context: &serde_json::Value) {
    match context["code"].as_str() {
        Some("ownership_conflict") => {
            payload["title"] = serde_json::json!("Ownership conflict");
            payload["required"] = serde_json::json!(
                "Run as the assigned owner, claim the item when appropriate, or use --force only for an approved override."
            );
            payload["next_steps"] = serde_json::json!([
                "Use --force for approved systematic metadata updates performed by leads or maintainers.",
                "Use --force when correcting known stale metadata after coordinating ownership changes.",
                "For non-terminal reassignment, prefer \"pm claim <ID>\" before running other mutations."
            ]);
            payload["examples"] = serde_json::json!([
                "pm claim pm-a1b2",
                "pm update pm-a1b2 --status in_progress --force"
            ]);
            payload["why"] = serde_json::json!(
                "Ownership checks prevent accidental concurrent mutations on claimed items and protect against conflicting writes."
            );
        }
        Some("lock_conflict") => {
            payload["title"] = serde_json::json!("Lock conflict");
            payload["required"] = serde_json::json!(
                "Wait for lock release, then retry the same operation. Set PM_LOCK_WAIT_MS to a larger bounded millisecond budget for expected contention."
            );
            payload["why"] =
                serde_json::json!("Locking protects item files from concurrent write races.");
            let retry = payload["recovery"]["attempted_command"].clone();
            payload["examples"] = serde_json::json!([retry]);
            payload["recovery"]["suggested_retry"] = retry;
            payload["recovery"]["suggested_retry_args"] =
                payload["recovery"]["normalized_args"].clone();
        }
        None => {
            payload["title"] = serde_json::json!("Command failed");
        }
        _ => {}
    }
}

/// Mirrors the published withdrawal when delimited command data resembles a missing evidence flag.
fn close_retry_unambiguous(args: &[String], missing: &[String]) -> bool {
    !args.iter().any(|arg| arg == "--")
        || args.iter().skip(1).all(|arg| {
            let flag = match arg.split('=').next().unwrap_or_default() {
                "--expected" | "--expected_result" => "--expected-result",
                "--actual" | "--actual_result" => "--actual-result",
                flag => flag,
            };
            !missing.iter().any(|missing| missing == flag)
        })
}

/// Adds actionable close evidence recovery at the shared refusal boundary.
fn apply_close_error_recovery(
    payload: &mut serde_json::Value,
    context: &serde_json::Value,
    args: &[String],
) {
    if args.first().is_some_and(|command| command == "close")
        && let Some(recovery) = context["recovery"].as_object()
    {
        for (key, value) in recovery {
            payload["recovery"][key] = value.clone();
        }
    }
    if args.first().is_some_and(|command| command == "close") {
        if let Some(values) = payload["recovery"]["suggested_retry_args"].as_array() {
            let mut retry = values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            strip_bootstrap_scope(&mut retry);
            payload["recovery"]["suggested_retry"] = serde_json::json!(render_recovery(&retry));
            payload["recovery"]["suggested_retry_args"] = serde_json::json!(retry);
        }
        if context["code"] == "close_validation_missing_fields" {
            let missing = payload["recovery"]["missing"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let mut retry = args.to_vec();
            let insertion = if retry.iter().any(|arg| arg == "--") {
                1
            } else {
                retry.len()
            };
            retry.splice(
                insertion..insertion,
                missing
                    .iter()
                    .flat_map(|flag| [flag.clone(), "<value>".to_owned()]),
            );
            let target = context["recovery"]["suggested_retry_args"]
                .as_array()
                .and_then(|values| {
                    values
                        .windows(2)
                        .find(|pair| pair[0] == "update")
                        .and_then(|pair| pair[1].as_str())
                })
                .or_else(|| args.get(1).map(String::as_str))
                .unwrap_or_default();
            let mut update = vec!["update".to_owned(), target.to_owned()];
            for flag in &missing {
                update.extend([flag.clone(), "<value>".to_owned()]);
            }
            let mut recovery = serde_json::json!({"attempted_command":payload["recovery"]["attempted_command"],"normalized_args":payload["recovery"]["normalized_args"],"provided_fields":payload["recovery"]["provided_fields"],"missing":missing});
            if close_retry_unambiguous(args, &missing) {
                recovery["suggested_retry"] = serde_json::json!(render_recovery(&retry));
                recovery["suggested_retry_args"] = serde_json::json!(retry);
            }
            recovery["next_best_command"] = serde_json::json!(render_recovery(&update));
            payload["recovery"] = recovery;
            payload["title"] = serde_json::json!("Close is missing required evidence");
            payload["required"] = serde_json::json!(format!(
                "Supply {}: pm close accepts them and records them in the same write as the closure.",
                missing.join(", ")
            ));
            payload["next_steps"] = serde_json::json!([
                "Run recovery.suggested_retry when it is present: the same close with the missing flags added, so one write records the evidence and the closure.",
                "Without it, record the evidence with recovery.next_best_command, then repeat the original close.",
                "Use --validate-close warn only when governance permits an evidenced exception."
            ]);
        }
    }
}

/// Formats command-specific rejected flag values in usage refusals.
fn command_refusal_detail(detail: &str, args: &[String], exit_code: u8) -> serde_json::Value {
    let parsed = Invocation::parse(args);
    let surface = detail
        .split_whitespace()
        .filter(|word| word.starts_with("--"))
        .map(|word| word.trim_end_matches([',', '.']))
        .find(|word| args.iter().any(|arg| arg == word));
    if let Some(surface) = surface {
        if detail.contains("not both") {
            return serde_json::json!({"surface":surface,"exit_code":exit_code});
        }
        let rejected = args
            .iter()
            .position(|arg| arg == surface)
            .and_then(|index| {
                args.get(index + 1)
                    .filter(|_| !parsed.is_identity_or_format(index + 1))
            });
        if let Some(rejected) = rejected {
            return serde_json::json!({"surface":surface,"rejected_value":rejected,"exit_code":exit_code});
        }
        return serde_json::json!({"surface":surface,"exit_code":exit_code});
    }
    serde_json::json!({"surface":args[0],"exit_code":exit_code})
}

/// Extracts closure refusal flags from prose and recovery evidence in oracle order.
fn close_error_refusal(
    detail: &str,
    payload: &serde_json::Value,
    exit_code: u8,
) -> serde_json::Value {
    let normalized = payload["recovery"]["normalized_args"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let args: Vec<String> = normalized
        .iter()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_owned)
        .collect();
    let parsed = Invocation::parse(&args);
    let flag = payload["flag"].as_str().or_else(|| {
        detail
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_')
            .filter(|word| word.starts_with("--"))
            .find(|word| args.iter().any(|arg| arg == word))
    });
    let surface = flag
        .or_else(|| payload["recovery"]["missing"][0].as_str())
        .unwrap_or("close");
    let mut receipt = serde_json::json!({"surface":surface});
    if let Some(flag) = flag {
        let rejected = args.iter().position(|arg| *arg == flag).and_then(|index| {
            args.get(index + 1)
                .filter(|_| !parsed.is_identity_or_format(index + 1))
        });
        if let Some(rejected) = rejected {
            receipt["rejected_value"] = serde_json::json!(rejected);
        }
    }
    receipt["exit_code"] = serde_json::json!(exit_code);
    receipt
}

/// Writes list output in the same encoding used for its budget measurement.
fn write_list_to(
    writer: &mut dyn Write,
    result: &pm_rust::ListOutput,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        result.write_read_json(writer)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        return Ok(());
    }
    writer.write_all(pm_rust::stringify_toon(result)?.as_bytes())?;
    writer.flush()?;
    Ok(())
}

/// Writes the published human-readable cursor diagnostic when JSON was not requested.
fn write_cursor_refusal_to(
    writer: &mut dyn Write,
    payload: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let attempted = payload["recovery"]["normalized_args"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(pm_rust::quote_command_arg)
        .collect::<Vec<_>>()
        .join(" ");
    let attempted = payload["recovery"]["attempted_command"]
        .as_str()
        .map_or_else(|| format!("pm {attempted}"), str::to_owned);
    let close_evidence = payload["code"] == "close_validation_missing_fields";
    let run = payload["recovery"]["suggested_retry"].as_str().filter(|_| close_evidence)
        .map_or_else(String::new, |retry| format!("\nRun: {retry}\nTemplate: replace every placeholder with real evidence before running."));
    let mut parts = vec![format!(
        "Error: {}{run}\n\nWhat is required:\n  {}",
        payload["title"].as_str().unwrap_or_default(),
        payload["required"].as_str().unwrap_or_default()
    )];
    if let Some(steps) = payload["next_steps"].as_array() {
        parts.push("\nNext steps:".to_owned());
        parts.extend(
            steps
                .iter()
                .map(|step| format!("  - {}", step.as_str().unwrap_or_default())),
        );
    }
    if payload["detail"] != payload["title"] {
        parts.push(format!(
            "\nWhat happened:\n  {}",
            payload["detail"].as_str().unwrap_or_default()
        ));
    }
    if let Some(holder) = payload["claimed_by"].as_str() {
        let holder = if holder.chars().count() > 120 {
            format!("{}...", holder.chars().take(120).collect::<String>())
        } else {
            holder.to_owned()
        };
        let since = payload["claimed_at"]
            .as_str()
            .filter(|value| !value.is_empty())
            .map_or_else(String::new, |value| format!(" since {value}"));
        parts.push(format!(
            "\nHeld by: {holder} ({}{since})",
            payload["claim_source"].as_str().unwrap_or("unknown")
        ));
    }
    append_refusal_why(&mut parts, payload);
    if !close_evidence && let Some(examples) = payload["examples"].as_array() {
        parts.push("\nExamples:".to_owned());
        parts.extend(
            examples
                .iter()
                .map(|example| format!("  - {}", example.as_str().unwrap_or_default())),
        );
    }
    append_refusal_details(&mut parts, payload);
    if payload["recovery"]["token_budget"].is_object() {
        append_budget_recovery(&mut parts, &payload["recovery"]);
    } else {
        parts.push("\nRecovery bundle:".to_owned());
        if !close_evidence && let Some(retry) = payload["recovery"]["suggested_retry"].as_str() {
            parts.push(format!("  suggested_retry: {retry}"));
        }
        parts.push(format!("  attempted_command: {attempted}"));
    }
    if let Some(fields) = payload["recovery"]["provided_fields"]
        .as_array()
        .filter(|fields| !fields.is_empty())
    {
        parts.push(format!(
            "  provided_fields: {}",
            fields
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if close_evidence {
        if let Some(missing) = payload["recovery"]["missing"].as_array() {
            parts.push(format!(
                "  missing: {}",
                missing
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if let Some(command) = payload["recovery"]["next_best_command"].as_str() {
            parts.push(format!("  next_best_command: {command}"));
        }
    }

    append_recovery_domains(&mut parts, &payload["recovery"]);
    writer.write_all(format!("{}\n", parts.join("\n")).as_bytes())?;
    writer.flush()?;
    Ok(())
}

/// Appends structured refusal details in the published human indentation.
fn append_refusal_details(parts: &mut Vec<String>, payload: &serde_json::Value) {
    if let Some(details) = payload.get("details") {
        parts.push("\nDetails:".to_owned());
        parts.extend(
            pm_rust::stringify_json(details, true)
                .lines()
                .map(|line| format!("  {line}")),
        );
    }
}

/// Appends published human explanations while keeping close-evidence guidance in its recovery hint.
fn append_refusal_why(parts: &mut Vec<String>, payload: &serde_json::Value) {
    if payload["code"] == "close_validation_missing_fields" {
        return;
    }
    if let Some(why) = payload["why"].as_str() {
        parts.push(format!("\nWhy:\n  {why}"));
    } else if payload["recovery"]["token_budget"].is_object() {
        parts.push("\nWhy:\n  Budget advice is verified against the current query. A minimum is reported only after every smaller positive budget is checked.".to_owned());
    } else if payload["code"] == "command_failed" {
        parts.push("\nWhy:\n  pm enforces explicit, deterministic contracts for data and command semantics.".to_owned());
    } else if payload["code"] == "invalid_argument_value" {
        parts.push("\nWhy:\n  Validation protects data consistency and deterministic behavior across commands.".to_owned());
    } else if payload["code"] == "unknown_option" {
        parts.push(
            "\nWhy:\n  Option contracts are command-specific and intentionally validated."
                .to_owned(),
        );
    }
}

/// Bounds human legal-domain examples without changing complete structured evidence.
fn append_recovery_domains(parts: &mut Vec<String>, recovery: &serde_json::Value) {
    for key in [
        "allowed_values",
        "candidate_commands",
        "candidate_commands_total",
        "option_scope",
    ] {
        if let Some(value) = recovery.get(key) {
            let value = value.as_array().map_or_else(
                || {
                    value
                        .as_str()
                        .map_or_else(|| value.to_string(), str::to_owned)
                },
                |values| {
                    values
                        .iter()
                        .take(if key == "allowed_values" {
                            8
                        } else {
                            usize::MAX
                        })
                        .filter_map(serde_json::Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                },
            );
            parts.push(format!("  {key}: {value}"));
            if key == "allowed_values"
                && let Some(values) = recovery[key].as_array().filter(|values| values.len() > 8)
            {
                parts.push(format!("  allowed_values_omitted: {} (complete domain via SDK error.context.recovery.allowed_values; JSON diagnostics may be budgeted)",values.len()-8));
            }
        }
    }
}

/// Renders compact, verified packet recovery without inventing attempted-command fields.
fn append_budget_recovery(parts: &mut Vec<String>, recovery: &serde_json::Value) {
    let bounds = &recovery["token_budget"];
    let feasible = bounds["feasible_tokens"]
        .as_u64()
        .map_or_else(|| "unavailable".to_owned(), |value| value.to_string());
    let minimum = bounds["minimum_feasible_tokens"]
        .as_u64()
        .map_or_else(|| "not established".to_owned(), |value| value.to_string());
    parts.push(format!("\nRecovery bundle:\n  requested_token_budget: {}\n  feasible_token_budget: {feasible}\n  minimum_token_budget: {minimum}", bounds["requested_tokens"]));
    parts.push(format!(
        "  suggested_retry: {}",
        recovery["suggested_retry"].as_str().unwrap_or_default()
    ));
    parts.push("  suggested_flags: --token-budget".to_owned());
    parts.push(format!(
        "  next_best_command: {}",
        recovery["next_best_command"].as_str().unwrap_or_default()
    ));
    parts.push("  fallback_candidates:".to_owned());
    for candidate in recovery["fallback_candidates"]
        .as_array()
        .into_iter()
        .flatten()
    {
        parts.push(format!(
            "    - {}: {} ({})",
            candidate["source"].as_str().unwrap_or_default(),
            candidate["command"].as_str().unwrap_or_default(),
            candidate["reason"].as_str().unwrap_or_default()
        ));
    }
}

/// Counts distinct fields across close and its separately locked release, including intervening claims.
fn close_release_changed_field_count(close_fields: &[String], release_changed: usize) -> usize {
    let mut fields: std::collections::BTreeSet<&str> =
        close_fields.iter().map(String::as_str).collect();
    if release_changed > 0 {
        fields.extend(["assignee", "claim_principal"]);
    }
    fields.len()
}

/// Indexes a serialized comment receipt while retaining errors and older non-object rows.
fn indexed_comment_receipt(
    outcome: Result<serde_json::Value, serde_json::Error>,
) -> Result<serde_json::Value, serde_json::Error> {
    let mut payload = outcome?;
    for (index, comment) in payload["item"]["comments"]
        .as_array_mut()
        .into_iter()
        .flatten()
        .enumerate()
    {
        if let Some(fields) = comment.as_object_mut() {
            fields.shift_remove("index");
            let mut indexed = serde_json::Map::new();
            indexed.insert("index".to_owned(), serde_json::json!(index + 1));
            indexed.extend(std::mem::take(fields));
            *fields = indexed;
        }
    }
    Ok(payload)
}
