# CLI Output Style

`rpotato` CLI output defaults to Korean.

## Principles

- The first line briefly names the command result topic.
- Subsequent lines use `- label: value`.
- Destructive actions that have not run must be shown as dry-run or plan output.
- File paths, commands, model IDs, and plugin IDs preserve their original text.
- Failure reasons are written in Korean, while error codes and original log lines may be preserved.
- If there is a next step, show only one at the end.

## Example

```text
monitor 상태
- observability store: /path/to/observability.sqlite
- runtime ledger: /path/to/runtime-ledger.jsonl
- raw prompt/source 저장: 기본 비활성
- 현재 상태: SQLite projection은 Phase 2에서 생성 예정
```

## Forbidden

- placeholder output that looks successful
- claiming deletion happened when only a plan was shown
- unverified model/license/benchmark claims
- presenting remote plugin marketplaces or registries as available
