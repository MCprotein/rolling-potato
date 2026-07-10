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
rpotato evidence validate logs/test.log
rpotato skill list
rpotato skill run fix-test
rpotato plugin import --from claude-code ./my-plugin
rpotato plugin inspect imported.example-plugin
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

Plugin adapter는 marketplace를 거치지 않습니다. 사용자가 직접 가진 Codex/Claude Code형 plugin directory를 local path로 import하고, `rpotato`가 manifest와 권한 위험을 inspect/validate한 뒤 enable합니다.

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
- `rpotato backend start --model <path> [--ctx-size <tokens>]`
- `rpotato backend status`
- `rpotato backend stop`
- `rpotato backend verify-archive <path> --sha256 <hash>`
- `rpotato backend health-check`
- `rpotato backend chat --prompt <text> [--max-tokens <tokens>]`
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
- `rpotato cancel`
- `rpotato evidence validate <artifact-pointer>`
- `rpotato skill list`
- `rpotato skill run <id>`
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
- `rpotato patch approve <proposal-id> --token <token> [--dry-run] [--verify-command <command>]`
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

`state reconcile`은 stale/corrupt current-state를 보존 이동한 뒤 새 current-state를 기록합니다. `state resume`은 active workflow pointer를 감지하거나, 재개할 작업이 없으면 no-op ledger event를 남깁니다.

`session list`와 `session history`는 SQLite projection에서 현재 project의 session history를 읽습니다. `session new`는 새 session identity를 만들고 current-state를 그 session으로 전환합니다. `session resume <session-id>`와 `resume <session-id>`는 history에서 고른 session id를 current-state에 기록해 이후 명령이 같은 session ledger/SQLite projection으로 이어지게 합니다. 실제 model/backend agent loop의 transcript replay와 대화 이어달리기는 후속 agent-loop phase에서 이 current-state를 사용해 구현합니다.

`evidence validate`는 artifact pointer가 local project-relative path이고 project boundary를 벗어나지 않는지 확인합니다.

`run`은 user request를 skill/mode/context/evidence 요구사항으로 정규화하고, source pointer가 포함된 bounded repository context pack을 만들며, runtime-owned action candidate와 next gate를 준비한 뒤 실행 중인 backend sidecar를 호출하고 model의 structured action line 또는 인식 가능한 action text를 실행 없이 파싱합니다. Intent, context, action-candidate, model-action, backend chat ledger event와 token/latency metric을 local SQLite observability projection에 기록합니다. 아직 patch 적용, command 실행, model output의 승인된 action 처리까지는 수행하지 않습니다.

`intent classify`, `intent routes`, `skill run`은 model을 호출하지 않는 pre-execution surface입니다. Routing state를 정규화하고 ledger event만 남깁니다.

`tui`, `tui monitor`, `tui sessions`, `tui transcript <session-id>`, `tui approvals`, `tui diff <proposal-id>`, `tui evidence`는 기존 runtime state, SQLite observability projection, project-local patch proposal record, team admission approval request record, evidence store path를 읽는 read-only TUI beta surface입니다. Project/session 상태, model/token/TPS summary, CPU/RSS/disk resource pressure, session history, 선택한 session의 event timeline, approval queue record, proposal metadata, literal diff, evidence count, stop-gate result count, stale policy, read-only boundary를 terminal-friendly ASCII layout으로 보여줍니다. Approval, patch apply, resume, cancel, transcript replay, stop gate pass/fail 판정, workflow mutation은 수행하지 않습니다.

`policy`와 `hooks` 명령은 command/path 권한 결정, credential redaction, lifecycle hook registry, fail-closed hook result 검사를 제공합니다. 실제 tool execution은 아직 이 policy surface 뒤에 연결되지 않았습니다.

`patch preview`는 project-local text file을 읽고 명시적인 단일 find/replace proposal에 대한 unified diff를 렌더링하며, `.rpotato/patch-proposals/` 아래에 project-local proposal record를 저장하고 approval token을 출력합니다. `patch approve <proposal-id> --token <token> --dry-run`은 token을 검증하고 target file을 수정하지 않은 채 approval gate를 기록합니다. `--dry-run` 없이 실행하면 current file SHA-256이 preview 당시 original SHA-256과 일치할 때만 승인된 proposal을 적용하고, rollback record를 쓴 뒤 applied SHA-256을 검증해 ledger event를 남깁니다. `--verify-command <command>`는 apply 이후 allow 정책을 통과한 단순 argv verification command만 실행하며, verification 실패 시 rollback을 시도하고 성공으로 보고하지 않습니다.

`monitor baseline`은 local ledger/SQLite projection metric을 읽어 p50/p95 latency, average tokens/sec, context clamp count, peak RSS, pressure-state distribution, model/backend/session grouping을 보여주는 read-only performance baseline report를 출력합니다. Raw prompt/source text는 저장하지 않으며 model artifact를 선택하지 않습니다. `monitor optimize`는 이 local metric과 `measured-locally` benchmark row만 읽어 context budget, team lane count, fallback mode, model route hint를 추천합니다. 실제 model artifact를 선택하거나 public benchmark parity를 주장하지 않습니다. `monitor export`는 runtime ledger를 JSONL/CSV로 출력합니다. `monitor prune`은 현재 dry-run만 허용하며 실제 삭제는 수행하지 않습니다.

