#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$repo_root"

require_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    printf 'missing web/browser documentation file: %s\n' "$path" >&2
    exit 1
  fi
}

require_marker() {
  local path="$1"
  local marker="$2"
  if ! grep -Fq -- "$marker" "$path"; then
    printf 'missing web/browser documentation marker in %s: %s\n' "$path" "$marker" >&2
    exit 1
  fi
}

required_files=(
  README.md
  README.ko.md
  PRIVACY.md
  docs/ko/PRIVACY.md
  SECURITY.md
  docs/ko/SECURITY.md
  docs/threat-model.md
  docs/ko/threat-model.md
  docs/tui.md
  docs/ko/tui.md
  docs/current-capabilities.md
  docs/ko/current-capabilities.md
  docs/runtime-architecture.md
  docs/ko/runtime-architecture.md
  docs/v0.50-web-research-browser-plan.md
  docs/ko/v0.50-web-research-browser-plan.md
)

for path in "${required_files[@]}"; do
  require_file "$path"
done

package_version="$(
  sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1
)"
if [[ -z "$package_version" ]]; then
  printf 'missing package version in Cargo.toml\n' >&2
  exit 1
fi

require_marker README.md "Current release | \`v${package_version}\`"
require_marker README.ko.md "현재 릴리즈 | \`v${package_version}\`"
require_marker README.md 'restricted browser search-form'
require_marker README.ko.md '제한된 브라우저 search-form'
require_marker PRIVACY.md 'loopback HTTPS CONNECT'
require_marker docs/ko/PRIVACY.md 'loopback HTTPS CONNECT'
require_marker SECURITY.md 'address-pinned loopback CONNECT proxy'
require_marker docs/ko/SECURITY.md 'address-pinned loopback CONNECT proxy'
require_marker docs/threat-model.md '### Restricted Browser Abuse'
require_marker docs/ko/threat-model.md '### 제한된 브라우저 오용'
require_marker docs/tui.md '### Restricted Browser Search Form (v0.50.0)'
require_marker docs/ko/tui.md '### 제한된 브라우저 Search Form (v0.50.0)'
require_marker docs/current-capabilities.md 'v0.50.0 release'
require_marker docs/ko/current-capabilities.md 'v0.50.0 릴리즈'
require_marker docs/runtime-architecture.md '## Web Research and Restricted Browser'
require_marker docs/ko/runtime-architecture.md '## 웹 연구와 제한된 브라우저'
require_marker docs/README.md '(v0.50-web-research-browser-plan.md)'
require_marker docs/ko/README.md '(v0.50-web-research-browser-plan.md)'

printf 'web/browser documentation contract ok: release capability privacy security threat tui architecture indexes\n'
