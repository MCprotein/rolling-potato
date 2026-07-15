# 관측성과 Monitoring

`rolling-potato`는 모델별 token 사용량, latency, CPU, memory, retry, guard, tool, evidence 결과를 runtime core가 직접 관측해야 합니다.

Monitoring은 외부 telemetry가 아니라 local-first runtime capability입니다. 기본값은 로컬 저장이며, 사용자 코드나 prompt 원문을 외부로 보내지 않습니다.

Monitoring UX의 기준은 [DESIGN.md](DESIGN.md)와 [tui.md](tui.md)를 따릅니다. TUI는 SSH/Linux server에서 쓰는 primary monitoring surface이고, HTML은 이후 선택 가능한 local report/dashboard surface입니다.

## 목표

- 모델별 token 사용량과 context 사용량을 기록한다.
- backend sidecar CPU, memory, disk, resource-pressure sample을 기록한다.
- backend/model 성능과 실패율을 session/workflow 단위로 비교한다.
- 작은 모델의 실패 모드: invalid diff, Korean guard rejection, tool failure, stop gate failure를 계량화한다.
- benchmark 결과와 실제 사용 결과를 같은 schema로 비교할 수 있게 한다.
- TUI와 `doctor`가 현재 상태와 최근 장애 원인을 보여줄 수 있게 한다.
- 로컬 resource pressure가 높을 때 OS failure를 기다리지 않고 runtime이 작업량을 줄일 수 있게 한다.
- 전체 backend prompt, hidden/raw model response, raw source body 없이도 기본 진단이 가능하게 한다.

## Resource Monitoring Rollout

Resource monitoring은 자율 resource governor가 소비하기 전에 먼저 구현되어야 합니다.
릴리즈 묶음은 다음과 같습니다.

| Version | 범위 | Contract |
| --- | --- | --- |
| v0.9.0 | resource sampler와 logging | backend sidecar CPU, average/peak RSS, disk/cache/log bytes, sample count, pressure status를 sampling하고 redacted ledger event와 SQLite projection row를 기록 |
| v0.10.0 | TUI monitor 표시 | CPU, memory, latency, token throughput, pressure state를 terminal-safe layout으로 표시 |
| v0.11.0 | backend chat resource governor | chat 전 sampling, critical pressure 차단, degraded pressure max token clamp, CLI/runtime ledger의 governor decision 표시 |
| v0.12.0 | team admission preview | 최신 resource sample을 읽어 admitted lane을 표시하고, unknown/degraded pressure에서는 sequential fallback을 선호하며, critical pressure에서는 dispatch를 차단 |
| v0.13.0 | team admission gate | dispatch 전에 requested lane admission을 강제하고, decision을 ledger에 기록하며, unknown/degraded pressure에서는 sequential lane 하나로 fallback하고 critical pressure는 차단 |
| v0.14.0 | team admission policy preflight | dispatch 전에 요청 write path와 command policy check를 실행한다. allow-only check는 통과하고 ask/deny check는 worker launch를 차단 |
| v0.15.0 | team file ownership preflight | lane별 write path를 dispatch 전에 정규화하고 ownership status를 ledger output에 기록하며 cross-lane write conflict를 차단 |
| v0.16.0 | team admission approval queue integration | policy/ownership block에 대한 project-local approval request record를 쓰고 `tui approvals`에 표시 |
| v0.17.0 | context/model governor preflight | 요청 context를 설정 budget과 resource pressure에 맞춰 clamp하고, model route hint를 내며, decision을 ledger에 기록 |
| v0.18.0 | performance baseline report | raw prompt/source text를 저장하지 않고 local p50/p95 latency, tokens/sec, context clamp count, peak RSS, pressure state, backend/model/session metric을 집계 |
| v0.19.0 | benchmark harness foundation | benchmark run을 ledger/projection에 기록하고 fixture metadata를 검증하며 reproducibility metadata와 redacted local report를 출력 |
| v0.20.0 | executable benchmark runner | 같은 runtime monitoring schema로 active backend sidecar 실행 결과, local score, token/latency/resource metric을 연결 |
| v0.21.0 | benchmark-driven optimization policy | `monitor optimize`가 측정된 local metric과 benchmark evidence로 context budget, lane count, fallback, model route를 추천 |
| v0.22.0 | dispatcher hardening | dispatch-time file ownership을 강제하고 failed-worker continuation과 최신 team runtime status를 기록/표시 |

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
- 전체 backend prompt, hidden response, raw source body 장기 보관

