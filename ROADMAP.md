# Roadmap

This roadmap turns `rolling-potato` into a local coding-agent runtime for small models, not just a prompt harness.

Reference documents reviewed:

- `/Users/sys/Desktop/codes/harness/HARNESS-OPERATING-MODELS.md`
- `/Users/sys/Desktop/codes/anamnesis/docs/ONTOLOGY-BOOTSTRAP.md`
- `/Users/sys/Desktop/codes/anamnesis/docs/CONTEXT-INDEX-DESIGN.md`
- `/Users/sys/Desktop/codes/anamnesis/docs/RUNTIME-EVIDENCE.md`
- `/Users/sys/Desktop/codes/anamnesis/docs/README-CLAIMS.md`

Principles reflected in the roadmap:

- Do not assume the model becomes smarter.
- Runtime owns instruction, permission, state, ledger, transcript, and evidence gates.
- Ontology is project-memory substrate that small models should not reconstruct from scratch every turn.
- The canonical ontology is an internal typed graph/store, not a YAML prompt file. Human-readable YAML/JSON/RDF/OWL views are import/export or inspection surfaces unless a later interoperability requirement proves otherwise.
- Split Layer A facts that runtime can prove from Layer B semantics that agents/runtime may enrich only with source references.
- Context is supplied through source pointers, stable refs, digests, and resume bundles instead of unbounded prompt dumps.
- Snippets are not authoritative source. Important decisions require rereading the original file.
- Finish small vertical slices instead of giving small models excessive freedom.
- Completion is decided by verification evidence and the stop gate, not by model confidence.
- Keep current-state views separate from append-only ledgers.
- Cross-store state authority, write ordering, replay, and recovery are explicit runtime contracts.
- Use SQLite as monitoring/query projection while keeping the append-only ledger as the audit trail.
- Store session identity and queryable session history in the local DB so users can resume from history, not only from the latest pointer.
- Mode transitions are deterministic runtime-state transitions, not spontaneous model decisions.
- Hooks, skills, subagents, team runtime, and TUI are first-class capabilities for a replacement-level runtime.
- Claude Code/Codex-style plugins are not executed directly; they are imported, validated, and enabled through `rpotato` capability adapters.
- Implement the Codex plugin adapter before the Claude Code adapter.
- External plugin shell commands, `bin/`, MCP servers, background processes, remote connectors, and file-write paths are blocked by default and require explicit approval when enabled.
- Do not integrate external marketplaces, registries, catalogs, or mirrors.
- Plugins are imported only from local directories the user already has.
- Monitoring is shown first through TUI for SSH/Linux-server use; HTML is a later optional local report/dashboard.
- Compaction, resume, cancel, and corrupt-state fallback are part of the early runtime design.
- Public claims and model claims must not exceed available evidence.
- The LLM wiki is a model knowledge/evidence index, not an independent truth source or automatic model recommendation engine.

## Phase 0: Project Definition

- [x] Product positioning
- [x] MVP acceptance criteria
- [x] Rust + `llama.cpp` sidecar direction
- [x] Korean-only user-facing output requirement
- [x] Open-source operating documents
- [x] Model source policy
- [x] Harness operating model review
- [x] `anamnesis` ontology/context lifecycle review
- [x] Runtime surface/core/backend boundary documentation
- [x] Required hooks/skills/subagents/team/TUI capabilities reflected
- [x] Plugin adapter boundary documented
- [x] Codex-first, Claude Code-second plugin adapter priority reflected
- [x] No-marketplace and local-directory-only plugin import policy reflected

## Phase 1: Runtime Entrypoint And CLI Surface

Goal: make `rpotato` a stable first surface while the runtime core owns state, policy, and error boundaries.

