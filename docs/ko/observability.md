# 관측성과 Monitoring

`rolling-potato`는 모델별 token 사용량, latency, CPU, memory, retry, guard, tool, evidence 결과를 runtime core가 직접 관측해야 합니다.

Monitoring은 외부 telemetry가 아니라 local-first runtime capability입니다. 기본값은 로컬 저장이며, 사용자 코드나 prompt 원문을 외부로 보내지 않습니다.

Monitoring UX의 기준은 [DESIGN.md](../DESIGN.md)와 [tui.md](tui.md)를 따릅니다. TUI는 SSH/Linux server에서 쓰는 primary monitoring surface이고, HTML은 이후 선택 가능한 local report/dashboard surface입니다.

## 목표

- 모델별 token 사용량과 context 사용량을 기록한다.
- backend sidecar CPU, memory, disk, resource-pressure sample을 기록한다.
- backend/model 성능과 실패율을 session/workflow 단위로 비교한다.
- 작은 모델의 실패 모드: invalid diff, Korean guard rejection, tool failure, stop gate failure를 계량화한다.
- benchmark 결과와 실제 사용 결과를 같은 schema로 비교할 수 있게 한다.
- TUI와 `doctor`가 현재 상태와 최근 장애 원인을 보여줄 수 있게 한다.
- 로컬 resource pressure가 높을 때 OS failure를 기다리지 않고 runtime이 작업량을 줄일 수 있게 한다.
- raw prompt나 source code 원문 없이도 기본 진단이 가능하게 한다.

## Resource Monitoring Rollout

Resource monitoring은 자율 resource governor가 소비하기 전에 먼저 구현되어야 합니다.
릴리즈 묶음은 다음과 같습니다.

| Version | 범위 | Contract |
| --- | --- | --- |
| v0.9.0 | resource sampler와 logging | backend sidecar CPU, average/peak RSS, disk/cache/log bytes, sample count, pressure status를 sampling하고 redacted ledger event와 SQLite projection row를 기록 |
| v0.10.0 | TUI monitor 표시 | CPU, memory, latency, token throughput, pressure state를 terminal-safe layout으로 표시 |
| v0.11.0 | backend chat resource governor | chat 전 sampling, critical pressure 차단, degraded pressure max token clamp, CLI/runtime ledger의 governor decision 표시 |
| v0.12.0 | team admission preview | 최신 resource sample을 읽어 admitted lane을 표시하고, unknown/degraded pressure에서는 sequential fallback을 선호하며, critical pressure에서는 dispatch를 차단 |
| v0.13.0+ | enforced subagent/team admission | 실제 dispatcher에서 관측한 pressure로 team concurrency 제한을 강제하고 model downgrade/escalation hint 표시 |

## 저장소 결정

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

## 현재 구현

Phase 2의 현재 구현은 runtime store foundation입니다.

