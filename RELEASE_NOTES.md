# Release Notes

## v0.34.2 - Windows ConPTY Release Recovery

Release date: 2026-07-16

This patch release supersedes the incomplete v0.34.1 binary publication. The v0.34.1
source tag remains immutable, but its Windows native-terminal gate exposed two test-host
lifecycle defects before the Windows archive could be built.

### Fixed

- Keeps the ConPTY-side pipe handles alive until the first attached client is created,
  following the Windows pseudoconsole startup contract so the initial console-mode probe
  receives valid standard handles.
- Closes the host output pipe before `ClosePseudoConsole` during fixture cleanup, avoiding
  the documented deadlock risk on Windows versions where pseudoconsole close waits.
- Adds a manually dispatched Windows native-terminal workflow with separate five-minute
  bounds for the entry/EOF lifecycle and full adapter tests.

### Release Recovery

- v0.34.0 and v0.34.1 remain immutable source history, but their partial binary assets are
  not supported release sets.
- v0.34.2 is the supported replacement and must publish the verified exact 11-asset set.

## v0.34.1 - Portable Release Recovery

Release date: 2026-07-16

This patch release supersedes the incomplete v0.34.0 binary publication. The v0.34.0
source tag remains immutable, but its release workflow could not publish the exact
11-asset set because the Windows build used unstable metadata APIs and the Linux ARM64
source-recovery path used Linux x86-specific `openat` flag values.

### Fixed

- Replaces unstable Windows `MetadataExt` file identifiers with stable
  `GetFileInformationByHandle` volume/file identity checks, preserving the existing
  path-versus-open-handle fail-closed contract without adding a dependency.
- Restricts Unix-only source-recovery tests to Unix so Windows test binaries compile.
- Avoids unsupported transition-parent directory opens on Windows, matching the existing
  Windows atomic-replace durability path.
- Uses the architecture-correct Linux ARM64 values for `O_DIRECTORY` and `O_NOFOLLOW`,
  preserving fail-closed descriptor-relative source traversal on both Linux architectures.
- Adds a native Windows file-identity test to the release matrix.

### Release Recovery

- v0.34.0 remains available as immutable source history, but its partial binary assets
  are not the supported release set.
- v0.34.1 remains immutable source history, but its partial binary assets are not a
  supported release set. v0.34.2 supersedes it.

## v0.34.0 - Runtime-Owned Interactive TUI

Release date: 2026-07-16

This release turns the dependency-free terminal surface into a real interactive
controller while keeping all authority in the runtime core. It also makes an approved
built-in-skill patch one recoverable, exact prepared transaction instead of a sequence of
loosely coupled writes.

### Included

- Adds attached-terminal auto-selection through `rpotato tui` and an explicit
  `rpotato tui interactive` line controller with canonical views, paging, selection,
  patch/verification approval, pending-gate denial, workflow resume, and cancellation.
- Uses runtime-owned selection leases, intent IDs, confirmations, no-echo one-shot secret
  input, and a closed 27-row outcome table. Terminal output escapes ANSI/OSC/control
  bytes, respects live terminal dimensions, restores echo before SIGINT/SIGTERM or
  Windows console termination, and distinguishes pre-dispatch frame failure from
  post-commit display failure.
- Binds every mutating TUI lease to the selected active workflow and every approval/diff
  read to that workflow's bounded canonical proposal. Cross-project tool artifacts,
  tampered or oversized resume proposals, and replaced live lock-file inodes fail closed;
  a kernel-locked per-lock owner namespace plus claim prevents a replacement inode from admitting a second writer.
- Commits successful built-in-skill patch approval as one immutable exact-11 bundle and
  appends the preplanned E0-E9 event chain at its physical effect boundaries. Workflow
  R+1/R+2 snapshots and the shared pointer, one current-state C+1 image, sanitized
  no-stream tool output, TranscriptRecord v2, rollback/source data, and projection lag
  metadata are hash- and identity-bound.
- Recovers every T1-T10 interruption from stored journal bytes without duplicate events
  or revisions, never downgrades an installed R+2 pointer, and returns a refresh-only
  receipt for a repeated committed intent. Projection repair preserves an exact E9 lag
  marker until project ledger, operation log, and SQLite converge in order.
- Promotes current-state to strict canonical schema v2 with revision/hash/ledger/workflow
  bindings and preserves validated schema-v1 promotion. Runtime ledger remains the sole
  event authority; project and SQLite records are rebuildable projections.
- Moves release-branch cleanup behind successful verification of the published exact
  11-asset set. Tag-policy and failed/cancelled release paths preserve the branch, while
  failure diagnostics report the actual failed job result and always distinguish a
  preserved, missing, or unverifiable remote branch.

### Known Limits

- Approved source installation succeeds only on Unix in v0.34.0. Unsupported platforms
  block before journal commitment and before any target effect.
- Interaction is line-oriented rather than a raw-key/full-screen protocol.
- A concurrent external writer that starts after final pathname validation and wins the
  validate-to-unlink race is outside the supported guarantee. Observable conflicts fail
  closed; that unobservable interval is not claimed to be atomic.
- Secret values are never replayed. A repeated committed intent returns only a
  refresh-only receipt.

### Verified During Implementation

- `cargo fmt --all -- --check`
- `cargo test --locked -- --test-threads=1`
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo build --release --locked`
- release policy, toolchain pin, target matrix, exact release-asset fixtures,
  packaged-binary, and uninstall smoke checks

## v0.33.0 - Executable Hooks And Skills

Release date: 2026-07-13

This release moves built-in skills and runtime-owned lifecycle hooks from
validation/routing foundations into the durable agent loop. Policy, evidence,
and stop criteria are now executable state-machine gates rather than prompt
conventions.

### Included

- Dispatches the 17 lifecycle hook points in deterministic runtime, project,
  skill, session, and observer layer order, then by hook id. Strict JSON parsing
  fails closed and the conflict rule keeps the stricter result.
- Executes explicit `rpotato skill run <id> "<request>"` and naturally routed
  built-in skills through the same context, model, typed-action, approval,
  verification, final-report, and stop-gate loop.
- Enforces declared context, allowed tools, required hooks, evidence, and stop
  criteria at state transitions. Missing context fails before a model request;
  incomplete terminal state cannot be accepted.
- Requires workflow phase and skill state to agree before every patch or command
  side effect. `fix-test` runs the same canonical `cargo test` command before and
  after an approved patch, binding the observed pre-patch failure to workflow and
  command hashes in the ledger.
- Rejects empty or non-Korean read-only model answers and derives completion
  evidence only from source, line, diagnostic, benchmark, checksum, or ranked
  finding details visible in the guarded answer.
- Adds workflow schema v4 for active skill, invocation, skill state, completed
  hooks, evidence, and stop criteria while preserving immutable v2/v3 artifacts
  and one-way upgrade compatibility. SQLite projects the actual active skill
  from canonical checkpoint data.
- Records hook dispatch evidence using payload SHA-256 values rather than raw
  payloads. Direct command and project-file writes are not hook capabilities.
  Imported/plugin hook executables remain disabled and default-deny.

### Breaking Before 1.0

- `rpotato skill run` now requires both a built-in skill id and a request. The
  previous id-only command only recorded pre-execution routing state and did not
  have enough input to execute the agent loop.

### Verified During Implementation

- `cargo fmt --all -- --check`
- `cargo test --locked -- --test-threads=1` (`351` unit, `1` backend
  lifecycle, and `28` process tests passed)
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo build --release --locked`
- release policy, toolchain pin, target matrix, packaged-binary, and uninstall
  smoke checks

## v0.32.1 - Stable Toolchain Refresh

Release date: 2026-07-13

This maintenance release moves every repository-managed active tool pin to the
latest stable upstream release checked on 2026-07-13 without rewriting
historical model measurements.

### Included

- Pinned Rust `1.97.0` consistently in `rust-toolchain.toml`, `mise.toml`, and
  `Cargo.toml`; refreshed the Cargo lock graph to `bytes 1.12.1`, `cc 1.2.67`,
  and `memchr 2.8.3`. All six direct Cargo dependencies were already at their
  latest stable releases; the published `zip 9.0.0-pre2` prerelease was not
  adopted.
- Upgraded the release workflows to immutable commits for the current Node.js
  24 actions: `actions/checkout` v7.0.0, `actions/upload-artifact` v7.0.1, and
  `actions/download-artifact` v8.0.1.
- Moved release jobs to current GA hosted images: macOS 26 arm64, macOS 26
  Intel, Ubuntu 24.04 x64/arm64, and Windows Server 2025 x64.
- Updated the managed backend manifest from `llama.cpp b9878` to source-pinned
  `b9982` with official release-API names, sizes, and SHA-256 digests for all
  six supported backend artifacts. A real macOS arm64 archive download passed
  size/SHA-256/install verification and the installed binary reported build
  `9982 (99f3dc322)`.
- Rechecked the pinned SSE, disconnect cancellation, response-reader, and final
  usage contracts against the `b9982` upstream source. Model adoption results
  measured on `b9878` remain historical evidence and are not relabeled as
  `b9982` measurements.

### Upstream Evidence

- Rust 1.97.0: https://static.rust-lang.org/dist/channel-rust-1.97.0.toml
- GitHub Actions releases: https://github.com/actions/checkout/releases/tag/v7.0.0,
  https://github.com/actions/upload-artifact/releases/tag/v7.0.1, and
  https://github.com/actions/download-artifact/releases/tag/v8.0.1