- [x] Rust workspace scaffold
- [x] `rpotato` command router
- [x] CLI surface and runtime core module boundary
- [x] `rpotato init`
- [x] `rpotato doctor`
- [x] `rpotato config`
- [x] `rpotato state`
- [x] `rpotato cancel`
- [x] `rpotato cache status`
- [x] `rpotato monitor status`
- [x] `rpotato monitor models`
- [x] `rpotato uninstall --keep-cache`
- [x] `rpotato uninstall --purge-cache`
- [x] Config path decision
- [x] App data/cache/project state boundary decision
- [x] Structured Korean error reporting
- [x] Command output style guide
- [x] Operation log path decision
- [x] Setup/upgrade/uninstall boundary decision
- [x] Uninstall dry-run path listing
- [x] Basic command smoke tests

## Phase 2: Runtime State, Ledger, And Observability

Goal: restore and query current workflow state, audit history, and per-model monitoring metrics across sessions.

- [x] Local state root layout
- [x] Repo-root state and session-scoped state priority
- [x] Project/session identity
- [x] SQLite-backed session history query
- [x] Session history list/resume CLI surface
- [x] Current-state persistence for selected resume session
- [x] Workflow manifest and transition table
- [x] Terminal state definitions
- [x] State read/write/cancel API
- [x] Append-only ledger format
- [x] Ledger replay
- [x] Session transcript JSONL format
- [x] Transcript parent/branch pointer
- [x] Transcript metadata entry
- [x] Operation log format
- [x] State owner rules
- [x] Stale state reconcile
- [x] Corrupt state fallback
- [x] Checkpoint record schema
- [x] Evidence record schema
- [x] Runtime evidence JSONL store
- [x] Evidence stale criteria
- [x] Evidence artifact pointer validation
- [x] Current-state view and event/evidence ledger boundary
- [x] Local SQLite observability store decision
- [x] SQLite schema migration
- [x] Ledger-to-SQLite projection
- [x] Token usage record schema
- [x] Model run metric schema
- [x] Backend health metric schema
- [x] Tool/command metric schema
- [x] Guard/stop-gate metric schema
- [x] Raw prompt/source text not stored by default
- [x] Credential redaction before persistence
- [x] Corrupt SQLite fallback
- [x] Monitoring export: JSONL/CSV
- [x] Monitoring retention/prune dry-run
- [x] Interrupted run resume behavior
- [x] Compaction boundary marker
- [x] Compacted summary preservation policy
- [ ] Cross-store state authority matrix: ledger, SQLite, current-state, ontology, model knowledge, plugin registry, evidence
- [ ] Runtime write-ordering contract: policy validation, stable event id, ledger append, state mutation, projection, evidence, diagnostics
- [ ] Idempotent replay and partial-write recovery tests
- [ ] Retention/deletion matrix by ledger, SQLite, evidence, transcripts, logs, exports, model knowledge, and plugin data
- [ ] Routing decision record schema for model, skill, mode, ontology view, backend, subagent/team lane, and escalation choices
- [ ] Retry/failure handling matrix for transient, permanent, policy, quota, parse, projection, backend, and permission failures

## Phase 3: Mode And Constraint Recognition

Goal: normalize mode, constraints, and completion criteria before user input reaches the model.

- [x] Explicit invocation priority
- [x] Skill invocation grammar
- [x] `rpotato skill list`
- [x] `rpotato skill run <id>`
- [x] `rpotato plugin import --from codex <local-path>`
- [x] `rpotato plugin import --from claude-code <local-path>`
- [x] `rpotato plugin import --dry-run`
- [x] `rpotato plugin list`
- [x] `rpotato plugin inspect <id>`
- [x] `rpotato plugin validate <id>`
- [x] `rpotato plugin enable <id>`
- [x] `rpotato plugin disable <id>`
- [x] `rpotato plugin remove <id> --keep-data`
- [x] `rpotato plugin remove <id> --purge-data`
- [x] Source runtime namespace rule: native, codex, claude-code
- [x] Active workflow ownership rule
- [x] Deterministic keyword/phrase rule table
- [x] Structural signal extraction: read-only, plan-only, review-only, test-spec
- [x] Optional classifier usage criteria
- [x] User constraint record schema
- [x] Repo instruction loading boundary
- [x] Generated artifact requirement extraction
- [x] Nested/subagent prompt keyword non-activation
- [x] TUI command palette routing
- [x] Intent fixture test