- `rpotato init`이 app data root, project-local `.rpotato/`, current-state, runtime ledger, project session ledger, runtime evidence JSONL, SQLite projection을 만든다.
- Append-only ledger는 source of truth이며, SQLite `ledger_events`는 replay 가능한 projection이다.
- SQLite session history는 projection이 재생성될 때 replay된 `ledger_events`에서 현재 project 기준으로 복원할 수 있다.
- SQLite migration v2는 `sessions`, `workflows`, `workflow_transitions`, `checkpoint_records`, `resource_samples`, `model_runs`, `token_usage`, `backend_runs`, `tool_calls`, `command_runs`, `guard_results`, `stop_gate_results`, `evidence_records`, `benchmark_runs`를 만든다.
- `rpotato state`는 current-state와 ledger/projection count를 보여준다.
- `rpotato state reconcile`은 missing/stale/corrupt current-state를 복구하고 보존 이동 이벤트를 ledger에 남긴다.
- `rpotato state resume`은 no active workflow, active pointer detected, blocked 상태를 구분해 ledger에 남긴다.
- `rpotato session list`와 `rpotato session history`는 현재 project의 SQLite projection에서 session history를 읽는다.
- `rpotato session new`는 새 session identity를 만들고 current-state에 기록한 뒤 `session.new` ledger event와 SQLite projection을 남긴다.
- `rpotato session resume <session-id>`와 `rpotato resume <session-id>`는 SQLite history에서 이전 session을 선택하고 그 session id를 current-state에 다시 기록한다.
- id 없이 실행한 `rpotato resume`은 session history를 보여주므로 TUI/CLI surface가 사용자가 재개할 대상을 고르게 만들 수 있다.
- `rpotato cancel`은 active workflow가 없으면 no-op cancel event만 append한다.
- `rpotato evidence validate <artifact-pointer>`는 project-relative artifact pointer가 project boundary 안에 있는지 검증한다.
- `rpotato monitor status`와 `rpotato monitor models`는 SQLite projection을 읽는다.
- `rpotato monitor export --format jsonl|csv`는 runtime ledger/projection을 사람이 볼 수 있는 형태로 출력한다.
- `rpotato monitor prune --before 30d --dry-run`은 삭제 후보 count만 계산한다.
- `rpotato backend start`, `rpotato backend status`, `rpotato backend chat`은 event-driven backend CPU/RSS/disk resource sample을 기록한다.
- `rpotato backend chat`은 첫 runtime resource governor slice를 적용한다. Critical pressure는 model 실행 전에 chat을 차단하고, degraded pressure는 effective max-token budget을 clamp하며, normal/unknown pressure는 요청 token budget을 유지한다.
- `rpotato team status`는 최신 resource sample을 읽고 read-only team admission을 표시한다. Normal pressure는 parallel lane을 허용하고, unknown/degraded pressure는 sequential lane 하나로 fallback하며, critical pressure는 dispatch를 차단한다.
- corrupt SQLite file은 `.corrupt.<timestamp>` suffix로 보존 이동한 뒤 새 projection을 만든다.
- corrupt/stale current-state는 `state reconcile`에서 `.corrupt.<timestamp>` 또는 `.stale.<timestamp>` suffix로 보존 이동한다.
- evidence stale 기준은 artifact 누락, project boundary 이탈, `stale_after_ms` 만료다.

아직 구현하지 않은 부분:

- managed backend sidecar의 continuous background CPU/memory/disk resource sampling
- resource pressure 기반 실제 subagent/team dispatcher admission
- 선택한 session resume 이후 전체 transcript replay와 대화 이어달리기
- 실제 agent loop의 active workflow resume 실행
- 실제 retention 삭제

## 로컬 파일 Layout

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

### 모델 Run

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

### 런타임 Resource

- backend startup time
- process uptime
- process CPU percent
- average RSS memory
- peak RSS memory
- resource sample count
- resource pressure status: normal, degraded, critical
- disk space used by models/cache/logs
- backend crash count
- health check latency
- active session count

### Agent Reliability 지표

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

### Privacy와 Safety

- approval prompt count
- denied action count
- destructive command blocked count
- credential redaction count
- project boundary violation count
- network download approval count

## Schema 방향

초기 SQLite table 후보:

