#!/usr/bin/env bash
# Rust and syscall installers share one runner-owned root and bounded host lock.
tool_root="${RUNNER_TOOL_CACHE:?RUNNER_TOOL_CACHE must name persistent tool storage}"
mkdir -p "$tool_root"

lock_runner_tools() {
  if ! flock -w "${RUNNER_TOOL_LOCK_TIMEOUT_SECONDS:-300}" 9; then
    echo 'Timed out waiting for the shared runner tool installation lock (rust.lock).' >&2
    exit 1
  fi
}
