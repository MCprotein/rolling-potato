# Plugin Adapters

`rolling-potato` does not clone the Claude Code or Codex plugin ecosystems directly.

The goal is to absorb Claude Code/Codex-style plugin assets that users already have into safe `rpotato` runtime capabilities. Foreign plugins are import targets; they are not execution units that bypass runtime policy.

## Decision

Foreign plugin compatibility is implemented through an adapter layer.

```text
foreign plugin package
  -> source-runtime parser
  -> normalized rpotato plugin manifest
  -> capability validation
  -> permission and hook policy gate
  -> runtime-owned skill/hook/subagent/MCP execution
```

Reasons not to execute directly:

- Claude Code, Codex, and `rpotato` have different hook events, permissions, state, tools, and agent lifecycles.
- Passing foreign plugin freedom directly into a small-model runtime increases failure surface.
- User-installed plugins may contain shell commands, MCP servers, background processes, or remote app connectors.
- Bypassing the `rpotato` Korean guard, evidence gate, ledger, and ontology/context boundary breaks the core product value.

The adapter reads foreign manifests and converts them into `rpotato` capability manifests. Features that cannot be converted are marked `unsupported` instead of being ignored silently.

User decisions already reflected:

- Implement the Codex plugin adapter first.
- Implement the Claude Code plugin adapter after the Codex adapter.
- Long term, review skills, commands, agents, hooks, MCP, LSP, monitors, `bin/`, settings, and theme/output style.
- Block shell, `bin/`, MCP server, background process, remote connector, and file-write path capabilities by default; ask for explicit permission when the user tries to use them.
- Do not integrate external marketplaces.
- Limit plugin use to local plugin directory import owned by the user.

## No Marketplace Integration

`rpotato` does not integrate with external plugin marketplaces, registries, catalogs, or mirrors.

Excluded:

- remote marketplace browsing
- marketplace search
- marketplace install
- marketplace metadata sync
- curated remote catalog
- third-party plugin package mirror
- plugin store operation

Allowed:

- local path import
- local path dry-run inspect
- local path validate
- imported plugin enable/disable
- imported plugin remove with keep-data/purge-data

This only covers plugin packages that already exist on the user's local machine. `rpotato` does not guarantee where the user got the plugin. It checks only local directory manifest, file layout, capability, and permission risk.

## Protocol Status

Codex and Claude Code each document plugin rules inside their own runtimes. They do not share one standard plugin protocol.

Classification:

- runtime-specific: plugin manifest, skill format, command format, hook event, agent lifecycle, marketplace metadata
- external protocol: MCP server, LSP server
- `rpotato` protocol: normalized plugin manifest, runtime policy, ledger, evidence gate, Korean output guard

Therefore, `rpotato` does not run Codex plugins and Claude Code plugins in the same way. Each source-runtime parser reads a foreign dialect and converts it into the internal `rpotato` manifest.

## Confirmed External Surfaces

### Codex

Based on the Codex official manual, a Codex plugin is a reusable workflow bundle. Confirmed components:

- skills
- app integrations
- MCP servers
- `.codex-plugin/plugin.json`
- marketplace source

Codex plugins can be scaffolded with `@plugin-creator` and distributed through a repo or personal marketplace.

### Claude Code

Based on Anthropic official docs, a Claude Code plugin is a self-contained directory. Confirmed components:

- skills and commands
- agents
- hooks
- MCP servers
- LSP servers
- monitors
- `bin/`
- settings
- `.claude-plugin/plugin.json`
- marketplace source

Claude Code distinguishes user/project/local/managed scopes and provides marketplace, enable/disable, trust boundary, and managed policy concepts.

This document treats only the facts above as confirmed external surfaces. Detailed schemas and version-specific behavior must be rechecked against official docs immediately before implementation.

## rpotato Plugin Model

In `rpotato`, a plugin is a capability package. Runtime core still owns actual execution.

```json
{
  "schemaVersion": 1,
  "id": "imported.example-plugin",
  "displayName": "Example Plugin",
  "sourceRuntime": "claude-code",
  "sourceRef": {
    "kind": "local-path",
    "path": "./vendor/example-plugin"
  },
  "sourceManifestHash": null,
  "status": "imported",
  "capabilities": [
    {
      "kind": "skill",
      "id": "example-plugin.review",
      "sourcePath": "skills/review/SKILL.md",
      "status": "validated"
    }
  ],
  "unsupported": [],
  "requiredPermissions": [],
  "sourceCheckedAt": null
}
```

Status values:

- `imported`: manifest was parsed, but not enabled
- `validated`: capability mapping passed static validation
- `enabled`: user enabled it for a scope
- `blocked`: policy blocked it
- `unsupported`: source capability cannot be represented safely

## CLI Surface

Initial command shape:

```sh
rpotato plugin import --from codex <local-plugin-path>
rpotato plugin import --from claude-code <local-plugin-path>
rpotato plugin list
rpotato plugin inspect <id>
rpotato plugin validate <id>
rpotato plugin enable <id>
rpotato plugin disable <id>
rpotato plugin remove <id> --keep-data
rpotato plugin remove <id> --purge-data
```

