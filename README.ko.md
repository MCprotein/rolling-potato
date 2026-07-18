# rolling-potato

<p align="center">
  <a href="README.md">ENGLISH</a> |
  <strong>한국어</strong>
</p>

`rolling-potato`는 저사양 PC에서도 굴러가는 로컬 우선 코딩 에이전트 런타임입니다.

- CLI 명령어: `rpotato`
- 태그라인: `Local coding agents for potato PCs.`
- 한국어 포지셔닝: `똥컴에서도 굴러가는 로컬 코딩 에이전트`

## 핵심 방향

`rolling-potato`는 Claude Code나 Codex를 작은 모델로 흉내 내는 도구가 아닙니다.

핵심 가정은 반대입니다. 작은 로컬 모델은 유용하지만 취약하므로, 런타임이 선택지를 좁히고 실패를 관리해야 합니다.

> 작은 모델에는 작은 프롬프트가 아니라 작은 모델용 런타임이 필요합니다.

따라서 이 프로젝트의 주요 제품은 CLI wrapper나 프롬프트 묶음이 아니라 다음을 책임지는 로컬 런타임입니다.

- 모델 설치와 캐시 관리
- 로컬 추론 백엔드 실행
- 저장소 컨텍스트 압축
- 도구 권한 정책
- 훅과 스킬
- 서브에이전트와 팀 실행
- Claude Code/Codex형 플러그인 어댑터
- 패치 생성과 검증
- 재시도 제어
- 모델별 token/latency/CPU/memory/resource monitoring
- CLI와 TUI surface
- 한국어 전용 최종 응답 검증

`rpotato` CLI는 이 런타임을 사용하는 첫 번째 surface입니다. Claude Code, Codex, 가재코드처럼 사용자는 CLI에서 작업을 시작하지만, 세션 상태, 도구 권한, 온톨로지, context, agent loop, 검증 gate는 CLI가 아니라 runtime core가 소유해야 합니다.

최종 목표는 Claude Code/Codex를 대신해 쓸 수 있는 로컬 agent runtime입니다. 따라서 hooks, skills, subagents, team runtime, TUI는 선택 기능이 아니라 제품 완성에 필요한 1급 capability입니다.

## 대상 사용자

- 한국어 사용자
- Claude Code / Codex 같은 구독형 코딩 에이전트가 부담스러운 사용자
- 16 GB RAM 수준의 저사양 또는 중급 노트북 사용자
- 코드와 모델 실행을 로컬에서 처리하고 싶은 사용자
- 로컬 LLM 도구를 자세히 모르지만 코딩 도움은 받고 싶은 사용자

현재 binary 지원 우선순위는 maintainer 주도 GitHub Release artifact 기준 macOS, Linux, Windows입니다.

## MVP 범위

첫 번째 유용한 버전은 다음을 만족해야 합니다.

1. `rpotato` 명령으로 실행된다.
2. 사용자 동의 후 권장 GGUF 모델 하나를 다운로드한다.
3. 로컬 추론 백엔드를 시작하거나 재사용한다.
4. 한국어로 대화한다.
5. 로컬 저장소를 읽고 필요한 파일을 찾는다.
6. 작은 수정 패치를 제안한다.
7. 적용 전 diff를 보여준다.
8. 사용자 승인 후 검증 명령을 실행한다.
9. 최종 보고는 한국어만 사용한다.

자세한 기준은 [docs/mvp.md](docs/mvp.md)를 따릅니다.

## Runtime Surface 초안

MVP의 첫 surface는 CLI입니다.
TUI는 replacement-level runtime에 필요한 필수 surface입니다.

