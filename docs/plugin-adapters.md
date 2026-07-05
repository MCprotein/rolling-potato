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

Phase 3 currently implements:

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

Import accepts only local directories and rejects remote URLs, marketplace, registry, catalog sources, `..` path traversal, and source symlinks.

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

`enable` changes registry state; it does not grant execution authority. Shell, `bin/`, MCP, hook, and background process capabilities remain listed in the permission report and blocked until runtime policy approval.

All import commands must be dry-run friendly:

```sh
rpotato plugin import --from claude-code ./my-plugin --dry-run
```

Dry-run output must include:

- detected source runtime
- source manifest path
- capability list
- required permissions
- shell/background/server components
- unsupported components
- files that would be copied into app data
- plugin data path

## Compatibility Mapping

| Source capability | rpotato target | MVP behavior |
| --- | --- | --- |
| Codex skill | `rpotato` skill | Import only after manifest/frontmatter parse and policy review |
| Codex MCP server | MCP adapter entry | Disabled until user enables and approves server command |
| Codex app integration | Unsupported | Requires separate app connector contract |
| Claude Code skill | `rpotato` skill | Import as namespaced skill with explicit tool/evidence requirements |
| Claude Code command | `rpotato` skill | Import as prompt-backed skill if no direct shell bypass exists |
| Claude Code agent | `rpotato` subagent role | Import only if tool/path/model fields map safely |
| Claude Code hook | `rpotato` hook | Import only when event and hook type map to runtime hook contract |
| Claude Code MCP server | MCP adapter entry | Disabled until user enables and approves server command |
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
- ledger record creation
- remove with `--keep-data` and `--purge-data`

## Implementation Phases

1. Codex local plugin import parser for `.codex-plugin/plugin.json`.
2. Codex static inspect/validate report with no execution.
3. Codex imported skill execution through `rpotato skill run`.
4. Codex MCP server import with explicit enable and approval.
5. Claude Code local plugin import parser for `.claude-plugin/plugin.json`.
6. Claude Code static inspect/validate report with no execution.
7. Claude Code skill and command import.
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

- Codex official manual, `Plugins` and `Build plugins` sections, fetched through the local Codex documentation helper on 2026-06-29.
- OpenAI Codex Docs: [Plugins](https://developers.openai.com/codex/plugins)
- OpenAI Codex Docs: [Build plugins](https://developers.openai.com/codex/plugins/build)
- Anthropic Claude Code Docs: [Create plugins](https://code.claude.com/docs/en/plugins)
- Anthropic Claude Code Docs: [Plugins reference](https://code.claude.com/docs/en/plugins-reference)
- Anthropic Claude Code Docs: [Claude Code settings](https://code.claude.com/docs/en/settings)
