# 플러그인 Adapter

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

사용자 결정:

- Codex plugin adapter를 먼저 구현한다.
- Claude Code plugin adapter는 Codex adapter 뒤에 구현한다.
- 장기적으로는 skills, commands, agents, hooks, MCP, LSP, monitors, `bin/`, settings, theme/output style까지 전부 검토 대상에 포함한다.
- shell, `bin/`, MCP server, background process, remote connector, file write path는 기본 차단하고, 사용자가 쓰려고 할 때 별도 권한 prompt로 설정을 풀 수 있게 한다.
- 외부 marketplace는 연동하지 않는다.
- Plugin 사용 경로는 사용자가 직접 소유한 local plugin directory import로 제한한다.

## Marketplace 미연동

`rpotato`는 외부 plugin marketplace, registry, catalog, mirror를 연동하지 않습니다.

제외:

- remote marketplace browsing
- marketplace search
- marketplace install
- marketplace metadata sync
- curated remote catalog
- third-party plugin package mirror
- plugin store 운영

허용:

- local path import
- local path dry-run inspect
- local path validate
- imported plugin enable/disable
- keep-data/purge-data 옵션을 가진 imported plugin remove

이 방식은 사용자가 이미 로컬에 가지고 있는 plugin package만 대상으로 합니다. 사용자가 plugin을 어디서 받았는지는 `rpotato`가 보증하지 않습니다. `rpotato`는 local directory의 manifest, file layout, capability, permission risk만 검사합니다.

## Protocol 상태

Codex와 Claude Code는 각각 자기 runtime 안에서 plugin 규칙을 문서화하고 있습니다. 하지만 두 runtime이 공유하는 하나의 표준 plugin protocol은 아닙니다.

분류:

- runtime별 규칙: plugin manifest, skill format, command format, hook event, agent lifecycle, marketplace metadata
- 외부 protocol: MCP server, LSP server
- `rpotato` protocol: normalized plugin manifest, runtime policy, ledger, evidence gate, Korean output guard

따라서 `rpotato`는 Codex plugin과 Claude Code plugin을 같은 방식으로 실행하지 않습니다. 각 source runtime parser가 외부 방언을 읽고, `rpotato` 내부 manifest로 변환합니다.

## 확인된 외부 표면

### Codex 표면

Codex 공식 매뉴얼 기준으로 Codex plugin은 재사용 가능한 workflow 묶음입니다. 확인된 구성 요소:

- skills
- app integrations
- MCP servers
- `.codex-plugin/plugin.json`
- marketplace source

Codex plugin은 `@plugin-creator`로 scaffolding할 수 있고, repo 또는 personal marketplace에 등록해 배포할 수 있습니다.

### Claude Code 표면

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

## 플러그인 모델

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

상태 값:

- `imported`: manifest는 parse되었지만 아직 enable되지 않은 상태
- `validated`: capability mapping이 static validation을 통과한 상태
- `enabled`: 사용자가 특정 scope에 대해 enable한 상태
- `blocked`: policy가 차단한 상태
- `unsupported`: source capability를 안전하게 표현할 수 없는 상태

## CLI 명령 Surface

초기 command 형태:

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

## 현재 구현

v0.27.0의 현재 구현:

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

Import는 local directory만 허용하며 remote URL, marketplace, registry, catalog source, `..` path traversal, source symlink를 차단합니다.
Normalized manifest schema는 version 2이며 다음을 기록합니다.

- source runtime과 adapter version
- permission policy version
- source manifest SHA-256
- imported source snapshot SHA-256
- mapping된 capability summary
- required permission과 blocked permission
- unsupported 또는 review-required asset

저장 위치:

```text
rpotato app data root/
  plugins/
    imported/<plugin-id>/
      source/
      rpotato-plugin.json
      validation-report.json
    data/<plugin-id>/
```

`validate`와 `enable`은 imported `source/` directory를 저장된 manifest/snapshot hash와 다시 대조합니다. Imported source가 바뀌면 `rpotato`는 plugin을 `blocked`로 표시하고 ledger event를 남기며, 신뢰 가능한 local directory에서 다시 import하도록 요구합니다.

`enable`은 실행 권한 부여가 아니라 registry 상태 변경입니다. shell, `bin/`, MCP, hook, LSP, monitor/background process, runtime setting, remote connector, sensitive config, file-write capability는 permission report에 남고 runtime policy 승인 전까지 기본 차단입니다.

모든 import command는 dry-run friendly해야 합니다.

```sh
rpotato plugin import --from claude-code ./my-plugin --dry-run
```

dry-run output에는 다음이 포함되어야 합니다.

- 감지한 source runtime
- source manifest path
- source manifest hash
- source snapshot hash
- capability list
- required permission
- blocked permission
- shell/background/server component
- unsupported component
- app data로 copy될 file 목록
- plugin data path

## 호환성 Mapping

| Source capability | rpotato target | MVP 동작 |
| --- | --- | --- |
| Codex skill | `rpotato` skill | manifest/frontmatter parse와 policy review 후에만 import |
| Codex MCP server | MCP adapter entry | 사용자가 enable하고 server command를 승인하기 전까지 disabled |
| Codex app integration | Unsupported | 별도 app connector contract 필요 |
| Claude Code skill | `rpotato` skill | 명시적 tool/evidence requirement를 가진 namespaced skill로 import |
| Claude Code command | `rpotato` skill | 직접 shell bypass가 없을 때 prompt-backed skill로 import |
| Claude Code agent | `rpotato` subagent role | tool/path/model field가 안전하게 mapping될 때만 import |
| Claude Code hook | `rpotato` hook | event와 hook type이 runtime hook contract로 mapping될 때만 import |
| Claude Code MCP server | MCP adapter entry | 사용자가 enable하고 server command를 승인하기 전까지 disabled |
| Claude Code LSP server | future code-intel adapter | MVP 아님 |
| Claude Code monitor | future observer capability | MVP 아님. background process 위험이 큼 |
| Claude Code `bin/` executable | tool asset | 기본 PATH에 올리지 않음. tool policy를 통해서만 호출 가능 |
| Claude Code theme/output style | TUI/theme setting | 이후 UI-only import |
| Claude Code channel | Unsupported | remote/event ingestion policy 필요 |
| Marketplace entry | Unsupported | marketplace integration은 범위 밖 |