## 현재 구현

Phase 2의 현재 구현은 runtime store foundation입니다.

- `rpotato init`이 app data root, project-local `.rpotato/`, current-state, runtime ledger, project session ledger, runtime evidence JSONL, SQLite projection을 만든다.
- Append-only ledger는 source of truth이며, SQLite `ledger_events`는 replay 가능한 projection이다.
- SQLite session history는 projection이 재생성될 때 replay된 `ledger_events`에서 현재 project 기준으로 복원할 수 있다.
- SQLite migration v6는 재생성 가능한 `transcript_records`를 추가하고 append-only `workflow.checkpoint` detail에서 `workflows`를 계속 재구성한다. SQLite는 workflow/transcript 권위가 아닌 projection이다.
- `rpotato state`는 current-state와 ledger/projection count를 보여준다.
- `rpotato state reconcile`은 missing/stale/corrupt current-state를 복구하고 보존 이동 이벤트를 ledger에 남긴다.
- `rpotato state resume`은 모든 ledger line을 strict하게 parse하고 canonical snapshot/checkpoint 전체 hash chain과 latest committed revision을 검증한 뒤 안전한 phase를 idempotent하게 재개한다. Pending approval은 backend 호출 없이 diff와 token placeholder를 표시하며 최초 발급 token은 다시 표시할 수 없다.
- Patch verification은 source, command output, approval token plaintext 대신 hash와 status를 담은 project evidence JSON/runtime evidence JSONL을 기록한다. Stop gate는 성공 전 artifact와 authoritative source를 다시 읽는다.
- `rpotato session list`와 `rpotato session history`는 현재 project의 canonical runtime ledger에서 재생성한 SQLite session view를 읽는다. Replay는 SQLite에만 있는 session row를 제거한다.
- `rpotato session new`는 새 session identity를 만들고 current-state에 기록한 뒤 `session.new` ledger event와 SQLite projection을 남긴다.
- `rpotato session resume <session-id>`, `rpotato resume <session-id>`, `rpotato continue <session-id>`는 canonical ledger 소유권을 요구하고 current-state mutation 전에 immutable transcript artifact와 source hash를 검증한 뒤 같은 session의 안전한 workflow checkpoint만 계속한다. 인자 없는 `continue`는 현재 선택을 재개한다.
- id 없이 실행한 `rpotato resume`은 session history를 보여주므로 TUI/CLI surface가 사용자가 재개할 대상을 고르게 만들 수 있다.
- `rpotato cancel`은 active workflow가 없으면 no-op cancel event만 append한다.
- `rpotato evidence validate <artifact-pointer>`는 project-relative artifact pointer가 project boundary 안에 있는지 검증한다.
- `rpotato monitor status`와 `rpotato monitor models`는 SQLite projection을 읽는다.
- `rpotato monitor baseline`은 local ledger/SQLite projection metric을 읽어 p50/p95 latency, average tokens/sec, context clamp count, peak RSS, pressure-state distribution, model/backend/session grouping을 보여주는 read-only performance baseline report를 출력한다. Raw prompt/source text는 저장하지 않으며 model artifact를 선택하지 않는다.
- `rpotato monitor optimize`는 local performance baseline, 최신 resource sample, `measured-locally` benchmark row를 읽어 context budget, team lane count, fallback mode, model route hint를 추천한다. Read-only 기능이며 실제 model artifact를 선택하거나 model status를 승격하거나 public benchmark parity를 주장하지 않는다.
- `rpotato monitor export --format jsonl|csv`는 runtime ledger/projection을 사람이 볼 수 있는 형태로 출력한다.
- `rpotato monitor prune --before 30d --dry-run`은 삭제 후보 count만 계산한다.
- `rpotato benchmark validate <fixture.json>`는 project-local fixture metadata를 검증한다. Runtime capability, model/runtime responsibility, expected route, policy decision, escalation target, required tool/source/evidence record, abstention requirement, ontology view, context budget, backend/model artifact identifier, sampling policy, raw artifact retention policy를 확인한다.
- `rpotato benchmark record --fixture <fixture.json>`는 metadata-only benchmark run을 append-only ledger와 SQLite `benchmark_runs` projection에 기록한다. 기록은 `claim_state=not-comparable`, score 없음, reproducibility manifest, redacted local report만 포함한다.
- `rpotato benchmark run --fixture <fixture.json> --prompt <artifact>`는 실행 중인 backend sidecar로 prompt artifact를 실행하고 `model_run_id`, prompt artifact checksum, token/latency/resource summary, deterministic score metadata, reproducibility manifest, redacted report를 연결한 local `measured-locally` benchmark row를 기록한다.
- `rpotato benchmark report --format jsonl`은 redacted benchmark projection과 reproducibility metadata를 export한다. Public benchmark parity는 명시적으로 주장하지 않는다.
- `rpotato backend start`, `rpotato backend status`, `rpotato backend chat`은 event-driven backend CPU/RSS/disk resource sample을 기록한다.
- `rpotato backend chat`은 첫 runtime resource governor slice를 적용한다. Critical pressure는 model 실행 전에 chat을 차단하고, degraded pressure는 effective max-token budget을 clamp하며, normal/unknown pressure는 요청 token budget을 유지한다.
- Backend chat은 첫 visible token latency, total latency, 정상 완료 token usage, effective output budget, terminal resource sample, lifecycle event를 기록한다. Cancellation과 timeout은 서로 다른 ledger event type으로 남기고 model-run interruption flag는 둘 다 interrupted로 표시한다.
- 중단되거나 실패한 SSE stream이 final usage chunk를 보내지 않으면 해당 run의 `token_usage`를 의도적으로 생략한다. 누락 usage는 0이 아니라 unknown이다. Raw prompt, raw response, reasoning trace는 저장하지 않는다.
- `rpotato team status`는 최신 resource sample을 읽고 read-only team admission을 표시한다. Normal pressure는 parallel lane을 허용하고, unknown/degraded pressure는 sequential lane 하나로 fallback하며, critical pressure는 dispatch를 차단한다. 또한 현재 project의 최신 `team.*` runtime ledger event를 표시한다.
- `rpotato team admit --lanes <count>`는 첫 enforced team admission gate다. Admission decision을 append-only ledger와 SQLite projection에 기록하고, normal pressure에서는 requested lane을 허용하며, unknown/degraded pressure에서는 sequential lane 하나로 fallback하고, critical pressure에서는 worker launch가 생기기 전 blocked error를 반환한다.
- `rpotato team admit --lanes <count> --write <path> --command <command>`는 admission gate에 policy preflight를 추가한다. 요청 write path와 command는 `policy check-path`, `policy check-command`와 같은 policy engine으로 분류하며, `ask` 또는 `deny` decision이 하나라도 있으면 dispatch를 차단하고 team admission ledger event에 기록한다.
- `rpotato team admit --lanes <count> --write-owner <lane:path>`는 file ownership preflight를 추가한다. Ownership path는 dispatch 전에 정규화되며, 정규화된 같은 write path를 여러 lane이 소유할 수 없고 conflict는 blocked team admission event로 기록된다.
- 차단된 team admission policy/ownership decision은 `.rpotato/approval-requests/` 아래에 redacted project-local approval request record를 쓴다. `rpotato tui approvals`는 해당 directory를 scan하지 않고 대응하는 canonical ledger event와 active workflow-bound patch proposal만 읽는다.
- `rpotato team dispatch --lanes <count> --write-owner <lane:path>`는 dispatch boundary에서 normalized file ownership을 다시 검사하고 ready/fallback/blocked event를 append-only ledger와 SQLite projection에 기록하며, worker launch가 생기기 전 cross-lane ownership conflict를 차단한다. `--failed-lane <lane> --failure <reason>`은 failed-worker continuation 상태와 남은 admitted lane 진행 가능 여부를 기록한다.
- `rpotato team governor --lanes <count> --context-tokens <tokens>`는 context/model governor preflight를 기록한다. 최신 resource sample을 읽고 admitted lane을 표시하며, `--context-limit` 또는 runtime default에 맞춰 effective context token을 clamp하고, local model-tier route hint(`keep`, `downgrade`, `escalate`, `defer`)를 낸 뒤 append-only ledger와 SQLite projection에 decision을 기록한다. 이 hint는 local runtime policy hint이며 실제 model artifact에 대한 source-backed capability claim이 아니다.
- corrupt SQLite file은 `.corrupt.<timestamp>` suffix로 보존 이동한 뒤 새 projection을 만든다.
- corrupt/stale current-state는 `state reconcile`에서 `.corrupt.<timestamp>` 또는 `.stale.<timestamp>` suffix로 보존 이동한다.
- evidence stale 기준은 artifact 누락, project boundary 이탈, `stale_after_ms` 만료다.