- Hosted runner labels: https://github.com/actions/runner-images
- llama.cpp b9982: https://github.com/ggml-org/llama.cpp/releases/tag/b9982

### Verified During Implementation

- `cargo update --dry-run` under Rust 1.97.0 (`0 packages` pending)
- `cargo fmt --all -- --check`
- `cargo test --locked -- --test-threads=1` (`331` unit, `1` backend
  lifecycle, and `23` process tests passed)
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-toolchain-pins.sh`
- `scripts/release/verify-release-target-matrix.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.32.1`
- `scripts/release/verify-uninstall-smoke.sh target/release/rpotato`
- Real managed `llama.cpp b9982` macOS arm64 download, checksum, install,
  install-record, and version-detection smoke

## v0.32.0 - Durable Conversation Resume

Release date: 2026-07-13

This release makes local conversation state process-restart durable. Immutable app-data transcript artifacts store user, visible/normalized model, tool, and evidence turns; canonical ledger events own their order and bindings, while SQLite migration v6 provides a rebuildable `transcript_records` query projection.

### Included

- `run` reconstructs at most 8 recent turns within 2,400 characters and shares one 4-pointer/3,200-character source budget across current-request and resumed context before workflow creation or model execution.
- `state resume`, `resume <session-id>`, and the new `continue [session-id]` validate transcript artifacts, current source SHA-256 values, and workflow/proposal/evidence bindings before selecting a session or continuing a safe checkpoint. Pending approval never re-enters the backend, and uncertain backend/verification side effects are not retried.
- `tui transcript <session-id>`, `state`, and `monitor status` expose ledger-ordered durable transcript records without showing hidden model responses, raw source bodies, patch fragments, or verification-command text.
- Process-level tests delete SQLite and prove ordered projection rebuild, repeated resume without a second backend call, selected-session continuation, preflight-before-mutation, patch-fragment exclusion, and fail-closed artifact tamper handling.

### Privacy Boundary

Durable resume stores the local user turn and visible/normalized model, tool, and evidence records. It does not store the complete backend prompt, hidden reasoning/raw backend response, raw source body, patch fragment, verification-command text, or credential-bearing command output. Source context persists as a project-relative pointer and SHA-256 and is reread on resume.

### Verified During Implementation

- `cargo fmt --all -- --check`
- `cargo test --locked -- --test-threads=1` (`331` unit, `1` backend lifecycle, and `23` process tests passed)
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-target-matrix.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.32.0`
- `scripts/release/verify-uninstall-smoke.sh target/release/rpotato`
- Independent release-blocker re-review: approved after proposal-binding, transcript-root, privacy, ordering, and preflight corrections

## v0.31.1 - Windows Sidecar Stop Fallback

Release date: 2026-07-11

This patch release restores the missing Windows artifact from the incomplete v0.31.0 release. The v0.31.0 Windows release job proved the new native cancellation lifecycle but exposed that a non-forced `taskkill` failure returned before the existing forced-stop branch could run.

### Included

- `backend stop` checks process liveness after a graceful termination command fails and immediately attempts the force fallback when the process remains alive.
- A force command that races with natural process exit is accepted only after liveness confirms the recorded process is gone.
- Platform-independent unit tests cover both fallback paths; the Windows release job reruns the real fake-sidecar cancellation and stop lifecycle before building the artifact.

### Verified During Implementation

- `cargo test --locked -- --test-threads=1` (326 unit tests and 20 process-level integration tests)
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-target-matrix.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.31.1`

## v0.31.0 - Backend Streaming And Cancellation

Release date: 2026-07-11

This release replaces the buffered one-shot backend chat transport with a bounded SSE lifecycle. It adds live display, explicit cross-process cancellation, timeout cleanup, and terminal observability while keeping the managed `llama.cpp` sidecar alive after an interrupted generation.

### Included

- `backend chat` always requests SSE with `stream_options.include_usage=true`; `--stream` flushes filtered visible deltas while the default display remains buffered.
- `backend cancel` targets the active generation published behind an exclusive lock, closes only the chat connection, waits for a terminal record, and reports the recorded `completed`, `cancelled`, `timed-out`, or `failed` outcome instead of inferring cancellation from lease disappearance. The managed sidecar remains running.
- Total request timeout defaults to 30 seconds, accepts 1-300,000 ms through `--timeout-ms`, and covers address resolution, connection, request upload, and response reading. Request upload and response reading poll cancellation at intervals no longer than 100 ms.
- Requests are not retried after their HTTP body is sent. The adapter uses the normal non-resumable stream path and does not send `X-Conversation-Id`.
- Incremental filtering discards `reasoning_content` and split `<think>` traces before either buffered or streaming display. Streaming language validation holds complete text units before emission, so forbidden model bytes are never flushed. First-token latency starts at the first visible filtered delta.
- SSE event, HTTP chunk, and incomplete body buffers are bounded, and cumulative visible completion text is capped at 2 MiB. Upstream error payloads are reduced to a fixed category before display or persistence.
- Start, cancellation request, cancellation, timeout, failure, completion, and stale-lease cleanup write lifecycle evidence. Terminal paths also record resource and model-run evidence without raw prompt/response text. `backend stop` waits up to five seconds for a terminal acknowledgement before sidecar shutdown and records a forced-stop outcome if that wait expires. Concurrent ledger readers share the recoverable writer lease so they cannot misclassify an in-progress JSONL/head update as corruption.
- Final token usage is projected only when the final usage chunk arrives. Interrupted or failed runs keep missing usage unknown instead of recording fabricated zero tokens.

### Boundary

- One app-data root permits one active generation at a time.
- Cancellation and timeout interrupt generation; they do not stop the backend sidecar.
- Streaming is available in the CLI. Interactive TUI stream operation remains planned for v0.34.0.
- The current SQLite model-run projection has an interruption boolean; cancellation and timeout remain distinguishable through their lifecycle ledger event types.
- A cross-platform process test compiles a Rust fake sidecar and proves `backend cancel` keeps it running while `backend stop` waits for cancellation acknowledgement and then terminates it. Unix-only hostile fixtures additionally cover timeout, language rejection, error redaction, and stop ordering. The Windows release job runs the cross-platform process test plus the portable streaming and generation-state suites natively.

### Upstream Contract

The implementation is pinned to `llama.cpp b9878`. Upstream SSE, cancellation-on-reader-destruction, disconnect, and final-usage behavior were checked on 2026-07-11: [chat completions](https://github.com/ggml-org/llama.cpp/blob/b9878/tools/server/README.md#post-v1chatcompletions), [response-reader lifecycle](https://github.com/ggml-org/llama.cpp/blob/b9878/tools/server/server-queue.h#L168-L208), [cancellation posting](https://github.com/ggml-org/llama.cpp/blob/b9878/tools/server/server-queue.cpp#L441-L460), [disconnect handling](https://github.com/ggml-org/llama.cpp/blob/b9878/tools/server/server-http.cpp#L521-L565), and [final usage chunk](https://github.com/ggml-org/llama.cpp/blob/b9878/tools/server/server-task.cpp#L526-L537).

### Verified During Implementation

- `cargo test --locked -- --test-threads=1` (323 unit tests and 20 process-level integration tests)
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-target-matrix.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.31.0`

## v0.30.0 - Verified Local Model Adoption

Release date: 2026-07-11

This release turns source-backed Qwen/Gemma candidates into a measured, fail-closed local adoption path. It does not bundle model weights and does not treat public benchmark scores as comparable to the local GGUF runs.

### Included

- Added strict persistent `model default [<id>]` selection and optional `backend start --model`; every resolution revalidates registry, artifact bytes, and promotion evidence.
- Bound promotion to an exact `backend.chat.completed` event carrying backend binary, model artifact, context, sampling, mmproj, OS, and architecture provenance.
- Added the canonical `model-adoption-smoke-v1` fixture and direct benchmark-to-chat event linkage.
- Kept Qwen-only non-thinking options out of Gemma requests.
- Preserved and rebuilt corrupt project ledger mirrors from the valid app-global canonical ledger without weakening global-ledger validation.
- Evaluated both pinned artifacts on the same local host; Gemma alone passed exact-response equality and was promoted, installed, and selected as the persistent default.

### Measured Local Evidence

Host: MacBook Pro `Mac17,8`, Apple M5 Pro, 64 GB RAM, macOS arm64; managed `llama.cpp b9878`; context 4096; temperature 0.1; top-p 0.8.

| Artifact | Result | Latency | Tokens/s | Peak RSS |
| --- | --- | ---: | ---: | ---: |
| Qwen3.5 4B Q4_K_M | `2/3`, exact response failed; markers `5/5`, forbidden `0` | `1680 ms` | `61.9048` | `3296378880` bytes |
| Gemma 4 E4B IT QAT q4_0 | `3/3`, exact response passed; markers `5/5`, forbidden `0` | `1686 ms` | `61.6845` | `5521932288` bytes |

Qwen emitted the instruction sentence before the five required lines, so its lower measured RSS did not override the failed contract. Gemma's local `recommendedRamGb=8` is derived as the measured peak RSS rounded up to GiB plus 2 GiB headroom. This is not a 16 GB host test, a universal model ranking, or public benchmark parity.

### Boundary