## Permission 규칙

foreign plugin import는 그 자체로 실행 권한을 부여하지 않습니다.

필수 gate:

1. source manifest를 parse한다.
2. remote URL과 marketplace source를 거부한다.
3. 사용자 승인 후에만 local plugin을 `rpotato` app data에 copy 또는 snapshot한다.
4. source manifest hash를 계산한다.
5. static capability report를 출력한다.
6. 모든 shell, HTTP, MCP, background, file-write, download path를 permission-requiring으로 표시한다.
7. scope별 명시적 enable을 요구한다.
8. 모든 imported capability는 `rpotato` runtime policy를 통해 실행한다.
9. import, enable, execution, denial, removal event를 ledger에 기록한다.

foreign plugin code는 다음을 하면 안 됩니다.

- `rpotato` patch policy 밖에서 project file을 변경
- command policy 밖에서 shell command 실행
- background process를 조용히 시작
- artifact를 조용히 download
- plugin install root에 state 기록
- 명시적 sensitive config handling 없이 credential 접근
- `rpotato` evidence 없이 stop gate 완료 처리
- source record 없이 model/license/benchmark claim 주입

## Permission Lease와 재검증

Plugin enablement와 capability별 approval은 영구 권한이 아니라 lease입니다.

다음이 바뀌면 재검증이 필요합니다.

- source manifest hash
- normalized `rpotato` plugin manifest schema version
- runtime/plugin adapter version
- command policy 또는 tool policy version
- 요청 capability scope
- plugin source path 또는 imported snapshot hash
- capability가 shell, `bin/`, MCP, background, remote, download, file-write 동작을 새로 얻는 경우

재검증에 실패하면:

- plugin import 상태는 유지하되 영향받은 capability를 `blocked`로 표시한다.
- 사용자가 purge를 요청하지 않는 한 plugin data는 유지한다.
- ledger에 사유와 이전 approval id를 기록한다.
- 실행 전 새 명시적 approval을 요구한다.

## 저장소

권장 layout:

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

`source/`는 versioned imported content입니다. `data/`는 persistent plugin state입니다.

삭제 동작:

- `rpotato plugin remove <id> --keep-data`: imported package를 제거하고 plugin data는 유지한다.
- `rpotato plugin remove <id> --purge-data`: imported package와 plugin data를 함께 제거한다.
- global `rpotato uninstall --keep-cache`는 plugin package와 data를 유지한다.
- global `rpotato uninstall --purge-cache`는 imported plugin package와 app-level plugin data를 제거한다.

project-local `.rpotato/` data는 global uninstall로 삭제하지 않습니다.

## 검증

adapter validation은 다음을 포함해야 합니다.

- source manifest parse
- source runtime detection
- remote URL과 marketplace source rejection
- path traversal rejection
- unsupported capability reporting
- command/background process detection
- MCP server command approval requirement
- hook event mapping
- skill manifest/frontmatter conversion
- subagent tool/path boundary conversion
- sensitive user config detection
- manifest, policy, adapter, scope 변경 이후 permission lease 재검증
- ledger record creation
- `--keep-data`와 `--purge-data` remove

## 구현 단계

1. `.codex-plugin/plugin.json`용 Codex local plugin import parser.
2. 실행 없는 Codex static inspect/validate report.
3. `rpotato skill run`을 통한 Codex imported skill 실행.
4. 명시적 enable과 approval을 요구하는 Codex MCP server import.
5. `.claude-plugin/plugin.json`용 Claude Code local plugin import parser.
6. 실행 없는 Claude Code static inspect/validate report.
7. Claude Code skill과 command import.
8. 안전한 lifecycle event에 대한 Claude Code hook mapping.
9. Claude Code agent를 `rpotato` subagent role로 mapping.
10. 안전한 경우 LSP, monitor, `bin/`, settings, theme/output import.
11. TUI local plugin browser와 permission review.

## Source 정책

외부 plugin spec은 제품 사실입니다. 구현할 때 기억만으로 field나 compatibility를 추론하지 않습니다.

구현 전 확인:

- 공식 Codex plugin docs를 다시 확인한다.
- 공식 Claude Code plugin docs를 다시 확인한다.
- 가능하면 지원하는 source-runtime schema version을 고정한다.
- 실제 local example plugin으로 fixture test를 수행한다.
- remote URL과 marketplace source rejection을 fixture test한다.
- unsupported field를 조용히 무시하지 않고 기록한다.

## 참고 자료

- Codex official manual, `Plugins` and `Build plugins` sections, fetched through the local Codex documentation helper on 2026-06-29.
- OpenAI Codex Docs: [Plugins](https://developers.openai.com/codex/plugins)
- OpenAI Codex Docs: [Build plugins](https://developers.openai.com/codex/plugins/build)
- Anthropic Claude Code Docs: [Create plugins](https://code.claude.com/docs/en/plugins)
- Anthropic Claude Code Docs: [Plugins reference](https://code.claude.com/docs/en/plugins-reference)
- Anthropic Claude Code Docs: [Claude Code settings](https://code.claude.com/docs/en/settings)
