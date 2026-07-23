# 현재 기능

이 문서는 출시된 `rolling-potato v0.47.1` runtime의 읽기 쉬운 상태 지도입니다.
하나의 긴 명령 목록을 반복하지 않고 런타임 책임별로 기능을 묶었습니다.

[한국어 README](../../README.ko.md) · [문서 인덱스](README.md) ·
[English](../current-capabilities.md)

> 이 문서는 기능 안내서입니다. 정확한 명령 문법은 설치된 바이너리의
> `rpotato --help`를 기준으로 확인하십시오.

## 설치, 첫 실행, 업데이트 (`v0.42.0`-`v0.47.1`)

GitHub Release archive에서 압축을 푼 binary는 사용자 전용 CLI directory에
자기 자신을 설치·갱신하고 zsh, bash, fish 또는 Windows 사용자 PATH에 해당
directory를 등록할 수 있습니다. 하나의 소유 block을 사용하므로 등록은
멱등입니다. 설치된 binary로 `init`을 실행하면 등록을 보정하며, shell
profile 또는 환경 등록 실패는 runtime state 초기화를 막지 않고 report로
남깁니다.

```sh
rpotato install
rpotato install --clean --dry-run
rpotato install --clean --yes
rpotato uninstall --clean --dry-run
rpotato uninstall --clean --yes
rpotato init
rpotato update --check
rpotato update
```

일반 install은 config, model, backend asset, project state를 보존합니다.
Clean install은 전역 application-data root와 현재 project의 `.rpotato`만
삭제하며 명시적 확인이 필요하고 관리형 backend 또는 generation이 실행
중이면 차단됩니다. dry-run은 두 삭제 대상과 함께 binary·PATH 등록의 정확한
변경 상태를 표시합니다. Backend/generation 게시와 삭제는 하나의
cross-process lease로 직렬화하며, process 생존 확인 오류도 삭제 차단으로
처리합니다.

Clean uninstall은 설치 binary, 소유한 PATH 등록, 전역 application data, 현재
project의 `.rpotato`를 제거합니다. 압축을 푼 실행 원본과 source repository는
사용자 소유 file로 보존합니다. Windows self-delete는 현재 process 종료 직후
완료하도록 예약합니다.

TUI 시작 시 6시간 cache와 짧은 request timeout으로 공식 최신 stable GitHub
Release를 확인하며 offline 시작을 막지 않습니다. 새 버전은 `/update` 안내와
함께 표시하고 `rpotato update --check`로도 명시적으로 확인할 수 있습니다.
업데이트 적용은 rpotato가 소유한 관리형 설치본으로 제한하며, 현재 platform의
정확한 release archive와 대응 SHA-256 sidecar를 검증하고 정확한 binary entry만
staging합니다. Unix는 atomic replace를 사용하고 Windows는 process 종료 뒤 교체를
예약하며 이동 실패 시 이전 binary로 rollback합니다.

선택된 session/workflow의 `current-state` pointer는 각 project의
`.rpotato/state/` directory에 격리합니다. 기존 설치 전역 pointer는 binding이
일치하는 project에만 이전하며, 돌아온 project의 동기화는 저장된 binding이
canonical ledger의 ancestor일 때만 허용합니다. System startup error는 원래
message와 exit code를 보존합니다.

Read-only TUI view는 legacy v1 ledger prefix 뒤에 hash-chained v2 suffix가 오는
정상 업그레이드 형태를 허용합니다. History를 재작성하지 않으면서 legacy digest,
physical chain, head count, bounded-read genesis를 계속 검증합니다.

## 1. 에이전트 루프와 컨텍스트

런타임은 요청 정규화, 의도·스킬 route 선택, 제한된 저장소 context 구성,
backend 호출, runtime-owned typed action parsing을 수행합니다. 모델 텍스트가
도구를 직접 실행하지는 않습니다.

대표 진입점:

```sh
rpotato run "<request>"
rpotato intent classify "<request>"
rpotato intent routes
rpotato skill list
rpotato skill run <id> "<request>"
```