## Phase 4: Permission And Tool Boundary

Goal: control file writes, command execution, and downloads through runtime policy instead of raw model output.

- [x] Tool/action schema
- [x] Hook/control-point input/output schema
- [x] Hook registry
- [x] Lifecycle hooks: session_start, user_request_received, pre_context_pack, post_context_pack
- [x] Lifecycle hooks: pre_model_request, post_model_response, pre_action_parse, post_action_parse
- [x] Lifecycle hooks: pre_tool_call, post_tool_result, pre_patch_apply, post_patch_apply
- [x] Lifecycle hooks: pre_command_run, post_command_run, pre_final_report, stop_gate, session_end
- [x] Hook ordering: runtime, project, skill, session, observer
- [x] Hook conflict rule: deny > ask > modify > allow > observe
- [x] Project boundary checker
- [x] File read allow/exclude rules
- [x] Rule sources: user, project, local, session, policy
- [x] Allow/ask/deny decision model
- [x] Diff-before-write gate
- [x] Managed artifact manifest/hash tracking
- [x] `create`/`update`/`noop`/`user-modified`/`blocked` action status
- [x] User-modified file/region preservation rule
- [x] Command classifier
- [x] Command approval prompt
- [x] Destructive command deny/high-confirm policy
- [x] Credential redaction in logs
- [x] Network/download approval policy
- [x] Permission decision audit record
- [x] Foreign plugin import permission report
- [x] Foreign plugin shell/background-process approval gate
- [x] Foreign plugin local path canonicalization
- [x] Foreign plugin symlink boundary check
- [x] Plugin import/enable/remove ledger record
- [x] Policy fixture test
- [x] Hook JSON input/output fixture test
- [x] Hook fail-closed fixture test

## Phase 5: Model Manifest And Install

Goal: install and register only source- and checksum-verified model candidates.

- [x] Source-backed model manifest schema
- [x] Model candidate states: `candidate`, `unverified`, `verified`
- [x] Official model card/source/license research per candidate
- [x] Public benchmark claim/source ledger per candidate
- [x] Public benchmark reproducibility review: harness, dataset, prompt, scoring, hardware/backend conditions
- [ ] Model knowledge base schema: evidence index over manifest, benchmark, observability, and ontology records
- [ ] Model knowledge claim subject taxonomy: artifact, license, public benchmark, local benchmark, runtime observation, routing note, ontology-view observation
- [ ] Model knowledge state namespace separation from ontology, manifest, and benchmark states
- [ ] Model knowledge automatic observation policy: frequency creates observed/candidate notes, not confirmed claims
- [ ] Model knowledge frequency validity fields: sample count, success/failure count, time window, condition key, reset reason
- [ ] Model knowledge drift/reset criteria for manifest, backend, prompt compiler, tool policy, ontology view, artifact, scoring, and fixture changes
- [ ] Model knowledge safety tests: no license/default-model/RAM-fit claims from frequency alone
- [x] `rpotato model list`
- [ ] `rpotato model knowledge`
- [ ] `rpotato model knowledge inspect <model-id>`
- [ ] `rpotato model knowledge promote <entry-id> --dry-run`
- [ ] `rpotato model knowledge prune --before <duration> --dry-run`
- [ ] `rpotato model install <id>`
- [x] Block unverified artifact install
- [ ] Resumable model download
- [x] SHA-256 verification
- [x] Pre-download license/source display
- [x] Failed artifact cleanup
- [x] Local model registry
- [x] Manifest test

## Phase 6: Backend Runtime

Goal: let the runtime core manage the `llama.cpp` sidecar and expose backend diagnostics through the CLI surface.

