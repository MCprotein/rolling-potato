# MVP Acceptance Criteria

## Goal

The MVP must prove that a small local model can safely finish small coding tasks.

The scope is intentionally narrow. It is not the full interactive coding-agent product yet. It starts with a CLI surface and proves one runtime-owned flow from local model setup to a small patch and verification.

The full product target remains a runtime that can replace Claude Code/Codex. Hooks, skills, local plugin adapters, subagents, team runtime, and TUI are required capabilities for the replacement-level beta after the MVP vertical slice.

## User Scenario

Representative flow:

1. User runs `rpotato init`.
2. CLI surface forwards the init request to runtime core.
3. Runtime core checks the environment.
4. Runtime core installs or verifies the managed `llama.cpp` sidecar.
5. Runtime core suggests a source- and checksum-verified model candidate.
6. CLI surface asks for download approval.
7. Runtime core downloads the model and verifies the hash.
8. User runs `rpotato run "테스트 실패 고쳐줘"` inside a project.
9. Runtime core finds relevant files and ontology/context, then creates a small patch proposal.
10. CLI surface shows the diff and asks for apply approval.
11. Runtime core applies the patch after approval.
12. CLI surface asks for verification-command approval.
13. Runtime core interprets verification results and the reporter emits a final Korean report.

## Functional Criteria

### Install And Init

- `rpotato` runs on macOS and Windows.
- `rpotato init` checks OS, CPU architecture, RAM, and free disk space.
- Unsupported environments show a clear Korean reason.
- Model weights are not included in the `rpotato` install artifact.
- `llama.cpp` backend is installed or verified as a managed sidecar without requiring global user installation.
- `rpotato uninstall --keep-cache` and `rpotato uninstall --purge-cache` show planned paths before deletion.
- CLI surface does not bypass runtime-core policy decisions.

### Model Management

- `rpotato model list` distinguishes installable and installed models.
- `rpotato model install <id>` shows verified download size and license information before download.
- Downloads can resume after interruption.
- SHA-256 is verified after download.
- Models are not registered when verification fails.

### Backend Management

- Runtime core starts or reuses the `llama.cpp` sidecar.
- `rpotato doctor` shows backend binary, model file, port, and health-check state.
- `rpotato backend doctor` separately checks managed backend binary, version, executable bit, and health check.
- Backend startup failures are narrowed into Korean cause reports.
- Runtime core records token, latency, and backend health metrics per model run.

### Repository Work

- Runtime core can read files inside the current project.
- Default behavior reads only necessary files.
- Files outside the project are excluded by default.
- Generated/vendor large directories are excluded from default indexing.
- Runtime core does not create patches from snippets without source pointers.

### Patch Flow

- Changes are shown as unified diff or internal patch format.
- User approval is required before application.
- Files are not written before approval.
- Patch failure preserves original files.
- Unrelated formatting churn is avoided.

### Verification Flow

- Verification commands require approval before execution.
- Command output is summarized, while key failure lines are preserved.
- After failure, the next action is narrowed to one step.
- Success reports include which verification passed.

### Korean Output

- Final natural-language reports are Korean-only.
- Code, commands, file paths, package names, and quoted logs are allowed exceptions.
- English, Chinese, or Japanese leakage triggers one regeneration attempt.
- If regeneration still fails, output closes with a Korean error message.

## Out Of Scope For First Vertical Slice

These are deferred, not rejected:

- GUI app
- multimodal screenshot understanding
- multiple models loaded simultaneously
- remote GPU server as the default
- automatic destructive command execution
- large automatic refactors
- npm wrapper or Homebrew/Scoop packaging
- external plugin marketplace, registry, catalog, or mirror integration
- remote URL plugin install
- direct execution of external plugins

Required after the first vertical slice:

- lifecycle hooks
- reusable skills
- local plugin import adapter
- bounded subagents
- team runtime
- TUI surface

## Definition Of Done

MVP is complete when:

- macOS and Windows run `rpotato init`, `model install`, `chat`, `run`, and `doctor`
- one source-verified GGUF model candidate installs from a manifest
- sidecar backend health check passes
- a small fixture repository completes patch proposal, approval, application, and verification
- runtime core owns state, permissions, context, tool result, and evidence
- per-model token/latency/resource metrics are recorded locally
- final-report Korean guard is tested
- destructive-action policy violation tests are zero
- uninstall keep-cache and purge-cache smoke tests pass

## Open Decisions

- exact `Qwen3.5-4B` GGUF artifact
- default quantization level
- config file path and format
- operation log location
- Windows binary packaging strategy
- fixture benchmark repository design
