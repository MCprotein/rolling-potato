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

clean_output="$("$binary_path" uninstall --clean --dry-run)"
case "$clean_output" in
  *"rpotato uninstall (clean dry-run)"*) ;;
  *) fail "clean dry-run output did not include heading" ;;
esac
case "$clean_output" in
  *"installed binary:"*"PATH registration:"*"remove app data:"*"remove project state:"*) ;;
  *) fail "clean dry-run output did not include every managed target" ;;
esac
case "$clean_output" in
  *"rpotato uninstall --clean --yes"*) ;;
  *) fail "clean dry-run output did not include explicit confirmation command" ;;
esac

smoke_root="$(mktemp -d)"
trap 'rm -rf "$smoke_root"' EXIT
smoke_home="$smoke_root/home"
smoke_project="$smoke_root/project"
smoke_data="$smoke_root/data"
smoke_local_app_data="$smoke_root/local-app-data"
smoke_source_dir="$smoke_root/source"
mkdir -p \
  "$smoke_home" \
  "$smoke_project" \
  "$smoke_local_app_data" \
  "$smoke_source_dir"

native_path() {
  if [[ "$binary_path" == *.exe ]] && command -v cygpath >/dev/null 2>&1; then
    cygpath -w "$1"
  else
    printf '%s\n' "$1"
  fi
}

if [[ "$binary_path" == *.exe ]]; then
  smoke_source="$smoke_source_dir/rpotato.exe"
  installed_binary="$smoke_local_app_data/Programs/rpotato/bin/rpotato.exe"
else
  smoke_source="$smoke_source_dir/rpotato"
  installed_binary="$smoke_home/.local/bin/rpotato"
fi
cp "$binary_path" "$smoke_source"
chmod +x "$smoke_source" 2>/dev/null || true

smoke_home_native="$(native_path "$smoke_home")"
smoke_project_native="$(native_path "$smoke_project")"
smoke_data_native="$(native_path "$smoke_data")"
smoke_local_app_data_native="$(native_path "$smoke_local_app_data")"

run_isolated() {
  HOME="$smoke_home_native" \
  USERPROFILE="$smoke_home_native" \
  LOCALAPPDATA="$smoke_local_app_data_native" \
  RPOTATO_PROJECT_ROOT="$smoke_project_native" \
  RPOTATO_DATA_HOME="$smoke_data_native" \
  SHELL="/bin/sh" \
  "$@"
}

run_isolated "$smoke_source" install >/dev/null
[ -f "$installed_binary" ] \
  || fail "isolated install did not create the managed binary"

mkdir -p "$smoke_data/state" "$smoke_project/.rpotato"
printf 'managed\n' >"$smoke_data/state/uninstall-smoke.txt"
printf 'managed\n' >"$smoke_project/.rpotato/uninstall-smoke.txt"

isolated_plan="$(run_isolated "$installed_binary" uninstall --clean --dry-run)"
case "$isolated_plan" in
  *"installed binary:"*"PATH registration:"*"remove app data:"*"remove project state:"*) ;;
  *) fail "isolated clean uninstall plan omitted a managed target" ;;
esac

confirmed_output="$(run_isolated "$installed_binary" uninstall --clean --yes)"
case "$confirmed_output" in
  *"rpotato uninstall (clean)"*) ;;
  *) fail "isolated clean uninstall did not report execution" ;;
esac

for _attempt in $(seq 1 100); do
  [ ! -e "$installed_binary" ] && break
  sleep 0.1
done
[ ! -e "$installed_binary" ] \
  || fail "clean uninstall did not remove the installed binary after process exit"
[ ! -e "$smoke_data" ] \
  || fail "clean uninstall did not remove global application data"
[ ! -e "$smoke_project/.rpotato" ] \
  || fail "clean uninstall did not remove current-project state"
[ -f "$smoke_source" ] \
  || fail "clean uninstall removed the user-owned invocation source"
if grep -R -l -F "# >>> rpotato managed PATH >>>" "$smoke_home" >/dev/null 2>&1; then
  fail "clean uninstall left an owned Unix PATH block"
fi

printf 'uninstall smoke ok: %s\n' "$binary_path"