아직 구현하지 않은 부분:

- managed backend sidecar의 continuous background CPU/memory/disk resource sampling
- dispatch preflight 이후 실제 subagent/team dispatcher 실행
- bounded recent-turn window를 넘는 transcript compaction/summarization
- 실제 retention 삭제
- cancellation과 timeout을 구분하는 별도 SQLite terminal-outcome enum
- token stream 통계를 실시간으로 표시하는 TUI

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
transcript_records
```

원칙:

- Durable resume는 user와 visible/normalized model/tool/evidence turn을 저장하며 normalized patch action에는 patch fragment나 verification command 원문 대신 path, action metadata, SHA-256만 저장한다.
- 전체 backend prompt, hidden/raw model response, source file 전체 body는 transcript projection에 저장하지 않는다.
- source path는 project-relative path와 hash 중심으로 저장한다.
- command output은 redacted summary와 artifact pointer를 우선 저장한다.
- raw log 보관은 opt-in 또는 짧은 retention으로 제한한다.
- schema migration은 versioned, forward-only로 둔다.

## 노출 Surface (`CLI/TUI`)

초기 command 후보:

```sh
rpotato monitor status
rpotato monitor models
rpotato monitor baseline
rpotato monitor optimize
rpotato monitor session <id>
rpotato session list
rpotato session history
rpotato session resume <session-id>
rpotato session new
rpotato resume
rpotato resume <session-id>
rpotato continue
rpotato continue <session-id>
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
| durable transcript artifact | app-data lifetime | app-data cleanup만 | local user 및 visible/normalized model/tool/evidence turn, hidden response/raw source body 없음 |
| evidence artifact | stale 또는 user cleanup 전까지 | `evidence validate`, 이후 evidence prune | project-bound pointer 필요 |
| patch rollback bytes | project cleanup 전까지 | project `.rpotato/` cleanup | 제한된 project-local 원본 bytes이며 SQLite/monitor 또는 ledger/evidence payload로 projection하지 않음 |
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

