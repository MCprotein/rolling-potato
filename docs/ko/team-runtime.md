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

`rpotato team status`는 현재 read-only admission preview입니다. 최신 resource sample을
사용해 향후 team dispatch가 parallel, sequential fallback, blocked 중 어디에 해당하는지
보여줍니다. 아직 worker를 시작하거나 workflow state를 변경하지 않습니다.

`rpotato team admit --lanes <count>`는 첫 enforced admission gate입니다. 같은 resource
policy를 사용하지만 ledger event를 기록하고 critical pressure에서는 blocked error를
반환합니다. Normal pressure에서는 요청한 lane 수를 허용하고, unknown 또는 degraded
pressure에서는 sequential lane 하나로 fallback합니다. 이 명령은 아직 worker를 시작하거나
team stage를 전진시키지 않으므로, 후속 dispatcher 작업은 admission contract를 바꾸지
않고 worker launch를 gate 뒤에 붙일 수 있습니다.

`team admit`은 요청 write path, lane ownership, command도 preflight할 수 있습니다.

```text
rpotato team admit --lanes 2 --write README.md --command "cargo test"
rpotato team admit --lanes 2 --write-owner 1:src/app.rs --write-owner 2:src/cli.rs
```

Preflight는 공통 runtime policy engine을 사용합니다. `allow` check는 gate를 통과할 수
있고, `ask`와 `deny` check는 dispatch를 차단하며 같은 admission ledger event에
기록됩니다. `--write-owner <lane:path>`는 dispatch 전에 lane별 write path를 추가로
정규화합니다. 두 lane이 같은 normalized path를 claim하면 admission은 ownership-blocked
결과를 반환하고 같은 ledger event에 기록합니다. 이것은 아직 worker launch나 merge-time
ownership enforcement가 아니라 admission-time preflight입니다.

Policy 또는 ownership preflight가 admission을 차단하면 runtime은
`.rpotato/approval-requests/` 아래에 redacted project-local approval request도 기록합니다.
`rpotato tui approvals`는 이 team admission request를 patch proposal approval과 함께
표시합니다. TUI는 계속 read-only이며, approval execution과 worker dispatch는 별도의 후속
gate입니다.

`rpotato team governor --lanes <count> --context-tokens <tokens>`는 첫 context/model
governor preflight입니다.

```text
rpotato team governor --lanes 2 --context-tokens 6000 --context-limit 4096 --model-tier standard
```

이 명령은 최신 resource sample을 읽고 admitted lane을 표시하며, 설정 budget과 pressure
state에 맞춰 effective context token을 clamp하고, ledger event를 기록하며, local
model-tier route hint인 `keep`, `downgrade`, `escalate`, `defer`를 냅니다. 이 값은 runtime
policy hint일 뿐입니다. 실제 model capability를 주장하거나, model artifact를 다운로드/선택하거나,
worker를 시작하지 않습니다.

## 터미널 UI Integration

TUI는 다음을 보여야 합니다.

- team stage
- worker status
- active task slice
- pending approvals
- team admission approval request
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
