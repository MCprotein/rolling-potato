# 서브에이전트

Subagents는 runtime core가 실행하는 bounded worker agent입니다.

Subagent는 runtime state를 무시하는 독립 process가 아닙니다. Parent workflow의 policy, context limit, ledger requirement, stop gate를 상속해야 합니다.

## 목표

- 독립적인 분석 또는 검증 작업을 분리한다.
- 각 worker를 좁은 role과 context 안에 둔다.
- scope 제한으로 작은 모델의 reliability를 보강한다.
- auditability를 잃지 않고 multi-agent workflow로 확장할 수 있게 한다.

## 비목표

- unbounded autonomous agents
- 기본 recursive orchestration
- ownership 없는 parallel file write
- hidden command execution
- worker별 별도 policy engine

## 역할

초기 role:

- `explore`: repo lookup and source mapping
- `planner`: task decomposition
- `executor`: patch proposal
- `verifier`: tests, logs, evidence
- `critic`: risk and regression review
- `writer`: documentation and final report

Role은 personality label이 아니라 capability constraint입니다.

## Runtime 계약

각 subagent는 다음 입력을 받습니다.

- parent workflow id
- role
- task slice
- allowed tools
- allowed paths
- context bundle
- output schema
- evidence requirements
- time/token budget

각 subagent는 다음 결과를 반환합니다.

- status: `complete`, `blocked`, `failed`, `cancelled`
- structured result
- evidence id
- suggested next action
- validation gaps

## 소유권

Subagent는 global state를 소유하지 않습니다.

Subagent가 만들 수 있는 것:

- findings
- patches
- evidence
- summaries

Runtime core가 소유하는 것:

- action approval
- patch apply
- command execution
- merge decision
- stop gate

## 동시성

기본은 sequential입니다. Parallel subagents는 작업이 독립적일 때만 허용합니다.

안전한 parallelism 예시:

- 한 subagent가 repo structure를 mapping하는 동안 다른 subagent가 docs를 review한다.
- 한 subagent가 benchmark fixture design을 점검하는 동안 다른 subagent가 command policy를 점검한다.
- 한 subagent가 model manifest source를 검증하는 동안 다른 subagent가 backend release artifact를 점검한다.

Serialization이 필요한 예시:

- 두 subagent가 같은 file을 editing하는 경우
- patch application과 verification
- state migration과 state read

## Resource Admission

Subagent launch는 runtime admission control을 통과해야 합니다.

Admission input:

- 사용 가능한 memory와 backend health
- active model/backend process count
- parent workflow의 token/context budget
- subagent별 time/token budget
- file ownership conflict
- command/tool permission risk
- 현재 TUI 또는 approval queue 상태

기본 policy:

- subagent parallelism만을 위해 local model을 여러 개 load하지 않는다.
- memory 또는 context가 부족하면 sequential execution을 우선한다.
- token, time, memory, ownership limit을 넘는 subagent는 deny 또는 defer한다.
- admission decision은 ledger에 기록한다.
- admission 실패는 작업을 조용히 버리는 대신 scope를 좁히게 만든다.

현재 구현된 slice:

- `rpotato team status`는 mutation 없이 resource admission을 preview한다.
- `rpotato team admit --lanes <count>`는 resource lane gate를 기록하고 강제한다.
- `rpotato team admit --lanes <count> --write <path> --command <command>`는 요청
  write와 command에 대한 policy preflight를 추가한다. `ask`와 `deny` decision은
  후속 approval flow가 생기기 전까지 dispatch를 차단한다.
- `rpotato team admit --lanes <count> --write-owner <lane:path>`는 file ownership
  preflight를 추가한다. 정규화된 cross-lane write conflict는 worker launch 전에
  dispatch를 차단한다.
- 차단된 policy/ownership admission은 `.rpotato/approval-requests/` record를 쓰며,
  `rpotato tui approvals`는 이 team request를 patch proposal approval 옆에 표시한다.
