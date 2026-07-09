#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'uninstall smoke error: %s\n' "$1" >&2
  exit 1
}

if [ "$#" -ne 1 ]; then
  fail "usage: scripts/release/verify-uninstall-smoke.sh <binary-path>"
fi

binary_path="$1"

if [ ! -f "$binary_path" ]; then
  fail "binary was not found: $binary_path"
fi

run_uninstall_smoke() {
  local mode="$1"
  local output

  output="$("$binary_path" uninstall --dry-run "$mode")"

  case "$output" in
    *"uninstall 계획 ($mode)"*) ;;
    *) fail "output did not include uninstall mode heading: $mode" ;;
  esac

  case "$output" in
    *"dry-run 명시됨"*) ;;
    *) fail "output did not report dry-run execution state: $mode" ;;
  esac

  case "$output" in
    *"program/runtime assets:"*) ;;
    *) fail "output did not include program/runtime assets path: $mode" ;;
  esac

  case "$output" in
    *"project state는 global uninstall에서 삭제하지 않음:"*) ;;
    *) fail "output did not include project-state preservation boundary: $mode" ;;
  esac

  if [ "$mode" = "--keep-cache" ]; then
    case "$output" in
      *"보존:"*) ;;
      *) fail "keep-cache output did not include preserved cache paths" ;;
    esac
  else
    case "$output" in
      *"models:"*"downloads:"*"cache:"*) ;;
      *) fail "purge-cache output did not include cache deletion plan paths" ;;
    esac
  fi
}

run_uninstall_smoke "--keep-cache"
run_uninstall_smoke "--purge-cache"

printf 'uninstall smoke ok: %s\n' "$binary_path"