현재 보호 장치에는 active request와 resume context가 공유하는 source budget,
source-pointer evidence, policy 검사, lifecycle hook, 한국어 최종 보고 guard가
포함됩니다. Context compaction은 측정 사용량 75%에서 자동으로 시작하거나 TUI
`/compact`로 수동 실행하며, context limit의 40%를 목표로 최근 transcript record
최대 4개를 보존합니다. 제한된 semantic rationale 호출 한 번을 실행할 수 없으면
deterministic typed extraction만으로 계속합니다.

언어 guard는 한국어 문장과 함께 숫자, 수식, 코드, 경로, URL, 범위가 제한된 기술
제목을 허용합니다. CJK 누출이나 이어지는 외국어 문장에는 사실을 보존하는 한국어
재작성 한 번을 시도한 뒤 유효한 한국어 line을 안전하게 투영하고, 사용할 수 있는
응답이 없을 때만 차단합니다.

기본 TUI는 저장소 도구가 필요 없는 일반 지식, 계산, 설명, 글쓰기 질문을 위한
가벼운 범용 답변 경로도 제공합니다. 인터넷 검색을 명시한 요청과 최신성이 필요한
질문은 Brave Search REST API를 직접 호출하는 제한된 읽기 전용 검색을 사용합니다.
별도 MCP process나 provider SDK는 사용하지 않으며 `BRAVE_SEARCH_API_KEY`는
environment에서만 읽고 저장하거나 출력하지 않습니다. 현재 질문은 HTTPS로 해당
서비스에 전송되며 redirect는 허용하지 않습니다. 반환 text는 신뢰하지 않는
context로만 취급하고 웹 문서의 지시는 실행 권한을 얻지 못합니다. 최대 4개 출처가
6 KiB 근거 budget을 공유하며, runtime이 model이 만든 citation과 출처 block을 제거한
뒤 검증된 HTTPS 출처 link를 직접 표시합니다. 요약이나 언어 보정이 사용할 수 없어도
성공한 검색의 출처 link는 유지합니다. 사용자가 offline을 요청하면 이 경로를
사용하지 않습니다.

[런타임 아키텍처](runtime-architecture.md), [명령 정책](command-policy.md),
[훅](hooks.md), [스킬](skills.md)을 참고하십시오.

## 2. 영속 세션과 복구

Canonical append-only ledger가 session과 workflow history를 소유합니다.
SQLite는 재생성 가능한 projection이며 resume 권위가 아닙니다. Durable
transcript에는 검증된 user, visible-model, tool, evidence record를 남기고 hidden
reasoning과 raw backend response는 제외합니다.

대표 진입점:

```sh
rpotato state
rpotato state reconcile
rpotato state resume
rpotato session list
rpotato session history
rpotato session new
rpotato session resume <session-id>
rpotato resume [<session-id>]
rpotato continue [<session-id>]
rpotato cancel
```

복구는 일치하는 안전한 checkpoint만 계속합니다. 결과가 불확실한 backend
request나 verification command를 자동으로 반복하지 않습니다. Incremental
compaction checkpoint는 project, session, 이전 checkpoint, transcript boundary에
binding된 immutable hash chain입니다. Field는 신뢰하지 않는 resume hint이며
canonical transcript, ledger, instruction, source artifact가 계속 정본입니다.

[상태 수명주기](state-lifecycle.md)와 [관측성](observability.md)을
참고하십시오.

## 3. 패치와 검증 워크플로

독립 preview surface는 diff-only proposal을 만듭니다. Agent workflow 안에서
생성된 patch에는 별도 apply·verification gate, source hash 검사, 일회성
credential, rollback record가 적용됩니다.

대표 진입점:

```sh
rpotato patch preview --path <path> --find <text> --replace <text>
rpotato patch approve <proposal-id> --token <token> --dry-run
rpotato patch approve <proposal-id> --token <token>
rpotato patch verify <proposal-id> --token <token>
rpotato patch token-rotate <proposal-id>
rpotato evidence validate <artifact-pointer>
```

Standalone preview는 승인하거나 적용할 수 없습니다. 검증은 미리 결합되고
policy가 허용한 command만 실행합니다.

[명령 정책](command-policy.md)과 [상태 수명주기](state-lifecycle.md)를
참고하십시오.

## 4. 백엔드 수명주기

관리형 `llama.cpp` 경로는 출처 기반 install plan, archive 검증, staging 설치,
process lifecycle, health check, chat, streaming, cancellation, CPU/RSS/disk
sampling을 제공합니다.