```sh
rpotato init
rpotato chat
rpotato run "이 에러 고쳐줘"
rpotato intent classify "테스트 실패 고쳐줘"
rpotato tui
rpotato tui interactive
rpotato tui monitor
rpotato tui sessions
rpotato tui transcript <session-id>
rpotato tui approvals
rpotato tui diff <proposal-id>
rpotato tui evidence
rpotato state reconcile
rpotato state resume
rpotato session list
rpotato session history
rpotato session resume <session-id>
rpotato session new
rpotato resume
rpotato resume <session-id>
rpotato continue
rpotato continue <session-id>
rpotato evidence validate logs/test.log
rpotato skill list
rpotato skill run fix-test "tests/api.rs의 실패를 고쳐줘"
rpotato subagent launch --role explore --task "module 구조를 확인해줘" --tool read_file --read src/lib.rs
rpotato subagent status
rpotato subagent cancel <subagent-id>
rpotato plugin import --from codex ./my-plugin
rpotato plugin validate imported.codex.my-plugin
rpotato plugin enable imported.codex.my-plugin
rpotato skill run imported.codex.my-plugin.review "이 저장소를 리뷰해줘"
rpotato team status
rpotato team admit --lanes 2
rpotato team admit --lanes 2 --write README.md --command "cargo test"
rpotato team admit --lanes 2 --write-owner 1:src/app.rs --write-owner 2:src/cli.rs
rpotato team dispatch --lanes 2 --write-owner 1:src/team.rs --write-owner 2:src/cli.rs
rpotato team dispatch --lanes 3 --write-owner 1:src/team.rs --write-owner 2:src/cli.rs --write-owner 3:src/app.rs --failed-lane 2 --failure "worker timed out"
rpotato team governor --lanes 2 --context-tokens 6000 --context-limit 4096 --model-tier standard
rpotato model list
rpotato model manifest
rpotato model inspect qwen3.5-4b
rpotato model eval-plan qwen3.5-4b
rpotato model benchmark-plan qwen3.5-4b
rpotato model fetch-candidate qwen3.5-4b --for-evaluation
rpotato model promote qwen3.5-4b --evidence evidence/qwen3.5-4b-local.json
rpotato model install qwen3.5-4b
rpotato model default gemma-4-e4b
rpotato model default
rpotato backend start --ctx-size 4096
rpotato backend doctor
rpotato cache status
rpotato monitor status
rpotato monitor models
rpotato monitor baseline
rpotato monitor optimize
rpotato monitor export --format jsonl
rpotato monitor export --format csv
rpotato monitor prune --before 30d --dry-run
rpotato benchmark validate benchmarks/fixtures/sample.json
rpotato benchmark record --fixture benchmarks/fixtures/sample.json
rpotato benchmark run --fixture benchmarks/fixtures/executable-smoke.json --prompt benchmarks/prompts/executable-smoke.txt --max-tokens 32
rpotato benchmark report --format jsonl
rpotato uninstall --keep-cache
rpotato uninstall --purge-cache
rpotato doctor
rpotato config
```

Plugin adapter는 marketplace를 거치지 않습니다. 사용자가 직접 가진 Codex/Claude Code형 plugin directory를 local path로 import하고, `rpotato`가 manifest와 권한 위험을 inspect/validate한 뒤 enable합니다. Enable된 Codex plugin은 canonical instruction-only `skills/<name>/SKILL.md`를 `skill list`와 `skill run imported.codex.<plugin>.<skill>`로 노출할 수 있습니다. Runtime은 실행 시 imported snapshot과 frontmatter를 다시 검증하고 capability를 read-only로 유지합니다. Skill script, hook, MCP server, app integration, shell/background process, remote connector, write capability는 계속 차단하며 plugin enable은 이들의 실행 승인이 아닙니다.

초기 흐름은 명확해야 합니다.

1. 사용자가 `rpotato init` 또는 `rpotato model install`을 실행한다.
2. CLI surface가 요청을 runtime core에 전달한다.
3. runtime core가 OS, 아키텍처, RAM, 디스크 여유 공간을 확인한다.
4. runtime core가 관리형 `llama.cpp` sidecar를 설치 또는 확인한다.
5. runtime core가 출처 검증된 manifest 기준으로 적합한 모델을 추천한다.
6. CLI surface가 사용자 승인 prompt를 표시한다.
7. runtime core가 이어받기 가능한 방식으로 모델을 다운로드한다.
8. runtime core가 해시를 검증한다.
9. runtime core가 local smoke, RAM/mmproj, measured benchmark evidence가 있을 때만 후보를 승격한다.
10. runtime core가 로컬 설정에 모델을 등록한다.
11. runtime core가 추론 백엔드를 시작하거나 기존 프로세스를 재사용한다.

모델 가중치는 `rpotato` release artifact에 포함하지 않습니다.
`llama.cpp`도 사용자가 직접 전역 설치하지 않아도 되는 관리형 sidecar를 기본 경로로 둡니다.
삭제는 `rpotato uninstall --keep-cache` 또는 `rpotato uninstall --purge-cache`로 실행하며, 캐시 보존 여부를 사용자가 선택합니다.

