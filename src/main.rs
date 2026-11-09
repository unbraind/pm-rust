//! `pm-rust` command-line interface.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

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
            intent,
            sections,
            activity_limit,
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
                    intent,
                    sections,
                    activity_limit,
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
            return write_context_to(writer, &mut std::io::stderr().lock(), &result, json);
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
                if result.get("output_budget_exceeded").is_some() {
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
            if start && args.id.is_none() {
                return Err(Box::new(pm_rust::PmRustError::CommandRefusal {
                    detail: "Specify an item id or pass --next".to_owned(),
                    exit_code: 2,
                    context: serde_json::json!({}),
                }));
            }
            if start && (next || if_available) {
                return Err(Box::new(pm_rust::PmRustError::CommandRefusal {
                    detail: "--start cannot be combined with --next or --if-available for an explicit item".to_owned(), exit_code: 2,
                    context: serde_json::json!({"code":"invalid_argument_value","required":"Remove --next and --if-available when starting an explicit item.","next_steps":["Remove --next and --if-available, keep the explicit item ID and other supplied arguments, then retry."]}),
                }));
            }
            if next == args.id.is_some() {
                return Err(Box::new(pm_rust::PmRustError::CommandRefusal {
                    detail: if next {
                        "Specify either an item id or --next, not both"
                    } else {
                        "Specify an item id or pass --next"
                    }
                    .to_owned(),
                    exit_code: 2,
                    context: serde_json::json!({}),
                }));
            }
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
                let mut changed: std::collections::BTreeSet<String> =
                    result.close_changed_fields.into_iter().collect();
                if release.changed_field_count > 0 {
                    changed.extend(["assignee", "claim_principal"].map(str::to_owned));
                }
                receipt = serde_json::json!({"id":result.item.metadata.id,"action":"close_task","close":receipt,"release":release,"status":result.item.metadata.status,"changed_field_count":changed.len()});
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

/// Appends one comment without an argv-derived role.
fn comment_payload(
    workspace: &Workspace,
    identity: &AuthorIdentity,
    request: &CommentItem,
) -> Result<MutationResult, Box<dyn std::error::Error>> {
    identity
        .with(|| workspace.comment(request))
        .map_err(Into::into)
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
    let mut index = 1;
    while let Some(arg) = args.get(index) {
        let arg = arg.to_string_lossy();
        if matches!(arg.as_ref(), "--workspace" | "--pm-path" | "--path") {
            index += 2;
        } else if ["--workspace=", "--pm-path=", "--path="]
            .iter()
            .any(|flag| arg.starts_with(flag))
        {
            index += 1;
        } else {
            if arg == "item" {
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
            break;
        }
    }
    args
}

/// Resolves explicit wire encoding before the legacy JSON switch for error output.
fn json_requested(args: &[String]) -> bool {
    let explicit = args
        .windows(2)
        .find(|pair| pair[0] == "--output-format")
        .map(|pair| pair[1].as_str())
        .or_else(|| {
            args.iter()
                .find_map(|arg| arg.strip_prefix("--output-format="))
        });
    explicit.map_or_else(
        || args.iter().any(|arg| arg == "--json"),
        |format| format == "json",
    )
}

/// Resolves the tracker start path from raw arguments, mirroring the native parser.
///
/// The parser-refusal path cannot read the parsed workspace option, so the
/// same `--workspace`/`--pm-path`/`--path` spellings are scanned from the raw
/// command line, defaulting to the current directory like the parser does.
fn native_workspace_path<I, S>(args: I) -> PathBuf
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut values = args.into_iter();
    while let Some(raw) = values.next() {
        let arg = raw.as_ref().to_string_lossy();
        // Workspace is a top-level option. Rejected command option values must
        // never redirect the diagnostic reader into another tracker.
        if !arg.starts_with('-') || arg == "--" {
            break;
        }
        for flag in ["--workspace", "--pm-path", "--path"] {
            if let Some(value) = arg.strip_prefix(&format!("{flag}=")) {
                return PathBuf::from(value);
            }
            if arg == *flag
                && let Some(value) = values.next()
            {
                return PathBuf::from(value.as_ref());
            }
        }
    }
    PathBuf::from(".")
}

/// Parses the command line and maps success or failure to the process exit code.
#[allow(clippy::too_many_lines)]
fn main() -> ExitCode {
    let cli = match Cli::try_parse_from(native_arguments(std::env::args_os().collect())) {
        Ok(cli) => cli,
        Err(error) => {
            let mut args = published_read_arguments(std::env::args_os().skip(1));
            let orientation = args
                .iter()
                .position(|arg| !arg.starts_with('-'))
                .filter(|&index| matches!(args[index].as_str(), "context" | "history" | "deps"));
            if error.kind() == clap::error::ErrorKind::UnknownArgument
                && let Some(index) = orientation
            {
                let command = args.remove(index);
                args.insert(0, command);
                let allowed = match args[0].as_str() {
                    "context" => vec![
                        "--for",
                        "--section",
                        "--activity-limit",
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
                let mut payload = orientation_error_json(
                    &detail,
                    &serde_json::json!({"code":"unsupported_native_option","required":"Use the published pm CLI for options outside the documented native slice.","title":"Unsupported native option","flag":flag,"value":flag}),
                    &args,
                    2,
                );
                // The published CLI reports stored settings diagnostics before
                // refusing; only context commands load context settings.
                let settings_warning = (args[0] == "context")
                    .then(|| native_workspace_path(std::env::args_os().skip(1)))
                    .and_then(|path| Workspace::discover(&path).ok())
                    .and_then(|workspace| workspace.context_settings_warning());
                let mut stderr = std::io::stderr().lock();
                if json_requested(&args) {
                    attach_settings_warning(&mut payload, settings_warning);
                    let _ = write_json_to(&mut stderr, &payload);
                } else {
                    if let Some(warning) = settings_warning {
                        let _ = write_settings_warning_to(&mut stderr, warning, false);
                    }
                    let _ = write_cursor_refusal_to(&mut stderr, &payload);
                }
                return ExitCode::from(2);
            }
            // Keep Clap's bytes and statuses while allowing normal process teardown.
            // Immediate exit bypasses profile finalization on Windows.
            let _ = error.print();
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
            let settings_warning = error
                .downcast_ref::<pm_rust::PmRustError>()
                .and_then(pm_rust::PmRustError::settings_warning);
            let args = published_read_arguments(std::env::args_os().skip(1));
            if !json_requested(&args)
                && let Some(warning) = settings_warning
            {
                let _ = write_settings_warning_to(&mut std::io::stderr().lock(), warning, false);
            }
            let error: &(dyn std::error::Error + 'static) =
                if let Some(pm_rust::PmRustError::ContextSettingsFallback { source }) =
                    error.downcast_ref::<pm_rust::PmRustError>()
                {
                    source.as_ref()
                } else {
                    error.as_ref()
                };
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
                let mut payload = command_error_json(detail, context, &args, *exit_code);
                attach_settings_warning(&mut payload, settings_warning);
                let mut stderr = std::io::stderr().lock();
                let _ = write_command_refusal_to(&mut stderr, detail, context, &args, &payload);
                return ExitCode::from(*exit_code);
            }
            if let Some(pm_rust::PmRustError::OwnershipRefusal { detail, context }) =
                error.downcast_ref::<pm_rust::PmRustError>()
            {
                let args = published_read_arguments(std::env::args_os().skip(1));
                let mut payload = ownership_error_json(detail, context, &args);
                attach_settings_warning(&mut payload, settings_warning);
                if context["code"] == "lock_conflict" {
                    preserve_lock_scope(
                        &mut payload,
                        &std::env::args_os()
                            .skip(1)
                            .map(|arg| arg.to_string_lossy().into_owned())
                            .collect::<Vec<_>>(),
                    );
                }
                let mut stderr = std::io::stderr().lock();
                let _ = if args.first().is_some_and(|s| s == "item") && !json_requested(&args) {
                    write_cursor_refusal_to(&mut stderr, &payload)
                } else {
                    write_json_to(&mut stderr, &payload)
                };
                return ExitCode::from(4);
            }
            if let Some(pm_rust::PmRustError::LockConflict { .. }) =
                error.downcast_ref::<pm_rust::PmRustError>()
            {
                let args = published_read_arguments(std::env::args_os().skip(1));
                let mut payload = ownership_error_json(
                    &error.to_string(),
                    &serde_json::json!({"code":"lock_conflict"}),
                    &args,
                );
                attach_settings_warning(&mut payload, settings_warning);
                preserve_lock_scope(
                    &mut payload,
                    &std::env::args_os()
                        .skip(1)
                        .map(|arg| arg.to_string_lossy().into_owned())
                        .collect::<Vec<_>>(),
                );
                let _ = write_json_to(&mut std::io::stderr().lock(), &payload);
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
                let mut payload = cursor_error_json(code, detail, &args);
                attach_settings_warning(&mut payload, settings_warning);
                let mut stderr = std::io::stderr().lock();
                let _ = if json_requested(&args) {
                    write_json_to(&mut stderr, &payload)
                } else {
                    write_cursor_refusal_to(&mut stderr, &payload)
                };
            } else {
                let _ = write_runtime_error_to(
                    &mut std::io::stderr().lock(),
                    error,
                    settings_warning,
                    json_requested(&args),
                );
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
            args.iter()
                .map(String::as_str)
                .filter(|arg| arg.starts_with('-'))
                .find(|arg| !allowed.contains(&arg.split('=').next().unwrap_or_default()))
        })
        .map_or("option", |arg| arg.split('=').next().unwrap_or_default())
}

/// Writes a command refusal using its requested encoding and published human fallback.
fn write_command_refusal_to(
    writer: &mut dyn Write,
    detail: &str,
    context: &serde_json::Value,
    args: &[String],
    payload: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    if context["code"] == "unknown_error" && !json_requested(args) {
        writeln!(writer, "{detail}")?;
        return Ok(());
    }
    if args.first().is_some_and(|command| {
        matches!(
            command.as_str(),
            "item" | "search" | "context" | "history" | "deps"
        )
    }) && !json_requested(args)
    {
        write_cursor_refusal_to(writer, payload)
    } else {
        write_json_to(writer, payload)
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
    if args.first().is_some_and(|arg| arg == "context") {
        for arg in &mut args {
            if arg == "--token_budget" || arg.starts_with("--token_budget=") {
                *arg = arg.replacen("--token_budget", "--token-budget", 1);
            }
        }
    }
    if args.first().is_some_and(|arg| arg == "deps") {
        args.insert(0, "item".to_owned());
    }
    args
}

/// Matches orientation command failures to the published problem envelope.
#[allow(clippy::too_many_lines)]
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
        for (key, value) in [
            (
                "required",
                serde_json::json!("Use values that match documented command constraints."),
            ),
            ("title", serde_json::json!("Invalid argument value")),
            (
                "next_steps",
                serde_json::json!([
                    "Check allowed values in command help, then rerun with corrected input."
                ]),
            ),
            (
                "examples",
                serde_json::json!([
                    format!("pm {command} --help"),
                    "pm contracts --command <command> --flags-only --json"
                ]),
            ),
        ] {
            if context.get(key).is_none() {
                context[key] = value;
            }
        }
    }
    let mut result = ownership_error_json(detail, &context, &args);
    if command != "history" {
        let mut normalized = Vec::new();
        for arg in args.iter().filter(|arg| arg.starts_with("--")) {
            let flag = arg.split_once('=').map_or(arg.as_str(), |(flag, _)| flag);
            if !normalized.contains(&flag) {
                normalized.push(flag);
            }
        }
        result["recovery"]["provided_fields"] = serde_json::json!(normalized);
    }
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
        let expanded: Vec<_> = args
            .iter()
            .flat_map(|arg| {
                if command != "history"
                    && arg.starts_with("--")
                    && let Some((flag, value)) = arg.split_once('=')
                {
                    vec![flag.to_owned(), value.to_owned()]
                } else {
                    vec![arg.clone()]
                }
            })
            .collect();
        result["refusal"] = command_refusal_detail(detail, &expanded, exit_code);
    }
    if let Some(domain) = context["recovery"].get("allowed_values") {
        result["recovery"]["allowed_values"] = domain.clone();
        result["refusal"] = serde_json::json!({"surface":"--section","rejected_value":result["refusal"]["rejected_value"],"legal_domain":domain,"exit_code":exit_code});
        if let Some(index) = args
            .iter()
            .position(|arg| arg == "--section")
            .filter(|&index| {
                args.get(index + 1)
                    .is_some_and(|value| !value.starts_with("--"))
            })
        {
            let mut retry = args.clone();
            domain[0]
                .as_str()
                .unwrap_or_default()
                .clone_into(&mut retry[index + 1]);
            result["examples"] = serde_json::json!([
                format!(
                    "pm {}",
                    retry
                        .iter()
                        .map(|arg| pm_rust::quote_command_arg(arg))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                "pm context --help"
            ]);
        } else {
            result["examples"] = serde_json::json!([
                "pm context --help",
                "pm contracts --command <command> --flags-only --json"
            ]);
        }
    }
    if let Some(flag) = context.get("flag") {
        result["refusal"] = serde_json::json!({"surface":flag,"rejected_value":context["value"],"exit_code":exit_code});
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
    if !json_requested(&args)
        && let Some(why) = context.get("why")
    {
        result["why"] = why.clone();
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
    let mut arguments = Vec::new();
    let mut args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string_lossy().into_owned());
    while let Some(arg) = args.next() {
        if matches!(
            arg.as_str(),
            "--timestamp" | "--workspace" | "--pm-path" | "--path"
        ) {
            args.next();
        } else if !["--timestamp=", "--workspace=", "--pm-path=", "--path="]
            .iter()
            .any(|prefix| arg.starts_with(prefix))
        {
            arguments.push(arg);
        }
    }
    if arguments.starts_with(&["item".to_owned(), "deps".to_owned()]) {
        arguments.remove(0);
    }
    if arguments.first().is_some_and(|command| command == "close") {
        for arg in &mut arguments {
            let alias = match arg.as_str() {
                "--expected" => Some("--expected-result"),
                "--actual" => Some("--actual-result"),
                _ => None,
            };
            if let Some(alias) = alias {
                alias.clone_into(arg);
            }
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

/// Maps an explicit native workspace into the published retry command's tracker scope.
fn preserve_lock_scope(payload: &mut serde_json::Value, raw: &[String]) {
    let workspace = raw.iter().enumerate().find_map(|(index, arg)| {
        if matches!(arg.as_str(), "--workspace" | "--pm-path" | "--path") {
            raw.get(index + 1).cloned()
        } else {
            ["--workspace=", "--pm-path=", "--path="]
                .iter()
                .find_map(|prefix| arg.strip_prefix(prefix).map(str::to_owned))
        }
    });
    if let Some(workspace) = workspace {
        let mut args = vec!["--pm-path".to_owned(), workspace];
        args.extend(
            payload["recovery"]["normalized_args"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned),
        );
        let command = format!(
            "pm {}",
            args.iter()
                .map(|arg| pm_rust::quote_command_arg(arg))
                .collect::<Vec<_>>()
                .join(" ")
        );
        payload["recovery"]["attempted_command"] = serde_json::json!(command);
        payload["recovery"]["normalized_args"] = serde_json::json!(args);
        payload["recovery"]["suggested_retry"] = serde_json::json!(command);
        payload["recovery"]["suggested_retry_args"] = serde_json::json!(args);
        payload["examples"] = serde_json::json!([command]);
    }
}

/// Formats the published machine-readable refusal for a rejected list cursor.
fn cursor_error_json(code: &str, detail: &str, args: &[String]) -> serde_json::Value {
    let mut provided = Vec::new();
    for arg in args {
        if arg.starts_with("--") && !provided.contains(arg) {
            provided.push(arg.clone());
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
    apply_close_error_recovery(&mut payload, context, args);
    for key in ["required", "examples", "next_steps", "title"] {
        if let Some(value) = context.get(key) {
            payload[key] = value.clone();
        }
    }
    let include_why = context["code"]
        .as_str()
        .is_some_and(|code| code != "invalid_argument_value");
    if include_why && let Some(why) = context.get("why") {
        payload["why"] = why.clone();
    }
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
    for key in ["flag", "value"] {
        if let Some(value) = context.get(key) {
            payload[key] = value.clone();
        }
    }
    payload
}

/// Adds actionable close recovery fields and the published compact title limit.
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
}

/// Formats command-specific rejected flag values in usage refusals.
fn command_refusal_detail(detail: &str, args: &[String], exit_code: u8) -> serde_json::Value {
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
            .and_then(|index| args.get(index + 1))
            // The published extractor omits these shared author/output controls.
            .filter(|arg| !matches!(arg.as_str(), "--author" | "--json"));
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
    let args: Vec<&str> = normalized
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
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
        let rejected = args
            .iter()
            .position(|arg| *arg == flag)
            .and_then(|index| args.get(index + 1))
            .filter(|arg| !matches!(**arg, "--json" | "--author"));
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

/// Selects the current context/dependency diagnostic contract without changing other commands.
fn current_orientation_refusal(payload: &serde_json::Value) -> bool {
    let command = &payload["recovery"]["normalized_args"];
    payload["recovery"]["token_budget"].is_object()
        || command[0] == "context"
        || (command[0] == "item" && command[1] == "deps")
}

/// Current orientation diagnostics omit a detail that duplicates the title.
fn compact_orientation_refusal(payload: &serde_json::Value) -> bool {
    payload["detail"] == payload["title"] && current_orientation_refusal(payload)
}

/// Preserves line breaks while replacing control characters in current diagnostic text.
fn orientation_diagnostic_text(payload: &serde_json::Value, text: String) -> String {
    if !current_orientation_refusal(payload) {
        return text;
    }
    text.replace("\r\n", "\n")
        .chars()
        .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
        .collect()
}

/// Writes the published human-readable cursor diagnostic when JSON was not requested.
#[allow(clippy::too_many_lines)]
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
    let mut parts = vec![format!(
        "Error: {}\n\nWhat is required:\n  {}",
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
    if !compact_orientation_refusal(payload) {
        parts.push(format!(
            "\nWhat happened:\n  {}",
            payload["detail"].as_str().unwrap_or_default()
        ));
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
    if let Some(examples) = payload["examples"].as_array() {
        parts.push("\nExamples:".to_owned());
        parts.extend(
            examples
                .iter()
                .map(|example| format!("  - {}", example.as_str().unwrap_or_default())),
        );
    }
    if payload["recovery"]["token_budget"].is_object() {
        append_budget_recovery(&mut parts, &payload["recovery"]);
    } else {
        parts.push("\nRecovery bundle:".to_owned());
        if let Some(retry) = payload["recovery"]["suggested_retry"].as_str() {
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
    for key in [
        "allowed_values",
        "candidate_commands",
        "candidate_commands_total",
        "option_scope",
    ] {
        if let Some(value) = payload["recovery"].get(key) {
            let value = value.as_array().map_or_else(
                || {
                    value
                        .as_str()
                        .map_or_else(|| value.to_string(), str::to_owned)
                },
                |values| {
                    values
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                },
            );
            parts.push(format!("  {key}: {value}"));
        }
    }
    let output = orientation_diagnostic_text(payload, format!("{}\n", parts.join("\n")));
    writer.write_all(output.as_bytes())?;
    writer.flush().map_err(Into::into)
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

/// Emit the same warning frame before either a successful context packet or its refusal.
fn write_settings_warning_to(
    writer: &mut dyn Write,
    warning: &str,
    json: bool,
) -> std::io::Result<()> {
    if json {
        let payload = serde_json::json!({"type":"warning","warnings":["settings_read_invalid_schema"],"warning_details":[{"code":"settings_read_invalid_schema","message":warning}]});
        writeln!(writer, "{}", pm_rust::stringify_json(&payload, false))
    } else {
        writeln!(writer, "{warning}")
    }
}

/// Preserve warning details inside the published JSON refusal envelope.
fn attach_settings_warning(payload: &mut serde_json::Value, warning: Option<&str>) {
    if let Some(warning) = warning {
        payload["warnings"] = serde_json::json!(["settings_read_invalid_schema"]);
        payload["warning_details"] =
            serde_json::json!([{"code":"settings_read_invalid_schema","message":warning}]);
    }
}

/// Preserve independent settings diagnostics even for unclassified item and I/O errors.
fn write_runtime_error_to(
    writer: &mut dyn Write,
    error: &dyn std::error::Error,
    warning: Option<&str>,
    json: bool,
) -> std::io::Result<()> {
    if json && warning.is_some() {
        let mut payload = serde_json::json!({"error":format!("pm-rust: {error}"),"exit_code":2});
        attach_settings_warning(&mut payload, warning);
        writeln!(writer, "{}", pm_rust::stringify_json(&payload, true))
    } else {
        writeln!(writer, "pm-rust: {error}")
    }
}

/// Render context through caller-owned streams, including diagnostic write failures.
fn write_context_to(
    writer: &mut dyn Write,
    diagnostics: &mut dyn Write,
    result: &pm_rust::ListOutput,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(warning) = result.settings_warning() {
        write_settings_warning_to(diagnostics, warning, json)?;
    }
    write_search_to(writer, result, json)
}
