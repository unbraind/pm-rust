#!/usr/bin/env bash
# No sudo or system package changes: extract the distro's signed-index packages into tool storage.
set -euo pipefail
prefix="${RUNNER_TOOL_CACHE:?}/syscall-tools"
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
) 9>"$RUNNER_TOOL_CACHE/rust.lock"
"$prefix/bin/strace" -V
# Match the crash harness flags, including its required child-lifetime protection.
# The unreachable injection ordinal checks parsing without killing the probe.
if ! "$prefix/bin/strace" -f --kill-on-exit -qq -yy -s 4096 \
  -o "$RUNNER_TEMP/ptrace-probe" \
  -e trace=rename,fsync,link,linkat,write,writev,unlink,unlinkat \
  -e inject=rename:signal=SIGKILL:when=65535 -- true; then
  echo 'strace setup requires --kill-on-exit, syscall injection and permission to ptrace a child; install a compatible strace and allow child tracing.' >&2
  exit 1
fi
printf '%s\n' "$prefix/bin" >> "$GITHUB_PATH"