## Current Implementation

v0.38.0 implements the local import/inspection surface plus instruction-only
execution adapters for Codex and Claude Code:

- `rpotato plugin import --from codex <local-path> --dry-run`
- `rpotato plugin import --from claude-code <local-path> --dry-run`
- `rpotato plugin import --from codex <local-path>`
- `rpotato plugin import --from claude-code <local-path>`
- `rpotato plugin list`
- `rpotato plugin inspect <id>`
- `rpotato plugin validate <id>`
- `rpotato plugin enable <id>`
- `rpotato plugin disable <id>`
- `rpotato plugin remove <id> --keep-data`
- `rpotato plugin remove <id> --purge-data`
- `rpotato skill list` discovery for enabled, instruction-only Codex/Claude Code capabilities
- `rpotato skill run imported.codex.<plugin>.<skill> "<request>"`
- `rpotato skill run imported.claude-code.<plugin>.<skill-or-command> "<request>"`

Import accepts only local directories and rejects remote URLs, marketplace, registry, catalog sources, `..` path traversal, and source symlinks.
The normalized manifest schema is version 2 and records:

- source runtime and adapter version
- permission policy version
- source manifest SHA-256
- imported source snapshot SHA-256
- mapped capability summaries
- required permissions and blocked permissions
- unsupported or review-required assets

Storage:

```text
rpotato app data root/
  plugins/
    imported/<plugin-id>/
      source/
      rpotato-plugin.json
      validation-report.json
    data/<plugin-id>/
```

`validate` and `enable` re-check the imported `source/` directory against the stored manifest and snapshot hashes. If the imported source drifts, `rpotato` marks the plugin `blocked`, records a ledger event, and requires re-import from a trusted local directory.

`enable` changes registry state; it does not grant general execution authority.
The Codex adapter admits only a canonical `skills/<name>/SKILL.md` regular file
up to 64 KiB when:

- its YAML frontmatter contains a `name` matching the directory and a non-empty `description`
- its instruction body is non-empty
- the imported plugin is enabled under the current adapter and permission-policy versions
- the imported source snapshot still matches its recorded SHA-256
- the same skill directory has no script or other permission-requiring capability

The v0.38 Claude Code adapter admits canonical `skills/<name>/SKILL.md` and
flat `commands/<name>.md` regular files up to 64 KiB. The path name is the
invocation name, the instruction body must be non-empty, and a missing
description falls back to the first Markdown paragraph. When a skill and
command have the same name, the skill wins. If the source manifest declares a
custom `commands` path, the default `commands/` directory is not admitted
because that replacement semantic is not yet mapped.

The imported instruction is untrusted prompt content. Execution is always read-only and goes through the native context, lifecycle-hook, typed-action, Korean-output, evidence, ledger, and stop-gate contracts. Admission and completion events bind the plugin ID, skill ID, source path, and source SHA-256. A model response cannot turn the imported skill into a patch or command path.

Claude Code dynamic shell interpolation is classified as `shell-command` and
blocked. Source-runtime frontmatter such as `allowed-tools`, `context`, `agent`,
`hooks`, `model`, `paths`, and argument or environment substitution is reported
as unsupported and cannot widen the native read-only contract. Root
`SKILL.md`, custom component paths, agents, hooks, MCP, LSP, monitors, `bin/`,
settings, themes, output styles, channels, dependencies, and user configuration
are also reported explicitly when they are not mapped.

Shell, skill scripts, `bin/`, MCP, app integrations, plugin hooks, LSP,
monitor/background processes, runtime settings, remote connectors, sensitive
config, and file-write capabilities remain listed in the permission report and
blocked. v0.38 has no approval grant or execution surface for these risky
capabilities; `plugin enable` is not such an approval. A later implementation
must add an explicit capability-scoped approval lease and revalidation before
any of them can execute.

`skill list` shows enabled candidates without mutating plugin state. `skill run` performs the authoritative source snapshot and frontmatter revalidation. Plugins imported under an older adapter or permission-policy version must be re-imported from a trusted local directory.

All import commands must be dry-run friendly:

```sh
rpotato plugin import --from claude-code ./my-plugin --dry-run
```

Dry-run output must include:

- detected source runtime
- source manifest path
- source manifest hash
- source snapshot hash
- capability list
- required permissions
- blocked permissions
- shell/background/server components
- unsupported components
- files that would be copied into app data
- plugin data path

## Compatibility Mapping

| Source capability | rpotato target | MVP behavior |
| --- | --- | --- |
| Codex skill | `rpotato` skill | Execute canonical instruction-only skills through the native read-only runtime after snapshot/frontmatter revalidation |
| Codex MCP server | MCP adapter entry | Disabled until user enables and approves server command |
| Codex app integration | Unsupported | Requires separate app connector contract |
| Claude Code skill | `rpotato` skill | Execute canonical default-path instruction-only skills through the native read-only runtime |
| Claude Code command | `rpotato` skill | Execute flat default-path prompt-backed commands only when no dynamic shell bypass exists |
| Claude Code agent | `rpotato` subagent role | Explicitly unsupported until tool/path/model fields map safely |
| Claude Code hook | `rpotato` hook | Explicitly unsupported and blocked until event and hook type map to the runtime hook contract |
| Claude Code MCP server | MCP adapter entry | Reported and blocked; no execution surface exists |
| Claude Code LSP server | Future code-intel adapter | Not MVP |
| Claude Code monitor | Future observer capability | Not MVP; background process risk is high |
| Claude Code `bin/` executable | Tool asset | Not on PATH by default; may be callable only through tool policy |
| Claude Code theme/output style | TUI/theme setting | Later UI-only import |
| Claude Code channel | Unsupported | Requires remote/event ingestion policy |
| Marketplace entry | Unsupported | Marketplace integration is out of scope |

