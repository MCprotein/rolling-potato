#!/bin/sh
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
cd "$root"

fail() {
  printf 'toolchain pin verification failed: %s\n' "$1" >&2
  exit 1
}

expect_line() {
  file="$1"
  value="$2"
  grep -F "$value" "$file" >/dev/null || fail "$file is missing: $value"
}

expect_count() {
  file="$1"
  value="$2"
  expected="$3"
  actual="$(awk -v value="$value" 'index($0, value) { count++ } END { print count + 0 }' "$file")"
  [ "$actual" -eq "$expected" ] \
    || fail "$file expected $expected occurrence(s), found $actual: $value"
}

reject_unapproved_action_refs() {
  file="$1"
  awk \
    -v checkout="$checkout_pin" \
    -v upload="$upload_pin" \
    -v download="$download_pin" '
      /uses: actions\/checkout@/ && index($0, checkout) == 0 { bad = 1 }
      /uses: actions\/upload-artifact@/ && index($0, upload) == 0 { bad = 1 }
      /uses: actions\/download-artifact@/ && index($0, download) == 0 { bad = 1 }
      END { exit bad }
    ' "$file" || fail "$file contains an unapproved GitHub Action ref"
}

rust_version="1.97.0"
checkout_pin="actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0 # v7.0.0"
upload_pin="actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1"
download_pin="actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c # v8.0.1"
release_workflow=".github/workflows/release-binaries.yml"
policy_workflow=".github/workflows/release-policy.yml"

expect_line Cargo.toml "rust-version = \"$rust_version\""
expect_line mise.toml "rust = \"$rust_version\""
expect_line rust-toolchain.toml "channel = \"$rust_version\""

reject_unapproved_action_refs "$release_workflow"
reject_unapproved_action_refs "$policy_workflow"
expect_count "$release_workflow" "$checkout_pin" 3
expect_count "$release_workflow" "$upload_pin" 2
expect_count "$release_workflow" "$download_pin" 1
expect_count "$policy_workflow" "$checkout_pin" 1

printf 'toolchain pins ok: rust=%s actions=node24 runners=ga\n' "$rust_version"
