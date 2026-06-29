# Plugin Adapters

`rolling-potato`는 Claude Code와 Codex의 플러그인 생태계를 그대로 복제하지 않습니다.

목표는 사용자가 이미 가진 Claude Code/Codex 플러그인형 자산을 `rpotato` 런타임의 안전한 capability로 흡수하는 것입니다. 외부 플러그인은 import 대상이지, runtime policy를 우회하는 실행 단위가 아닙니다.

## 결정

외부 플러그인 호환은 중간 adapter layer로 구현합니다.

```text
foreign plugin package
  -> source-runtime parser
  -> normalized rpotato plugin manifest
  -> capability validation
  -> permission and hook policy gate
  -> runtime-owned skill/hook/subagent/MCP execution
```

직접 실행하지 않는 이유:

- Claude Code, Codex, `rpotato`는 hook event, permission, state, tool, agent lifecycle이 다르다.
- 작은 모델용 런타임에서는 외부 플러그인의 자유도를 그대로 넘기면 실패 표면이 커진다.
- 사용자가 설치한 플러그인이 shell, MCP server, background process, remote app connector를 포함할 수 있다.
- `rpotato`의 한국어 guard, evidence gate, ledger, ontology/context boundary를 우회하면 제품의 핵심 가치가 깨진다.

따라서 adapter는 foreign manifest를 읽고 `rpotato` capability manifest로 변환합니다. 변환할 수 없는 기능은 조용히 무시하지 않고 `unsupported`로 표시합니다.

## 확인된 외부 표면

### Codex

Codex 공식 매뉴얼 기준으로 Codex plugin은 재사용 가능한 workflow 묶음입니다. 확인된 구성 요소:

- skills
- app integrations
- MCP servers
- `.codex-plugin/plugin.json`
- marketplace source

Codex plugin은 `@plugin-creator`로 scaffolding할 수 있고, repo 또는 personal marketplace에 등록해 배포할 수 있습니다.

### Claude Code

Anthropic 공식 문서 기준으로 Claude Code plugin은 self-contained directory입니다. 확인된 구성 요소:

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

Claude Code는 user/project/local/managed scope를 구분하고, marketplace, enable/disable, trust boundary, managed policy를 제공합니다.

이 문서는 위 사실만 확정된 외부 표면으로 사용합니다. 세부 schema와 version별 동작은 구현 직전에 공식 문서를 다시 확인해야 합니다.

## rpotato Plugin Model

`rpotato`에서 plugin은 capability package입니다. 실제 실행 단위는 여전히 runtime core가 소유합니다.

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
rpotato plugin import --from codex <path-or-marketplace>
rpotato plugin import --from claude-code <path-or-marketplace>
rpotato plugin list
rpotato plugin inspect <id>
rpotato plugin validate <id>
rpotato plugin enable <id>
rpotato plugin disable <id>
rpotato plugin remove <id> --keep-data
rpotato plugin remove <id> --purge-data
```

All import commands must be dry-run friendly:

```sh
rpotato plugin import --from claude-code ./my-plugin --dry-run
```

Dry run output must include:

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

## Permission Rules

Foreign plugin import never grants execution permission by itself.

Required gates:

1. Parse the source manifest.
2. Copy or snapshot the plugin into `rpotato` app data only after user approval if the source is remote.
3. Compute source manifest hash.
4. Emit a static capability report.
5. Mark every shell, HTTP, MCP, background, file-write, and download path as permission-requiring.
6. Require explicit enablement per scope.
7. Execute every imported capability through `rpotato` runtime policy.
8. Record import, enable, execution, denial, and removal events in the ledger.

Foreign plugin code must not:

- mutate project files outside `rpotato` patch policy
- run shell commands outside command policy
- start background processes silently
- download artifacts silently
- write to plugin install root as state
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

1. Local import parser for Codex and Claude Code plugin directories.
2. Static inspect/validate report with no execution.
3. Imported skill execution through `rpotato skill run`.
4. MCP server import with explicit enable and approval.
5. Claude/Codex hook mapping for safe lifecycle events.
6. Claude agent to `rpotato` subagent role mapping.
7. Marketplace source support after signing/checksum/source policy is defined.
8. TUI plugin browser and permission review.

## Source Policy

External plugin specs are product facts. Do not infer fields or compatibility from memory when implementing.

Before implementation:

- re-check official Codex plugin docs
- re-check official Claude Code plugin docs
- pin supported source-runtime schema versions when possible
- fixture-test real local example plugins
- record unsupported fields rather than silently ignoring them

## References

- Codex official manual, `Plugins` and `Build plugins` sections, fetched through the local Codex documentation helper on 2026-06-29.
- Anthropic Claude Code Docs: [Create plugins](https://code.claude.com/docs/en/plugins)
- Anthropic Claude Code Docs: [Plugins reference](https://code.claude.com/docs/en/plugins-reference)
- Anthropic Claude Code Docs: [Claude Code settings](https://code.claude.com/docs/en/settings)
