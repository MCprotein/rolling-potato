# 상태 수명주기

이 문서는 `rolling-potato`의 cross-store state authority를 정의합니다.

Runtime ledger, SQLite projection, ontology graph, model knowledge base, plugin registry, evidence artifact, current-state pointer를 연결합니다. 목표는 replay, recovery, routing, failure handling을 deterministic하게 만드는 것입니다.

## 저장소 권한

| Store | Owns | Does not own |
| --- | --- | --- |
| append-only ledger | event order, audit trail, mutation intent | query performance |
| SQLite projection | query/index/reporting view | source-of-truth event order |
| current state | 선택된 session/workflow pointer | historical truth |
| ontology graph | source-backed project semantics와 invariant | model artifact trust |
| model manifest | model/backend artifact trust | runtime observation |
| model knowledge base | evidence index와 반복 observation | install trust 또는 default-model truth |
| plugin registry | imported plugin state와 normalized capability | execution approval |
| evidence artifact | redacted verification/debug pointer | policy decision |

## 쓰기 순서

State-changing operation은 다음 순서로 진행합니다.

1. policy와 project boundary를 검증한다.
2. stable event id를 만든다.
3. ledger event를 append한다.
4. state mutation을 적용한다.
5. SQLite/query projection을 갱신한다.
6. evidence pointer를 기록한다.
7. diagnostics 또는 TUI update를 emit한다.

후속 단계가 실패하면 recovery가 ledger event를 replay해서 projection을 완성하거나 compensating failure event를 기록합니다. Projection write는 event id 기준으로 idempotent해야 합니다.

## Replay와 Recovery

Replay rule:

- ledger event는 event-time/order sequence로 replay한다.
- runtime ledger, project session ledger, operation log append는 하나의
  recoverable writer lease를 공유해 동시 process가 hash chain을 fork하지 못하게 한다.
- projection row는 stable event id로 idempotency를 보장한다.
- SQLite `ledger_events`와 선택 가능한 session row는 canonical runtime ledger에서
  재생성하며 SQLite에만 존재하는 session은 제거한다.
- partial write는 missing projection row 또는 mismatched hash로 감지한다.
- corrupt projection file은 재생성 전에 보존한다.
- current-state pointer는 ledger/session history를 읽은 뒤에만 복구한다.
- ontology/model knowledge/plugin projection은 replay 중 event를 발명하면 안 된다.

## 라우팅 결정 기록

Runtime이 model, skill, mode, ontology view, backend, subagent/team lane, escalation target을 선택하는 결정은 routing decision record를 남겨야 합니다.

필수 field:

- user request id
- session id와 workflow id
- 선택된 skill과 mode
- 선택된 model, backend, quantization, ontology view
- routing input: manifest status, model knowledge hint, benchmark evidence, user constraint, policy constraint, context budget
- 필요한 경우 rejected alternative
- escalation target 또는 fallback path
- 최종 decision reason

Routing record는 explainability evidence입니다. 선택된 모델이 전역적으로 최고라는 증명이 아닙니다.

## 재시도와 실패 처리

Failure handling은 failure category에 따라 달라집니다.

| Failure class | Default action |
| --- | --- |
| model output failure | bounded regeneration 1회 후 escalate 또는 fail closed |
| prompt/context packing failure | source pointer에서 context를 재구성하고 1회 retry |
| ontology/source-pointer failure | source를 다시 읽거나 completion 차단 |
| runtime parser/policy failure | action deny 후 validation gap 기록 |
| tool/command failure | retry 전 idempotency 분류 |
| backend/runtime failure | process cleanup, diagnostic 기록, 안전한 경우만 retry |
| fixture/expected-output issue | fixture quarantine, model score에 반영하지 않음 |

Repeated invalid output은 scope를 좁히거나 escalate합니다. Runtime은 risky action이 통과할 때까지 무한 retry하면 안 됩니다.

## 검증

필수 test:

- event id idempotent replay
- corrupt SQLite 이후 projection rebuild
- partial-write recovery
- routing decision record 생성
- model knowledge hint가 manifest/policy를 우회하지 않음
- retry budget이 repeated invalid output을 멈춤
- fixture issue를 model failure로 scoring하지 않음

