# Reproducible evidence

Evidence certifies observed checks and outcomes. An expected-check inventory is
an obligation, never a source of success counts. The crash-oracle recorder reads
Node's terminal `test:pass` and `test:fail` events through a JSON-lines reporter.
Skipped, todo and missing expected checks are `not_run`; any missing or failing
check makes certification exit nonzero. Both negative and restored observations
are retained. Mutation and phase summaries are derived from those observations.

Run `node scripts/crash-oracle-proof.mts --checks-only` to inspect results without
replacing the receipt. For example, adding `--test-name-pattern='phase parity'`
executes one of 33 expected checks, reports 32 `not_run`, and exits 1. The full
recorder runs all 24 real-state mutations, all eight live restart/return mutations,
and the phase-pairing regression. This does not certify cross-runtime conformance:
the separately measured public-copy failures remain failures.

## Sources must survive squash merges

Every receipt commit must resolve in a fresh full clone of origin and be reachable
from `main` or a preserved `evidence/<name>` branch. A PR branch can disappear after
a squash merge. Local objects, reflogs and a successful local `git show` do not
prove remote reproducibility. Never retain evidence with a tag: release automation
uses the newest tag as its release base.

Before recording: commit the source, push an `evidence/<name>` branch at that
commit if it is not on main, then run the recorder with matching source bytes.
Keep that branch for as long as a receipt cites it. Both crash proof recorders
refuse uncommitted source changes and check remote retention before controls run.
The oracle receipt hashes the recorder, reporter, parser, tests, harness, native
source and dependency locks against that preserved source commit.

`node scripts/crash-evidence-commits.mts` runs inside `just crash-check` and hence
`just release-check` for npm and Bun. It enumerates tracked JSON documents beneath
`docs/`, recursively collects commit/head fields, historical `reverted_to` and
`rust_behavior_proved_at` fields, and repository commit URLs, then performs a fresh
full `git clone --no-local --no-checkout --no-tags` of origin. It resolves every
reference as a commit and checks main/evidence ancestry. A failure names the
receipt, field and commit. Git tree digests and SHA-256 content hashes are not
commit references. New receipt formats must expose source commits through these
fields and extend the scanner when introducing another reference convention.

Historical receipts remain unchanged. Their original source commits are retained
on `evidence/crash-review-2026-10-23`, `evidence/native-context-2026-10-19`,
`evidence/native-hot-paths-2026-10-22`, `evidence/native-orientation-2026-10-18`,
and `evidence/native-read-2026-10-18`. The corrected oracle recorder's source is
retained on `evidence/crash-oracle-recorder-2026-10-25`.

The evidence clone gate and proof recorders require Git and a reachable origin.
The corpus and normal tracker operations still work in plain directories without
Git. No raw tracker data, host coordinates or temporary paths belong in receipts.