SQLite projection은 status/TUI query를 위해 verification evidence와 stop-gate result row를
포함합니다. 이 row에는 ID, hash, pass/fail state, event timestamp만 있고 rollback artifact가
보존하는 raw source는 포함하지 않습니다.

## Compaction과 Resume 정책

Compacted summary는 source of truth가 아닙니다.

- current-state는 `compaction_boundary`와 `compacted_summary_path` pointer만 보존한다.
- 원본 판단 근거는 runtime ledger, project session ledger, evidence artifact pointer를 다시 읽어 확인한다.
- compacted summary는 resume bundle의 탐색 힌트로만 사용하고, 파일/명령/모델 claim을 확정하는 근거로 쓰지 않는다.
- compacted summary artifact도 `evidence validate`와 같은 project boundary 검증을 통과해야 한다.
- Runtime core는 workflow를 만들거나 안전한 영속 phase를 계속하기 전에 최근 transcript turn 최대 8개·2,400자를 재구성하고 현재 요청과 resume context 전체에 source pointer 최대 4개·3,200자의 단일 공유 budget을 적용한다.
- session resume 권위는 ledger/artifact에 있다. SQLite는 선택 가능한 session/transcript view를 표시하고 append-only ledger event와 immutable transcript artifact가 replay를 승인한다. current-state는 선택한 `session_id`와 resume metadata를 저장한다.
- 각 transcript projection row는 canonical ledger event ID와 monotonic event ordinal을 저장하며 timestamp가 같아도 replay 후 `(session_id, event_ordinal)` 순서를 복원한다.
- `resume`/`continue`는 불확실한 backend request나 verification command를 자동 재실행하지 않는다. Stale source hash, corrupt artifact, cross-project binding, cross-session active workflow ownership은 mutation 전에 fail-closed한다.

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
