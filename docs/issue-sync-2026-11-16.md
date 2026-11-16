# Scheduled GitHub issue sync diagnosis, 2026-11-16

[Failed workflow run](https://github.com/unbraind/pm-rust/actions/runs/43828004648).

Observed failure:

```text
Atomic GitHub import transaction failed. Underlying error: Missing required options --acceptance-criteria/--ac, --assignee, --comment, --deadline, --dep, --doc, --estimate/--estimated-minutes, --file, --learning, --note, --priority, --test for type "Issue".
```

The tracker intentionally uses strict governance. pm-github 2026.11.15 buildAtomicImportMutations constructs create options containing title, type, description, body, tags, author, status and message (plus source assignee/milestone when present). It does not provide the strict collection assertions or planning fields. The SDK applies the same admission policy as manual creation.

Reproduced in a new synthetic strict tracker with the locked pm-cli 2026.11.11 oracle. Passing the importer create shape exits 2 with the same missing-options list. The import command exposes no create-mode, template or default-field configuration; no documented importer exemption was found. A caller cannot supply these options through the reusable workflow. The correct fix belongs in pm-github: define an explicit import admission policy or provide meaningful required values and empty-collection assertions. Do not switch tracker governance to progressive or minimal and do not fabricate deadlines, estimates or dependencies for imported issues.

Status: upstream blocker; no local governance or importer patch applied.

## Import gate evidence

A dry-run does not exercise strict create admission and cannot prove this repair. The synthetic strict create reproduction is the write-admission proof.

For non-blocked importers the read-only command is:

```sh
pm github import unbraind/pm-rust --state all --atomic --gate --dry-run --with-comments --json
```

A passing plan proves completeness and identity planning, not successful writes or a new scheduled CI run. Shared reference tags remain references; new mirror items in a plan are not applied by this investigation.

## Validation

Cargo check, formatting and clippy pass. The package test sequence is still running at the initial draft. The pinned tracker validation returns `ok: true` with existing advisories, and seeded validation passes through `pm test --run --progress`. Remaining results are recorded in the linked sync repair item. CI is not dispatched manually, and no successful scheduled rerun is claimed.