- Static manifest candidates remain `unverified`; local promotion is valid only while its evidence revalidates.
- The adoption smoke checks a narrow instruction/safety contract, not full repository code-edit or tool-execution quality.
- Model weights remain in user app data and are not committed or attached to the GitHub Release.
- Windows and 16 GB model runtime validation remain open.

## v0.29.1 - Cross-Platform Aggregate Checksum Fix

Release date: 2026-07-11

This patch release fixes the Windows checksum line ending discovered by
independently downloading and verifying all v0.29.0 release assets. PowerShell
emitted the Windows `.sha256` file with CRLF, so the aggregate checksum retained
an invisible carriage return in the zip filename and failed `shasum -c` on Unix.
v0.29.1 supersedes the v0.29.0 publication while retaining its runtime changes.

### Included

- Write the Windows checksum with explicit ASCII encoding and an LF terminator.
- Reject carriage returns in per-asset and aggregate checksum files.
- Exercise valid LF and invalid CRLF fixtures in the release target guard.
- Keep the five-platform build, packaged binary smoke, Windows uninstall smoke,
  and aggregate checksum publication unchanged.

### Verified During Implementation

- `bash -n scripts/release/verify-checksum-basenames.sh`
- `bash -n scripts/release/verify-release-target-matrix.sh`
- `scripts/release/verify-release-target-matrix.sh`
- `cargo fmt --all -- --check`
- `cargo test --locked -- --test-threads=1` (294 tests)
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- Release-binary `doctor` smoke through `scripts/release/verify-release-binary-smoke.sh`

### Boundary

This release changes packaging and checksum validation only. It does not alter
the v0.29 runtime behavior, model manifests, or supported target matrix.

## v0.29.0 - Durable Single-Agent Runtime Correction

Release date: 2026-07-11

v0.29.0 connects `run`, `state resume`, `patch approve`, and `patch verify`
through one restart-safe workflow. Model text is persisted as a non-executable
typed action; runtime-owned ontology context/source rereads, separate approval
bindings, guarded apply, verification evidence, and the stop gate own every side
effect and completion decision.

### Included

- Separated the patch-only workflow identity into a generic workflow envelope
  and typed action state, allowing read-only `run` and patch proposals to finish
  through independent runtime paths.
- Made the ontology projection the first context-selection layer and reread the
  authoritative source before binding a source pointer into a patch proposal.
- Stopped rendering raw model responses as final output and applied the guarded
  Korean report contract to read-only, pending, blocked, and terminal results.
- Split patch application and verification-command execution into separate
  one-time credentials. `patch approve` never runs the command; only
  `patch verify` can approve the pre-bound verification plan.
- Added workflow schema v3 while preserving immutable v2 snapshots and hashes,
  with one-way v2-to-v3 append migration and strict recovery binding.
- Serialized runtime/project ledger mutation under a recoverable writer lease,
  made corrupt current-state mutation fail closed, and removed SQLite-only
  sessions from resume authority.
- Made approve/cancel races, already-restored rollback, and tampered source
  recovery artifact paths idempotent or fail closed.
- Added immutable versioned workflow snapshots, a synced recovery transaction,
  and an atomic committed-revision pointer with schema/revision/hash chains matched
  to strictly parsed append-only ledger checkpoints.
- Added full nonterminal artifact discovery with multi-active fail-closed recovery,
  including atomic cleanup of a revalidated terminal active pointer.
- Bound approval to workflow/action/proposal IDs, before/after hashes, and the
  exact policy-allowed verification plan; an OS-CSPRNG nonce is shown once, only
  its hash is stored, approval is persisted before apply, and an explicit
  `patch token-rotate` command safely replaces a lost pending token.
- Replaced substring command checks with one shell-free parsed argv grammar shared
  by classification and execution. Patch verification permits only `pwd` and
  narrow current-crate Cargo checks and rejects command smuggling, external
  manifest/package/workspace selection, path executables, and metacharacters.
- Added restart-safe pending approval and approved-apply resume without another
  model call, atomic guarded apply, hash-verified atomic rollback with truthful
  failure evidence, hash-only evidence, and fresh-source/evidence stop-gate checks
  even when resuming a completed workflow.
- Made standalone previews diff-only, added PID/nonce recoverable leases and
  explicit crash reconciliation through `cancel`, installed source bytes with a
  no-clobber guard transaction, and chained physical ledger order with a synced
  truncation-detecting head.
- Added deterministic Korean success/failure reports and subprocess coverage for
  happy path, restart, stale hashes, token rejection/redaction, denied commands,
  rollback, corrupt workflow state, idempotence, complete-state tampering, and
  hostile model text/path parsing. Portable unit/state tests cover checkpoint fault
  windows, multi-active state, ledger partial/chain conflicts, rollback tamper and
  failure, token recovery, evidence deduplication, strict malformed artifacts,
  projection truth, and the reusable eight-fixture Korean output guard.

### Verified During Implementation

- `cargo fmt --all -- --check`
- `cargo test --locked -- --test-threads=1` (294 tests: 282 unit and 12 Unix subprocess integration)
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-target-matrix.sh`
- Release-binary `doctor` smoke through `scripts/release/verify-release-binary-smoke.sh`

### Boundary

This release does not implement full conversation transcript replay, streaming,
subagent/team execution, or interactive TUI mutation. Those remain later roadmap
items. SQLite is a workflow/session projection rebuilt from the canonical runtime
ledger, never a second authority.
Mode-0600 project-local workflow/proposal artifacts retain the snippets, proposal
diff/proposed source, transaction metadata, and rollback source required for
recovery until project cleanup; SQLite/monitor, ledger details, and evidence do
not store raw source bytes. Legacy v2 plaintext credentials are atomically scrubbed
to hash-only form and then require a fresh canonical workflow preview.
The portable unit/state boundary runs on Windows, but the true fake-sidecar
subprocess suite remains Unix-only; Windows subprocess end-to-end coverage is not
claimed in v0.29.0.

## v0.28.5 - Unix PID Guard for Release Gate

Release date: 2026-07-10

This patch release fixes the root cause behind the release-gate runner
shutdowns. A stale sidecar test used `u32::MAX` as a fake PID; on Linux, passing
that value to process commands can wrap through `pid_t` semantics and signal the
runner process group. v0.28.5 rejects invalid Unix PID values before invoking
`kill` or `ps`, then restores the full serialized release test gate.

### Included

- Added a Unix PID argument guard that rejects `0` and values above
  `i32::MAX` before `kill`/`ps` process checks.
- Added a regression test for wrapping PID values.
- Restored the GitHub Release test gate to the full
  `cargo test --locked -- --test-threads=1` suite.
- Updated release docs, README binary download notes, and roadmap entries to
  treat v0.28.5 as the complete Linux artifact publication.

### Verified In This Release

- `cargo fmt --check`
- `cargo test --locked` (216 tests)
- `cargo test --locked -- --test-threads=1` (216 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.28.5`
- `scripts/release/verify-release-target-matrix.sh`

### Boundary

This release does not add Homebrew, Scoop, winget, apt, rpm, or container
distribution. It only stabilizes direct GitHub Release archives and checksums.

## v0.28.4 - Release Runner Safe Test Gate

Release date: 2026-07-10

This patch release attempted to avoid the GitHub runner shutdown by skipping the
sidecar timeout fixture on the hosted release runner. The published v0.28.4
release still failed because the stale-record test exposed a Unix PID wrap
hazard. v0.28.5 supersedes it.

### Included

- Keeps full local release verification on `cargo test --locked`, including the
  sidecar timeout fixture, before merge.
- Changes the GitHub Release test gate to
  `cargo test --locked -- --test-threads=1 --skip backend::tests::start_timeout_removes_record_and_keeps_logs`
  so the hosted runner does not execute the fixture that repeatedly triggers
  runner shutdown.
- Keeps target jobs focused on native target build, packaged-binary smoke,
  archive creation, checksum generation, and release upload.
- Updated release docs, README binary download notes, and roadmap entries for
  the attempted Linux artifact publication path.

### Verified In This Release

- `cargo fmt --check`
- `cargo test --locked` (215 tests)
- `cargo test --locked -- --test-threads=1` (215 tests)
- `cargo test --locked -- --test-threads=1 --skip backend::tests::start_timeout_removes_record_and_keeps_logs` (214 tests, 1 filtered out)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.28.4`
- `scripts/release/verify-release-target-matrix.sh`

### Boundary

This release does not add Homebrew, Scoop, winget, apt, rpm, or container
distribution. It only stabilizes direct GitHub Release archives and checksums.

## v0.28.3 - Serialized Release Test Gate

Release date: 2026-07-10

This patch release serialized the release test gate after v0.28.2 received a
GitHub runner shutdown signal. The published v0.28.3 release still received a
shutdown signal immediately after the sidecar timeout fixture and did not upload
binary assets. v0.28.4 supersedes it.

### Included

- Runs the release test gate with `cargo test --locked -- --test-threads=1` so
  process-oriented backend lifecycle tests do not overlap in the release job.
- Keeps the fake timeout backend fixture as `exec sleep` so timeout cleanup does
  not leave a wrapper child behind.
- Leaves target jobs focused on native target build, packaged-binary smoke,
  archive creation, checksum generation, and release upload.
- Updated release docs, README binary download notes, and roadmap entries for
  the attempted Linux artifact publication path.

### Verified In This Release

- `cargo fmt --check`
- `cargo test --locked` (215 tests)
- `cargo test --locked -- --test-threads=1` (215 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.28.3`
- `scripts/release/verify-release-target-matrix.sh`

