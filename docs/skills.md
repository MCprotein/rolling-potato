# Skills

Skills are reusable runtime capabilities.

A skill is not just a prompt template. It is a runtime unit that groups instruction, context requirements, tool permissions, hook attachment, evidence requirements, and stop criteria for a repeatable workflow.

## Goals

- Make repeatable workflows reproducible.
- Reduce prompt sprawl.
- Keep small models inside narrow lanes.
- Attach workflow-specific policy and evidence gates.
- Support both user-invoked and runtime-invoked capabilities.

## Examples

Initial skill candidates:

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

## Skill Manifest

Each skill should have a manifest.

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

## Invocation

Skills can be invoked through:

- explicit command: `rpotato skill run fix-test "fix the failing API test"`
- natural command routing: `rpotato run "테스트 실패 고쳐줘"`
- TUI command palette
- team plan step
- benchmark fixture

Runtime core interprets invocation into:

1. skill id
2. mode
3. context requirements
4. tool permissions
5. evidence requirements
6. stop criteria

## Current Implementation

The v0.33 runtime executes built-in skills as durable state machines inside the agent loop.

- `rpotato skill list` prints the built-in skill registry.
- `rpotato skill run <id> "<request>"` explicitly selects a built-in skill and executes the same durable loop as `run`.
- `rpotato run "<request>"` selects ontology-backed context, persists a runtime-owned typed read-only or patch action, rereads authoritative source for a valid patch, and stops at either a guarded Korean report or the exact `patch approve` gate.
- `rpotato intent classify "<request>"` runs the same rules but prints only a classification report instead of planning an agent loop.
- `rpotato intent routes` prints TUI command-palette routing to runtime commands.
- `rpotato patch preview --path <path> --find <text> --replace <text>` renders a diff-only standalone record that cannot be approved, applied, or verified.
- `rpotato patch approve <proposal-id> --token <token> --dry-run` verifies the approval gate and records a ledger event without applying the patch.
- `rpotato patch approve <proposal-id> --token <token>` accepts only a workflow proposal created by `run`, applies it when the source and proposal bindings remain valid, writes a rollback record, and issues a separate verification credential without running the command.
- `rpotato patch verify <proposal-id> --token <token>` separately approves and runs the pre-bound policy-allowed argv verification plan.
- Current state owns the active workflow; skill/plugin/TUI actions need a parent workflow pointer.
- Each transition validates required context, allowed tools, completed lifecycle hooks, evidence, and stop criteria. Missing requirements fail closed before completion.
- Workflow phase and skill state must agree at every side-effect boundary. A resumed or tampered workflow cannot apply a patch or run verification from a skipped skill state.
- `fix-test` accepts an actual `cargo test` argv plan only. The same canonical command must fail before proposal creation and pass after the approved patch; the pre-patch ledger event is bound to the workflow id and command hash.
- Read-only and review skills require a non-empty Korean answer whose evidence is present in the visible answer. A source pointer alone does not satisfy file, line, diagnostic, benchmark, checksum, or ranked-finding evidence.
- Workflow schema v4 persists the active skill, invocation, state, completed hooks, evidence, and stop criteria so restart and resume cannot bypass the skill contract.
- The optional model classifier is disabled. Current routing uses deterministic rules only.
- `run` owns the persisted workflow/action/proposal loop and typed final reporting; general model-output-to-tool orchestration beyond the bounded patch action remains a later phase.

Current built-in skills:

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

## Skill Boundary

Skills may request tools, but runtime policy decides whether tools execute.

Skills cannot:

- bypass project boundaries
- run destructive commands without user approval
- silently download artifacts
- overwrite hook policy
- mark stop gates complete without evidence
- inject unverified model/license claims

## Imported Skills

Workflows imported from Claude Code/Codex-style plugins do not automatically receive native skill authority.

Import rules:

- Record source runtime and source manifest hash.
- Scope skill IDs under the source plugin namespace.
- Block conservatively during `validate` when allowed tools, required hooks, or evidence requirements are empty.
- Show source prompts or commands that require shell/background/MCP execution in the permission report.
- Execution must pass `rpotato` runtime policy and the stop gate.

See [plugin-adapters.md](plugin-adapters.md) for the plugin compatibility boundary.

## Skill Runtime State

Skill execution records:

- active skill id
- parent workflow id
- invocation source
- current skill state
- completed required hooks
- evidence keys
- satisfied stop criteria

The built-in state machine uses `selected`, `context-ready`, `model-requested`, `action-recorded`, `awaiting-approval`, `awaiting-verification`, `stop-passed`, `complete`, `failed`, and `cancelled`. Invalid transitions and incomplete completion attempts fail closed.

## Subagents And Teams

A skill can use subagents or teams only when its manifest declares:

- allowed roles
- max subagents
- shared context scope
- write ownership
- merge policy
- verification requirements

Default skill execution is single-agent and sequential.

## Validation

Skills need fixture tests for:

- explicit invocation
- command routing
- missing context fail-closed
- denied tool request
- missing stop-gate evidence
- resume after interruption
- Korean final-report guard
