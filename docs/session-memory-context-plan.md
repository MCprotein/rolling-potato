# Session Memory And Context Plan

Status: implemented on `feature/session-memory-context`; release publication is out of scope.

## Goal

Make a restarted `rpotato` conversation remember completed dialogue without
forcing a small local model to reread the entire session. Use the selected
model's declared context window instead of a fixed 4,096-token fallback, keep
the current request authoritative, and preserve an auditable canonical history.

## Non-Goals

- Persisting hidden reasoning or raw backend responses
- Treating generated summaries as source of truth
- Adding a vector database, hosted memory service, or search dependency
- Filling every available model token on every request
- Automatically replaying uncertain tools, commands, or backend requests

## Ownership Boundaries

### Canonical Conversation Store

The TUI conversation adapter owns append-only persistence of completed
user/assistant pairs for every successful interactive request, including coding
and agent workflows. Workflow transcripts remain a separate execution-audit
stream. The adapter ignores incomplete tails during restore, keeps a bounded
in-memory prompt view instead of rescanning the full session on every request,
and records `/clear` as a unique causal reset boundary rather than deleting
audit history.

### Dialogue Recall Policy

`runtime_core::knowledge::recall` is a dependency-free policy module. It:

- recognizes typed user-memory candidates such as preferences, corrections, and
  identity statements;
- ranks older complete pairs by deterministic lexical overlap and recency;
- preserves pair integrity and chronological rendering; and
- truncates an oversized latest pair without dropping either role.

This policy is intentionally deterministic and cheap enough for small models.

### Prompt Assembly

`runtime_core::knowledge::prompt` owns token allocation and section order.
Stable instructions are first. Typed memory, query recall, recent dialogue, and
attachments have separate bounded allowances. The current user request and
response cue are always last.

Historical dialogue and attachment payloads are encoded as untrusted data.
They can provide context but cannot override stable instructions or the current
request. Agent/workflow prompts use the same effective runtime context source
and reserve output/runtime capacity before bounding resume and repository
sections.

### Backend Reconciliation

The inference adapter exposes the configured runtime specification. The TUI
backend adapter compares desired and observed model path, context length, and
vision projector. Any drift causes a controlled restart and a second exact
comparison before a request is accepted.

### Resume And Compaction

Canonical transcripts remain authoritative. Compaction produces a typed,
incremental checkpoint plus a bounded tail of complete exchanges. Record and
token ceilings remove whole older exchanges; an oversized newest exchange keeps
its user/model boundary while tool detail is reduced. Invalid or stale derived
artifacts fall back to canonical recent history.

## Model-Window Policy

- Model context length comes from the exact ready backend when one is active;
  otherwise it comes from the selected, validated manifest.
- No missing context value silently becomes 4,096.
- Prompt output reserve is explicit; runtime reserve scales from 512 to 4,096
  tokens.
- Typed-memory target is one eighth of available input, capped at 8,192 tokens.
- Query-recall target is one quarter, capped at 32,768 tokens.
- Recent-dialogue target is one quarter, capped at 16,384 tokens.
- Resume transcript budget is one eighth of the model window, bounded to
  512–16,384 estimated tokens and 8–64 turns.
- Automatic compaction starts at 75% measured usage and targets 40% of the model
  window.
- The retained compaction tail scales across 2–8 complete exchanges and
  512–16,384 estimated tokens.
- Repository source retrieval deliberately remains a smaller shared budget:
  4 pointers and 3,200 characters.

These caps let a large-context model use more of its capability without making
small-model prompts noisy or expensive.

## Safety Invariants

1. Only completed user/assistant pairs are restored or recalled.
2. The current user request is the final prompt data section.
3. Historical memory and attachments are explicitly untrusted.
4. Compaction never rewrites or deletes the canonical transcript.
5. Derived state cannot authorize commands, file changes, or factual claims.
6. Runtime readiness requires an exact desired-versus-observed specification.
7. Missing manifest context is an actionable error, not a guessed default.
8. No hidden reasoning is persisted or presented as memory.

## Acceptance Evidence

- Restart restores completed pairs and excludes incomplete tails.
- `/clear` hides earlier dialogue while preserving the audit stream.
- Coding and agent workflow answers survive restart in the canonical
  conversation while retaining their separate workflow audit records.
- Repeated questions across multiple reset boundaries keep unique causal
  records, and reset never pairs an earlier orphan user with a later model turn.
- Typed memory and query recall preserve complete pairs and chronology.
- Prompt assembly stays within the declared model input budget and ends with
  the current request.
- Agent/workflow prompt assembly also stays within a 1,024-token active runtime
  window with maximal resume and repository inputs.
- 4K and 131K manifests produce different resume and compaction limits.
- A ready backend with the wrong context or projector is restarted.
- Compaction retains complete bounded exchanges under token, record, and
  artifact ceilings.
- TUI, context, recall, compaction, backend reconciliation, and architecture
  contract tests pass for the candidate branch.

## Future Extension Points

- Replace deterministic ranking behind the recall policy interface only after
  local measurements show a quality gain that justifies the cost.
- Add typed memory categories without changing canonical transcript storage.
- Add tokenizer-specific accounting behind the budget estimator while keeping
  the same fail-closed model-window contract.
- Add user-visible memory inspection and selective forgetting as explicit
  canonical events, never silent transcript mutation.