## 기술 방향

MVP의 기본 결정은 다음과 같습니다.

- 구현 언어: Rust
- 기본 백엔드: `llama.cpp`
- 모델 포맷: GGUF
- 실행 방식: 관리형 `llama-server` sidecar
- 첫 surface: CLI
- 필수 surface: TUI
- 필수 runtime capability: hooks, skills, subagents, team runtime
- 필수 compatibility 경계: Claude Code/Codex형 plugin adapter
- 필수 monitoring store: local SQLite projection + append-only ledger
- 필수 model evidence index: 제품 논의에서 LLM wiki라고 부르는 model knowledge base
- 우선 평가 후보: `unsloth/Qwen3.5-4B-GGUF`의 `Qwen3.5-4B` Q4_K_M GGUF, local runtime 검증 전 `unverified`
- 비교 평가 후보: Google `Gemma 4 E4B` IT QAT q4_0 GGUF, local runtime 검증 전 `unverified`

`llama.cpp`는 backend일 뿐 모델 후보가 아닙니다. 모델 후보는 Qwen/Gemma 라인으로만 추적하며, 라이선스와 artifact 관련 claim은 [docs/model-licenses.md](docs/model-licenses.md)에 출처와 함께 기록합니다.

자세한 구조는 [docs/architecture.md](docs/architecture.md)를 따릅니다.
런타임 계층은 [docs/runtime-architecture.md](docs/runtime-architecture.md)를 따릅니다.
모델 evidence index는 [docs/model-knowledge-base.md](docs/model-knowledge-base.md)를 따르고, 용어는 [docs/glossary.md](docs/glossary.md)를 따릅니다.

## 중요 제약

사용자에게 보이는 자연어 출력은 한국어여야 합니다. 코드 블록, 파일 경로, 명령어, 원문 로그처럼 의미상 보존해야 하는 내용은 예외로 다루되, 최종 설명문이 영어, 중국어, 일본어를 무작위로 섞어서는 안 됩니다.

모델 선택만으로 이 요구사항을 만족할 수 없으므로 런타임에서 다음을 강제합니다.

- 최종 응답의 언어 누수 검사
- 코드 블록과 자연어 설명 분리
- 혼합 언어 감지 시 1회 재생성
- 재생성 실패 시 한국어 오류로 fail closed

## 현재 상태

이 저장소는 제품 정의와 초기 Rust runtime/CLI scaffold 단계입니다.

현재 구현된 최소 명령:

