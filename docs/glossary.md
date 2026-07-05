# Glossary

This document fixes the core terminology used by `rolling-potato`.

## Agent Runtime

A system that turns user requests into one controlled flow across model, context, tools, patches, verification, and reporting.

It is the product body of `rolling-potato`. The CLI is a surface that uses the runtime.

## Surface

An entrypoint through which users access the runtime.

The MVP surface is the `rpotato` CLI. Surfaces handle display and approval while delegating policy and state to the runtime core.

## CLI Surface

The `rpotato` command.

Responsibilities:

- command parsing
- user prompt
- approval display
- diff display
- progress display
- final report display

## TUI Surface

An interactive terminal surface for runtime state, transcript, diff, approval, tool output, subagent/team status, and evidence.

The TUI does not own policy. It forwards user decisions to the runtime core.

## Runtime Core

The internal layer that owns state, policy, ontology, context, agent loop, evidence, and stop gates.

Responsibilities:

- state and ledger
- hooks
- skills
- model/backend management
- ontology/context lifecycle
- subagents and teams
- tool policy
- patch and verification
- Korean output guard

## Hook

A runtime lifecycle control point.

Examples:

- `pre_model_request`
- `pre_tool_call`
- `pre_patch_apply`
- `stop_gate`

Hooks cannot widen permissions and cannot become looser than runtime policy.

## Skill

A reusable runtime capability.

It is not only a prompt template. It also carries context requirements, allowed tools, hooks, evidence requirements, and stop criteria.

## Plugin Adapter

A compatibility layer that converts Claude Code/Codex-style plugin packages into `rpotato` skill, hook, subagent, and MCP capabilities.

Foreign plugins are not executed directly. They pass through import, inspect, validate, and enable stages. Unsupported features are recorded as `unsupported`.

## Local Plugin Import

The install path that converts a Claude Code/Codex-style plugin directory already owned locally by the user into `rpotato` capabilities.

`rpotato` does not integrate with external plugin marketplaces, registries, catalogs, or mirrors.

## Subagent

A bounded worker agent executed by the runtime core under a parent workflow.

Subagents do not own global state. They inherit runtime policy and context boundaries.

## Team Runtime

A runtime feature that coordinates multiple subagents by stage under one parent workflow.

Team runtime manages plan, dispatch, execute, review, verify, merge, and report behind ledgers and evidence gates.

## Backend

The engine that runs model inference. The MVP backend is the `llama.cpp` sidecar.

The backend does not own coding-agent policy.

## Model Artifact

A model file such as GGUF. It is a third-party artifact separate from the `rolling-potato` code license.

## Manifest

A file that contains trust information for model or backend artifacts.

Required information:

- source
- URL
- license
- checksum
- file size
- compatibility

## Agent Loop

The staged runtime flow for work.

MVP stages:

- planner
- executor
- verifier
- reporter

## Tool Policy

Runtime policy that controls side effects such as file writes, command execution, downloads, and deletion.

Model output cannot bypass tool policy.

## Layer A Facts

Repository facts collected deterministically by the runtime.

Examples:

- file list
- source hash
- package manifest
- test command candidates
- entrypoint candidates

## Layer B Ontology

Project meaning structure. It may be enriched by the runtime or agents, but it requires source references and confidence.

Examples:

- domain entity
- relationship
- ownership
- invariant
- workflow
- open question

## Source Pointer

A stable reference to the original source location instead of only a context snippet.

Important decisions must promote source pointers to original-file reads first.

## Context Index

The runtime-maintained index used for context search. Snippets are hints, not authoritative source.

## Evidence

Verification results that support completion or claims.

Examples:

- test output
- command exit code
- file hash
- source URL
- benchmark log

## Ledger

An append-only record of runtime events and evidence.

Current-state views and ledgers are separate.

## Observability Store

The local monitoring store for querying per-model token usage, latency, backend health, guard results, tool results, and stop-gate results.

The default direction is a SQLite projection. The append-only ledger is the event source; SQLite is the query layer for TUI, `doctor`, and benchmark reports.

## Stop Gate

The runtime gate that decides whether work is complete. Even if the model says a task is done, it is not complete when evidence is missing.
