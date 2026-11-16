#!/usr/bin/env bash
# No sudo or system package changes: extract the distro's signed-index packages into tool storage.
set -euo pipefail
prefix=/home/runner/_work/_tool/syscall-tools
(
  flock 9
  if [[ ! -x "$prefix/usr/bin/strace" ]]; then
    mkdir -p "$prefix/apt/lists/partial" "$prefix/apt/cache/archives/partial" "$prefix/downloads"
    apt_options=(-o "Dir::State::lists=$prefix/apt/lists" -o "Dir::Cache=$prefix/apt/cache" -o Debug::NoLocking=true)
    apt-get "${apt_options[@]}" update
    ( cd "$prefix/downloads"
      apt-get "${apt_options[@]}" download strace libunwind8
      for package in ./*.deb; do dpkg-deb --extract "$package" "$prefix"; done
    )
  fi
  if [[ ! -x "$prefix/bin/strace" ]]; then
    mkdir -p "$prefix/bin"
    library_path="$prefix/usr/lib/$(gcc -dumpmachine)"
    printf '#!/bin/sh\nLD_LIBRARY_PATH=%s exec %s "$@"\n' "$library_path" "$prefix/usr/bin/strace" > "$prefix/bin/strace"
    chmod +x "$prefix/bin/strace"
  fi
) 9>/home/runner/_work/_tool/rust.lock
printf '%s\n' "$prefix/bin" >> "$GITHUB_PATH"
"$prefix/bin/strace" -V
# Fail visibly if the container's existing policy denies tracing its own child.
"$prefix/bin/strace" -o "$RUNNER_TEMP/ptrace-probe" true
