# Subagents

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

## Roles

초기 role:

- `explore`: repo lookup and source mapping
- `planner`: task decomposition
- `executor`: patch proposal
- `verifier`: tests, logs, evidence
- `critic`: risk and regression review
- `writer`: documentation and final report

Role은 personality label이 아니라 capability constraint입니다.

## Runtime Contract

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

## Ownership

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

## Concurrency

기본은 sequential입니다. Parallel subagents는 작업이 독립적일 때만 허용합니다.

안전한 parallelism 예시:

- 한 subagent가 repo structure를 mapping하는 동안 다른 subagent가 docs를 review한다.
- 한 subagent가 benchmark fixture design을 점검하는 동안 다른 subagent가 command policy를 점검한다.
- 한 subagent가 model manifest source를 검증하는 동안 다른 subagent가 backend release artifact를 점검한다.

Serialization이 필요한 예시:

- 두 subagent가 같은 file을 editing하는 경우
- patch application과 verification
- state migration과 state read

## Failure Mode

Subagent failure는 parent state를 손상시키면 안 됩니다.

규칙:

- failed subagent result는 ledger에 남긴다.
- parent workflow는 reduced confidence로 계속 진행할 수 있다.
- evidence가 없으면 stop gate를 통과하지 못한다.
- 반복 실패는 scope를 좁히거나 사용자에게 묻는다.

## Validation

Subagent runtime은 test가 필요합니다.

- role boundary enforcement
- path boundary enforcement
- shared file conflict detection
- parent cancellation propagation
- failed worker result handling
- merge evidence tracking
