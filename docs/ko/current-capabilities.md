# 현재 기능

이 문서는 `rolling-potato v0.41.0`의 읽기 쉬운 상태 지도입니다. 하나의 긴
명령 목록을 반복하지 않고 런타임 책임별로 기능을 묶었습니다.

[한국어 README](../../README.ko.md) · [문서 인덱스](README.md) ·
[English](../current-capabilities.md)

> 이 문서는 기능 안내서입니다. 정확한 명령 문법은 설치된 바이너리의
> `rpotato --help`를 기준으로 확인하십시오.

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
포함됩니다.

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
request나 verification command를 자동으로 반복하지 않습니다.

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

대표 진입점:

```sh
rpotato backend doctor
rpotato backend install-plan
rpotato backend install
rpotato backend start [--model <path>] [--ctx-size <tokens>]
rpotato backend status
rpotato backend chat --prompt <text> [--stream]
rpotato backend cancel
rpotato backend stop
```

전송된 model request는 자동 재시도하지 않습니다. Monitoring record에는 raw
prompt/response text를 저장하지 않습니다.

[백엔드 어댑터](backend-adapters.md)와
[런타임 아키텍처](runtime-architecture.md)를 참고하십시오.

## 5. 모델과 로컬 근거

Model command는 출처 기반 후보, manifest data, download plan, local registry,
promotion/install evidence를 표시합니다. Model weight는 managed storage로
다운로드하며 저장소에 commit하지 않습니다.

대표 진입점:

```sh
rpotato model list
rpotato model manifest
rpotato model inspect <id>
rpotato model download-plan <id>
rpotato model fetch-candidate <id> --for-evaluation
rpotato model eval-plan <id>
rpotato model benchmark-plan <id>
rpotato model promote <id> --evidence <file>
rpotato model install <id>
rpotato model default [<id>]
```

Qwen과 Gemma는 평가 후보입니다. Artifact, license, backend, memory/mmproj,
smoke, measured local benchmark evidence가 install gate를 통과하기 전에는
기본 모델이 아닙니다.

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

`rpotato tui`는 terminal에서 interactive line controller를 시작하고
non-terminal 환경에서는 read-only overview를 유지합니다. TUI는 canonical
state를 직접 수정하지 않고 monitoring, session, 검증된 transcript/tool view,
approval, diff, evidence, resume, cancel을 제공합니다.

대표 진입점:

```sh
rpotato tui
rpotato tui interactive
rpotato tui monitor
rpotato tui sessions
rpotato tui transcript <session-id>
rpotato tui approvals
rpotato tui diff <proposal-id>
rpotato tui evidence
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
- Plugin script, agent, external hook, MCP/LSP, background process, remote
  connector, write grant는 실행 권한을 얻지 못합니다.
- `monitor prune`은 dry-run만 지원합니다.
- HTML monitoring은 local static export이며 server나 remote dashboard가
  아닙니다.
- `v0.41.0` 이후 버전은 현재 정의되어 있지 않습니다.

버전 기록과 다음 버전 규칙은 [로드맵](ROADMAP.md)에 있습니다.