- `rpotato doctor`
- `rpotato backend doctor`
- `rpotato backend install-plan`
- `rpotato backend install`
- `rpotato backend start [--model <path>] [--ctx-size <tokens>]`
- `rpotato backend status`
- `rpotato backend stop`
- `rpotato backend cancel`
- `rpotato backend verify-archive <path> --sha256 <hash>`
- `rpotato backend health-check`
- `rpotato backend chat --prompt <text> [--max-tokens <tokens>] [--stream] [--timeout-ms <ms>]`
- `rpotato cache status`
- `rpotato config`
- `rpotato init`
- `rpotato run "<request>"`
- `rpotato intent classify "<request>"`
- `rpotato intent routes`
- `rpotato state`
- `rpotato state reconcile`
- `rpotato state resume`
- `rpotato session list`
- `rpotato session history`
- `rpotato session resume <session-id>`
- `rpotato session new`
- `rpotato resume`
- `rpotato resume <session-id>`
- `rpotato continue`
- `rpotato continue <session-id>`
- `rpotato cancel`
- `rpotato evidence validate <artifact-pointer>`
- `rpotato skill list`
- `rpotato skill run <id> "<request>"`
- `rpotato subagent launch --role <role> --task <text> --tool <tool> --read <path>`
- `rpotato subagent status [subagent-id]`
- `rpotato subagent cancel <subagent-id>`
- `rpotato policy schema`
- `rpotato policy check-command <command>`
- `rpotato policy check-path --read <path>`
- `rpotato policy check-path --write <path>`
- `rpotato policy redact <text>`
- `rpotato hooks list`
- `rpotato hooks validate-result <json>`
- `rpotato tui`
- `rpotato tui monitor`
- `rpotato tui sessions`
- `rpotato tui transcript <session-id>`
- `rpotato tui approvals`
- `rpotato tui diff <proposal-id>`
- `rpotato tui evidence`
- `rpotato patch preview --path <path> --find <text> --replace <text>`
- `rpotato patch approve <proposal-id> --token <token> [--dry-run]`
- `rpotato patch verify <proposal-id> --token <token>`
- `rpotato patch token-rotate <proposal-id>`
- `rpotato monitor status`
- `rpotato monitor models`
- `rpotato monitor baseline`
- `rpotato monitor optimize`
- `rpotato monitor export --format jsonl`
- `rpotato monitor export --format csv`
- `rpotato monitor prune --before 30d --dry-run`
- `rpotato ontology status`
- `rpotato ontology seed`
- `rpotato ontology inspect`
- `rpotato ontology context --query <text>`
- `rpotato ontology reread <source-pointer>`
- `rpotato ontology export --format json`
- `rpotato ontology export --format jsonl`
- `rpotato ontology import --file <path> --dry-run`
- `rpotato benchmark validate <fixture.json>`
- `rpotato benchmark record --fixture <fixture.json>`
- `rpotato benchmark run --fixture <fixture.json> --prompt <artifact> [--max-tokens <tokens>]`
- `rpotato benchmark report --format jsonl`
- `rpotato model list`
- `rpotato model manifest`
- `rpotato model inspect <id>`
- `rpotato model registry`
- `rpotato model download-plan <id>`
- `rpotato model eval-plan <id>`
- `rpotato model benchmark-plan <id>`
- `rpotato model fetch-candidate <id> --for-evaluation`
- `rpotato model verify-file <path> --sha256 <hash>`
- `rpotato model promote <id> --evidence <file>`
- `rpotato model cleanup-failed <id> --dry-run`
- `rpotato model install <id>`
- `rpotato model default [<id>]`
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
- `rpotato uninstall --keep-cache`
- `rpotato uninstall --purge-cache`

`rpotato init`은 app data root와 project-local `.rpotato/` 아래에 current-state, append-only ledger, runtime evidence JSONL, SQLite observability projection, project-local ontology store/schema를 초기화합니다. Ontology store에는 raw source text를 장기 저장하지 않고, source-backed project file에서 결정적 Layer A fact만 source pointer와 hash로 seed합니다.

`state reconcile`은 stale/corrupt current-state를 보존 이동한 뒤 새 current-state를 기록합니다. `state resume`과 `continue`는 선택한 session의 bounded durable context를 검증·재구성한 뒤 안전한 active workflow checkpoint만 계속합니다. Pending approval에서는 backend를 다시 호출하지 않고, 결과가 불확실한 backend request나 verification command는 자동 재실행하지 않습니다.

`session list`와 `session history`는 canonical runtime ledger에서 재생성한 SQLite projection으로 현재 project의 session history를 표시합니다. `session new`는 새 session identity를 만들고 current-state를 그 session으로 전환합니다. `session resume <session-id>`, `resume <session-id>`, `continue <session-id>`는 canonical ledger 소유권을 요구하고 current-state 변경 전에 durable transcript artifact와 source hash를 검증한 뒤 같은 session의 안전한 workflow checkpoint만 계속합니다. SQLite-only row는 resume 권위가 될 수 없고, 다른 session 소유의 non-terminal workflow가 있으면 mutation 없이 차단합니다.

`evidence validate`는 artifact pointer가 local project-relative path이고 project boundary를 벗어나지 않는지 확인합니다.

`run`은 user request를 skill/mode/context/evidence 요구사항으로 정규화하고 최근 durable turn을 최대 8개·2,400자 안에서 재구성합니다. 현재 요청과 resume context 전체에 source pointer 최대 4개·3,200자의 단일 공유 budget을 적용한 뒤에만 workflow를 만들고 backend sidecar를 호출합니다. Canonical transcript artifact에는 user turn, visible 또는 normalized model result, normalized tool record, evidence record를 저장합니다. Source 원문과 patch fragment는 pointer와 SHA-256만 남기고 hidden reasoning/raw backend response는 제외합니다. SQLite `transcript_records`는 순서를 보존하는 재생성 가능한 projection이며 resume 권위가 아닙니다. 유효한 patch action은 restart-safe workflow와 proposal을 저장하고 정확한 `patch approve` gate에서 멈춥니다.