## Permission Rules

Foreign plugin import never grants execution permission by itself.

Required gates:

1. Parse the source manifest.
2. Reject remote URLs and marketplace sources.
3. Copy or snapshot the local plugin into `rpotato` app data only after user approval.
4. Compute source manifest hash.
5. Emit a static capability report.
6. Mark every shell, HTTP, MCP, background, file-write, and download path as permission-requiring.
7. Require explicit enablement per scope.
8. Execute every imported capability through `rpotato` runtime policy.
9. Record import, enable, execution, denial, and removal events in the ledger.

Foreign plugin code must not:

- mutate project files outside `rpotato` patch policy
- run shell commands outside command policy
- start background processes silently
- download artifacts silently
- write state to plugin install root
- access credentials without explicit sensitive config handling
- mark stop gate complete without `rpotato` evidence
- inject model/license/benchmark claims without source records

## Permission Lease And Revalidation

Plugin enablement and per-capability approvals are leases, not permanent grants.

Revalidation is required when any of these changes:

- source manifest hash
- normalized `rpotato` plugin manifest schema version
- runtime/plugin adapter version
- command policy or tool policy version
- requested capability scope
- plugin source path or imported snapshot hash
- capability gains shell, `bin/`, MCP, background, remote, download, or file-write behavior

On revalidation failure:

- keep the plugin imported but mark affected capabilities `blocked`
- keep plugin data unless the user requests purge
- record the reason and previous approval id in the ledger
- require a new explicit approval before execution

## Storage

Suggested layout:

```text
rpotato app data root/
  plugins/
    imported/
      <plugin-id>/
        source/
        rpotato-plugin.json
        validation-report.json
    data/
      <plugin-id>/
```

`source/` is versioned imported content. `data/` is persistent plugin state.

Removal behavior:

- `rpotato plugin remove <id> --keep-data`: remove imported package, keep plugin data.
- `rpotato plugin remove <id> --purge-data`: remove imported package and plugin data.
- Global `rpotato uninstall --keep-cache` keeps plugin packages and data.
- Global `rpotato uninstall --purge-cache` removes imported plugin packages and app-level plugin data.

Project-local `.rpotato/` data is not deleted by global uninstall.

## Validation

Adapter validation must cover:

- source manifest parse
- source runtime detection
- remote URL and marketplace source rejection
- path traversal rejection
- unsupported capability reporting
- command/background process detection
- MCP server command approval requirement
- hook event mapping
- skill manifest/frontmatter conversion
- subagent tool/path boundary conversion
- sensitive user config detection
- permission lease revalidation after manifest, policy, adapter, or scope changes
- ledger record creation
- remove with `--keep-data` and `--purge-data`

## Implementation Phases

1. Codex local plugin import parser for `.codex-plugin/plugin.json`.
2. Codex static inspect/validate report with no execution.
3. Codex imported skill execution through `rpotato skill run`.
4. Codex MCP server import with explicit enable and approval.
5. Claude Code local plugin import parser for `.claude-plugin/plugin.json`.
6. Claude Code static inspect/validate report with no execution.
7. Claude Code skill and command import. **Implemented in v0.38.0 for canonical default paths.**
8. Claude Code hook mapping for safe lifecycle events.
9. Claude Code agent to `rpotato` subagent role mapping.
10. LSP, monitor, `bin/`, settings, and theme/output import where safe.
11. TUI local plugin browser and permission review.

## Source Policy

External plugin specs are product facts. Do not infer fields or compatibility from memory when implementing.

Before implementation:

- re-check official Codex plugin docs
- re-check official Claude Code plugin docs
- pin supported source-runtime schema versions when possible
- fixture-test real local example plugins
- fixture-test remote URL and marketplace source rejection
- record unsupported fields rather than silently ignoring them

## References

- Codex official manual, `Plugins`, `Build plugins`, and `Build skills` sections, fetched through the local Codex documentation helper on 2026-07-16.
- OpenAI Codex Docs: [Plugins](https://developers.openai.com/codex/plugins)
- OpenAI Codex Docs: [Build plugins](https://developers.openai.com/codex/plugins/build)
- Anthropic Claude Code Docs: [Create plugins](https://code.claude.com/docs/en/plugins)
- Anthropic Claude Code Docs: [Plugins reference](https://code.claude.com/docs/en/plugins-reference)
- Anthropic Claude Code Docs: [Claude Code settings](https://code.claude.com/docs/en/settings)