- [x] `llama.cpp` sidecar discovery
- [x] Source-backed `llama.cpp` release artifact manifest
- [x] Managed backend binary download/install
- [x] Backend archive checksum verification
- [x] Backend binary version detection
- [x] Backend binary path config
- [x] Port selection
- [ ] Sidecar process lifecycle
- [x] Health check
- [ ] Startup timeout handling
- [ ] Streaming response path
- [ ] Cancellation path
- [ ] stderr/stdout capture
- [x] Backend diagnostics in `doctor`
- [x] Backend diagnostics in `rpotato backend doctor`
- [ ] Managed backend removal during uninstall
- [x] Backend adapter trait

## Phase 7: Tool Runtime

Goal: put shell, file, patch, and verifier execution behind consistent schemas and result formats.

- [ ] File read tool contract
- [ ] Patch apply tool contract
- [ ] Shell command tool contract
- [ ] PTY/non-PTY execution criteria
- [ ] Streaming output handling
- [ ] Command cancellation
- [ ] Output truncation and artifact spill
- [ ] Tool result shaping
- [ ] Command failure classification
- [ ] Unattended environment hardening

## Phase 8: Instruction, Ontology, And Context Plane

Goal: make the runtime assemble instructions, ontology, context, and output shape instead of improvising them in every prompt.

- [ ] Prompt compiler
- [ ] Skill manifest schema
- [ ] Skill registry
- [ ] Normalized plugin manifest schema
- [ ] Foreign plugin parser: Codex `.codex-plugin/plugin.json`
- [ ] Codex plugin inspect/validate dry-run
- [ ] Codex skill import
- [ ] Codex MCP import with default disabled server command
- [ ] Foreign plugin parser: Claude Code `.claude-plugin/plugin.json`
- [ ] Claude Code plugin inspect/validate dry-run
- [ ] Claude Code skill/command import
- [ ] Claude Code agent import as subagent role
- [ ] Claude Code hook import
- [ ] Claude Code LSP/monitor/bin/settings/theme import policy
- [ ] Plugin capability mapping: skill, hook, subagent, MCP, unsupported
- [x] Plugin import dry-run report
- [ ] Plugin enable/disable scope policy
- [ ] Plugin source snapshot and manifest hash record
- [ ] Plugin data path separation
- [ ] Unsupported plugin capability ledger record
- [ ] Default-block policy for shell/bin/MCP/background/remote/file-write capability
- [ ] Plugin permission lease and revalidation on manifest, policy, adapter, source, or capability-scope changes
- [x] Reject remote plugin URL
- [x] Reject plugin marketplace source
- [x] Reject plugin registry/catalog source
- [ ] Local plugin directory path traversal test
- [ ] Skill context requirements
- [ ] Skill allowed tools
- [ ] Skill evidence requirements
- [ ] Skill stop criteria
- [ ] Role templates: planner, executor, verifier, reporter
- [ ] Korean final-response instruction
- [ ] Structured action output format
- [ ] Project ontology root layout
- [ ] Runtime ontology canonical store decision: typed graph records, not YAML-as-source-of-truth
- [ ] Ontology graph schema: entities, relationships, flows, invariants, ownership, decisions, open_questions, source_refs
- [ ] Ontology claim-state contract: confirmed, proposed, weak, superseded, rejected, open_question
- [ ] Invariant schema and enforcement metadata: scope, severity, must/must_not, enforcement mode, exception policy
- [ ] Ontology persistence schema and indexes for graph traversal, source lookup, status/confidence filtering, and TUI/report queries
- [ ] Ontology ledger events for create/update/supersede/reject/confirm and replay into query projection
- [ ] Ontology import/export serializers for JSON/YAML fixtures and optional future JSON-LD/RDF views
- [ ] Ontology serialization round-trip gate for stable IDs, source refs, claim state, supersedes, drift/conflict, and invariant metadata
- [ ] Small-model ontology view candidates: compact graph summary, JSON slice, triple list, optional RDF/OWL/JSON-LD export, no-ontology baseline
- [ ] Layer A deterministic repo facts schema
- [ ] Layer A fact generators: files, package/build/test signals, symbols, entrypoints
- [ ] Layer A fact freshness/hash tracking
- [ ] Layer B semantic ontology schema
- [ ] Layer B source/ref/confidence requirements
- [ ] Layer B merge/supersede/open-question lifecycle
- [ ] Reviewed semantic claim preservation: regenerated Layer A facts must not overwrite Layer B meaning
- [ ] Ontology gap diagnostics in `doctor`
- [ ] Ontology drift detection
- [ ] Ontology conflict diagnostics: duplicate IDs, contradictory relationships, stale source hashes, superseded entries treated as current
- [ ] Source-backed ontology claim rule
- [ ] Repository file discovery
- [ ] Context index JSONL: source_path, source_hash, stable_ref, snippet, freshness
- [ ] Context query contract
- [ ] Source-pointer-first retrieval rule
- [ ] Compact resume bundle
- [ ] Context packing budget
- [ ] Generated/vendor exclusion rules
- [ ] Command/log summarization input format
- [ ] Static ontology vs Layer A vs Layer B context fixture
- [ ] Prompt fixture test

