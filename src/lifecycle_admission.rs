//! Shared locked admission for atomic and composed lifecycle transitions.
use crate::{ItemDocument, OwnershipItem, PmRustError, list_registry::Lifecycle};
use serde_json::{Value, json};
#[cfg(test)]
use std::fs;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

/// The published commit boundary determines which checks precede this write.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Phase {
    Claim,
    AtomicStart,
    Start,
    Pause,
}
/// One admitted status and its diagnostics, computed before touching durable metadata.
pub(crate) struct Admission {
    /// Canonical target, including the registered progress fallback.
    pub status: String,
    /// Published workflow and dependency warnings in admission order.
    pub warnings: Vec<String>,
    /// Whether update semantics must remove prior terminal completion evidence.
    pub from_terminal: bool,
}
thread_local! {
    /// Explicit compositions preflight blockers under the claim lock and deduplicate warnings.
    static EXPLICIT: RefCell<Option<BTreeSet<String>>> = const { RefCell::new(None) };
}
/// Restores nested composition state even if a caller unwinds.
struct Scope(Option<BTreeSet<String>>);
impl Drop for Scope {
    fn drop(&mut self) {
        EXPLICIT.with(|state| *state.borrow_mut() = self.0.take());
    }
}
/// Runs both steps under a request-local explicit-start context, without holding two item locks.
pub(crate) fn with_explicit_start<T>(operation: impl FnOnce() -> T) -> T {
    let _scope = Scope(EXPLICIT.with(|state| state.replace(Some(BTreeSet::new()))));
    operation()
}
/// Whether the current claim must preflight a later explicit start before ownership publication.
pub(crate) fn explicit_claim() -> bool {
    EXPLICIT.with(|state| state.borrow().is_some())
}

/// Resolves status, workflow and declared-blocker admission under the caller's item lock.
/// Claim preflight intentionally defers workflow validation to the published second commit.
pub(crate) fn admit_lifecycle_transition(
    root: &Path,
    request: &OwnershipItem,
    before: &ItemDocument,
    phase: Phase,
) -> Result<Admission, PmRustError> {
    let registry = Lifecycle::load(root)?;
    admit_with_registry(root, request, before, phase, &registry)
}

