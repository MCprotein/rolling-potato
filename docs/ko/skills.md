# 스킬 설계

Skills는 재사용 가능한 runtime capability입니다.

Skill은 단순 prompt template이 아닙니다. 반복 workflow에 필요한 instruction, context requirement, tool permission, hook attachment, evidence requirement, stop criteria를 하나로 묶은 runtime 단위입니다.

## 목표

- 반복 workflow를 재현 가능하게 만든다.
- prompt sprawl을 줄인다.
- 작은 모델이 좁은 lane 안에서 동작하게 한다.
- workflow별 policy와 evidence gate를 붙인다.
- user-invoked capability와 runtime-invoked capability를 모두 지원한다.

## 예시

초기 skill 후보:

- `fix-test`
- `explain-error`
- `small-patch`
- `code-review`
- `repo-map`
- `benchmark-model`
- `model-artifact-audit`
- `runtime-doctor`
- `ontology-refresh`
- `release-check`
- `imported.<plugin>.<skill>`

## 스킬 Manifest

각 skill은 manifest를 가져야 합니다.

```json
{
  "schemaVersion": 1,
  "id": "fix-test",
  "displayName": "Fix Test",
  "description": "Fix one failing test with approval and verification.",
  "allowedTools": ["read_file", "render_diff", "apply_patch", "run_command"],
  "requiredHooks": ["pre_tool_call", "post_tool_result", "stop_gate"],
  "contextRequirements": ["test_output", "source_pointer", "package_manifest"],
  "evidenceRequirements": ["failing_test_before", "passing_test_after"],
  "stopCriteria": ["patch_applied", "verification_passed", "korean_report_passed"]
}
```

## 호출

Skill은 다음 경로로 호출될 수 있습니다.

- explicit command: `rpotato skill run fix-test`
- natural command routing: `rpotato run "테스트 실패 고쳐줘"`
- TUI command palette
- team plan step
- benchmark fixture

Runtime core는 invocation을 다음 runtime decision으로 해석합니다.

1. skill id
2. mode
3. context requirements
4. tool permissions
5. evidence requirements
6. stop criteria

## 현재 구현

Phase 3의 현재 구현은 skill 실행 전 정규화 단계입니다.

- `rpotato skill list`는 built-in skill registry를 출력한다.
- `rpotato skill run <id>`는 skill id, mode, allowed tools, context requirements, evidence requirements, stop criteria를 정규화하고 ledger event를 남긴다.
- `rpotato run "<request>"`는 deterministic intent rule로 user request를 skill/mode로 매핑하고 ledger event를 남긴다.
- `rpotato intent classify "<request>"`는 같은 rule을 실행하되 agent loop 계획 대신 classification report만 출력한다.
- `rpotato intent routes`는 TUI command palette action이 어떤 runtime command로 매핑되는지 출력한다.
- active workflow는 current-state가 소유하고, skill/plugin/TUI는 parent workflow pointer를 받아야 한다.
- optional model classifier는 아직 비활성이다. 현재는 deterministic rule만 사용한다.
- 실제 model/backend 실행, context packing, tool call, patch 적용은 후속 phase에서 처리한다.

현재 built-in skill:

- `fix-test`
- `explain-error`
- `small-patch`
- `code-review`
- `repo-map`
- `benchmark-model`
- `model-artifact-audit`
- `runtime-doctor`
- `ontology-refresh`
- `release-check`

## Skill 경계

Skill은 tool을 요청할 수 있지만, tool 실행 여부는 runtime policy가 결정합니다.

Skill은 다음을 할 수 없습니다.

- project boundary 우회
- 사용자 승인 없는 destructive command 실행
- silent artifact download
- hook policy overwrite
- evidence 없는 stop gate complete 처리
- 검증되지 않은 model/license claim 주입

## 가져온 Skill

Claude Code/Codex형 plugin에서 가져온 workflow는 native skill과 같은 권한을 자동으로 얻지 않습니다.

Import 규칙:

- source runtime과 source manifest hash를 기록한다.
- skill id는 source plugin namespace 아래로 격리한다.
- allowed tools, required hooks, evidence requirements가 비어 있으면 `validate` 단계에서 보수적으로 막는다.
- source prompt나 command가 shell/background/MCP 실행을 요구하면 permission report에 표시한다.
- 실행은 반드시 `rpotato` runtime policy와 stop gate를 통과한다.

자세한 plugin 호환 경계는 [plugin-adapters.md](plugin-adapters.md)를 따릅니다.

## 스킬 Runtime State

Skill 실행은 다음 상태를 기록해야 합니다.

- active skill id
- parent workflow id
- context bundle id
- tool call id
- evidence id
- stop gate result
- final report id

이 기록으로 skill run을 resume 가능하고 audit 가능하게 만듭니다.

## Subagents와 Teams

Skill은 manifest에 다음을 선언한 경우에만 subagents 또는 team을 사용할 수 있습니다.

- allowed roles
- max subagents
- shared context scope
- write ownership
- merge policy
- verification requirements

기본 skill execution은 single-agent, sequential입니다.

## 검증

Skill은 fixture test가 필요합니다.

- explicit invocation
- command routing
- missing context fail-closed
- denied tool request
- stop gate evidence missing
- interruption 이후 resume
- Korean final report guard