```text
schema_migrations
sessions
workflows
ledger_events
resource_samples
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

## 노출 Surface (`CLI/TUI`)

초기 command 후보:

```sh
rpotato monitor status
rpotato monitor models
rpotato monitor session <id>
rpotato session list
rpotato session history
rpotato session resume <session-id>
rpotato session new
rpotato resume
rpotato resume <session-id>
rpotato monitor export --format jsonl
rpotato monitor export --format csv
rpotato monitor prune --before 30d --dry-run
```

TUI는 다음 view를 가져야 합니다.

- model/token usage summary
- live session latency and token stream stats
- CPU/memory/resource-pressure summary
- backend health
- guard/stop gate results
- subagent/team metric summary
- recent failures and validation gaps

HTML은 MVP primary surface가 아닙니다. 이후 추가한다면 SQLite/export data를 읽는 local-only report 또는 dashboard로 둡니다. HTML이 별도 monitoring source of truth를 만들면 안 됩니다.

## 보존 기간

Retention은 privacy와 debugging value를 같이 봅니다.

초기 원칙:

- aggregate metric은 장기 보관 가능
- raw command output과 backend log는 짧은 retention
- credential-like 값은 저장 전 redaction
- export 전 민감 정보 scan
- `rpotato monitor prune`은 dry-run을 지원

초기 retention matrix:

| Data | Default retention | Delete/prune surface | Notes |
| --- | --- | --- | --- |
| aggregate metric | long term | `monitor prune --dry-run` 후 explicit prune | raw prompt/source 없음 |
| SQLite projection | app data가 있는 동안 long term | 가능한 경우 ledger에서 재생성 | projection이지 event source가 아님 |
| append-only runtime ledger | long term | 명시적 user cleanup만 | audit source |
| project session ledger | project-local | project cleanup만 | `.rpotato/`에 묶임 |
| transcript metadata | project-local | project cleanup만 | raw transcript storage는 opt-in/later |
| evidence artifact | stale 또는 user cleanup 전까지 | `evidence validate`, 이후 evidence prune | project-bound pointer 필요 |
| command output summary | short 또는 redacted | monitor/log prune | raw log보다 summary 우선 |
| backend log | short | monitor/log prune | crash 진단에 유용하지만 privacy-sensitive |
| benchmark report | redacted면 long term | benchmark/report prune | reproducibility manifest 포함 |
| model knowledge entry | redacted면 long term | `model knowledge prune --dry-run` | raw prompt/source 대신 pointer 저장 |
| plugin data | plugin removal 전까지 | `plugin remove --keep-data|--purge-data` | plugin source와 분리 |
| JSONL/CSV export | user-owned artifact | user deletion | export command가 쓰기 전 scan |

Export redaction behavior:

- sensitive value를 안전하게 redact할 수 없으면 export는 fail closed한다.
- 실패한 export는 redacted reason과 함께 ledger event를 남긴다.
- export artifact는 새로운 source of truth로 취급하지 않는다.
- export는 raw prompt/source text를 저장하지 않으면서 local evidence를 다시 조회할 수 있는 id를 보존해야 한다.

## Compaction과 Resume 정책

Compacted summary는 source of truth가 아닙니다.

- current-state는 `compaction_boundary`와 `compacted_summary_path` pointer만 보존한다.
- 원본 판단 근거는 runtime ledger, project session ledger, evidence artifact pointer를 다시 읽어 확인한다.
- compacted summary는 resume bundle의 탐색 힌트로만 사용하고, 파일/명령/모델 claim을 확정하는 근거로 쓰지 않는다.
- compacted summary artifact도 `evidence validate`와 같은 project boundary 검증을 통과해야 한다.
- active workflow resume은 current-state pointer를 감지하고 ledger event를 남긴 뒤, 후속 agent loop phase가 실제 실행을 맡는다.
- session resume은 히스토리 우선으로 동작한다. SQLite가 선택 가능한 session list를 제공하고, append-only ledger는 audit source로 남으며, current-state는 선택된 `session_id`와 resume metadata만 저장한다.
- `rpotato resume <session-id>`는 현재 선택한 session을 이후 명령의 대상 session으로 정한다. model transcript replay는 후속 agent-loop capability다.

## 검증

필수 test:

- SQLite schema migration
- event ledger append 후 SQLite projection
- token usage aggregation
- model별 metric query
- resource sample projection
- resource pressure classification boundary test
- prompt/source 원문 미저장 기본값
- redaction before persistence
- corrupt SQLite fallback
- JSONL export
- retention prune dry-run
- TUI metric view smoke test
