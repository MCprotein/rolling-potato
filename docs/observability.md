# Observability And Monitoring

`rolling-potato`는 모델별 token 사용량, latency, memory, retry, guard, tool, evidence 결과를 runtime core가 직접 관측해야 합니다.

Monitoring은 외부 telemetry가 아니라 local-first runtime capability입니다. 기본값은 로컬 저장이며, 사용자 코드나 prompt 원문을 외부로 보내지 않습니다.

Monitoring UX의 기준은 [DESIGN.md](../DESIGN.md)와 [tui.md](tui.md)를 따릅니다. TUI는 SSH/Linux server에서 쓰는 primary monitoring surface이고, HTML은 이후 선택 가능한 local report/dashboard surface입니다.

## 목표

- 모델별 token 사용량과 context 사용량을 기록한다.
- backend/model 성능과 실패율을 session/workflow 단위로 비교한다.
- 작은 모델의 실패 모드: invalid diff, Korean guard rejection, tool failure, stop gate failure를 계량화한다.
- benchmark 결과와 실제 사용 결과를 같은 schema로 비교할 수 있게 한다.
- TUI와 `doctor`가 현재 상태와 최근 장애 원인을 보여줄 수 있게 한다.
- raw prompt나 source code 원문 없이도 기본 진단이 가능하게 한다.

## Storage Decision

기본 방향은 SQLite + append-only ledger입니다.

- SQLite: query/index/reporting store
- append-only ledger: 원본 runtime event와 audit trail
- JSONL export: 사람이 보거나 issue에 첨부하기 쉬운 export format

SQLite를 기본 local store로 두는 이유:

- model/session/workflow/tool/evidence를 조인해서 볼 수 있다.
- `모델별 평균 tokens/sec`, `Korean guard rejection rate`, `context truncation count` 같은 질의가 쉽다.
- TUI가 긴 session을 계속 tail하면서도 집계 화면을 빠르게 보여줄 수 있다.
- benchmark와 실제 사용 metric을 같은 query layer에서 비교할 수 있다.
- 단일 파일이라 low-end local runtime 배포와 backup이 단순하다.

SQLite가 소유하지 않는 것:

- user approval policy
- stop gate 판정
- source of truth인 event append 순서
- raw prompt/source code 장기 보관

## Local File Layout

예상 위치:

```text
rpotato app data root/
  state/
    runtime-ledger.jsonl
    observability.sqlite
    observability.sqlite-wal
  logs/
    backend/
    commands/

project root/
  .rpotato/
    evidence/
    session-ledger.jsonl
```

Project-local ledger는 project boundary와 evidence에 가깝고, app-level SQLite는 cross-project model/runtime monitoring에 가깝습니다.

## 필수 Metric

### Model Run

- model id
- model artifact hash
- backend id and version
- quantization
- context length limit
- prompt tokens
- completion tokens
- total tokens
- context tokens used
- context tokens dropped
- ontology tokens
- tool summary tokens
- max output tokens
- first token latency
- total latency
- prompt eval time
- generation eval time
- tokens per second
- cancellation flag

### Runtime Resource

- backend startup time
- process uptime
- peak RSS memory
- disk space used by models/cache/logs
- backend crash count
- health check latency
- active session count

### Agent Reliability

- workflow id
- active skill id
- subagent/team id
- retry count
- regeneration count
- invalid action count
- invalid diff count
- tool failure count
- command exit code class
- Korean guard pass/fail
- stop gate pass/fail
- missing evidence count

### Privacy And Safety

- approval prompt count
- denied action count
- destructive command blocked count
- credential redaction count
- project boundary violation count
- network download approval count

## Schema Direction

초기 SQLite table 후보:

```text
schema_migrations
sessions
workflows
model_runs
token_usage
backend_runs
tool_calls
command_runs
guard_results
stop_gate_results
evidence_records
benchmark_runs
```

원칙:

- prompt/source 원문은 기본 저장하지 않는다.
- source path는 project-relative path와 hash 중심으로 저장한다.
- command output은 redacted summary와 artifact pointer를 우선 저장한다.
- raw log 보관은 opt-in 또는 짧은 retention으로 제한한다.
- schema migration은 versioned, forward-only로 둔다.

## CLI/TUI Surface

초기 command 후보:

```sh
rpotato monitor status
rpotato monitor models
rpotato monitor session <id>
rpotato monitor export --format jsonl
rpotato monitor export --format csv
rpotato monitor prune --before 30d
```

TUI는 다음 view를 가져야 합니다.

- model/token usage summary
- live session latency and token stream stats
- backend health
- guard/stop gate results
- subagent/team metric summary
- recent failures and validation gaps

HTML은 MVP primary surface가 아닙니다. 이후 추가한다면 SQLite/export data를 읽는 local-only report 또는 dashboard로 둡니다. HTML이 별도 monitoring source of truth를 만들면 안 됩니다.

## Retention

Retention은 privacy와 debugging value를 같이 봅니다.

초기 원칙:

- aggregate metric은 장기 보관 가능
- raw command output과 backend log는 짧은 retention
- credential-like 값은 저장 전 redaction
- export 전 민감 정보 scan
- `rpotato monitor prune`은 dry-run을 지원

## Validation

필수 test:

- SQLite schema migration
- event ledger append 후 SQLite projection
- token usage aggregation
- model별 metric query
- prompt/source 원문 미저장 기본값
- redaction before persistence
- corrupt SQLite fallback
- JSONL export
- retention prune dry-run
- TUI metric view smoke test