## Phase 9: First Agent Vertical Slice

Goal: finish read, plan, patch proposal, approval, application, verification, and Korean reporting in one small fixture repository.

- [ ] `rpotato run "<task>"`
- [ ] Agent-loop transcript replay from selected session history
- [ ] Conversation continuation after `rpotato resume <session-id>`
- [ ] Run startup ontology/context retrieval
- [ ] Step that promotes source pointers to original-file reads
- [ ] Planner step
- [ ] Executor step
- [ ] Verifier step
- [ ] Reporter step
- [ ] Unified diff rendering
- [ ] Approved patch apply
- [ ] Patch apply rollback/failure handling
- [ ] Approved verification command execution
- [ ] Verification output interpretation
- [ ] Final Korean report

## Phase 10: Stop Gate And Reliability Gates

Goal: require evidence for completion instead of accepting the model's claim that the task is done.

- [ ] Stop gate completion contract
- [ ] Required evidence by command
- [ ] Ontology completeness gate
- [ ] Context source-read evidence gate
- [ ] Source-backed public claim ledger
- [ ] README/model/benchmark claim gate
- [ ] Pending action detection
- [ ] Continue after verification failure
- [ ] Validation gap record
- [ ] Invalid diff rejection
- [ ] Korean output guard
- [ ] Mixed-language regeneration
- [ ] Fail-closed Korean error
- [ ] Cancel cleanup
- [ ] Stale workflow terminal handling
- [ ] Destructive command policy tests
- [ ] Fixture benchmark suite
- [ ] Benchmark fixture metadata contract: runtime capability, model/runtime responsibility, expected route, policy decision, escalation target, required evidence
- [ ] Benchmark failure taxonomy: model, prompt/context, ontology/source-pointer, runtime policy/parser, tool/command, backend/runtime, fixture issue
- [ ] Regression fixture promotion policy from real unsafe actions, source-read omissions, stale-claim use, policy violations, and score regressions
- [ ] Regression fixture promotion gate: redaction, minimal repro, quarantine, owner review, supersede/demote reason
- [ ] Benchmark reliability controls: run count, warm/cold split, variance threshold, flaky quarantine, environment drift detection
- [ ] Benchmark privacy/redaction adversarial fixtures for secrets in logs, commands, paths, prompts, exports, and regression records
- [ ] Benchmark reproducibility manifest: harness version, fixture checksum, runner command, run count, seed policy, sampling limits, OS power/thermal note
- [ ] Static-only vs Layer A vs Layer B ontology benchmark
- [ ] 2B-4B ontology representation benchmark: view format vs task score, hallucination, source-read compliance, latency, memory, token budget
- [ ] Small-model abstention/escalation benchmark: evidence gaps, invariant risk, stale claims, context exhaustion, repeated invalid output
- [ ] Resume/compaction stale-claim benchmark with required source reread
- [ ] Benchmark result claim states: measured-locally, source-listed-unreproduced, not-comparable, rejected, superseded
- [ ] Per-model product benchmark suite
- [ ] Per-model public benchmark parity suite
- [ ] Public benchmark comparison report: published vs local, condition differences, unreproducible reasons
- [ ] Fake session lifecycle test: submit -> tool -> stop
- [ ] Corrupt state fallback test
- [ ] Resume/cancel E2E test
- [ ] Regression test report format