- `rpotato team dispatch --lanes <count> --write-owner <lane:path>`는 dispatch
  boundary에서 normalized file ownership을 다시 검사하고 dispatch status를 기록하며,
  `--failed-lane <lane> --failure <reason>`으로 failed-worker continuation을 기록할 수
  있다. 아직 worker를 시작하지 않는다.
- `rpotato team governor --lanes <count> --context-tokens <tokens>`는 context/model
  governor preflight를 기록한다. Worker를 시작하거나 실제 model artifact capability를
  주장하지 않고 effective context token을 clamp하며 local model-tier route hint를 낸다.

## 실패 모드

Subagent failure는 parent state를 손상시키면 안 됩니다.

규칙:

- failed subagent result는 ledger에 남긴다.
- parent workflow는 reduced confidence로 계속 진행할 수 있다.
- evidence가 없으면 stop gate를 통과하지 못한다.
- 반복 실패는 scope를 좁히거나 사용자에게 묻는다.

## 검증

Subagent runtime은 test가 필요합니다.

- role boundary enforcement
- path boundary enforcement
- shared file conflict detection
- parent cancellation propagation
- failed worker result handling
- merge evidence tracking
- resource admission denial과 sequential fallback

## v0.35.0 실행 계약

### 릴리스 경계

v0.35.0은 활성 parent workflow 아래에 bounded child worker 하나를 추가합니다.
Team stage orchestration은 추가하지 않으며 v0.36.0 범위로 유지합니다.

- Parent 하나에는 non-terminal subagent를 최대 하나만 허용합니다.
- Subagent는 다른 subagent나 team을 시작할 수 없습니다.
- Runtime이 context 준비, backend 호출, 상태 저장, evidence merge를 소유합니다.
  Model은 상태를 소유하거나 host side effect를 직접 실행하지 않습니다.
- Worker output은 finding 또는 patch proposal을 포함할 수 있지만 patch 적용, command
  실행, action 승인, parent workflow 진행은 할 수 없습니다.
- Resource admission이 parallel lane을 허용해도 v0.35.0 worker는 sequential로 실행합니다.

### CLI surface

```text
rpotato subagent launch --role <role> --task <text> \
  --tool <tool> --read <path> [--tool <tool>] [--read <path>] \
  [--write <path>] [--timeout-ms <ms>] [--max-tokens <tokens>]
rpotato subagent status [subagent-id]
rpotato subagent cancel <subagent-id>
```

`launch`는 현재 session에 활성 non-terminal parent workflow가 있어야 합니다. Runtime은
child를 parent의 정확한 revision과 artifact hash에 binding합니다. `status`는 read-only이며
기본적으로 활성 parent의 최신 child를 표시합니다. `cancel`은 이미 cancelled인 child에만
idempotent하고, 다른 terminal state는 변경하지 않고 그대로 보고합니다.

초기 role/tool 정책:

| Role | 허용 tool | Result capability |
| --- | --- | --- |
| `explore` | `read_file` | finding |
| `planner` | `read_file` | plan과 validation gap |
| `verifier` | `read_file` | evidence 기반 verdict |
| `critic` | `read_file` | 우선순위가 있는 risk |
| `writer` | `read_file` | documentation result |
| `executor` | `read_file`, `render_diff` | 적용되지 않은 patch proposal 하나 |

Tool은 명시적으로 선언해야 합니다. `render_diff`는 하나 이상의 write path 선언이
필요하고 모든 proposal target은 선언된 path 안에 있어야 합니다. v0.35.0 worker에는
command 실행 tool을 제공하지 않습니다.

### 입력 상한

- `task`: trim 뒤 1~4,096 UTF-8 byte. Raw task text는 ledger나 subagent state에 쓰지 않습니다.
- `read` path: 중복 없는 repository-relative path 1~4개. Project root 내부로 정규화하고
  backend dispatch 전에 다시 읽습니다.
