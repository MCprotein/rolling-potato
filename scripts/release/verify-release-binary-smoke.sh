#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'release binary smoke error: %s\n' "$1" >&2
  exit 1
}

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  fail "usage: scripts/release/verify-release-binary-smoke.sh <binary-path> [expected-version]"
fi

binary_path="$1"
expected_version="${2:-}"

if [ ! -f "$binary_path" ]; then
  fail "binary was not found: $binary_path"
fi

case "$binary_path" in
  *.exe) ;;
  *)
    if [ ! -x "$binary_path" ]; then
      fail "binary is not executable: $binary_path"
    fi
    ;;
esac

output="$("$binary_path" doctor)"

case "$output" in
  *"rpotato 진단"*) ;;
  *) fail "doctor output did not include the diagnostic heading" ;;
esac

case "$output" in
  *"package: rpotato"*) ;;
  *) fail "doctor output did not include package name" ;;
esac

if [ -n "$expected_version" ]; then
  case "$output" in
    *"package version: $expected_version"*) ;;
    *) fail "doctor output did not include expected version: $expected_version" ;;
  esac
fi

case "$output" in
  *"release target os:"*) ;;
  *) fail "doctor output did not include release target OS" ;;
esac

case "$output" in
  *"release target arch:"*) ;;
  *) fail "doctor output did not include release target architecture" ;;
esac

case "$output" in
  *"release smoke: available"*) ;;
  *) fail "doctor output did not report release smoke availability" ;;
esac

smoke_root="$(mktemp -d)"
trap 'rm -rf "$smoke_root"' EXIT
smoke_project="$smoke_root/project"
smoke_data="$smoke_root/data"
mkdir -p "$smoke_project"
RPOTATO_PROJECT_ROOT="$smoke_project" \
  RPOTATO_DATA_HOME="$smoke_data" \
  "$binary_path" init >/dev/null
tui_output="$(printf 'quit\n' | \
  RPOTATO_PROJECT_ROOT="$smoke_project" \
  RPOTATO_DATA_HOME="$smoke_data" \
  RPOTATO_TEST_TERMINAL_FAULT="invalid-release-smoke-value" \
  "$binary_path" tui interactive)"
case "$tui_output" in
  *"rpotato interactive | overview"*) ;;
  *) fail "release build did not ignore the debug-only terminal fault seam" ;;
esac

printf 'release binary smoke ok: %s\n' "$binary_path"