`intent classify`와 `intent routes`는 실행 전 surface로 유지됩니다. `skill run <id> "<request>"`는 built-in skill 또는 enable된 instruction-only Codex/Claude Code skill이나 command를 명시적으로 선택하고 `run`과 같은 영속 agent loop에 진입해 context 검사, lifecycle hook, runtime policy, evidence 수집, stop criteria를 적용합니다. Imported instruction은 신뢰하지 않는 prompt content이며 runtime이 소유한 action contract, tool policy, approval boundary, 한국어 guard, evidence requirement, stop gate를 넓힐 수 없습니다.

`subagent launch`는 active parent workflow 아래에서 sequential bounded child 하나를 실행합니다. Runtime은 backend dispatch 전에 role, 선언한 tool, project-relative read path, optional executor write ownership, timeout, token budget, resource admission, source-pointer context를 고정합니다. Child는 strict structured result 하나만 반환하며 command 실행, file write, patch apply, nested worker 시작, parent approval 우회를 할 수 없습니다. `subagent status`는 read-only이고, `subagent cancel`은 completion과 경쟁해 terminal state 하나만 얻습니다. Credential 형태의 output은 persistence 전에 차단하며 검증된 evidence merge는 restart 뒤에도 idempotent하게 복구됩니다.

`tui`는 stdin/stdout이 terminal에 연결되면 v0.34.0 interactive line controller를 자동 선택하고, terminal이 아니면 read-only overview를 유지합니다. `tui interactive`는 controller를 명시적으로 시작합니다. Canonical view 이동, paging과 selection, patch/verification 승인, pending gate 거부, workflow resume, cancel을 지원합니다. 모든 mutation은 runtime-owned selection lease를 얻고 명시적 확인을 요구하며, credential은 terminal echo를 끈 상태로 한 번만 읽고 closed runtime outcome 중 하나를 반환합니다. 첫 patch approval 성공 때 verification credential을 terminal에 정확히 한 번 출력하고, 같은 committed intent의 반복은 refresh receipt만 반환합니다. TUI가 state나 SQLite를 직접 수정하지는 않습니다. Transcript와 tool view는 검증되고 크기가 제한된 artifact만 표시하며 hidden model response, source file body, patch fragment, verification command 원문, credential, terminal control byte는 제외합니다. 읽기 surface는 새 product mutation을 만들지 않지만 command startup은 runtime 권위 아래 이미 commit된 transition journal을 마저 수렴하거나 지연된 derived projection을 재구축할 수 있습니다. v0.34.0의 승인된 source installation 성공 경로는 Unix만 지원하고 다른 platform은 prepared journal이나 target을 바꾸기 전에 차단합니다. 마지막 pathname validation 뒤 unlink 전에 끼어드는 동시 외부 writer race는 v0.34.0 보장 밖의 unsupported 조건이며 atomic하다고 주장하지 않습니다.

`policy`와 `hooks` 명령은 command/path 권한 결정, credential redaction, lifecycle hook registry, strict fail-closed hook result 검사를 제공합니다. Native runtime hook은 context packing, model request, action parsing, patch apply, verification command, final report, stop gate를 실제로 보호합니다. Hook result는 runtime, project, skill, session, observer layer 순서로 평가하고 더 엄격한 결과가 우선합니다. 외부/plugin hook executable은 계속 비활성 상태이며 import나 enable만으로 command 또는 file-write 권한을 얻을 수 없습니다.

`patch preview`는 project-local text file을 읽고 명시적인 단일 find/replace proposal에 대한 unified diff를 렌더링하며, `.rpotato/patch-proposals/` 아래에 project-local record를 저장합니다. 이 standalone surface는 diff-only라 approve/apply/verify할 수 없습니다. `patch approve`는 `run`이 생성한 workflow proposal에만 사용할 수 있습니다. `patch approve <proposal-id> --token <token> --dry-run`은 target file을 수정하지 않고 patch 적용 gate를 검증합니다. `--dry-run` 없이 실행하면 workflow/proposal binding과 current source SHA-256이 모두 유효할 때만 workflow proposal을 적용하며 command는 실행하지 않고 별도의 일회성 verification credential을 발급합니다. `patch verify <proposal-id> --token <token>`은 pre-bound되고 policy가 허용한 argv verification plan만 별도로 승인해 실행합니다. Verification 실패는 rollback을 시도하며 성공으로 보고하지 않습니다. `patch token-rotate`는 현재 승인 대기 중인 gate의 credential을 교체합니다. 두 credential 모두 plaintext로 저장하거나 최초 전달 뒤 다시 표시하지 않습니다.

