# Plugin Marketplace Policy

`rolling-potato`는 외부 plugin marketplace를 도입할 수 있습니다.

다만 marketplace 지원은 법적으로 자동 안전한 기능이 아닙니다. Marketplace는 code, prompt, MCP server, executable, remote connector, license, trademark, user data risk가 한 번에 들어오는 배포 경로입니다.

이 문서는 법률 자문이 아닙니다. 구현 전 실제 배포 방식, 국가, 결제 여부, plugin 저장 방식에 따라 법무 검토가 필요합니다.

## 결정

초기 marketplace는 trusted execution store가 아니라 opt-in catalog로 취급합니다.

기본 원칙:

- 외부 marketplace는 기본 비활성화한다.
- marketplace entry는 plugin metadata와 source pointer를 제공한다.
- remote plugin package를 mirror하거나 bundle하지 않는다. 단, license가 재배포를 허용하고 provenance/checksum이 기록된 경우는 예외로 검토한다.
- plugin install/import 전 license, source, checksum, required permissions, unsupported capabilities를 보여준다.
- imported plugin은 기본 비활성화한다.
- shell, `bin/`, MCP server, background process, remote connector, file write path는 별도 승인 전 실행하지 않는다.

## Marketplace Types

### Index-only catalog

권장 초기 형태입니다.

Marketplace가 저장하는 것:

- plugin id
- display name
- description
- source runtime: `codex` or `claude-code`
- source repository or release URL
- version or ref
- checksum if available
- declared license
- required permissions
- maintainer identity

Marketplace가 저장하지 않는 것:

- plugin code copy
- model weights
- third-party binary mirror
- private credential

이 방식은 재배포 위험을 줄입니다. 그래도 trademark, misleading endorsement, malicious listing, license metadata 오류 위험은 남습니다.

### Mirrored package store

후순위입니다.

허용 조건:

- plugin license가 redistribution을 허용한다.
- source URL, source commit/ref, checksum, license text를 기록한다.
- maintainer가 removal/takedown을 처리할 수 있다.
- binary나 generated artifact가 포함될 경우 별도 artifact license와 checksum을 기록한다.

## Legal And Policy Checklist

Marketplace entry는 최소한 다음 정보를 가져야 합니다.

- plugin id
- source runtime
- source URL
- version/ref
- license id or license text pointer
- author/maintainer
- redistribution status: `not-redistributed`, `redistribution-allowed`, `unknown`
- trademark disclaimer if plugin name mentions another product
- required permissions
- privacy impact
- removal contact

`unknown` license 또는 `unknown` redistribution status는 import 가능하더라도 marketplace mirror 대상이 될 수 없습니다.

## Trademark And Compatibility Wording

`rolling-potato`는 Claude Code 또는 Codex의 공식 호환 runtime이라고 표현하지 않습니다.

허용 표현:

- "imports Codex-style plugin packages"
- "imports Claude Code-style plugin packages"
- "best-effort adapter"

피해야 할 표현:

- "official Codex compatible"
- "official Claude Code compatible"
- "Claude Code plugin store"
- "Codex marketplace mirror"

명확한 공식 승인, 상표 사용 허가, 파트너십이 없는 한 공식 연계를 암시하면 안 됩니다.

## User Consent

Marketplace에서 plugin을 가져올 때 surface는 한국어로 다음을 보여줘야 합니다.

- source
- license
- permissions
- unsupported capabilities
- shell/background/MCP/server components
- files copied into app data
- plugin data path
- whether code is redistributed or fetched from original source

사용자는 다음을 따로 승인해야 합니다.

- plugin import
- plugin enable
- shell or `bin/` execution
- MCP server start
- background process start
- remote network connector use
- plugin data purge

## Takedown And Removal

Public marketplace를 열기 전 필요한 운영 정책:

- malicious plugin 신고 경로
- license violation 신고 경로
- trademark complaint 신고 경로
- maintainer dispute handling
- emergency disable list
- marketplace index signing or checksum policy

외부 코드 기여를 받지 않는 현재 운영 정책과 별개로, marketplace listing 제보를 받을지 여부는 별도 결정이 필요합니다.

## Implementation Order

1. Local plugin import only.
2. Local curated index file.
3. Remote index-only marketplace.
4. Signed marketplace index.
5. Mirrored package store only after legal and operational review.

## References

- Codex official manual, `Plugins` and `Build plugins` sections, fetched through the local Codex documentation helper on 2026-06-29.
- OpenAI Codex Docs: [Plugins](https://developers.openai.com/codex/plugins)
- OpenAI Codex Docs: [Build plugins](https://developers.openai.com/codex/plugins/build)
- Anthropic Claude Code Docs: [Create plugins](https://code.claude.com/docs/en/plugins)
- Anthropic Claude Code Docs: [Plugins reference](https://code.claude.com/docs/en/plugins-reference)