`ontology status`, `ontology seed`, `ontology inspect`는 project-local `.rpotato/ontology/graph.jsonl` typed graph store와 `.rpotato/ontology/schema.json` contract를 다룹니다. Layer A seed는 indexed file, package manifest, entrypoint, generated-exclusion rule 같은 결정적 사실을 source pointer와 SHA-256 hash로 기록합니다. `ontology context --query <text>`는 작은 모델 prompt용 source-pointer-first compact context view를 렌더링합니다. `ontology reread <source-pointer>`는 edit decision 전에 authoritative project file을 다시 열고 현재 file hash를 보고합니다. `ontology export --format json|jsonl`은 inspection view만 내보내며, JSON/YAML/RDF/OWL류 export가 runtime store보다 더 authoritative하지 않습니다. `ontology import --file <path> --dry-run`은 import 후보를 검증하고 source pointer/hash 없는 confirmed Layer B semantic claim을 차단합니다.

공식 binary download는 GitHub Releases로 배포합니다. v0.28.5부터 release workflow는 macOS Apple Silicon(`aarch64-apple-darwin`), macOS Intel(`x86_64-apple-darwin`), Linux x86_64(`x86_64-unknown-linux-gnu`), Linux ARM64(`aarch64-unknown-linux-gnu`), Windows x86_64(`x86_64-pc-windows-msvc`)용 `rpotato` archive를 만들고, basename-only `.sha256` checksum file과 aggregate checksums file을 생성하며, asset upload 전에 packaged binary smoke test를 실행합니다.

`benchmark validate <fixture.json>`는 project-local benchmark fixture metadata를 검증합니다. Runtime capability, model/runtime responsibility, expected route, policy decision, escalation target, required tool/source/evidence record, abstention requirement, ontology view, context budget, backend/model artifact identifier, sampling policy, raw artifact retention policy를 확인합니다. `benchmark record --fixture <fixture.json>`는 metadata-only benchmark run을 append-only ledger와 SQLite `benchmark_runs` projection에 기록합니다. 이 기록은 `claim_state=not-comparable`, score 없음, reproducibility manifest, redacted local report만 포함합니다. `benchmark run --fixture <fixture.json> --prompt <artifact> [--max-tokens <tokens>]`는 실행 중인 backend sidecar에 prompt artifact를 보내고 `claim_state=measured-locally`, deterministic 0-3 local product score, `model_run_id`, token/latency/resource summary, redacted reproducibility field를 기록합니다. SQLite에는 raw prompt/source text를 저장하지 않습니다. `benchmark report --format jsonl`은 redacted benchmark record를 JSONL로 출력합니다. Benchmark output은 public benchmark parity를 주장하지 않습니다.

`model list`, `model manifest`, `model inspect`, `model registry`, `model download-plan`은 source-backed manifest schema, 후보 상태, 공개 benchmark source ledger, local registry 위치, 다운로드 전 source/license/checksum 표시 항목을 보여줍니다. Qwen과 Gemma는 pinned revision URL, LFS SHA-256, file size가 기록된 source-backed `unverified` GGUF artifact 후보를 갖습니다. `model eval-plan <id>`는 read-only 로컬 평가 preflight입니다. source-backed artifact field, app-data artifact 존재 여부, size/SHA-256 상태, 다음 smoke/benchmark 명령을 다운로드 없이 확인합니다. `model benchmark-plan <id>`는 공개 benchmark 재현 조건과 로컬 제품 benchmark fixture를 분리하고 artifact, quantization, backend, prompt, dataset, scoring 조건이 함께 기록되기 전까지 점수 parity를 거부합니다. `model fetch-candidate <id> --for-evaluation`은 명시적인 평가 전용 다운로드 경로입니다. app-managed partial resume을 지원하고 size/SHA-256을 검증한 뒤 ledger event를 남기지만, 설치된 모델로 registry 등록하지 않습니다. `model verify-file`은 로컬 파일 bytes의 SHA-256을 검증하고 ledger event를 남깁니다. `model promote <id> --evidence <file>`은 local promotion evidence JSON을 app-managed artifact, backend smoke ledger event, RAM-fit/mmproj field, SQLite `measured-locally` benchmark row와 대조해 검증한 뒤 `models/evidence/<model-id>.promotion.json`을 기록합니다. `model cleanup-failed`는 app data 내부의 partial/failed artifact만 dry-run 또는 명시적 delete 대상으로 삼습니다. `model install`은 static `verified` manifest entry가 있거나 local promotion evidence가 여전히 재검증될 때만 registry에 등록합니다.

