<!-- pm-cli:agent-guidance:start:v1 -->
## pm Workflow (Agent Quickstart)

- Orient before mutate: `pm context --limit 10`, `pm search "<keywords>" --limit 10`, `pm list-open --limit 20`.
- Claim and execute: `pm claim <id>` then `pm update <id> --status in_progress`.
- Link evidence while coding: `pm files <id> --add ...`, `pm docs <id> --add ...`, `pm test <id> --add command="node scripts/run-tests.mjs test -- ..."`.
- Verify and close: `pm test <id> --run --progress`, `pm close <id> "<evidence>" --validate-close warn`, `pm release <id>`.
- Author identity is automatic for supported agent harnesses; use `--author` only for an explicit override.

<!-- pm-cli:agent-guidance:end -->
