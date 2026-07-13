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

- explicit command: `rpotato skill run fix-test "실패한 API 테스트를 고쳐줘"`
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

v0.33 runtime은 built-in skill을 agent loop 안의 영속 state machine으로 실행합니다.

- `rpotato skill list`는 built-in skill registry를 출력한다.
- `rpotato skill run <id> "<request>"`는 built-in skill을 명시적으로 선택하고 `run`과 같은 영속 loop를 실행한다.
- `rpotato run "<request>"`은 ontology 기반 context를 선택하고 runtime-owned typed read-only 또는 patch action을 저장하며, 유효한 patch는 authoritative source를 다시 읽은 뒤 guarded 한국어 보고 또는 정확한 `patch approve` gate에서 멈춘다.
- `rpotato intent classify "<request>"`는 같은 rule을 실행하되 agent loop 계획 대신 classification report만 출력한다.
- `rpotato intent routes`는 TUI command palette action이 어떤 runtime command로 매핑되는지 출력한다.
- `rpotato patch preview --path <path> --find <text> --replace <text>`는 approve/apply/verify할 수 없는 diff-only standalone record를 만든다.
- `rpotato patch approve <proposal-id> --token <token> --dry-run`은 patch를 적용하지 않고 approval gate를 검증해 ledger event를 남긴다.
- `rpotato patch approve <proposal-id> --token <token>`은 `run`이 생성한 workflow proposal만 받고, source와 proposal binding이 유효할 때 patch를 적용해 rollback record를 쓴 뒤 command 실행 없이 별도의 verification credential을 발급한다.
- `rpotato patch verify <proposal-id> --token <token>`은 pre-bound되고 policy가 허용한 argv verification plan을 별도로 승인해 실행한다.
- active workflow는 current-state가 소유하고, skill/plugin/TUI는 parent workflow pointer를 받아야 한다.
- 각 transition은 required context, allowed tool, 완료된 lifecycle hook, evidence, stop criteria를 검증하며 요구사항이 빠지면 completion 전에 fail-closed한다.
- Workflow schema v4는 active skill, invocation, state, completed hook, evidence, stop criteria를 저장하므로 restart/resume이 skill contract를 우회할 수 없다.
- optional model classifier는 아직 비활성이다. 현재는 deterministic rule만 사용한다.
- `run`은 영속 workflow/action/proposal loop와 typed 최종 보고를 소유한다. 제한된 patch action을 넘어서는 일반 model-output-to-tool orchestration은 후속 phase에서 처리한다.

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

Skill 실행은 다음 상태를 기록합니다.

- active skill id
- parent workflow id
- invocation source
- current skill state
- completed required hook
- evidence key
- satisfied stop criteria

Built-in state machine은 `selected`, `context-ready`, `model-requested`, `action-recorded`, `awaiting-approval`, `awaiting-verification`, `stop-passed`, `complete`, `failed`, `cancelled`를 사용합니다. 허용되지 않은 transition과 요구사항이 덜 채워진 completion은 fail-closed합니다.

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