/// Reuses the locked claim's captured registry for atomic status and blocker admission.
pub(crate) fn admit_with_registry(
    root: &Path,
    request: &OwnershipItem,
    before: &ItemDocument,
    phase: Phase,
    registry: &Lifecycle,
) -> Result<Admission, PmRustError> {
    let settings = crate::ownership::runtime_settings(root)?;
    let progress = registry.start_status();
    let status = if phase == Phase::Pause {
        registry.open_status()
    } else {
        progress.clone()
    };
    let terminal = registry.terminal(&before.metadata.status);
    if phase == Phase::AtomicStart && terminal {
        return Err(PmRustError::OwnershipRefusal {
            detail: format!(
                "Cannot start terminal item {}; reopen it with pm item reopen before starting it",
                request.id
            ),
            context: json!({"code":"terminal_state_conflict","item_id":request.id,"why":"Starting work moves an active item into progress; a terminal item must be reopened so its recurrence evidence is recorded.","next_steps":[format!("Run {}, then {}.",crate::render_pm_command(["item","reopen",&request.id,"<reason>"]),crate::render_pm_command(["claim",&request.id,"--start"]))]}),
        });
    }
    let mut warnings = if phase == Phase::Claim {
        vec![]
    } else {
        crate::mutation::validate_lifecycle_transition(&settings, registry, before, &status)?
    };
    if registry.canonical(&before.metadata.status) != registry.canonical(&status)
        && status == progress
    {
        let mut blockers = admit_blockers(root, &settings, registry, before, request.force)?;
        EXPLICIT.with(|state| {
            if let Some(recorded) = state.borrow_mut().as_mut() {
                if phase == Phase::Claim {
                    recorded.extend(blockers.iter().cloned());
                } else {
                    blockers.retain(|warning| !recorded.contains(warning));
                }
            }
        });
        warnings.extend(blockers);
    }
    Ok(Admission {
        status,
        warnings,
        from_terminal: terminal,
    })
}
/// Classifies explicit relationships with the same aliases accepted by the published resolver.
fn blocking_kind(kind: &str) -> bool {
    matches!(
        kind.trim().to_lowercase().replace('-', "_").as_str(),
        "blocked_by" | "depends_on"
    )
}
/// Keeps external provenance separate from local identifiers during de-duplication.
fn external(id: &str, source: &str) -> bool {
    id.contains("://")
        || ["github:", "gitlab:", "jira:", "linear:"]
            .iter()
            .any(|prefix| id.to_lowercase().starts_with(prefix))
        || matches!(source.trim().to_lowercase().as_str(), "global" | "external")
}
/// Retains declared spellings until an actual local item supplies its canonical identity.
fn declared(before: &ItemDocument) -> Vec<(String, bool)> {
    let mut ids = BTreeMap::new();
    let scalar = before
        .metadata
        .extra
        .get("blocked_by")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let deps = before
        .metadata
        .extra
        .get("dependencies")
        .and_then(Value::as_array);
    for (id, source) in std::iter::once((scalar, "")).chain(
        deps.into_iter()
            .flatten()
            .filter(|dep| blocking_kind(dep["kind"].as_str().unwrap_or_default()))
            .map(|dep| {
                (
                    dep["id"].as_str().unwrap_or_default().trim(),
                    dep["source_kind"].as_str().unwrap_or_default(),
                )
            }),
    ) {
        if id.is_empty() || id.eq_ignore_ascii_case("no-active-blocker") {
            continue;
        }
        let external = external(id, source);
        ids.insert(
            (
                external,
                if external {
                    id.to_owned()
                } else {
                    id.to_lowercase()
                },
            ),
            (
                if external {
                    id.to_owned()
                } else {
                    id.to_lowercase()
                },
                external,
            ),
        );
    }
    let mut ids: Vec<_> = ids.into_values().collect();
    ids.sort_by(|a, b| crate::collation::compare(&a.0, &b.0));
    ids
}
/// Searches filenames only; unrelated item payloads are never decoded during admission.
fn blocker_paths(
    root: &Path,
    top: bool,
    id: &str,
    found: &mut Vec<PathBuf>,
) -> Result<(), PmRustError> {
    // Keep test scheduling outside this shared filesystem control flow.
    before_path_read(root);
    for entry in crate::workspace::read_directory(root)? {
        let path = entry.path();
        #[cfg(test)]
        tests::before_entry_type(&path);
        if path.is_symlink() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if top && crate::workspace::NON_ITEM_DIRECTORIES.contains(&name.as_ref()) {
            continue;
        }
        if path.is_dir() {
            blocker_paths(&path, false, id, found)?;
        } else if path.is_file() && name.eq_ignore_ascii_case(&format!("{id}.toon")) {
            found.push(path);
        }
    }
    Ok(())
}
/// Loads only a declared target, retaining missing references as unresolved blockers.
fn blocker(root: &Path, id: &str, settings: &Value) -> Result<Option<ItemDocument>, PmRustError> {
    if id.contains(['/', '\\', ':', '\0']) || matches!(id, "." | "..") {
        return Ok(None);
    }
    let id = id.trim_start_matches('#');
    let prefix = settings["id_prefix"].as_str().unwrap_or("pm-");
    let mut canonical = id.to_owned();
    let mut paths = Vec::new();
    blocker_paths(root, true, &canonical, &mut paths)?;
    if paths.is_empty() && !id.starts_with(prefix) {
        canonical = format!("{prefix}{id}");
        blocker_paths(root, true, &canonical, &mut paths)?;
    }
    let Some(path) = paths.first() else {
        return Ok(None);
    };
    if paths.len() > 1 {
        return Err(PmRustError::DuplicateItemId {
            id: canonical,
            first: paths[0].clone(),
            second: paths[1].clone(),
        });
    }
    read_blocker(path, &canonical).map(Some)
}

/// Admits the opened handle, so replacement FIFOs cannot block an item lock.
fn read_blocker_bytes(path: &Path) -> std::io::Result<Vec<u8>> {
    read_blocker_handle(crate::changes::open_history(path)?)
}