`monitor baseline`은 local ledger/SQLite projection metric을 읽어 p50/p95 latency, average tokens/sec, context clamp count, peak RSS, pressure-state distribution, model/backend/session grouping을 보여주는 read-only performance baseline report를 출력합니다. Raw prompt/source text는 저장하지 않으며 model artifact를 선택하지 않습니다. `monitor optimize`는 이 local metric과 `measured-locally` benchmark row만 읽어 context budget, team lane count, fallback mode, model route hint를 추천합니다. 실제 model artifact를 선택하거나 public benchmark parity를 주장하지 않습니다. `monitor export`는 runtime ledger를 JSONL/CSV로 출력합니다. `monitor prune`은 현재 dry-run만 허용하며 실제 삭제는 수행하지 않습니다.

`ontology status`, `ontology seed`, `ontology inspect`는 project-local `.rpotato/ontology/graph.jsonl` typed graph store와 `.rpotato/ontology/schema.json` contract를 다룹니다. Layer A seed는 indexed file, package manifest, entrypoint, generated-exclusion rule 같은 결정적 사실을 source pointer와 SHA-256 hash로 기록합니다. `ontology context --query <text>`는 작은 모델 prompt용 source-pointer-first compact context view를 렌더링합니다. `ontology reread <source-pointer>`는 edit decision 전에 authoritative project file을 다시 열고 현재 file hash를 보고합니다. `ontology export --format json|jsonl`은 inspection view만 내보내며, JSON/YAML/RDF/OWL류 export가 runtime store보다 더 authoritative하지 않습니다. `ontology import --file <path> --dry-run`은 import 후보를 검증하고 source pointer/hash 없는 confirmed Layer B semantic claim을 차단합니다.

공식 binary download는 GitHub Releases로 배포합니다. v0.28.5부터 release workflow는 macOS Apple Silicon(`aarch64-apple-darwin`), macOS Intel(`x86_64-apple-darwin`), Linux x86_64(`x86_64-unknown-linux-gnu`), Linux ARM64(`aarch64-unknown-linux-gnu`), Windows x86_64(`x86_64-pc-windows-msvc`)용 `rpotato` archive를 만들고, basename-only `.sha256` checksum file과 aggregate checksums file을 생성하며, asset upload 전에 packaged binary smoke test를 실행합니다. Windows job은 portable streaming/generation suite와 실제 fake-sidecar process cancellation lifecycle test도 Windows에서 직접 실행합니다.

`benchmark validate <fixture.json>`는 project-local benchmark fixture metadata를 검증합니다. Runtime capability, model/runtime responsibility, expected route, policy decision, escalation target, required tool/source/evidence record, abstention requirement, ontology view, context budget, backend/model artifact identifier, sampling policy, raw artifact retention policy를 확인합니다. `benchmark record --fixture <fixture.json>`는 metadata-only benchmark run을 append-only ledger와 SQLite `benchmark_runs` projection에 기록합니다. 이 기록은 `claim_state=not-comparable`, score 없음, reproducibility manifest, redacted local report만 포함합니다. `benchmark run --fixture <fixture.json> --prompt <artifact> [--max-tokens <tokens>]`는 실행 중인 backend sidecar에 prompt artifact를 보내고 `claim_state=measured-locally`, deterministic 0-3 local product score, `model_run_id`, token/latency/resource summary, redacted reproducibility field를 기록합니다. SQLite에는 raw prompt/source text를 저장하지 않습니다. `benchmark report --format jsonl`은 redacted benchmark record를 JSONL로 출력합니다. Benchmark output은 public benchmark parity를 주장하지 않습니다.

