#!/usr/bin/env bash
# All mutations stay in the runner-owned persistent tool volume, under one host lock.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/setup-local-tool-root.sh"
export RUSTUP_HOME="$tool_root/rustup"
export CARGO_HOME="$tool_root/cargo"
export PATH="$CARGO_HOME/bin:$PATH"
export CARGO_TARGET_DIR="$GITHUB_WORKSPACE/target"
printf '%s\n' "RUSTUP_HOME=$RUSTUP_HOME" "CARGO_HOME=$CARGO_HOME" "CARGO_TARGET_DIR=$CARGO_TARGET_DIR" >> "$GITHUB_ENV"
printf '%s\n' "$CARGO_HOME/bin" >> "$GITHUB_PATH"
(
  lock_runner_tools
  if [[ ! -x "$CARGO_HOME/bin/rustup" ]]; then
    bootstrap=$(mktemp)
    curl --fail --show-error --location --retry 5 https://sh.rustup.rs --output "$bootstrap"
    sh "$bootstrap" -y --profile minimal --no-modify-path --default-toolchain none
    rm "$bootstrap"
  fi
  if ! rustup component list --toolchain 1.90.0 --installed 2>/dev/null | grep -q rustfmt ||
     ! rustup component list --toolchain 1.90.0 --installed 2>/dev/null | grep -q clippy; then
    rustup toolchain install 1.90.0 --profile minimal --component clippy,rustfmt
  fi
  if [[ "${COVERAGE:-false}" == true ]]; then
    if ! rustup component list --toolchain nightly-2026-08-06 --installed 2>/dev/null | grep -q llvm-tools; then
      rustup toolchain install nightly-2026-08-06 --profile minimal --component llvm-tools-preview
    fi
    if ! cargo llvm-cov --version | grep -q '0.8.7'; then
      cargo +1.90.0 install --locked cargo-llvm-cov --version 0.8.7
    fi
  fi
  if [[ "${QUALITY:-false}" == true ]]; then
    if ! cargo audit --version | grep -q '0.23.0'; then
      cargo +1.90.0 install --locked cargo-audit --version 0.23.0
    fi
  fi
  if ! just --version 2>/dev/null | grep -qx 'just 1.43.0'; then
    cargo +1.90.0 install --locked just --version 1.43.0
  fi
) 9>"$tool_root/rust.lock"
rustc -V
cargo -V
