# Hooks

Hooks are lifecycle control points owned by the runtime core.

A hook is not a shell callback that bypasses policy. It observes or transforms constrained runtime events, returns structured results, and records every result in the ledger.

## Goals

- Make lifecycle control points explicit.
- Connect user/project/runtime policy to runtime events.
- Keep model output behind deterministic gates.
- Let skills, subagents, team execution, TUI, and benchmark harnesses reuse the same policy boundary.
- Make all side effects auditable.

## Non-Goals

- arbitrary hidden shell execution
- plugin code that bypasses permission policy
- prompt mutation not recorded in the ledger
- hooks that write project files directly

## Required Hook Points

Minimum lifecycle hooks:

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

Each hook receives:

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

Each hook returns:

- status: `allow`, `ask`, `deny`, `modify`, `observe`, `error`
- optional modified payload
- Korean user-facing reason when blocking or asking for approval
- evidence record
- ledger metadata

## Current Implementation

Phase 4 currently implements:

- `rpotato hooks list`
- `rpotato hooks validate-result <json>`

`hooks list` prints lifecycle hook registry, ordering, conflict rule, and input/output schema.

`hooks validate-result` checks the hook output `status`. Unknown status or parse failure fails closed as `deny`.

Implemented conflict rule:

```text
error/deny > ask > modify > allow > observe
```

## Policy Boundary

Hooks can narrow behavior, but they cannot widen permissions beyond runtime policy.

Examples:

- A hook can deny a command that policy would otherwise ask approval for.
- A hook can add evidence required by the stop gate.
- A hook cannot run a destructive command without user approval.
- A hook cannot mark a model/license claim as confirmed without source evidence.

## Imported Hooks

Hooks imported from Claude Code/Codex-style plugins do not get the same authority as native `rpotato` hooks just because names look similar.

Import rules:

- Import only when the source hook event maps explicitly to a `rpotato` lifecycle hook.
- Hooks that call commands, HTTP, MCP, or background processes are disabled by default.
- Hook results normalize to one of `allow`, `ask`, `deny`, `modify`, `observe`, `error`.
- Foreign hooks cannot widen permissions beyond runtime policy.
- Unmapped hooks are recorded as `unsupported` in the ledger.

See [plugin-adapters.md](plugin-adapters.md) for the plugin compatibility boundary.

## Ordering

Hook order must be deterministic.

1. built-in runtime hooks
2. project policy hooks
3. skill hooks
4. session hooks
5. observation-only hooks

When hook results conflict, the stricter result wins.

`deny` > `ask` > `modify` > `allow` > `observe`

## Storage

Hook definitions live in app or project state, not in model prompts.

Possible locations:

```text
rpotato app data root/
  hooks/

project root/
  .rpotato/
    hooks/
```

## Validation

Hook behavior needs fixture tests for:

- hook ordering
- deny over allow
- ask over allow
- modified payload ledger record
- hook failure fail-closed behavior
- direct file-write bypass rejection
- command-execution bypass rejection