`model list`, `model manifest`, `model inspect`, `model registry`, `model download-plan`은 source-backed manifest schema, 후보 상태, benchmark source ledger, local registry path, 다운로드 전 source/license/checksum을 보여줍니다. Qwen과 Gemma는 pinned source-backed GGUF 후보입니다. `model fetch-candidate <id> --for-evaluation`은 app-managed storage에만 다운로드하고 size/SHA-256을 검증하며 설치하지 않습니다. `model promote <id> --evidence <file>`은 정확한 `backend.chat.completed` provenance와 hash-pinned canonical `model-adoption-smoke-v1` benchmark/prompt pair를 요구합니다. `model install`은 registry 등록 전에 이 evidence를 재검증합니다. `model default <id>`는 유효한 registry 모델만 선택하고 `model default`는 현재 선택을 보여줍니다. `backend start`에서 `--model`을 생략하면 selection, registry, artifact, promotion evidence를 모두 재검증하며 하나라도 stale하거나 손상되면 fail-closed합니다. 2026-07-11 strict local 비교에서는 Qwen이 지시문 한 줄을 추가해 exact-response equality를 실패했으므로 Gemma를 선택했습니다. 자세한 내용은 [docs/model-eval.md](docs/model-eval.md)에 있으며 public benchmark parity나 16GB 검증을 주장하지 않습니다.

`backend doctor`는 관리형 `llama.cpp` sidecar discovery, env override path, port, health URL, executable bit, install gate, 기록된 managed binary version을 보여줍니다. `backend install-plan`과 `backend install`은 source-backed `llama.cpp b9982` artifact를 선택하고 다운로드/cache, size/SHA-256, staging 배치, rollback, ledger를 관리합니다. `backend start [--model <path>] [--ctx-size <tokens>]`는 명시된 모델 또는 재검증한 지속 기본 모델로 sidecar를 시작하고 `/health`, CPU/RSS/disk, timeout cleanup을 기록합니다. `backend stop`은 generation cancellation을 요청하고 terminal outcome을 기다린 뒤 sidecar를 종료합니다. `backend chat --prompt <text> [--max-tokens <tokens>] [--stream] [--timeout-ms <ms>]`은 resource governor 뒤에서 `/v1/chat/completions` SSE를 소비합니다. 기본 display는 filtering된 delta를 모으고 `--stream`은 language guard를 통과한 완전한 unit만 출력합니다. 기본 30초, 최대 300초 timeout은 resolution부터 response read까지 포함합니다. `backend cancel`은 active chat connection을 닫고 정확한 terminal outcome을 기다리며 sidecar는 계속 실행합니다. 전송된 request는 재시도하지 않고, final usage가 없으면 token 사용량을 unknown으로 유지하며, split reasoning trace는 display 전에 제거하고 upstream error detail은 redaction합니다. Raw prompt/response는 저장하지 않습니다.

Backend CPU/RSS/disk resource sampling은 `backend start`, `backend status`, `backend chat`, `monitor status`, read-only `tui monitor` resource-pressure panel에서 사용할 수 있습니다. 첫 runtime resource governor slice는 backend chat에 적용되어 있습니다. `team status`는 계속 read-only admission preview이며 현재 project의 최신 `team.*` runtime ledger event를 표시합니다. `team admit --lanes <count>`는 enforced team admission gate입니다. Normal pressure에서는 parallel lane을 허용하고, unknown/degraded pressure에서는 sequential lane 하나로 fallback하며, critical pressure에서는 worker launch가 생기기 전 단계에서 dispatch를 차단하고 decision을 ledger에 기록합니다. `team admit`은 반복 가능한 `--write <path>`, `--write-owner <lane:path>`, `--command <command>` preflight check도 받습니다. Policy가 `ask` 또는 `deny`를 반환하면 dispatch를 차단하고, 정규화된 같은 write path를 여러 lane이 소유하려 하면 worker launch 전에 차단합니다. Policy/ownership block은 `.rpotato/approval-requests/` 아래 project-local approval request를 기록하며, `rpotato tui approvals`는 directory scan 대신 대응하는 canonical ledger event와 active workflow-bound patch proposal만 표시합니다. `team dispatch --lanes <count> --write-owner <lane:path>`는 dispatch 시점에 정규화된 file ownership을 다시 검사하고 cross-lane conflict를 차단하며 ledger/SQLite projection에 결과를 기록합니다. `--failed-lane <lane> --failure <reason>`으로 failed-worker continuation도 기록할 수 있지만 worker를 시작하거나 team stage를 전진시키지는 않습니다. `team governor --lanes <count> --context-tokens <tokens>`는 첫 context/model governor preflight를 기록합니다. Admitted lane을 표시하고, 요청 context를 설정 budget과 현재 resource pressure에 맞춰 clamp하며, 실제 model capability를 주장하거나 artifact를 선택하지 않고 local model-tier route hint(`keep`, `downgrade`, `escalate`, `defer`)만 냅니다.