## v0.29.0 Workflow Checkpoint

Patch workflow는 `.rpotato/workflows/` 아래에 저장됩니다. 변경 불가 versioned snapshot,
대응 append-only `workflow.checkpoint` event, atomic 교체되는 committed-revision pointer가
함께 resume 권위를 가집니다. Sync된 transaction record를 이용해 startup이 중단된
snapshot/ledger/pointer window를 idempotent하게 완료합니다. 각 revision은
`previous_hash`와 `artifact_hash`를 연결하며 malformed ledger line, 누락 revision, stale
latest checkpoint, chain conflict는 fail-closed로 차단합니다.
Legacy schema v2와 v3 snapshot은 변경하지 않고 계속 읽습니다. 다음 checkpoint가 필요한
legacy workflow는 이전 artifact hash를 보존한 채 schema v4 revision을 append합니다.
Schema version은 단방향으로만 증가하고 downgrade하지 않습니다.

Recovery는 `current-state.json`만 신뢰하지 않고 모든 workflow pointer, transaction,
snapshot directory를 검사합니다. Nonterminal workflow가 둘 이상이면 conflict로
fail-closed합니다. Crash 뒤 terminal workflow가 active pointer에 남으면 다시 검증한 뒤
pointer를 atomic하게 비웁니다. Patch approval과 verification approval은 서로 독립된
영속 gate입니다. `patch approve`는 binding된 patch만 적용하고
`pending-verification-approval`에서 멈추며, 별도로 발급한 credential만 `patch verify`를
승인할 수 있습니다. 두 pending gate, verification evidence, terminal failure,
completion은 process restart 뒤에도 유지됩니다. Resume은 일회성 credential을 다시
표시하거나 model backend에 재진입하지 않으며, complete resume도 proposal binding,
source, evidence, stop gate를 다시 검증합니다.
`model-pending`과 `action-recorded` recovery는 backend에 다시 진입하지 않고 사실에 맞는
terminal failure를 기록합니다. `verification-started`는 결과가 불명확한 durable
boundary이므로 resume은 fail-closed하고 command를 replay하지 않으며 새 explicit
user-controlled 경로가 필요합니다. Approval/token rotate와 workflow checkpoint는 PID/nonce
recoverable lease와 workflow revision CAS를 함께 사용합니다. Linux/macOS/Windows liveness가
owner 사망을 확정한 경우에만 reclaim하고 live/unknown owner는 fail-closed합니다. 명시적
`cancel`은 hash가 맞는 applied bytes를 복원하거나 verification replay 없이 durable conflict를
기록합니다.

손상된 workflow 또는 ledger artifact는 원래 위치에 보존합니다. 별도의 sync된
validation-gap JSONL에는 failure class와 artifact descriptor hash만 기록하므로 recovery가
손상된 authoritative ledger에 만들어 낸 history를 append하지 않습니다. Verification
evidence는 deterministic ID, atomic artifact replace, sync된 runtime append, ledger event
dedupe를 사용합니다. 새 ledger line은 `previous_event_hash`/`event_hash`로 physical append
order를 binding하고 sync된 head가 reorder, tamper, tail truncation을 차단합니다. Legacy
prefix는 chained suffix 앞에서만 허용합니다.

## v0.33.0 Skill State Checkpoint

Workflow schema v4는 `active_skill_id`, invocation source, skill state, completed lifecycle
hook, evidence key, satisfied stop criteria를 추가합니다. Canonical workflow snapshot과
ledger checkpoint가 이 상태의 권위를 가지며 SQLite는 query와 monitoring용 active skill만
projection합니다. Resume은 계속 실행하거나 terminal workflow를 인정하기 전에 저장된 skill
contract를 다시 검증합니다. Side effect는 정확한 workflow-phase/skill-state pairing도
요구합니다. `fix-test`는 workflow id와 정규화된 command hash에 binding된 patch 전 실제
failing-test event가 canonical ledger에 있어야 하며, event 없는 evidence label만으로는 apply,
verification, resume, completion을 승인할 수 없습니다.