TUI가 이 경로를 자동으로 준비합니다. 세부 진단 entrypoint는 고급 namespace 아래에
있습니다.

```sh
rpotato debug backend doctor
rpotato debug backend install-plan
rpotato debug backend status
rpotato debug backend start [--model <path>] [--ctx-size <tokens>]
rpotato debug backend stop
```

전송된 model request는 자동 재시도하지 않습니다. Monitoring record에는 raw
prompt/response text를 저장하지 않습니다.

[백엔드 어댑터](backend-adapters.md)와
[런타임 아키텍처](runtime-architecture.md)를 참고하십시오.

## 5. 모델과 로컬 근거

최초 설정과 `/model`은 출처 기반 후보를 보여주고 managed download, 검증, 선택,
backend start를 처리합니다. Model/version, quantization, download size, context limit,
RAM 상태, license, evidence를 보여주되 측정하지 않은 RAM/capability를 verified로
표시하지 않습니다. Model weight는 managed storage로 다운로드하며 저장소에
commit하지 않습니다.

세부 평가와 promotion 명령은 고급 surface로 유지합니다.

```sh
rpotato debug model list
rpotato debug model inspect <id>
rpotato debug model fetch-candidate <id> --for-evaluation
rpotato debug model benchmark-plan <id>
rpotato debug model promote <id> --evidence <file>
rpotato debug model install <id>
```

최초 실행에서 명시적으로 선택한 model은 고정 source, license, backend compatibility
source, artifact size, SHA-256이 재검증되면 해당 host의 runtime default가 될 수 있습니다.
Registry evidence는 `source-backed-manifest`로 남으며 보편적 RAM 적합성, capability,
benchmark claim은 미확정입니다. 고급 `model install`/promotion workflow는 더 엄격한
local evidence gate를 유지합니다.

[모델 출처 정책](model-source-policy.md), [모델 manifest](model-manifest.md),
[모델 평가](model-eval.md), [모델 라이선스](model-licenses.md)를
참고하십시오.

## 6. 훅, 스킬, 플러그인 어댑터

Runtime-owned hook과 built-in skill은 durable agent loop 안에서 실행됩니다.
Local Codex/Claude Code형 plugin directory를 import, inspect, validate, enable,
disable, remove할 수 있습니다.

대표 진입점:

```sh
rpotato hooks list
rpotato hooks validate-result <json>
rpotato plugin import --from codex <local-path> --dry-run
rpotato plugin import --from claude-code <local-path> --dry-run
rpotato plugin list
rpotato plugin inspect <id>
rpotato plugin validate <id>
rpotato plugin enable <id>
rpotato plugin disable <id>
rpotato plugin remove <id> --keep-data
```

Imported instruction은 신뢰하지 않는 prompt content입니다. Enable만으로 shell,
background, remote, 민감한 설정, file-write 권한을 얻을 수 없습니다. Script,
external hook, MCP/app integration, remote plugin source는 차단하거나 지원하지
않습니다.

[플러그인 어댑터](plugin-adapters.md), [훅](hooks.md),
[스킬](skills.md)을 참고하십시오.

## 7. 서브에이전트와 팀

런타임은 active parent workflow 아래에서 제한된 sequential child 하나를
실행할 수 있습니다. Team execution은 resource admission, lane ownership,
policy preflight, deterministic reconciliation, failure handling, stop gate를
추가합니다.

대표 진입점:

```sh
rpotato subagent launch --role <role> --task <text> --tool <tool> --read <path>
rpotato subagent status [subagent-id]
rpotato subagent cancel <subagent-id>
rpotato team status
rpotato team admit --lanes <count>
rpotato team dispatch --lanes <count> --write-owner <lane:path>
rpotato team governor --lanes <count> --context-tokens <tokens>
```

Worker는 command 직접 실행, patch 적용, nested worker 시작, parent approval
boundary 우회를 할 수 없습니다.

[서브에이전트](subagents.md)와 [팀 런타임](team-runtime.md)을
참고하십시오.

## 8. 모니터링과 벤치마크

Local append-only ledger와 SQLite projection은 token, latency, CPU, memory,
disk, pressure, backend, model, session, benchmark, evidence, team metric을
제공합니다.