`plugin import`는 local Codex/Claude Code형 plugin directory만 받습니다. Source를 app data 아래에 snapshot하고 normalized schema v2 manifest에 source manifest SHA-256과 source snapshot SHA-256을 기록하며, 보이는 capability와 required/blocked permission, unsupported semantic을 보고합니다. `plugin validate`와 `plugin enable`은 imported snapshot hash를 다시 확인하고 drift가 있으면 plugin을 `blocked`로 표시합니다. v0.38은 enable된 canonical Codex `SKILL.md`와 Claude Code `skills/<name>/SKILL.md`, `commands/<name>.md` instruction을 같은 native read-only hook/skill/evidence/stop-gate 경로로 실행합니다. Dynamic shell interpolation, script, custom component path, agent, hook, MCP/LSP, monitor, `bin/`, settings, theme, output style, substitution, source-runtime tool grant는 차단하거나 unsupported로 명시합니다. Import와 enable은 shell, background, remote-connector, sensitive-config, runtime-setting, file-write 권한을 그 자체로 부여하지 않습니다.

## 공개 저장소 운영

이 프로젝트는 Apache-2.0 라이선스의 공개 오픈소스 저장소입니다.

다만 현재는 외부 코드 기여와 외부 PR을 받지 않습니다. 버그 리포트, 사용성 피드백, 보안 제보는 받을 수 있지만, 코드는 maintainer가 직접 반영합니다.

- 라이선스: [LICENSE](LICENSE)
- 운영 정책: [GOVERNANCE.md](GOVERNANCE.md)
- 보안 정책: [SECURITY.md](SECURITY.md)
- 프라이버시 정책: [PRIVACY.md](PRIVACY.md)
- 로드맵: [ROADMAP.md](ROADMAP.md)

## 추가 문서

- [docs/development.md](docs/development.md): 개발 환경과 검증 흐름
- [docs/cli-output-style.md](docs/cli-output-style.md): CLI 출력 스타일
- [docs/release.md](docs/release.md): 릴리즈 정책
- [docs/ko/release-train.md](docs/ko/release-train.md): `v0.29.0`-`v0.41.0` 실행 순서와 건너뛸 수 없는 gate
- [DESIGN.md](DESIGN.md): CLI/TUI/monitoring UX source of truth
- [docs/runtime-architecture.md](docs/runtime-architecture.md): runtime core와 surface/backend 경계
- [docs/glossary.md](docs/glossary.md): 프로젝트 용어 정의
- [docs/ontology-runtime.md](docs/ontology-runtime.md): 온톨로지 runtime 설계
- [docs/observability.md](docs/observability.md): 모델/token/runtime monitoring 설계
- [docs/hooks.md](docs/hooks.md): runtime lifecycle hook 설계
- [docs/skills.md](docs/skills.md): 재사용 가능한 runtime skill 설계
- [docs/subagents.md](docs/subagents.md): 서브에이전트 실행 경계
- [docs/team-runtime.md](docs/team-runtime.md): 팀 실행 runtime 설계
- [docs/tui.md](docs/tui.md): TUI surface 설계
- [docs/plugin-adapters.md](docs/plugin-adapters.md): Claude Code/Codex형 플러그인 어댑터 설계
- [docs/model-manifest.md](docs/model-manifest.md): 모델 manifest schema
- [docs/model-source-policy.md](docs/model-source-policy.md): 모델 정보 출처 정책
- [docs/model-licenses.md](docs/model-licenses.md): 모델 라이선스 추적
- [docs/backend-adapters.md](docs/backend-adapters.md): backend adapter 경계
- [docs/command-policy.md](docs/command-policy.md): 파일/명령 승인 정책
- [docs/korean-output-guard.md](docs/korean-output-guard.md): 한국어 출력 guard
- [docs/threat-model.md](docs/threat-model.md): 위협 모델
- [docs/benchmarks.md](docs/benchmarks.md): benchmark 실행 계획