## Phase 11: Subagents, Team Runtime, And TUI Surface

Goal: implement the interactive surface and bounded multi-agent execution required for a Claude Code/Codex replacement-level runtime.

- [ ] Subagent role schema
- [ ] Subagent task slice contract
- [ ] Subagent allowed tools/path/context boundary
- [ ] Subagent lifecycle: start, complete, blocked, failed, cancelled
- [ ] Subagent resource admission control: memory, backend health, token/context budget, file ownership, tool risk, approval queue
- [ ] Parent cancellation propagation
- [ ] Shared file conflict detection
- [ ] Subagent evidence merge
- [ ] Team manifest schema
- [ ] Team pipeline: plan, dispatch, exec, review, verify, merge, report
- [ ] Team write policy: single writer per file
- [ ] Team merge policy: runtime-owned merge
- [ ] Team stage ledger
- [ ] Team resource admission control and sequential fallback
- [ ] Failed worker continuation policy
- [ ] `rpotato team status`
- [ ] TUI framework decision
- [ ] `rpotato tui`
- [ ] TUI monitoring UX spec from `DESIGN.md`
- [ ] TUI transcript/session view
- [ ] TUI plan/context/ontology view
- [ ] TUI approval queue
- [ ] TUI diff viewer
- [ ] TUI tool output viewer
- [ ] TUI subagent/team status view
- [ ] TUI model/token monitoring view
- [ ] TUI model knowledge panel
- [ ] TUI evidence/stop gate view
- [ ] TUI plugin permission review view
- [ ] Optional local HTML report/dashboard decision
- [ ] TUI cancel/resume controls
- [ ] TUI small terminal smoke test
- [ ] TUI policy-bypass regression test

## Phase 12: Packaging And Release

Goal: let users install the runtime surface without model weights and safely complete the first model install.

- [ ] Release build pipeline
- [x] Release version bump policy
- [x] Release branch and tag enforcement policy
- [x] Release policy CI check
- [ ] Binary checksum generation
- [ ] macOS Apple Silicon artifact
- [ ] macOS Intel artifact
- [ ] Windows x86_64 artifact
- [ ] Release notes template
- [ ] `rpotato doctor` release smoke test
- [ ] Uninstall keep-cache/purge-cache smoke test
- [ ] Plugin local-import rejection smoke test: remote URL, marketplace, registry, catalog
- [ ] GitHub Releases distribution
- [ ] Post-MVP Homebrew/Scoop decision

## Out Of Scope Before MVP

- GUI app
- Unbounded parallel agents without runtime ownership
- Multiple models loaded simultaneously
- Remote GPU/server mode as the default
- MCP server ecosystem
- All MCP transports
- Remote bridge
- IDE/Desktop/Web surface
- Direct execution of external runtime plugins
- External plugin marketplace integration
- External plugin registry/catalog integration
- Plugin package mirrors without confirmed license permission
- Public leaderboard score treated as product benchmark result
- Repeated runtime success treated as default-model approval
- Unofficial Claude Code/Codex compatibility claims
- Automatic destructive command execution
- External code PR workflow
- Model recommendations without sources
- Exhaustive framework ontology parser
- Semantic ontology claims without sources
- YAML/JSON/RDF/OWL ontology exports treated as canonical runtime truth
- Snippet-based automatic edits without original-file confirmation