- `write` path: 중복 없는 정규화 path 0~4개. Proposal ownership만 선언하며 write 권한을
  부여하지 않습니다.
- Context: 기존 canonical context pack의 최대 4 files, source 3,200 characters 제한을
  재사용합니다. Child는 parent context budget을 늘릴 수 없습니다.
- `timeout-ms`: 기본 30,000, 유효 범위 1~300,000이며 backend chat 상한과 같습니다.
- `max-tokens`: 기본 256, 유효 범위 1~1,024입니다. Resource governor는 effective 값을
  낮출 수 있지만 높일 수 없습니다.
- Result artifact: strict parsing 전 최대 65,536 UTF-8 byte입니다.

중복 tool/path, traversal, project 밖 absolute path, 미지원 role, unknown tool, 0 budget,
상한 초과 budget은 backend request 전에 실패합니다.

### 영속 상태

`SubagentRecordV1`은 다음 field를 갖는 canonical hash-chained artifact입니다.

- subagent id, revision, previous hash, artifact hash
- project id, session id, parent workflow id/revision/artifact hash
- role, task hash, declared tools, normalized read/write paths
- requested/effective token limit과 timeout
- status, backend model-run event id, result artifact id/hash, evidence id/hash, failure code,
  created/started/finished timestamp

Raw task, backend prompt, secret, command output, unredacted model response는 ledger field가
아닙니다. State write는 subagent별 recoverable lease와 compare-and-swap revision 검사를
사용합니다.

허용 transition은 닫혀 있습니다.

| Current | Next |
| --- | --- |
| `requested` | `admitted`, `blocked`, `cancelled` |
| `admitted` | `running`, `cancelled` |
| `running` | `completed`, `blocked`, `failed`, `cancelled`, `timed-out` |
| terminal | 없음 |

Restart 시 stale `running` child는 `interrupted-no-replay` failure code의 `failed`가 됩니다.
Runtime은 model request를 자동 반복하지 않으며 새 시도에는 새 subagent id가 필요합니다.

### Admission과 dispatch

`running` 기록 전에 다음 검사를 모두 통과해야 합니다.

1. Parent identity, revision, artifact hash가 launch binding과 여전히 같고 parent가
   non-terminal이어야 합니다.
2. 다른 non-terminal child가 없어야 하고 request가 recursive하지 않아야 합니다.
3. Role, tool, context, token, timeout, result-size 상한이 유효해야 합니다.
4. Read/write path가 project-root 정규화를 통과하고 ownership conflict가 없어야 합니다.
5. 기존 resource governor가 sequential lane 하나를 admit하고 backend가 healthy해야 합니다.
6. Context source byte가 dispatch용으로 capture한 source pointer와 여전히 같아야 합니다.

Runtime은 backend 호출 전에 `running`을 기록합니다. Timeout은 기존 bounded backend
generation 경로를 사용합니다. Running child의 수동 cancel은 backend generation cancel을
요청하고, subagent별 state lease가 completion과 cancellation의 동시 승리를 막습니다.

### Strict result와 parent merge

`SubagentResultV1`은 다음 logical schema를 가진 strict JSON입니다.

- `schema_version: 1`
- `subagent_id`, `parent_workflow_id`, `role`, terminal `status`
- bounded `summary`
- 최대 16개의 bounded finding
- target path, source hash, find text, replacement text를 갖는 optional patch proposal 하나
- 최대 16개의 evidence reference와 16개의 validation gap
- bounded suggested next action

Missing, duplicate, unknown, oversized, invalid UTF-8, identity mismatch field는 fail-closed합니다.
선언한 write ownership 밖의 executor proposal이나 변경된 source hash 대상은 차단합니다.
Executor가 아닌 role은 patch proposal을 반환할 수 없습니다. Summary, finding, validation
gap, next action, patch text에서 credential 형태의 text를 발견하면 result/evidence artifact를
설치하기 전에 차단합니다.