대표 진입점:

```sh
rpotato monitor status
rpotato monitor models
rpotato monitor baseline
rpotato monitor optimize
rpotato monitor export --format jsonl
rpotato monitor export --format csv
rpotato monitor export --format html > rpotato-monitor.html
rpotato monitor prune --before 30d --dry-run
rpotato benchmark validate <fixture.json>
rpotato benchmark record --fixture <fixture.json>
rpotato benchmark run --fixture <fixture.json> --prompt <artifact>
rpotato benchmark report --format jsonl
```

HTML export는 JavaScript, external asset, network request, 두 번째 telemetry
source of truth가 없는 self-contained local file입니다. `measured-locally`
benchmark record는 public benchmark parity를 주장하지 않습니다.

[관측성](observability.md)과 [벤치마크](benchmarks.md)를 참고하십시오.

## 9. CLI와 TUI 화면

인자 없는 `rpotato`는 terminal에서 기본 interactive line controller를 시작하고
non-terminal 환경에서는 read-only overview를 유지합니다. 최초 실행은 source-backed
model을 선택하고 managed backend를 자동 준비해 GGUF path 입력 없이 model을 시작합니다.
TUI는 canonical
state를 직접 수정하지 않고 monitoring, session, 검증된 transcript/tool view,
approval, diff, evidence, resume, cancel을 제공하며 일반 텍스트를 agent 요청으로
전달합니다. `rpotato tui`는 호환 alias입니다.

첫 frame은 compact welcome을 사용하고 대화가 시작되면 한 줄 identity header로
전환합니다. Focused bordered composer의 semantic status line은 `model | context
used/limit | compaction | backend | session` 순서를 유지합니다. 한국어와 wide
character turn은 terminal display cell 기준으로 줄바꿈하며 `/more`와 `/back`으로
모든 긴 응답 line을 확인할 수 있습니다. `/`를 입력하면 Enter 전에 command
palette가 열리며, 표시 항목은 `/help`와 같은 registry를 사용합니다.
↑↓, `Ctrl+P`/`Ctrl+N`, Enter, Esc로 palette를 조작하고 `/model`도 같은 키보드
선택기를 사용합니다. Terminal이 표준 escape sequence를 보내면 Option/Alt 단어 이동과
Command/Meta 줄 이동을 처리합니다. `/attach <경로>`와 붙여넣은 image/text 경로는
local 첨부 badge가 됩니다. 제한된 text/code 파일은 다음 요청에 포함하지만 현재
text-only model/backend 계약에서는 image inference를 dispatch 전에 차단합니다.
`/search <질문>`은 최신성·자연스러운 한국어 검색 자동 routing과 같은 읽기 전용 웹
근거 답변 경로를 명시적으로 실행합니다.
대표 public 진입점은 다음과 같습니다.

```sh
rpotato
rpotato init
rpotato doctor
rpotato run "<request>"
rpotato debug --help
```

[TUI](tui.md), [CLI 출력 스타일](cli-output-style.md),
[디자인](DESIGN.md)을 참고하십시오.

## 10. 알려진 경계

- 제한 없는 일반 tool orchestration은 구현하지 않았습니다.
- Runtime-owned native hook만 실행합니다.
- Interactive TUI의 source installation은 지원하는 Unix 경로에서만
  성공하며, 미지원 platform은 mutation 전에 차단합니다.
- Team worker는 제한된 evidence와 non-executing patch proposal만 반환하고,
  team reconciliation은 worker-authored patch를 적용하지 않습니다.
- Plugin script, agent, external hook, 임의 MCP/LSP connector, background process,
  remote connector, write grant는 실행 권한을 얻지 못합니다. 직접 연결하는 Brave
  검색 adapter는 제한된 신뢰하지 않는 text만 반환하며 tool이나 mutation을
  dispatch할 수 없습니다.
- `monitor prune`은 dry-run만 지원합니다.
- HTML monitoring은 local static export이며 server나 remote dashboard가
  아닙니다.
- `v0.42.0` 범위는 사용자 전용 설치, 환경 보정, clean reinstall, clean
  uninstall이며 package-manager channel은 추가하지 않습니다.

버전 기록과 다음 버전 규칙은 [로드맵](ROADMAP.md)에 있습니다.