`backend doctor`는 관리형 `llama.cpp` sidecar discovery, env override path, port, health URL, executable bit, install gate, 기록된 managed binary의 version detection을 보여줍니다. `backend install-plan`은 지원 OS/CPU 조합에 대해 source-backed `llama.cpp` release `b9878` CPU artifact를 선택하고 release URL, archive URL, SHA-256, size, license source, download path를 표시합니다. `backend install`은 archive를 다운로드하거나 cache를 재사용하고, size와 SHA-256을 검증한 뒤 staging에서 압축을 풀어 release payload를 managed backend directory에 배치합니다. Unix에서는 실행 권한을 설정하고, 교체 실패 시 rollback하며, install record와 ledger event를 남깁니다. `backend start --model <path> [--ctx-size <tokens>]`는 명시된 로컬 모델 파일과 선택적 runtime context limit으로 selected sidecar를 시작하고 pid/log path를 기록한 뒤 `/health`를 기다리며 CPU/RSS/disk resource status를 sampling하고, startup timeout이면 child를 종료합니다. `backend status`는 sidecar pid record, health 상태, 실행 중인 sidecar의 최신 sampled resource pressure를 읽고, `backend stop`은 stale record를 제거하거나 기록된 sidecar를 종료합니다. Env override binary는 `doctor`가 실행하지 않으며 명시적인 lifecycle 명령에서만 실행됩니다. `backend verify-archive`는 로컬 backend archive SHA-256을 검증합니다. `backend health-check`는 선택된 host/port의 `/health`를 짧은 timeout으로 진단합니다. `backend chat --prompt <text> [--max-tokens <tokens>]`는 요청 전에 실행 중인 sidecar를 sampling하고, critical resource pressure에서는 chat을 차단하며, degraded pressure에서는 더 작은 effective max-token budget으로 clamp한 뒤 `/v1/chat/completions`를 호출합니다. Qwen3.5 thinking은 `chat_template_kwargs.enable_thinking=false`로 끄며, 누수된 `<think>` trace는 표시 전에 제거하고 raw prompt/response 없이 token 사용량과 redacted resource sample을 ledger에 기록합니다.

Backend CPU/RSS/disk resource sampling은 `backend start`, `backend status`, `backend chat`, `monitor status`, read-only `tui monitor` resource-pressure panel에서 사용할 수 있습니다. 첫 runtime resource governor slice는 backend chat에 적용되어 있습니다. `team status`는 계속 read-only admission preview이며 현재 project의 최신 `team.*` runtime ledger event를 표시합니다. `team admit --lanes <count>`는 enforced team admission gate입니다. Normal pressure에서는 parallel lane을 허용하고, unknown/degraded pressure에서는 sequential lane 하나로 fallback하며, critical pressure에서는 worker launch가 생기기 전 단계에서 dispatch를 차단하고 decision을 ledger에 기록합니다. `team admit`은 반복 가능한 `--write <path>`, `--write-owner <lane:path>`, `--command <command>` preflight check도 받습니다. Policy가 `ask` 또는 `deny`를 반환하면 dispatch를 차단하고, 정규화된 같은 write path를 여러 lane이 소유하려 하면 worker launch 전에 차단합니다. Policy/ownership block은 `.rpotato/approval-requests/` 아래 project-local approval request를 기록하며, `rpotato tui approvals`는 이 team request를 patch proposal과 함께 표시합니다. `team dispatch --lanes <count> --write-owner <lane:path>`는 dispatch 시점에 정규화된 file ownership을 다시 검사하고 cross-lane conflict를 차단하며 ledger/SQLite projection에 결과를 기록합니다. `--failed-lane <lane> --failure <reason>`으로 failed-worker continuation도 기록할 수 있지만 worker를 시작하거나 team stage를 전진시키지는 않습니다. `team governor --lanes <count> --context-tokens <tokens>`는 첫 context/model governor preflight를 기록합니다. Admitted lane을 표시하고, 요청 context를 설정 budget과 현재 resource pressure에 맞춰 clamp하며, 실제 model capability를 주장하거나 artifact를 선택하지 않고 local model-tier route hint(`keep`, `downgrade`, `escalate`, `defer`)만 냅니다.

`plugin import`는 local Codex/Claude Code형 plugin directory만 받습니다. Source를 app data 아래에 snapshot하고 normalized schema v2 manifest에 source manifest SHA-256과 source snapshot SHA-256을 기록하며, 보이는 capability와 required/blocked permission을 보고합니다. `plugin validate`와 `plugin enable`은 imported snapshot hash를 다시 확인하고 drift가 있으면 plugin을 `blocked`로 표시합니다. Import와 enable은 shell, MCP, hook, background, runtime-setting, remote-connector, sensitive-config, file-write 실행 권한을 그 자체로 부여하지 않습니다.

다음 구현 전 작업:

- source-recorded GGUF artifact 후보의 반복 가능한 local smoke/RAM-fit/mmproj 증거 수집
- `Qwen3.5-4B` Q4_K_M 후보와 `Gemma 4 E4B` IT QAT q4_0 후보 벤치마크
- 실제 agent loop의 transcript replay와 active workflow resume 실행
- streaming response path와 generation cancellation path 설계

벤치마크 초안은 [docs/model-eval.md](docs/model-eval.md)를 따릅니다.

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
