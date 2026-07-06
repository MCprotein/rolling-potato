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

- explicit command: `rpotato skill run fix-test`
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

Phase 3 implements pre-execution normalization, and `rpotato run` now uses that routing state to enter the first context-aware model-response agent-loop skeleton.

- `rpotato skill list` prints the built-in skill registry.
- `rpotato skill run <id>` normalizes skill id, mode, allowed tools, context requirements, evidence requirements, and stop criteria, then records a ledger event.
- `rpotato run "<request>"` maps user requests to skill/mode through deterministic intent rules, builds a bounded repository context pack with source pointers, prepares a runtime-owned action candidate and next gate, calls the running backend sidecar, parses the model's structured action line or recognized action text without execution, records intent/context/action/model-action/backend chat ledger events, and records token/latency metrics.
- `rpotato intent classify "<request>"` runs the same rules but prints only a classification report instead of planning an agent loop.
- `rpotato intent routes` prints TUI command-palette routing to runtime commands.
- `rpotato patch preview --path <path> --find <text> --replace <text>` renders a diff and approval token without modifying the target file.
- `rpotato patch approve <proposal-id> --token <token> --dry-run` verifies the approval gate and records a ledger event without applying the patch.
- Current state owns the active workflow; skill/plugin/TUI actions need a parent workflow pointer.
- The optional model classifier is disabled. Current routing uses deterministic rules only.
- Tool calls, approved patch application, and verification command execution happen in later phases.

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

Skill execution should record:

- active skill id
- parent workflow id
- context bundle id
- tool call id
- evidence id
- stop gate result
- final report id

This makes skill runs resumable and auditable.

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
