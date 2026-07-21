# CLI Output Style

`rpotato` CLI output defaults to Korean.

## Principles

- The first line briefly names the command result topic.
- Subsequent lines use `- label: value`.
- Destructive actions that have not run must be shown as dry-run or plan output.
- File paths, commands, model IDs, and plugin IDs preserve their original text.
- Failure reasons are written in Korean, while error codes and original log lines may be preserved.
- If there is a next step, show only one at the end.
- Attached terminals may style the topic in cyan, section headings in blue,
  healthy/ready/completed states in green, waiting/degraded states in yellow,
  and failed/blocked/stale states in red.
- Status text remains present, so color is never the only signal. Redirected output,
  `NO_COLOR`, and `TERM=dumb` remain byte-stable plain text.
- `rpotato --help` shows the normal product surface. Granular diagnostic and
  compatibility commands live under `rpotato debug --help`.

## Example

```text
monitor 상태
- observability store: /path/to/observability.sqlite
- runtime ledger: /path/to/runtime-ledger.jsonl
- transcript records: user/visible-model/tool/evidence local persistence
- excluded content: complete backend prompt, hidden response, raw source body
```

## Forbidden

- placeholder output that looks successful
- claiming deletion happened when only a plan was shown
- unverified model/license/benchmark claims
- presenting remote plugin marketplaces or registries as available