/// Checks the opened object's type before reading, independently of platform open flags.
fn read_blocker_handle(mut file: std::fs::File) -> std::io::Result<Vec<u8>> {
    use std::io::Read as _;
    let metadata = file.metadata();
    #[cfg(test)]
    let metadata = crate::changes::inject_io(metadata, 1);
    if !metadata?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Blocker is not a regular file",
        ));
    }
    let mut bytes = Vec::new();
    let read = file.read_to_end(&mut bytes);
    #[cfg(test)]
    let read = crate::changes::inject_io(read, 4);
    read?;
    Ok(bytes)
}

/// Reads a resolved blocker at the final I/O boundary, retaining typed encoding diagnostics.
fn read_blocker(path: &Path, canonical: &str) -> Result<ItemDocument, PmRustError> {
    let bytes = read_blocker_bytes(path).map_err(|source| PmRustError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let raw = std::str::from_utf8(&bytes).map_err(|_| PmRustError::CommandRefusal {
        detail: "Item contains invalid UTF-8; preserve a binary backup before recovering it.".to_owned(),
        exit_code: 4,
        context: json!({"code":"item_document_encoding_invalid","item_id":canonical,"item_path":path,"byte_length":bytes.len(),"empty":false}),
    })?;
    let item = crate::item::decode_item(path, raw)?;
    crate::item::validate_file_identity(path, &item)?;
    Ok(item)
}
/// Builds executable unblock advice only when the scalar/edge removal is unambiguous.
fn unblock(before: &ItemDocument, first: &str, settings: &Value) -> String {
    let prefix = settings["id_prefix"].as_str().unwrap_or("pm-");
    let canonical = |id: &str| {
        let id = id.trim().trim_start_matches('#').to_lowercase();
        if id.starts_with(prefix) {
            id
        } else {
            format!("{prefix}{id}")
        }
    };
    let deps = before
        .metadata
        .extra
        .get("dependencies")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let scalar = before
        .metadata
        .extra
        .get("blocked_by")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let inspect = || {
        crate::render_pm_command([
            "get",
            &before.metadata.id,
            "--fields",
            "blocked_by,dependencies",
        ])
    };
    if deps.iter().any(|dep| {
        blocking_kind(dep["kind"].as_str().unwrap_or_default())
            && dep["kind"] != "blocked_by"
            && canonical(dep["id"].as_str().unwrap_or_default()) == canonical(first)
    }) {
        return inspect();
    }
    if canonical(scalar) == canonical(first)
        && deps
            .iter()
            .filter(|dep| dep["kind"] == "blocked_by")
            .all(|dep| {
                dep["id"]
                    .as_str()
                    .unwrap_or_default()
                    .trim()
                    .eq_ignore_ascii_case(scalar.trim())
            })
    {
        return crate::render_pm_command(["update", &before.metadata.id, "--unset", "blocked_by"]);
    }
    let matches: Vec<_> = deps
        .iter()
        .filter(|dep| {
            dep["kind"] == "blocked_by"
                && canonical(dep["id"].as_str().unwrap_or_default()) == canonical(first)
        })
        .collect();
    if matches.len() == 1
        && matches[0]["id"]
            .as_str()
            .is_some_and(|id| external(id, "") || id == canonical(id))
    {
        return crate::render_pm_command([
            "update",
            &before.metadata.id,
            "--dep-remove",
            &format!(
                "id={},kind=blocked_by",
                matches[0]["id"].as_str().unwrap_or_default()
            ),
        ]);
    }
    inspect()
}
/// Implements strict/default, warning, off and explicit-force dependency policies once.
fn admit_blockers(
    root: &Path,
    settings: &Value,
    registry: &Lifecycle,
    before: &ItemDocument,
    force: bool,
) -> Result<Vec<String>, PmRustError> {
    let enforcement = settings["governance"]["start_blocker_enforcement"]
        .as_str()
        .unwrap_or("strict");
    if enforcement == "off" {
        return Ok(vec![]);
    }
    let mut open = Vec::new();
    let mut seen = BTreeSet::new();
    for (id, external) in declared(before) {
        let item = if external {
            None
        } else {
            match blocker(root, &id, settings) {
                Ok(item) => item,
                Err(error)
                    if (enforcement == "warn" || force) && unverifiable_code(&error).is_some() =>
                {
                    return Ok(vec![format!(
                        "start_blocker_unverifiable:{}:{}",
                        before.metadata.id,
                        unverifiable_code(&error).unwrap_or_default()
                    )]);
                }
                Err(error) => return Err(error),
            }
        };
        if item
            .as_ref()
            .is_some_and(|item| registry.terminal(&item.metadata.status))
        {
            continue;
        }
        let key = (
            external,
            item.as_ref()
                .map_or_else(|| id.clone(), |item| item.metadata.id.to_lowercase()),
        );
        if !seen.insert(key) {
            continue;
        }
        open.push(if let Some(item)=item {json!({"id":item.metadata.id,"title":item.metadata.title,"status":item.metadata.status,"external":false})} else {json!({"id":id,"title":null,"status":null,"external":external})});
    }
    if open.is_empty() {
        return Ok(vec![]);
    }
    let ids = open
        .iter()
        .filter_map(|row| row["id"].as_str())
        .collect::<Vec<_>>()
        .join(",");
    let id = &before.metadata.id;
    if enforcement == "warn" {
        return Ok(vec![format!("start_with_open_blockers:{id}:{ids}")]);
    }
    if force {
        return Ok(vec![format!("start_blocker_override:{id}:{ids}")]);
    }
    let mut steps: Vec<_> = open
        .iter()
        .filter(|row| row["external"] != true)
        .take(3)
        .map(|row| {
            json!(crate::render_pm_command([
                "get",
                row["id"].as_str().unwrap_or_default()
            ]))
        })
        .collect();
    steps.push(json!(unblock(
        before,
        open[0]["id"].as_str().unwrap_or_default(),
        settings
    )));
    steps.push(json!(crate::render_pm_command(["next"])));
    let plural = if open.len() == 1 {
        "its blocker".to_owned()
    } else {
        format!("{} of its blockers", open.len())
    };
    Err(PmRustError::OwnershipRefusal {
        detail: format!(
            "Cannot start {id}: {plural} {ids} {} still open.",
            if open.len() == 1 { "is" } else { "are" }
        ),
        context: json!({"code":"start_blocked_by_open_dependency","item_id":id,"required":format!("Finish or close the open blocker{} first, remove the blocked_by edge if it no longer applies, or start anyway with --force.",if open.len()==1{""}else{"s"}),"why":"Starting work that a declared blocker still gates would leave an in-progress item that validate reports as non_blocked_status_has_blocked_by. governance.start_blocker_enforcement (strict by default) governs this check.","next_steps":steps,"details":{"enforcement":enforcement,"open_blockers":open}}),
    })
}

/// Keeps warning/force policy limited to the published unverifiable-document failures.
fn unverifiable_code(error: &PmRustError) -> Option<&str> {
    match error {
        PmRustError::InvalidItemDocument { reason, .. } => {
            Some(if reason == "stored id does not match the item filename" {
                "item_identity_conflict"
            } else {
                "item_document_invalid"
            })
        }
        PmRustError::DuplicateItemId { .. } => Some("item_identity_ambiguous"),
        PmRustError::CommandRefusal { context, .. } => context["code"].as_str().filter(|code| {
            matches!(
                *code,
                "item_document_encoding_invalid"
                    | "item_format_version_unsupported"
                    | "item_identity_conflict"
                    | "item_identity_ambiguous"
                    | "item_document_invalid"
            )
        }),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../tests/support/lifecycle_admission_unit.rs"]
pub(crate) mod tests;

/// Runs the filesystem scheduler in tests without changing traversal control flow.
#[cfg(test)]
pub(crate) fn before_path_read(root: &Path) {
    tests::before_path_read(root);
}

/// The production traversal uses the same call boundary without a scheduler.
#[cfg(not(test))]
fn before_path_read(_root: &Path) {}
