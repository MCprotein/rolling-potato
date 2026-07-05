# 팀 Runtime

Team runtime은 하나의 parent workflow 아래에서 여러 subagent를 조율하는 runtime capability입니다.

Team runtime은 parallel 또는 staged work가 실제로 도움이 되는 작업을 위한 경로입니다. 작은 patch 작업의 기본 경로는 아닙니다.

## 목표

- Claude Code/Codex replacement-level workflow를 지원한다.
- 여러 bounded agent를 조율한다.
- 하나의 runtime policy engine을 유지한다.
- team work를 resume 가능하고 audit 가능하게 만든다.
- worker conflict와 hidden side effect를 방지한다.

## 팀 Pipeline

기본 staged pipeline:

1. `team-plan`
2. `team-dispatch`
3. `team-exec`
4. `team-review`
5. `team-verify`
6. `team-merge`
7. `team-report`

각 stage는 runtime state transition입니다.

## 팀 Manifest

Team execution은 manifest를 가져야 합니다.

```json
{
  "schemaVersion": 1,
  "teamId": "fix-regression-team",
  "parentWorkflowId": "workflow-123",
  "members": [
    {"id": "explore-1", "role": "explore"},
    {"id": "executor-1", "role": "executor"},
    {"id": "verifier-1", "role": "verifier"}
  ],
  "writePolicy": "single_writer",
  "mergePolicy": "runtime_owned",
  "stopGate": "evidence_required"
}
```

## 쓰기 Policy

기본 write policy:

- subagent는 patch를 propose할 수 있다.
- runtime core가 patch를 apply한다.
- 한 file에는 한 번에 하나의 writer만 둔다.
- conflict는 parent workflow로 escalate한다.
- verification은 ownership 해결 뒤 merge 이후에 실행한다.

## Coordination 규칙

- parent workflow가 global plan을 소유한다.
- worker는 assigned slice만 실행한다.
- worker는 기본적으로 team을 spawn할 수 없다.
- worker는 스스로 scope를 넓힐 수 없다.
- team state는 ledger에 남긴다.
- team cancellation은 모든 active worker로 전파한다.

## Resource Admission

Team mode는 runtime resource가 감당할 수 있을 때만 허용합니다.

Admission check:

- 후속 backend policy가 명시적으로 허용하기 전까지 하나의 model/backend sidecar를 재사용한다.
- worker count가 memory, token budget, context budget, timeout 안에 들어온다.
- dispatch 전에 file ownership을 배정할 수 있다.
- approval queue와 TUI state가 pending decision을 모두 표현할 수 있다.
- worker가 요구하는 plugin/tool permission이 dispatch 전에 알려져 있다.

Admission이 실패하면 runtime은 sequential subagent 또는 single-agent workflow로 fallback하고 ledger에 이유를 기록해야 합니다. Team admission은 assigned work를 조용히 버리면 안 됩니다.

## 터미널 UI Integration

TUI는 다음을 보여야 합니다.

- team stage
- worker status
- active task slice
- pending approvals
- conflicts
- evidence status
- final merge status

TUI는 team state를 표시합니다. Coordination authority가 되지는 않습니다.

## 검증

Team runtime은 test가 필요합니다.

- team manifest parsing
- worker lifecycle state transition
- cancellation propagation
- shared file conflict
- failed worker continuation
- merge gate
- evidence-required stop gate
- team resource admission과 sequential fallback
