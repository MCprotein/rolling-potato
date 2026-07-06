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
- 모델별 token/latency/resource monitoring
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

초기 지원 우선순위는 macOS와 Windows입니다. Linux는 이후 maintainer 주도 또는 운영 정책 변경 후 확장합니다.

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
rpotato model list
rpotato model knowledge
rpotato model knowledge inspect qwen3.5-4b
rpotato model install qwen3.5-4b
rpotato backend doctor
rpotato cache status
rpotato monitor status
rpotato monitor models
rpotato monitor export --format jsonl
rpotato monitor export --format csv
rpotato monitor prune --before 30d --dry-run
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
9. runtime core가 로컬 설정에 모델을 등록한다.
10. runtime core가 추론 백엔드를 시작하거나 기존 프로세스를 재사용한다.

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
- `rpotato monitor status`
- `rpotato monitor models`
- `rpotato monitor export --format jsonl`
- `rpotato monitor export --format csv`
- `rpotato monitor prune --before 30d --dry-run`
- `rpotato model list`
- `rpotato model manifest`
- `rpotato model inspect <id>`
- `rpotato model registry`
- `rpotato model download-plan <id>`
- `rpotato model eval-plan <id>`
- `rpotato model benchmark-plan <id>`
- `rpotato model fetch-candidate <id> --for-evaluation`
- `rpotato model verify-file <path> --sha256 <hash>`
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

`rpotato init`은 app data root와 project-local `.rpotato/` 아래에 current-state, append-only ledger, runtime evidence JSONL, SQLite observability projection을 초기화합니다.

`state reconcile`은 stale/corrupt current-state를 보존 이동한 뒤 새 current-state를 기록합니다. `state resume`은 active workflow pointer를 감지하거나, 재개할 작업이 없으면 no-op ledger event를 남깁니다.

`session list`와 `session history`는 SQLite projection에서 현재 project의 session history를 읽습니다. `session new`는 새 session identity를 만들고 current-state를 그 session으로 전환합니다. `session resume <session-id>`와 `resume <session-id>`는 history에서 고른 session id를 current-state에 기록해 이후 명령이 같은 session ledger/SQLite projection으로 이어지게 합니다. 실제 model/backend agent loop의 transcript replay와 대화 이어달리기는 후속 agent-loop phase에서 이 current-state를 사용해 구현합니다.

`evidence validate`는 artifact pointer가 local project-relative path이고 project boundary를 벗어나지 않는지 확인합니다.

`run`, `intent classify`, `intent routes`, `skill run`은 현재 user request를 skill/mode/context/evidence 요구사항으로 정규화하고 ledger event를 남깁니다. 실제 model/backend agent loop 실행은 아직 수행하지 않습니다.

`policy`와 `hooks` 명령은 command/path 권한 결정, credential redaction, lifecycle hook registry, fail-closed hook result 검사를 제공합니다. 실제 tool execution은 아직 이 policy surface 뒤에 연결되지 않았습니다.

`monitor export`는 runtime ledger를 JSONL/CSV로 출력합니다. `monitor prune`은 현재 dry-run만 허용하며 실제 삭제는 수행하지 않습니다.

`model list`, `model manifest`, `model inspect`, `model registry`, `model download-plan`은 source-backed manifest schema, 후보 상태, 공개 benchmark source ledger, local registry 위치, 다운로드 전 source/license/checksum 표시 항목을 보여줍니다. Qwen과 Gemma는 pinned revision URL, LFS SHA-256, file size가 기록된 source-backed `unverified` GGUF artifact 후보를 갖습니다. `model eval-plan <id>`는 read-only 로컬 평가 preflight입니다. source-backed artifact field, app-data artifact 존재 여부, size/SHA-256 상태, 다음 smoke/benchmark 명령을 다운로드 없이 확인합니다. `model benchmark-plan <id>`는 공개 benchmark 재현 조건과 로컬 제품 benchmark fixture를 분리하고 artifact, quantization, backend, prompt, dataset, scoring 조건이 함께 기록되기 전까지 점수 parity를 거부합니다. `model fetch-candidate <id> --for-evaluation`은 명시적인 평가 전용 다운로드 경로입니다. app-managed partial resume을 지원하고 size/SHA-256을 검증한 뒤 ledger event를 남기지만, 설치된 모델로 registry 등록하지 않습니다. `model verify-file`은 로컬 파일 bytes의 SHA-256을 검증하고 ledger event를 남깁니다. `model cleanup-failed`는 app data 내부의 partial/failed artifact만 dry-run 또는 명시적 delete 대상으로 삼습니다. `model install`은 후보를 `verified`로 승격하기 전까지 registry 설치를 차단하며, local `llama.cpp b9878` smoke, RAM fit, mmproj 필요 여부, benchmark evidence가 남아 있습니다.

`backend doctor`는 관리형 `llama.cpp` sidecar discovery, env override path, port, health URL, executable bit, install gate, 기록된 managed binary의 version detection을 보여줍니다. `backend install-plan`은 지원 OS/CPU 조합에 대해 source-backed `llama.cpp` release `b9878` CPU artifact를 선택하고 release URL, archive URL, SHA-256, size, license source, download path를 표시합니다. `backend install`은 archive를 다운로드하거나 cache를 재사용하고, size와 SHA-256을 검증한 뒤 staging에서 압축을 풀어 release payload를 managed backend directory에 배치합니다. Unix에서는 실행 권한을 설정하고, 교체 실패 시 rollback하며, install record와 ledger event를 남깁니다. `backend start --model <path> [--ctx-size <tokens>]`는 명시된 로컬 모델 파일과 선택적 runtime context limit으로 selected sidecar를 시작하고 pid/log path를 기록한 뒤 `/health`를 기다리며, startup timeout이면 child를 종료합니다. `backend status`는 sidecar pid record와 health 상태를 읽고, `backend stop`은 stale record를 제거하거나 기록된 sidecar를 종료합니다. Env override binary는 `doctor`가 실행하지 않으며 명시적인 lifecycle 명령에서만 실행됩니다. `backend verify-archive`는 로컬 backend archive SHA-256을 검증합니다. `backend health-check`는 선택된 host/port의 `/health`를 짧은 timeout으로 진단합니다. `backend chat --prompt <text> [--max-tokens <tokens>]`는 실행 중인 sidecar의 `/v1/chat/completions`를 호출하고, Qwen3.5 thinking을 `chat_template_kwargs.enable_thinking=false`로 끄며, 누수된 `<think>` trace는 표시 전에 제거하고 raw prompt/response 없이 token 사용량만 ledger에 기록합니다.

`plugin import`는 local plugin directory의 source snapshot과 normalized manifest를 app data root 아래에 저장합니다. Import는 실행 권한을 부여하지 않으며, shell/MCP/background/file-write 같은 capability는 기본 차단 상태로 permission report에 남깁니다.

다음 구현 전 작업:

- source-recorded GGUF artifact 후보의 local smoke/RAM-fit/mmproj 검증
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