검증된 `completed` result만 parent에 merge할 수 있습니다. Merge는 launch 시 binding한
정확한 parent artifact hash를 요구하고, child evidence id를 parent skill evidence에 추가한
뒤 parent를 한 번 checkpoint하며, subagent id와 result hash로 key한 idempotent merge event
하나를 기록합니다. Parent checkpoint 뒤 merge event 기록 전에 process가 멈추면 다음 child
admission이 설치된 result/evidence artifact를 다시 검증하고 이미 설치된 evidence id를 인식해
두 번째 parent checkpoint 없이 누락된 event를 기록합니다. 해당 evidence가 없는 stale parent,
변조된 child artifact, missing evidence, 서로 다른 두 번째 result는 parent를 변경하지 않습니다.

Ledger event 이름은 다음으로 고정합니다.

- `team.subagent.requested`
- `team.subagent.admitted`
- `team.subagent.started`
- `team.subagent.completed`
- `team.subagent.blocked`
- `team.subagent.failed`
- `team.subagent.cancelled`
- `team.subagent.timed-out`
- `team.subagent.result-merged`

### 인수 테스트

| ID | 필수 proof |
| --- | --- |
| S01 | CLI가 전체 launch/status/cancel surface를 parse하고 missing/duplicate option을 거부합니다. |
| S02 | Unknown role, undeclared tool, role/tool mismatch가 backend dispatch 전에 실패합니다. |
| S03 | Traversal, project 밖 absolute path, duplicate, 4개 초과 read/write path가 fail-closed합니다. |
| S04 | Task, timeout, token, context, result byte 상한이 minimum/maximum/maximum-plus-one에서 정확합니다. |
| S05 | Missing, terminal, stale, cross-project, cross-session parent는 child를 admit하지 못합니다. |
| S06 | Non-terminal child 하나가 두 번째 child를 차단하고 nested launch는 항상 거부됩니다. |
| S07 | Resource denial과 ownership conflict는 backend request 없이 `blocked`를 기록합니다. |
| S08 | Context pack은 선언하고 다시 검증한 source만 포함하며 기존 file/character 상한 안에 있습니다. |
| S09 | Exact requested/admitted/started/completed event order와 state revision이 deterministic합니다. |
| S10 | Timeout은 `timed-out`을 기록하고 generation cancel을 요청하며 partial output을 merge하지 않습니다. |
| S11 | Manual cancel과 backend completion이 정확히 하나의 terminal state를 얻습니다. |
| S12 | Restart가 stale `running`을 두 번째 model request 없이 `interrupted-no-replay`로 바꿉니다. |
| S13 | Strict result parser가 unknown/missing/duplicate/oversized/invalid/identity mismatch field를 거부합니다. |
| S14 | Executor만 patch proposal 하나를 반환하며 target/hash가 ownership과 현재 byte에 일치해야 합니다. |
| S15 | Completed evidence는 한 번 merge되고 동일 retry는 no-op, 다른 두 번째 result는 차단됩니다. |
| S16 | Stale parent revision/hash, tampered result/evidence, missing evidence는 parent를 변경하지 않습니다. |
| S17 | Status는 bounded/read-only이며 requested/effective budget, lifecycle, evidence, failure code를 표시합니다. |
| S18 | Ledger/state/transcript/diagnostic/한국어 CLI output이 raw task, prompt, secret, unredacted response를 노출하지 않습니다. |

기능이 안정되기 전에는 다음 targeted 검증만 실행합니다.

```text
cargo test --locked subagent::tests::
cargo test --locked cli::tests::subagent
cargo test --locked --test subagent_lifecycle -- --test-threads=1
```

구현은 contract/state, CLI/admission/context, backend lifecycle/cancellation,
result/evidence merge의 네 단위로 나눕니다. 독립 리뷰와 전체 repository gate는 네 단위를
최종 candidate commit에 통합한 뒤 정확히 한 번 실행합니다.
