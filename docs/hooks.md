# Hooks

Hooks는 runtime core가 소유하는 lifecycle control point입니다.

Hook은 정책을 우회하는 shell callback이 아닙니다. 제한된 runtime event를 관찰하거나 변환하고, 구조화된 결과를 반환하며, 모든 결과는 ledger에 기록되어야 합니다.

## 목표

- lifecycle control point를 명시한다.
- user/project/runtime policy를 runtime event에 연결한다.
- 모델 출력을 deterministic gate 뒤에 둔다.
- skills, subagents, team execution, TUI, benchmark harness가 같은 policy 경계를 재사용하게 한다.
- 모든 side effect를 audit 가능하게 만든다.

## 비목표

- 임의의 hidden shell execution
- permission policy를 우회하는 plugin code
- ledger에 남지 않는 prompt mutation
- project file을 직접 쓰는 hook

## 필수 Hook Point

최소 lifecycle hook은 다음과 같습니다.

- `session_start`
- `user_request_received`
- `pre_context_pack`
- `post_context_pack`
- `pre_model_request`
- `post_model_response`
- `pre_action_parse`
- `post_action_parse`
- `pre_tool_call`
- `post_tool_result`
- `pre_patch_apply`
- `post_patch_apply`
- `pre_command_run`
- `post_command_run`
- `pre_final_report`
- `stop_gate`
- `session_end`

## Hook Contract

각 hook은 다음 입력을 받습니다.

- hook name
- session id
- workflow id
- project root
- current mode
- active skill id, if any
- actor id: user, runtime, agent, subagent, team
- input payload
- source evidence pointer
- policy context

각 hook은 다음 결과를 반환합니다.

- status: `allow`, `ask`, `deny`, `modify`, `observe`, `error`
- optional modified payload
- 차단 또는 승인 요청 시 한국어 user-facing reason
- evidence record
- ledger metadata

## Policy Boundary

Hook은 동작을 더 좁힐 수 있지만 runtime policy보다 권한을 넓힐 수 없습니다.

예시:

- Hook은 policy상 승인 요청 대상인 command를 deny할 수 있습니다.
- Hook은 stop gate에 필요한 evidence를 추가할 수 있습니다.
- Hook은 destructive command를 사용자 승인 없이 실행하게 만들 수 없습니다.
- Hook은 source evidence 없이 모델/license claim을 confirmed로 표시할 수 없습니다.

## Imported Hooks

Claude Code/Codex형 plugin에서 가져온 hook은 같은 이름처럼 보여도 `rpotato` hook과 동일한 권한을 갖지 않습니다.

Import 규칙:

- source hook event가 `rpotato` lifecycle hook에 명시적으로 매핑될 때만 import한다.
- command, HTTP, MCP, background process를 호출하는 hook은 기본 비활성화한다.
- hook result는 `allow`, `ask`, `deny`, `modify`, `observe`, `error` 중 하나로 정규화한다.
- 외부 hook은 runtime policy보다 권한을 넓힐 수 없다.
- 매핑할 수 없는 hook은 `unsupported`로 ledger에 기록한다.

자세한 plugin 호환 경계는 [plugin-adapters.md](plugin-adapters.md)를 따릅니다.

## Ordering

Hook 순서는 deterministic해야 합니다.

1. built-in runtime hooks
2. project policy hooks
3. skill hooks
4. session hooks
5. observation-only hooks

Hook 결과가 충돌하면 더 엄격한 결과가 이깁니다.

`deny` > `ask` > `modify` > `allow` > `observe`

## Storage

Hook definition은 model prompt가 아니라 app 또는 project state에 저장합니다.

가능한 위치:

```text
rpotato app data root/
  hooks/

project root/
  .rpotato/
    hooks/
```

## Validation

Hook behavior는 fixture test가 필요합니다.

- hook ordering
- deny over allow
- ask over allow
- modified payload ledger record
- hook failure fail-closed behavior
- direct file write bypass 차단
- command execution bypass 차단
