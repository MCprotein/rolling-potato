# Release Notes

This index keeps the current release train concise. Historical notes are preserved verbatim in bounded archives.

## Archive

- [v0.40–v0.51](docs/releases/v0.40-v0.51.md)
- [v0.38–v0.39](docs/releases/v0.38-v0.39.md)
- [v0.37](docs/releases/v0.37.md)
- [v0.30–v0.36](docs/releases/v0.30-v0.36.md)
- [v0.20–v0.29](docs/releases/v0.20-v0.29.md)
- [v0.10–v0.19](docs/releases/v0.10-v0.19.md)
- [v0.1–v0.9](docs/releases/v0.1-v0.9.md)

## v0.55.0 - Model-Aware Generation and Reliable Terminal Journeys

Release date: 2026-08-03

This minor release combines the unpublished v0.54.0 model-aware generation
milestone with the v0.55.0 terminal-parity work. It removes small fixed product
generation caps and hardens the complete default TUI journey without claiming
equivalence to the reference products.

### Included

- Derives generation, attachment, compaction, web, agent, and TUI budgets from
  the active model context and remaining capacity instead of fixed product caps.
- Treats `length` and unknown generation finishes as incomplete, while accepting
  only a normal stop as a complete model answer.
- Preserves valid typed model decisions and uses bounded recovery only for
  malformed small-model output.
- Persists typed web activity across follow-ups and `/resume` without replaying
  uncertain prior work.
- Runs bounded, cancellable Search→Open→Find→Answer research with ordered source
  evidence and a fixed-capacity managed network worker pool.
- Shows request phase and elapsed time, supports active `Ctrl+C` cancellation,
  renders richer Markdown, preserves composer drafts while scrolling, and keeps
  failed-request attachments recoverable.
- Separates cached language-model artifacts from lazy vision-projector readiness
  and verifies the projector bound to the active backend.
- Adds native-terminal regression journeys and a separately opt-in managed
  real-model smoke.

### Compatibility and boundaries

- Existing commands, stored sessions, model caches, and GitHub-only distribution
  remain compatible.
- The default CI proves deterministic production-boundary behavior; the opt-in
  real-model smoke is intentionally narrow and does not establish broad model
  quality, hardware compatibility, or parity with Codex, Claude Code, or OpenCode.
- Unrestricted tool orchestration, remote plugin sources, MCP, and external app
  integration remain outside the supported product boundary.

## v0.53.0 - Bounded Runtime Architecture and Autonomous Grounding

Release date: 2026-07-29

This minor release completes a repository-wide responsibility refactor and
makes freshness-sensitive questions enter runtime-owned web grounding before a
small local model can answer from stale memory. The public CLI and TUI command
surface remains compatible.

### Included

- Replaces named product, event, and answer-shape exceptions with generic
  freshness features, typed query plans, evidence coverage, and answer binding.
- Keeps local repository questions and explicit no-web requests on the offline
  path while routing volatile facts, current versions, and empirical
  comparisons through bounded `WebSearch`, `WebOpen`, and `WebFind`.
- Preserves runtime source attribution in generated and fallback grounded
  answers and rejects stale, conflicting, or unrelated evidence.
- Answers current-model identity questions from runtime configuration without
  intercepting model comparison or recommendation requests.
- Keeps ordinary active conversations tolerant of stale historical source
  pointers while explicit `/resume` continues to report invalid resume
  artifacts instead of silently discarding them.
- Splits large TUI, context, model, backend, install, workflow, patch,
  collaboration, web, CLI, persistence, terminal, and test owners into bounded
  modules with explicit dependency and migration contracts.
- Splits historical release notes and execution retrospectives into bounded
  archives while retaining machine-verifiable ownership coverage.

### Compatibility and boundaries

- Existing public commands, stored sessions, model artifacts, lazy projectors,
  managed backend behavior, and GitHub-only distribution remain compatible.
- Web evidence remains bounded, read-only, untrusted input and does not grant
  browser, shell, filesystem-write, credential, or approval authority.
- The refactor does not claim new model quality, benchmark, memory, or hardware
  results.

## v0.52.1 - Managed llama.cpp Structured-Chat Recovery

Release date: 2026-07-29

This patch restores the default TUI conversation path on the managed
`llama.cpp` backend. The v0.52.0 structured answer schema requested a grammar
repetition larger than the pinned compiler accepts, causing even a simple
greeting to fail with HTTP 400 before generation.

### Included

- Removes the grammar-level `answer.maxLength` repetition while preserving the
  runtime's independent 16 KiB visible-answer validation.
- Rejects unsupported string, array, and object repetition bounds before a
  generation lifecycle starts or a request reaches the backend.
- Traverses only JSON Schema subschema positions so output properties named
  `maxLength`, `minItems`, or similar keywords remain valid.
- Covers the verified 1,999/2,000 managed-grammar boundary, property-name
  collisions, the production turn schema, and the native structured TUI
  request.
- Requires a real pinned `llama-server` and installed supported model smoke
  whenever request-body, response-format, schema, or chat-template contracts
  change.

### Compatibility and boundaries

- Existing models, sessions, projectors, web tools, browser restrictions, and
  public commands remain compatible.
- The application still rejects visible structured answers larger than 16 KiB;
  only the incompatible grammar repetition was removed.
- GitHub Releases remains the only supported binary distribution channel.

## v0.52.0 - Structured Small-Model Tool Turns

Release date: 2026-07-28

This minor release replaces free-form web-tool instructions with a
schema-constrained decision and observation loop designed for small local
models. The runtime, rather than model text, now owns the transition from a tool
request through verified evidence to the final visible answer.

### Included

- Requires one bounded JSON-schema decision per conversational model turn:
  `Answer`, `WebSearch`, `WebOpen`, `WebFind`, or `ContinueLocal`.
- Executes `WebSearch`, `WebOpen`, and `WebFind` through the same
  `ToolCall → Observation → Answer` lifecycle and never displays a raw tool
  report as the final model answer.
- Generates the final answer only after a separate model call receives the
  runtime-owned, bounded observation.
- Builds follow-up search queries from recent user requests while excluding
  model responses, attachments, credentials, private values, and unrelated
  prior topics.
- Treats malformed structured decisions as a safe local continuation instead
  of exposing private protocol text.
- Distinguishes general analysis questions from repository inspection requests
  unless the request names a repository, project, source, file, or code scope.
- Separates search, open, find, query sanitation, and runtime request support
  into bounded owners protected by architecture and native-terminal tests.
- Prevents PR label and ready-state events from canceling one another while
  retaining same-event supersession for newer candidate commits.

### Compatibility and boundaries

- Existing models, sessions, projectors, web grounding, browser restrictions,
  and public commands remain compatible.
- Web observations are read-only, bounded, untrusted evidence. They cannot
  grant browser, shell, filesystem-write, credential, or approval authority.
- GitHub Releases remains the only supported binary distribution channel.
