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
- projection row는 stable event id로 idempotency를 보장한다.
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