### Boundary

This release does not add Homebrew, Scoop, winget, apt, rpm, or container
distribution. It only stabilizes direct GitHub Release archives and checksums.

## v0.28.2 - Linux Test Gate Process Cleanup

Release date: 2026-07-10

This patch release attempted to complete the Linux/macOS/Windows binary
publication after the v0.28.1 release test gate exited with signal 143 on Linux.
The published v0.28.2 release still received a GitHub runner shutdown signal
before binary assets were uploaded. v0.28.3 supersedes it.

### Included

- Added Unix process-group cleanup for backend sidecar startup timeouts so
  wrapper children do not survive timeout tests.
- Updated the fake timeout backend fixture to `exec sleep` instead of spawning a
  lingering child process.
- Kept the v0.28.1 `release test gate` design: full tests run once before target
  build jobs, while target jobs build, smoke, package, checksum, and upload.
- Updated release docs, README binary download notes, and roadmap entries for
  the attempted Linux artifact publication path.

### Verified In This Release

- `cargo fmt --check`
- `cargo test --locked` (215 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.28.2`
- `scripts/release/verify-release-target-matrix.sh`

### Boundary

This release does not add Homebrew, Scoop, winget, apt, rpm, or container
distribution. It only stabilizes direct GitHub Release archives and checksums.

## v0.28.1 - Release Artifact Workflow Stabilization

Release date: 2026-07-10

This patch release added the release test gate after the v0.28.0 GitHub Actions
run was interrupted by a shutdown signal on the Linux ARM64 runner. The
published v0.28.1 release did not upload binary assets because the Linux test
gate still exited with signal 143. v0.28.2 supersedes it.

### Included

- Added a dedicated `release test gate` job on `ubuntu-24.04` that runs
  `cargo test --locked` once before target builds.
- Kept target build jobs focused on native target build, packaged-binary smoke,
  archive creation, checksum generation, and release upload.
- Hardened `scripts/release/verify-release-target-matrix.sh` so release tests
  stay centralized and the Linux/macOS/Windows matrix remains pinned.
- Updated release docs, README binary download notes, and roadmap entries for
  the attempted Linux artifact publication path.

### Verified In This Release

- `cargo fmt --check`
- `cargo test --locked` (215 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.28.1`
- `scripts/release/verify-release-target-matrix.sh`

### Boundary

This release does not add Homebrew, Scoop, winget, apt, rpm, or container
distribution. It only stabilizes direct GitHub Release archives and checksums.

## v0.28.0 - Linux Release Artifacts

Release date: 2026-07-10

This release expands official GitHub Release binaries to Linux x86_64 and Linux
ARM64 while keeping the existing checksum and packaged-binary smoke gates.

### Included

- Added `x86_64-unknown-linux-gnu` release artifacts built on `ubuntu-24.04`.
- Added `aarch64-unknown-linux-gnu` release artifacts built on
  `ubuntu-24.04-arm`.
- Renamed the tarball packaging step so macOS and Linux share the same
  OS-neutral `tar.gz` packaging path.
- Added `scripts/release/verify-release-target-matrix.sh` to guard the release
  workflow target list.
- Updated release docs, README binary download notes, and roadmap entries for
  the new Linux targets.

### Verified In This Release

- `cargo fmt --check`
- `cargo test --locked` (215 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.28.0`
- `scripts/release/verify-release-target-matrix.sh`

### Boundary

This release does not add Homebrew, Scoop, winget, apt, rpm, or container
distribution. It only publishes direct GitHub Release archives and checksums.

## v0.27.0 - Plugin Adapter Hardening

Release date: 2026-07-10

This release hardens local Codex/Claude Code-style plugin import without
opening foreign plugin execution.

### Included

- Normalized plugin manifests now use schema version 2 with adapter version,
  permission policy, source manifest SHA-256, and imported source snapshot
  SHA-256 fields.
- `plugin import` maps visible Codex/Claude Code plugin surfaces into
  capability summaries and reports required plus blocked permissions.
- Shell, `bin/`, MCP, hook, LSP, monitor/background, runtime setting, remote
  connector, sensitive config, and file-write permissions remain blocked by
  default.
- `plugin validate` and `plugin enable` re-check the imported source snapshot
  hash and mark the plugin `blocked` if the imported source drifts.
- Added tests for capability reporting, manifest hash persistence, Claude Code
  surface detection, and drift blocking.

### Verified In This Release

- `cargo fmt --check`
- `cargo test --locked` (215 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.27.0`

### Boundary

This release still does not execute imported plugin code, integrate external
plugin marketplaces, grant MCP/shell/background permissions, or convert foreign
plugin formats into trusted native runtime extensions without policy approval.

## v0.26.0 - Ontology Runtime Store

Release date: 2026-07-10

This release moves the `rolling-potato` ontology from design documentation into
the first project-local runtime store slice. Small-model prompts receive compact
source-pointer-first views, while authoritative source decisions must reread the
original file.

### Included

- Added the `.rpotato/ontology/graph.jsonl` typed graph store and
  `.rpotato/ontology/schema.json` schema contract.
- `rpotato init` now creates the ontology store/schema and seeds source-backed
  deterministic Layer A facts.
- Added `rpotato ontology status`, `seed`, `inspect`, `context --query <text>`,
  `reread <source-pointer>`, `export --format json|jsonl`, and
  `import --file <path> --dry-run`.
- Layer A seed records indexed files, package manifests, entrypoints, and
  generated-exclusion rules with source pointers and SHA-256 hashes without
  storing raw source text in the ontology store.
- Import dry-run blocks confirmed Layer B semantic claims that lack source
  pointers and source hashes.
- `rpotato doctor` now reports ontology store diagnostics.

### Verified In This Release

- `cargo fmt --check`
- `cargo test --locked` (212 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.26.0`
- isolated `rpotato init`, `ontology status`, `ontology context`,
  `ontology reread`, and `ontology import --dry-run` smoke

### Boundary

This release does not promote model output into confirmed ontology, does not use
RDF/OWL as the canonical store, and does not yet wire ontology invariant checks
into patch apply or the agent loop.

## v0.25.0 - Verified Model Install Gate

Release date: 2026-07-10

This release adds the first registry install path for source-backed model
candidates, gated by local evidence instead of unsupported model claims.

### Included

- Added `rpotato model promote <id> --evidence <file>` to validate local
  promotion evidence before registry installation.
- Promotion now requires source-backed artifact size/SHA-256 match, a backend
  smoke ledger event, RAM-fit/mmproj fields, and a SQLite
  `measured-locally` benchmark row with matching peak RSS.
- `rpotato model install <id>` now registers a model only when the static
  manifest is `verified` or the normalized local promotion evidence revalidates.
- Model registry entries now record promotion evidence path, backend version,
  and benchmark run id.
- README and model manifest docs now document the fetch, benchmark, promote,
  install flow and the promotion evidence JSON schema.

### Verified In This Release

- `cargo fmt --check`
- `cargo test --locked` (203 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.25.0`
- `RPOTATO_DATA_HOME=<tmp> RPOTATO_PROJECT_ROOT=<tmp> target/release/rpotato model install qwen3.5-4b` (expected block before promotion evidence)

### Boundary

This release does not claim Qwen or Gemma public benchmark parity, does not
bundle model weights, and does not auto-promote a model without local evidence.

## v0.24.2 - Aggregate Checksum Checkout Fix

Release date: 2026-07-09

This patch release fixes the aggregate checksum job by checking out the
repository before running the repo-local checksum guard.

### Included

- The `checksums` job now runs `actions/checkout@v4` before downloading workflow
  artifacts and invoking `scripts/release/verify-checksum-basenames.sh`.
- Current release documentation now points at the v0.24.2 binary workflow.

### Verified In This Release

- `cargo fmt --check`
- `cargo test --locked` (197 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `bash -n scripts/release/verify-checksum-basenames.sh`
- `scripts/release/verify-checksum-basenames.sh <fixture-checksum-file>`
- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/release-binaries.yml")'`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.24.2`

### Boundary

This release does not change supported binary targets or package-manager
distribution.

## v0.24.1 - Checksum Workflow Fix

Release date: 2026-07-09

This patch release fixes checksum publishing so GitHub Release checksum files are
usable directly beside downloaded release assets.

### Included

- macOS `.sha256` files now record release asset basenames instead of
  build-directory paths.
- The aggregate checksum upload step now has explicit GitHub repository context.
- Added `scripts/release/verify-checksum-basenames.sh` and wired it into release
  packaging and aggregate checksum generation.

### Verified In This Release

- `cargo fmt --check`
- `cargo test --locked` (197 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `bash -n scripts/release/verify-checksum-basenames.sh`
- `scripts/release/verify-checksum-basenames.sh <fixture-checksum-file>`
- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/release-binaries.yml")'`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.24.1`

### Boundary

This release does not change supported binary targets or add package-manager
distribution.

## v0.24.0 - Cross-Platform Release Hardening

Release date: 2026-07-09

This release hardens the official binary publishing path with a macOS Intel
artifact, aggregate checksum publication, Windows uninstall smoke coverage, and
a release notes template for future releases.

### Included

- `release-binaries` now builds `x86_64-apple-darwin` in addition to macOS Apple
  Silicon and Windows x86_64.
- Release assets now include `rpotato-vX.Y.Z-checksums.txt` as an aggregate
  checksum publication file.
- Windows release jobs run `rpotato uninstall --dry-run --keep-cache` and
  `rpotato uninstall --dry-run --purge-cache` smoke checks.
- Added `docs/release-notes-template.md` and `docs/ko/release-notes-template.md`.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (197 tests)
- `cargo test --locked` (197 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --locked`
- `cargo build --release --locked`
- `cargo build --release --locked --target x86_64-apple-darwin`
- `file target/x86_64-apple-darwin/release/rpotato`
- `bash -n scripts/release/verify-uninstall-smoke.sh`
- `scripts/release/verify-uninstall-smoke.sh target/release/rpotato`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.24.0`
- `scripts/release/verify-release-binary-smoke.sh target/x86_64-apple-darwin/release/rpotato 0.24.0`

### Boundary

This release does not add Linux artifacts, package-manager channels, signing,
notarization, or auto-update distribution.

## v0.23.1 - Windows Binary Link Fix

Release date: 2026-07-09

This patch release fixes the Windows x86_64 release build by enabling the
`rusqlite` `bundled-windows` feature, so the release workflow does not depend on
a runner-provided `sqlite3.lib`.

### Included

- Windows release binaries link SQLite through the locked Rust dependency graph
  instead of requiring a system SQLite import library.
- v0.23.0 macOS packaging behavior is unchanged.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (197 tests)
- `cargo test --locked` (197 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --locked`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.23.1`

## v0.23.0 - Official Binary Download Foundation

Release date: 2026-07-09

This release adds the first official binary release pipeline. Published GitHub
Releases now trigger managed `rpotato` binary builds for macOS Apple Silicon and
Windows x86_64, with checksums and binary smoke validation.

### Included

- New `release-binaries` GitHub Actions workflow.
- Release asset targets:
  - `rpotato-vX.Y.Z-aarch64-apple-darwin.tar.gz`
  - `rpotato-vX.Y.Z-aarch64-apple-darwin.tar.gz.sha256`
  - `rpotato-vX.Y.Z-x86_64-pc-windows-msvc.zip`
  - `rpotato-vX.Y.Z-x86_64-pc-windows-msvc.zip.sha256`
- `scripts/release/verify-release-binary-smoke.sh` for packaged binary smoke
  checks.
- `rpotato doctor` now reports package name, package version, target OS/arch,
  binary suffix, and release-smoke availability.
- Release docs now define the v0.23.0 asset workflow and note that macOS Intel,
  Linux, and package-manager channels remain later work.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (197 tests)
- `cargo test --locked` (197 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `cargo build --locked`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/debug/rpotato 0.23.0`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.23.0`
- `rpotato doctor`

### Boundary

This release does not add macOS Intel, Linux, Homebrew, Scoop, winget, signing,
notarization, or auto-update distribution. It also does not include model
weights, backend binaries, or external plugin packages in the `rpotato` release
binary.

## v0.22.0 - Dispatcher Hardening

Release date: 2026-07-09

This release adds the first dispatch-time team hardening surface. It records
dispatch ownership decisions and failed-worker continuation state without
starting workers or advancing team stages.

### Included

- New `rpotato team dispatch --lanes <count> --write-owner <lane:path>`
  command.
- Dispatch-time normalized file ownership enforcement. Cross-lane ownership
  conflicts and critical resource pressure return blocked errors and record
  ledger/SQLite projection events.
- Failed-worker continuation recording with
  `--failed-lane <lane> --failure <reason>`, including remaining admitted lane
  count and continuation action.
- `rpotato team status` now surfaces the latest `team.*` runtime ledger event
  for the current project.
- English and Korean docs now describe `team dispatch` as a preflight/reporting
  boundary, not a worker launcher.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (196 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `scripts/release/verify-release-policy.sh`
- `rpotato team dispatch --lanes 2 --write-owner 1:src/team.rs --write-owner 2:src/cli.rs`
- `rpotato team status`

### Boundary

This release does not launch subagents, execute tools, merge worker output,
advance team stages, or implement a full dispatcher. It only records and
enforces the dispatch preflight state needed before worker launch can exist.

## v0.21.0 - Benchmark-Driven Optimization Policy

Release date: 2026-07-09

This release adds the first read-only optimization policy surface. It consumes
local runtime metrics and local benchmark evidence to recommend safer context,
team-lane, fallback, and model-route hints for small-model execution.

### Included

- New `rpotato monitor optimize` command.
- Deterministic optimization policy over resource pressure, model-run metrics,
  context clamp count, p95 latency, average tokens/sec, and local benchmark
  pass/fail evidence.
- `monitor optimize` reports measured benchmark run count, pass/fail count,
  average local score, latest measured benchmark row, recommended context
  tokens, recommended team lanes, fallback mode, and model route hint.
- Recommendations are read-only local runtime hints. They do not choose a real
  model artifact, promote model status, store raw prompt/source text, or claim
  public benchmark parity.
- English and Korean docs now describe `monitor optimize` as the v0.21.0
  benchmark-driven optimization policy surface.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (189 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `scripts/release/verify-release-policy.sh`
- `rpotato monitor baseline`
- `rpotato monitor optimize`

### Boundary

This release does not implement dispatcher worker launch, model promotion,
public benchmark parity, or automatic model selection. It only converts local
SQLite projection evidence into conservative runtime policy hints.

## v0.20.1 - Benchmark Evidence Status

Release date: 2026-07-09

This patch release records the first real Qwen executable smoke measurement and
fixes the model evaluation preflight so it reflects locally measured benchmark
rows.

### Fixed

- `rpotato model eval-plan qwen3.5-4b` now reports the latest local
  `measured-locally` benchmark row from the SQLite `benchmark_runs` projection
  instead of always showing `local benchmark status: not-run`.
- The status advances to `local-smoke-measured` when a measured row exists for
  the candidate artifact model id.

### Evidence Recorded

- Qwen3.5-4B Q4_K_M local artifact was already present and SHA-256 verified.
- Managed `llama.cpp` version `9878 (2da668617)` started the Qwen sidecar with
  `--ctx-size 4096`.
- `rpotato benchmark run --fixture benchmarks/fixtures/executable-smoke.json
  --prompt benchmarks/prompts/executable-smoke.txt --max-tokens 32` recorded
  score `3/3`, `local_pass=true`, latency `243ms`, total tokens `83`, resource
  pressure `normal`, and peak RSS `3351363584` bytes.
- The sidecar was stopped after measurement.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (186 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `rpotato model eval-plan qwen3.5-4b`
- `rpotato backend status`

### Boundary

This is a local smoke benchmark only. It does not promote Qwen3.5-4B to
`verified` and does not claim public benchmark parity.

## v0.20.0 - Executable Benchmark Runner

Release date: 2026-07-09

This release adds the first executable local benchmark runner. It is still a
source-only developer preview: it does not ship model weights, external plugin
packages, or prebuilt `rpotato` binaries.

### Included

- New `rpotato benchmark run --fixture <fixture.json> --prompt <artifact>
  [--max-tokens <tokens>]` command.
- `benchmark run` calls the currently running backend sidecar and records a
  local `claim_state=measured-locally` benchmark row.
- Deterministic 0-3 local product score based on expected/forbidden response
  markers, abstention requirement, and non-empty model output.
- SQLite migration v4 extends `benchmark_runs` with `model_run_id`, prompt
  artifact checksum/length, local pass flag, marker counts, latency, token
  counts, resource pressure, and peak RSS.
- `benchmark report --format jsonl` exports the new executable benchmark fields.
- `benchmarks/fixtures/executable-smoke.json` and
  `benchmarks/prompts/executable-smoke.txt` provide the first executable smoke
  fixture/prompt pair.
- English and Korean documentation updates for executable benchmark boundaries,
  redaction, and observability linkage.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (185 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `scripts/release/verify-release-policy.sh`
- `rpotato benchmark validate benchmarks/fixtures/sample.json`
- `rpotato benchmark validate benchmarks/fixtures/executable-smoke.json`
- `rpotato benchmark record --fixture benchmarks/fixtures/sample.json`
- `rpotato benchmark run --fixture benchmarks/fixtures/executable-smoke.json --prompt benchmarks/prompts/executable-smoke.txt --max-tokens 32` fail-closed without a running sidecar
- `rpotato benchmark report --format jsonl`

### Known Issues

- `benchmark run` requires an already running backend sidecar and a local model
  file started through `rpotato backend start`; this release does not bundle or
  auto-select model weights.
- The executable runner records local product scores only. It does not compare
  against public benchmark scores or claim leaderboard parity.
- Source-read compliance and hallucination scoring are still marker/proxy based;
  richer tool/evidence-aware scoring remains planned.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.19.0 - Benchmark Harness Foundation

Release date: 2026-07-09

This release adds the first metadata-only benchmark harness surface. It is still
a source-only developer preview: it does not ship model weights, external plugin
packages, or prebuilt `rpotato` binaries.

### Included

- New `rpotato benchmark validate <fixture.json>` command.
- New `rpotato benchmark record --fixture <fixture.json>` command.
- New `rpotato benchmark report --format jsonl` command.
- Project-local fixture schema validation for runtime capability, model/runtime
  responsibility, expected route, policy decision, escalation target, required
  tool/source/evidence records, abstention requirement, ontology view, context
  budget, backend/model artifact identifiers, sampling policy, and raw artifact
  retention policy.
- SQLite migration v3 extends `benchmark_runs` with session, fixture checksum,
  claim state, reproducibility manifest, and redacted report fields.
- Metadata-only benchmark records use `claim_state=not-comparable` and
  `score=null`; no model execution or public benchmark parity claim is made.
- `benchmarks/fixtures/sample.json` provides a no-raw-prompt/source CLI-contract
  smoke fixture.
- English and Korean documentation updates for benchmark fixture contracts,
  observability integration, and v0.19.0 rollout status.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (183 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `scripts/release/verify-release-policy.sh`
- `rpotato benchmark validate benchmarks/fixtures/sample.json`
- `rpotato benchmark record --fixture benchmarks/fixtures/sample.json`
- `rpotato benchmark report --format jsonl`
- `rpotato monitor status`

### Known Issues

- Benchmark commands do not execute models, score fixtures, or compare local
  scores with public benchmarks.
- Hardware/RAM/power/thermal manifest fields are present only as
  `not-recorded` placeholders until executable benchmark runs collect them.
- Fixture suites, ontology-view scoring, public benchmark parity reports, and
  benchmark-driven optimization policy remain planned.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.18.0 - Performance Baseline Report

Release date: 2026-07-08

This release adds a read-only local performance baseline report. It is still a
source-only developer preview: it does not ship model weights, external plugin
packages, or prebuilt `rpotato` binaries.

### Included

- New `rpotato monitor baseline` command.
- Aggregates existing local ledger/SQLite projection metrics without adding a
  new raw prompt/source store.
- Reports p50/p95 latency, average tokens/sec, context clamp count, context
  tokens dropped, peak RSS, pressure-state distribution, and
  model/backend/session grouping.
- Keeps the report as local metric evidence only; it does not select model
  artifacts or make source-backed model capability claims.
- English and Korean documentation updates for the v0.18.0 performance
  baseline scope.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (172 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `scripts/release/verify-release-policy.sh`
- `rpotato monitor baseline`

### Known Issues

- `monitor baseline` reports only metrics already present in the local
  projection. It does not run benchmarks or collect continuous background
  samples.
- Benchmark harness recording, redacted report export, and benchmark-driven
  optimization policy remain planned.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.17.0 - Team Context And Model Governor

Release date: 2026-07-08

This release adds the first team context/model governor preflight. It is still a
source-only developer preview: it does not ship model weights, external plugin
packages, or prebuilt `rpotato` binaries.

### Included

- New `rpotato team governor --lanes <count> --context-tokens <tokens>`
  command.
- Optional `--context-limit <tokens>` and `--model-tier small|standard|large`
  inputs for explicit runtime policy simulation.
- Latest resource sample consumption for admitted-lane and context/model
  governor decisions.
- Effective context-token clamp against the configured budget, degraded-pressure
  budget, and local small-model soft budget.
- Local model route hints: `keep`, `downgrade`, `escalate`, and `defer`.
- Ledger/SQLite recording for team governor decisions.
- English and Korean documentation updates for the v0.17.0 governor scope.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (170 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato team governor --lanes 2 --context-tokens 6000 --context-limit 4096 --model-tier standard`
- `rpotato team governor --lanes 2 --context-tokens 1024 --context-limit 4096 --model-tier small`
- `rpotato monitor status`

The smoke checks use a scratch project root under `/private/tmp` and verify
that normal pressure records a clamped context/model decision while critical
pressure blocks with a `defer` route hint.

### Known Issues

- `team governor` is a preflight/reporting surface. It does not start workers,
  select real model artifacts, or execute model routing.
- Model route hints are local runtime policy hints only; they are not
  source-backed claims about any real model artifact's capability.
- Dispatch-time ownership enforcement and failed-worker continuation remain
  planned.
- Resource sampling is still event-driven, not continuous live polling.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.16.0 - Team Approval Queue Integration

Release date: 2026-07-08

This release connects blocked team admission decisions to the read-only approval
queue. It is still a source-only developer preview: it does not ship model
weights, external plugin packages, or prebuilt `rpotato` binaries.

### Included

- New project-local approval request store under `.rpotato/approval-requests/`.
- Blocking `team admit` policy/ownership decisions now write redacted approval
  request records linked to the team admission ledger event.
- `rpotato tui approvals` now renders team admission approval requests beside
  patch proposal approvals.
- `rpotato init` creates the approval request directory as part of the project
  runtime layout.
- Team admission output includes the approval request id and path when a policy
  or ownership decision needs review.
- English and Korean documentation updates for the v0.16.0 approval queue
  integration scope.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (165 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato team admit --lanes 2 --command "cargo test"`
- `rpotato team admit --lanes 2 --write README.md`
- `rpotato team admit --lanes 2 --write-owner 1:README.md --write-owner 2:./README.md`
- `rpotato tui approvals`
- `rpotato monitor status`

The smoke checks use a scratch project root under `/private/tmp` and verify
that policy/ownership-blocked team admission records appear in the read-only TUI
approval queue.

### Known Issues

- `tui approvals` is read-only. It lists team admission requests but does not
  approve, deny, or resume dispatch.
- `team admit` still does not start subagents, dispatch team lanes, advance team
  stages, or enforce ownership during actual worker execution.
- Resource sampling is still event-driven, not continuous live polling.
- Runtime context clamp and model downgrade/escalation hints remain planned.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.15.0 - Team File Ownership Preflight

Release date: 2026-07-08

This release adds file ownership preflight to the enforced team admission gate.
It is still a source-only developer preview: it does not ship model weights,
external plugin packages, or prebuilt `rpotato` binaries.

### Included

- `rpotato team admit --lanes <count>` now accepts repeated
  `--write-owner <lane:path>` ownership claims.
- Ownership paths are normalized before dispatch so equivalent paths such as
  `README.md` and `./README.md` resolve to the same ownership key.
- Cross-lane ownership conflicts block admission before any future worker
  launch.
- Owned write paths also participate in the existing write policy preflight, so
  approval-required writes still block dispatch until approval queue integration
  exists.
- Team admission output and ledger event details include ownership claim count,
  ownership status, ownership blocked flag, owned write paths, and per-claim
  decisions.
- English and Korean documentation updates for the v0.15.0 ownership preflight
  scope.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (163 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato team admit --lanes 2`
- `rpotato team admit --lanes 2 --command "cargo test"`
- `rpotato team admit --lanes 2 --write-owner 1:src/app.rs --write-owner 2:src/cli.rs`
- `rpotato team admit --lanes 2 --write-owner 1:README.md --write-owner 2:./README.md`
- `rpotato monitor status`

The smoke checks use a scratch project root under `/private/tmp` and verify
that distinct lane-owned paths are allocated while normalized cross-lane
ownership conflicts block dispatch before worker launch.

### Known Issues

- `team admit` still does not start subagents, dispatch team lanes, advance team
  stages, or enforce ownership during actual worker execution.
- `ask` decisions block dispatch because approval queue integration is still
  planned.
- Resource sampling is still event-driven, not continuous live polling.
- Runtime context clamp and model downgrade/escalation hints remain planned.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.14.0 - Team Policy Preflight

Release date: 2026-07-08

This release adds policy preflight to the enforced team admission gate. It is
still a source-only developer preview: it does not ship model weights, external
plugin packages, or prebuilt `rpotato` binaries.

### Included

- `rpotato team admit --lanes <count>` now accepts repeated `--write <path>` and
  `--command <command>` preflight checks.
- Requested write paths are classified with the same policy engine as
  `policy check-path --write`.
- Requested commands are classified with the same policy engine as
  `policy check-command`.
- `allow` policy checks can pass the admission gate.
- `ask` and `deny` policy checks block dispatch before any future worker launch.
- Team admission output and ledger event details include policy check count,
  policy status, policy blocked flag, requested writes, redacted commands, and
  per-check decisions.
- English and Korean documentation updates for the v0.14.0 policy preflight
  scope.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (159 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato team admit --lanes 2`
- `rpotato team admit --lanes 2 --command "cargo test"`
- `rpotato team admit --lanes 2 --write README.md`
- `rpotato monitor status`

The smoke checks use a scratch project root under `/private/tmp` and verify
that command preflight can pass while write preflight blocks with
`approval-required` before worker launch.

### Known Issues

- `team admit` still does not start subagents, dispatch team lanes, advance team
  stages, or allocate file ownership.
- `ask` decisions block dispatch because approval queue integration is still
  planned.
- Resource sampling is still event-driven, not continuous live polling.
- Runtime context clamp and model downgrade/escalation hints remain planned.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.13.0 - Team Admission Gate

Release date: 2026-07-07

This release turns the v0.12.0 read-only team admission preview into the first
enforced admission gate. It is still a source-only developer preview: it does
not ship model weights, external plugin packages, or prebuilt `rpotato`
binaries.

### Included

- New `rpotato team admit --lanes <count>` command.
- Admission decisions are recorded in the append-only ledger and SQLite
  projection.
- Normal pressure admits the requested parallel lanes.
- Missing/unknown or degraded pressure falls back to one sequential lane.
- Critical pressure returns a blocked error before any future worker launch.
- `team status` remains read-only; `team admit` is the mutating gate.
- English and Korean documentation updates for the v0.13.0 admission gate
  scope.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (157 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato team admit --lanes 2`
- `rpotato monitor status`

The smoke checks use a scratch project root under `/private/tmp` and verify
that `team admit` records a ledger event while falling back to one sequential
lane when no resource sample exists.

### Known Issues

- Policy preflight for requested writes and commands is introduced in v0.14.0.
  Full worker dispatch and file ownership allocation remain planned.
- Resource sampling is still event-driven, not continuous live polling.
- Runtime context clamp, file ownership, tool risk, approval queue, and model
  downgrade/escalation hints remain planned.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.12.0 - Team Admission Preview

Release date: 2026-07-07

This release adds the first read-only team admission surface on top of the
resource monitoring/governor work. It is still a source-only developer preview:
it does not ship model weights, external plugin packages, or prebuilt `rpotato`
binaries.

### Included

- New `rpotato team status` command.
- Reusable resource lane admission policy for future subagent/team dispatch.
- Normal pressure admits the requested parallel lanes.
- Missing/unknown or degraded pressure falls back to one sequential lane.
- Critical pressure blocks new team dispatch.
- `team status` reports latest resource sample metadata, requested lanes,
  admitted lanes, admission, dispatch-blocked flag, fallback, reason, hint, and
  read-only boundary.
- English and Korean documentation updates for the v0.12.0 team admission
  preview scope.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (153 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato monitor status`

The smoke checks use a scratch project root under `/private/tmp` and verify
that `team status` reports sequential fallback without mutating workflow state
when no resource sample exists.

### Known Issues

- `team status` is an admission preview only; it does not start subagents,
  dispatch team lanes, mutate workflows, or enforce file ownership yet.
- Resource sampling is still event-driven, not continuous live polling.
- Enforced resource admission gate is introduced in v0.13.0; remaining
  dispatcher policy stays planned.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.11.0 - Backend Chat Resource Governor

Release date: 2026-07-07

This release adds the first runtime resource governor slice for the managed
backend sidecar. It is still a source-only developer preview: it does not ship
model weights, external plugin packages, or prebuilt `rpotato` binaries.

### Included

- `rpotato backend chat` now samples backend CPU/RSS/disk resource pressure
  before model execution.
- Critical resource pressure blocks chat before the `/v1/chat/completions`
  request is sent.
- Degraded resource pressure clamps the effective max-token budget while
  preserving normal and unknown-pressure requests.
- `backend chat` and `run` output now distinguish requested max tokens from
  effective max tokens and report the governor admission/token action.
- Redacted ledger events record governor admission, token action, reason, and
  sample event ids without storing raw prompts or raw responses.
- English and Korean documentation updates for the v0.11.0 governor scope.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (149 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato backend chat --prompt smoke --max-tokens 256`
- `rpotato monitor status`

The smoke checks use a scratch project root under `/private/tmp`; without a
running backend sidecar, `backend chat` must fail closed before model execution
and must not create raw prompt/response storage.

### Known Issues

- Resource sampling is still event-driven, not continuous live polling.
- The v0.11.0 governor applies to backend chat only. Team admission preview and
  sequential fallback are introduced in v0.12.0; enforced subagent/team
  dispatch admission remains planned.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.10.0 - TUI Resource Monitor

Release date: 2026-07-07

This release extends the read-only TUI beta with a resource-pressure monitor for
the managed backend sidecar. It is still a source-only developer preview: it
does not ship model weights, external plugin packages, or prebuilt `rpotato`
binaries.

### Included

- `rpotato tui monitor` now shows resource sample count, latest pressure status,
  CPU percent, average/peak RSS, disk bytes, and recorded timestamp.
- Model monitoring summaries now include average tokens per second alongside
  total tokens and average latency.
- The monitor layout stays dependency-free and terminal-safe, including narrow
  `COLUMNS=64` rendering.
- English and Korean documentation updates for the v0.10.0 TUI monitor scope.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (148 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato tui monitor`
- `COLUMNS=64 rpotato tui monitor`

The TUI smoke used a scratch project root under `/private/tmp`, initialized
runtime state with observability schema v2, and verified that the monitor view
renders resource pressure, resource sample count, model/token counts, read-only
actions, and the beta boundary without mutating workflow state.

### Known Issues

- Resource monitor data is event-driven and reflects the latest recorded sample;
  it is not continuous live polling.
- Runtime resource governor behavior is not included in v0.10.0; the first
  backend chat governor slice is introduced in v0.11.0.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.9.0 - Backend Resource Sampling

Release date: 2026-07-07

This release adds the first backend resource monitoring slice for the managed
`llama.cpp` sidecar. It is still a source-only developer preview: it does not
ship model weights, external plugin packages, or prebuilt `rpotato` binaries.

### Included

- `resource_samples` SQLite projection schema with CPU percent, average/peak
  RSS bytes, disk bytes, sample count, pressure status, and recorded timestamp.
- Backend resource sampling on `backend start`, already-running start reuse,
  `backend status` for running sidecars, and `backend chat`.
- Redacted `backend.resource.sampled` ledger events; raw prompts, responses, and
  source text are still not persisted by default.
- `monitor status` now shows resource sample counts and the latest sampled CPU,
  RSS, disk, and pressure fields.
- `monitor prune --dry-run` now includes resource sample row counts.
- English and Korean documentation updates for the v0.9.0 monitoring scope.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (147 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato monitor status`
- `rpotato backend status`
- `rpotato monitor prune --before 30d --dry-run`

The CLI smoke used a scratch project root under `/private/tmp`, initialized
runtime state with observability schema v2, and verified that monitor output
includes resource sample count plus latest resource CPU/RSS/disk/pressure
fields.

### Known Issues

- Resource sampling is event-driven, not continuous background polling.
- TUI resource-pressure display is not included in v0.9.0; it is introduced in v0.10.0.
- Runtime resource governor behavior is not included in v0.9.0; the first
  backend chat governor slice is introduced in v0.11.0.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.8.0 - TUI Evidence And Stop Gate View

Release date: 2026-07-07

This release extends the read-only TUI beta with evidence and stop-gate status
inspection. It is still a source-only developer preview: it does not ship model
weights, external plugin packages, or prebuilt `rpotato` binaries.

### Included

- `rpotato tui evidence` shows runtime evidence store paths, runtime evidence
  record counts, project evidence artifact counts, SQLite evidence record
  counts, SQLite stop-gate result counts, and the stale evidence policy summary.
- The TUI overview now points to the evidence view.
- `monitor status` now includes SQLite evidence and stop-gate result counts.
- Read-only evidence store status API with project-local artifact counting.
- English and Korean documentation updates for the expanded TUI beta surface.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (143 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato tui evidence`
- `COLUMNS=64 rpotato tui evidence`

The TUI smoke used a scratch project root under `/private/tmp`, initialized
runtime state, and rendered the evidence view with runtime evidence,
project-evidence, observability, stop-gate count, stale-policy, validation
command, and read-only beta-boundary fields.

### Known Issues

- The TUI beta is still a one-shot read-only render, not an interactive event
  loop.
- The evidence view reports evidence/stop-gate status only; it does not pass or
  fail workflows.
- Terminal stop-gate evaluation, tool output viewer, subagent/team status, and
  plugin permission review remain future work.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.7.0 - TUI Session Transcript View

Release date: 2026-07-07

This release extends the read-only TUI beta with selected-session event
inspection. It is still a source-only developer preview: it does not ship model
weights, external plugin packages, or prebuilt `rpotato` binaries.

### Included

- `rpotato tui transcript <session-id>` shows selected-session metadata and a
  timestamp-ordered event timeline.
- `rpotato tui sessions` now points users to the transcript inspection command.
- SQLite observability read API for session events.
- Read-only boundary that keeps transcript replay, resume, cancellation, and
  workflow mutation out of the TUI beta.
- English and Korean documentation updates for the expanded TUI beta surface.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (140 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato session new`
- `rpotato state resume`
- `rpotato tui sessions`
- `rpotato tui transcript <session-id>`
- `COLUMNS=64 rpotato tui transcript <session-id>`

The TUI smoke used a scratch project root under `/private/tmp`, created a new
session, recorded a no-op resume event, listed the session, and showed the two
projected ledger events in the transcript timeline without replaying raw model
transcripts or mutating workflow state.

### Known Issues

- The TUI beta is still a one-shot read-only render, not an interactive event
  loop.
- The transcript view shows projected ledger event metadata and summaries only;
  raw event details and model transcript replay remain future agent-loop work.
- Tool output viewer, subagent/team status, plugin permission review, and
  stop-gate evidence views remain future work.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.6.0 - TUI Approval And Diff Views

Release date: 2026-07-07

This release extends the read-only TUI beta with patch approval queue and diff
inspection views. It is still a source-only developer preview: it does not ship
model weights, external plugin packages, or prebuilt `rpotato` binaries.

### Included

- `rpotato tui approvals` lists project-local patch proposal records.
- `rpotato tui diff <proposal-id>` shows proposal metadata, approve/dry-run
  command hints, and the stored unified diff.
- Patch proposal read APIs for summaries and details.
- Literal diff rendering in the TUI so `---`, `+++`, `@@`, `-`, and `+` lines
  remain readable in terminal output.
- English and Korean documentation updates for the expanded TUI beta surface.

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (138 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato patch preview --path src/lib.rs --find 1 --replace 2`
- `rpotato tui approvals`
- `rpotato tui diff <proposal-id>`
- `COLUMNS=64 rpotato tui diff <proposal-id>`

The TUI smoke used a scratch project root under `/private/tmp`, created a patch
proposal, rendered it as a pending approval record, and showed the stored
unified diff without applying or approving the patch.

### Known Issues

- The TUI beta is still a one-shot read-only render, not an interactive event
  loop.
- Approval queue and diff views inspect existing patch proposal records only;
  approval and apply still happen through `rpotato patch approve`.
- Transcript view, tool output viewer, subagent/team status, plugin permission
  review, and stop-gate evidence views remain future work.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.5.0 - Read-Only TUI Beta

Release date: 2026-07-07

This release adds the first read-only TUI beta surface for terminal-only
environments. It is still a source-only developer preview: it does not ship
model weights, external plugin packages, or prebuilt `rpotato` binaries.

### Included

- `rpotato tui` overview dashboard
- `rpotato tui monitor` model/token monitoring view
- `rpotato tui sessions` session-history view with full session ids and resume
  hint
- Dependency-free ASCII layout for SSH/Linux-server friendly rendering
- Read-only boundary that does not approve, apply, resume, cancel, or mutate
  workflows
- English and Korean documentation updates for the TUI beta surface

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (133 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato tui`
- `rpotato tui monitor`
- `rpotato tui sessions`

The TUI smoke showed project/session state, SQLite observability path, recorded
model/token metrics, session history, and the read-only beta boundary.

### Known Issues

- The TUI beta is a one-shot read-only render, not an interactive event loop.
- Approval queue, diff viewer, transcript view, subagent/team status, plugin
  permission review, and stop-gate evidence views remain future work.
- The first beta intentionally avoids a TUI framework dependency; a richer TUI
  crate can be reconsidered after interaction requirements stabilize.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.4.0 - Approved Patch Apply

Release date: 2026-07-07

This release extends the patch approval surface from dry-run gate checks to
approved patch application with rollback records and optional verification
command execution. It is still a source-only developer preview: it does not ship
model weights, external plugin packages, or prebuilt `rpotato` binaries.

### Included

- `rpotato patch approve <proposal-id> --token <token>` applies an approved
  proposal without `--dry-run`
- Current-file SHA-256 guard before apply, blocking stale proposals when the
  target file changed after preview
- Rollback record creation under `.rpotato/patch-proposals/`
- Applied SHA-256 verification after write
- `--verify-command <command>` for allow-listed simple argv verification
  commands after apply
- Verification failure handling that attempts rollback and refuses success
  reporting
- English and Korean documentation updates for the new patch application
  boundary

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (127 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- Scratch-project smoke with `RPOTATO_PROJECT_ROOT=/private/tmp/rpotato-v040-smoke`
- `rpotato patch preview --path README.md --find "Local coding agents for potato PCs." --replace "Local coding agents for potato PCs. Smoke"`
- `rpotato patch approve <generated-proposal-id> --token <generated-token> --verify-command "rg Smoke README.md"`

The patch smoke returned `status: applied`, wrote a rollback record, returned
`verification status: passed`, and reported verification exit code `0`. The
smoke ran against a `/private/tmp` project fixture, not the repository working
tree.

### Known Issues

- Patch preview still supports one explicit find/replace proposal against a
  project-local UTF-8 text file.
- Verification commands are limited to policy-allowed simple argv commands; no
  shell syntax, quoting, pipes, redirects, or environment expansion are
  supported.
- Model action output is not yet connected automatically to patch preview/apply.
- Verification output interpretation and final Korean task reporting remain
  future work.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.3.0 - Patch Diff Approval Preview

Release date: 2026-07-06

This release adds the first patch diff display and approval gate surface. It is
still a source-only developer preview: it does not ship model weights, external
plugin packages, or prebuilt `rpotato` binaries.

### Included

- `rpotato patch preview --path <path> --find <text> --replace <text>`
- Unified diff rendering for one explicit project-local text replacement
- Project-local proposal records under `.rpotato/patch-proposals/`
- Approval token display for the generated proposal
- `rpotato patch approve <proposal-id> --token <token> --dry-run`
- Approval gate verification and ledger event recording without patch application
- English and Korean documentation updates for the new patch boundary

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (123 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato patch preview --path RELEASE_NOTES.md --find "Run Skeleton Preview" --replace "Run Skeleton Preview Smoke"`
- `rpotato patch approve <generated-proposal-id> --token <generated-token> --dry-run`

The patch smoke returned `status: diff-ready`, displayed the expected unified
diff, then returned `status: gate-passed` for the dry-run approval. The target
file had no Git diff after the smoke, proving it was not modified.

### Known Issues

- Patch preview supports a single explicit find/replace proposal against a
  project-local UTF-8 text file.
- Patch approval is dry-run only in this release; it records the gate result but
  does not apply the patch.
- Agent-loop integration from model action to patch preview remains future work.
- Verification command execution, rollback handling, and final Korean reporting
  remain future work.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.2.0 - Run Skeleton Preview

Release date: 2026-07-06

This release adds the first `rpotato run` vertical slice on top of the managed
`llama.cpp` sidecar. It is still a source-only developer preview: it does not
ship model weights, external plugin packages, or prebuilt `rpotato` binaries.

### Included

- Context-aware `rpotato run "<task>"` skeleton
- Deterministic request routing into skill, mode, signals, and constraints
- Bounded repository context packing with source pointers
- Runtime-owned action candidate and next gate reporting
- Non-executing model action parsing from structured action lines or recognized action text
- Model/token/latency metrics written to the local SQLite observability projection
- Ledger events for intent, context pack, action candidate, model action, backend chat, and model run
- Source policy cleanup for versioned backend/model user agents
- English and Korean documentation updates for the new `run` boundary

### Verified In This Release

- `cargo fmt --check`
- `cargo test` (117 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato backend start --model <qwen-gguf> --ctx-size 4096`
- `rpotato run "src/intent.rs 기준으로 다음 action candidate가 무엇인지 한국어 한 문장으로 요약해."`
- `rpotato monitor models`
- `rpotato backend stop`

The latest Qwen3.5 smoke returned `model action parse: heuristic-text`,
`model action kind: patch-proposal`, `model action executable now: no`,
`guard: pass`, and `finish reason: stop`. This proves the current non-executing
runtime boundary and observability path, not patch quality or autonomous tool use.

### Supported Environments

- Development and smoke-tested environment: macOS Apple Silicon
- Source-backed backend artifact manifest still includes macOS arm64/x64, Linux
  arm64/x64, and Windows arm64/x64 `llama.cpp b9878` CPU artifacts.

### Known Issues

- `rpotato run` still does not apply patches, execute commands, or treat model
  output as an approved action.
- Model action parsing is tolerant and non-executing; robust structured action
  generation and approval UI are future work.
- TUI, hooks execution, skills execution, subagents, and team runtime are still
  design/planning surfaces.
- Model candidates remain `unverified`; no default model is promoted.
- Gemma local artifact fetch and smoke are not complete.
- RAM-fit, peak memory, mmproj need, and benchmark scoring are not complete.
- Streaming generation and cancellation are not implemented.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.

## v0.1.0 - Developer Preview

Release date: 2026-07-06

This is the first `rolling-potato` developer preview. It is a source-only
release tag for the early Rust runtime and CLI scaffold. It is not a stable
runtime contract and does not ship model weights, external plugin packages, or
prebuilt model/backend bundles.

### Included

- Rust CLI scaffold for `rpotato`
- Project/app state initialization
- Session list/new/resume projection backed by SQLite
- Runtime ledger and evidence validation surfaces
- Command/path policy checks and credential redaction
- Hook registry and fail-closed hook result validation
- Local plugin import/inspect/validate/enable/disable/remove surfaces
- Monitoring status, model summary, export, and dry-run prune surfaces
- Source-backed Qwen/Gemma model candidate manifest and evaluation gates
- Evaluation-only model artifact fetch with size and SHA-256 verification
- Managed `llama.cpp b9878` backend install/start/status/stop/health surfaces
- Non-streaming backend chat smoke path through `/v1/chat/completions`
- Qwen3.5 non-thinking smoke path with
  `chat_template_kwargs.enable_thinking=false`
- English docs plus Korean translations for the main project documents

### Verified In This Release

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `rpotato backend start --model <qwen-gguf> --ctx-size 4096`
- `rpotato backend health-check`
- `rpotato backend chat --prompt "한국어로 한 문장만 답해. 감자는 무엇인가?" --max-tokens 64`
- `rpotato backend stop`

The Qwen chat smoke returned a clean Korean response through the managed
`llama.cpp` sidecar. This proves backend/model connectivity and the
non-thinking chat path, not full model quality.

### Supported Environments

- Development and smoke-tested environment: macOS Apple Silicon
- Source-backed backend artifact manifest includes macOS arm64/x64, Linux
  arm64/x64, and Windows arm64/x64 `llama.cpp b9878` CPU artifacts.

### Known Issues

- `rpotato run` still performs intent normalization only; the full agent loop is
  not implemented.
- TUI, hooks execution, skills execution, subagents, and team runtime are still
  design/planning surfaces.
- Model candidates remain `unverified`; no default model is promoted.
- Gemma local artifact fetch and smoke are not complete.
- RAM-fit, peak memory, mmproj need, and benchmark scoring are not complete.
- Streaming generation and cancellation are not implemented.
- No prebuilt `rpotato` binary artifacts are attached to this preview release.
