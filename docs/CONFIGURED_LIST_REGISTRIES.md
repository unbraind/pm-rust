# Configured list registries

Native list selection resolves lifecycle definitions from `settings.json` under
`schema.statuses` and `schema.workflow`. Optional status and workflow files
(`schema.files`, defaulting to `schema/statuses.json` and
`schema/workflows.json`) overlay the embedded settings. When schema files are missing, the oracle creates built-in scaffold files,
whose values override inline settings. Native reads model those defaults in
memory without creating files. Persist custom workflow definitions in the
configured schema files to override the scaffold. The default status registry
applies when no statuses are supplied. Registry reads and their I/O errors follow
workspace query-root spelling: physical roots on Unix and the resolved caller
spelling on Windows, preserving aliases without adding a canonical verbatim
prefix. Tests compare the exact configured file path under that platform root.

`--status open` selects active statuses except the configured in-progress status;
`--status in_progress` selects that configured status. Ordinary list reads exclude
all statuses carrying `terminal`, `terminal_done` or `terminal_canceled` roles.
All-status reads sort those terminal rows last. Dependency blocking resolves
prerequisites with the same terminal registry, and recognizes custom blocked
statuses. When no active role is declared, dependency blocking uses the
configured open status, matching oracle actionability. Selection happens before
projections, fingerprints and page slicing.

The published oracle registers custom hierarchy kinds through JavaScript
extensions. Native production code does not execute those extensions. To supply
the same registry declaratively, put its relationship definitions in
`schema.relationship_kinds` in tracker settings:

```json
{
  "schema": {
    "relationship_kinds": [
      {
        "kind": "belongs_to",
        "aliases": ["member_of"],
        "hierarchy": true,
        "hierarchyDirection": "target_parent"
      }
    ]
  }
}
```

Use `source_parent` for edges whose holder is the parent. Other hierarchy edges
point to their parent. The built-in kinds and scalar `parent` remain available.
Only the hierarchy projection of these definitions is consumed by native list;
this is not a general extension loader or an oracle settings feature. Keep the
snapshot consistent with the extension definitions used by the published CLI.

`tests/pagination_differential.rs` compares three disposable lifecycle
configurations and both custom hierarchy directions against the locked oracle.
The hierarchy trackers activate a real oracle schema extension with the same
kind definitions, including an alias. JSON and TOON producer pages, complete
walks, snapshot continuations and their full envelopes are compared byte for
byte, including page fingerprints and exit status/stderr.

Every oracle process is launched through `tests/oracle/launch.mts`, which stamps
`PM_SENTRY_DISABLED=1` and `PM_TELEMETRY_SOURCE_CONTEXT=test` in the final child.
Rust integration helpers, CI health/merge setup and `just oracle` share it.
Git in the branch-merge test also runs through the launcher, so merge-driver
descendants inherit the same test environment.
Just recipes and workflow jobs also export the boundary for published changelog
tooling. `tests/oracle_launcher.rs` observes a real shim child's environment,
including hostile inherited values, a cleared parent environment, arguments and
exit propagation. No service endpoint changes are involved.
