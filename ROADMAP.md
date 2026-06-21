# Roadmap

## Phase 0: Project Definition

- [x] Product positioning
- [x] MVP acceptance criteria
- [x] Rust + `llama.cpp` sidecar direction
- [x] Korean-only output requirement
- [x] Open-source operating docs

## Phase 1: CLI Foundation

- [ ] Rust workspace scaffold
- [ ] `rpotato` command router
- [ ] config path resolution
- [ ] `rpotato doctor`
- [ ] structured error reporting in Korean

## Phase 2: Model Management

- [ ] model manifest schema
- [ ] `rpotato model list`
- [ ] resumable model download
- [ ] SHA-256 verification
- [ ] model registration in local config

## Phase 3: Backend Runtime

- [ ] `llama.cpp` sidecar discovery
- [ ] sidecar process lifecycle
- [ ] health check
- [ ] streaming response path
- [ ] backend diagnostics in `doctor`

## Phase 4: First Agent Loop

- [ ] repo file discovery
- [ ] context packing
- [ ] planner/executor/verifier/reporter sequence
- [ ] diff preview
- [ ] approved patch apply
- [ ] approved verification command execution

## Phase 5: Reliability Gates

- [ ] Korean output guard
- [ ] destructive command policy tests
- [ ] invalid diff rejection
- [ ] fixture benchmark suite
- [ ] release build pipeline
