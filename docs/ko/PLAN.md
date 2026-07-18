# rolling-potato 계획

## 1. 제품 정의

### 이름

- 프로젝트 이름: `rolling-potato`
- CLI 명령어: `rpotato`
- 태그라인: `Local coding agents for potato PCs.`
- 한국어 포지셔닝: `똥컴에서도 굴러가는 로컬 코딩 에이전트`

### 의도

`rolling-potato`는 작은 로컬 모델을 위한 로컬 우선 코딩 에이전트 런타임입니다. 첫 사용자 surface는 `rpotato` CLI입니다.

목표는 Claude Code나 Codex를 더 약한 모델로 복제하는 것이 아닙니다. 목표는 작은 로컬 모델의 실패 표면을 줄여 실제로 쓸 수 있게 만드는 런타임을 만드는 것입니다.

핵심 명제:

> 작은 모델에는 작은 프롬프트가 아니라 작은 모델용 런타임이 필요합니다.

Claude Code, Codex, 가재코드류 도구는 CLI 같은 에이전트 경험을 제공하지만, 실제 제품의 본체는 그 뒤의 런타임입니다. `rolling-potato`는 모델이 유용하지만 취약하다고 보고, runtime core가 context, ontology, hooks, skills, subagents, team execution, actions, validation, retries, 사용자에게 보이는 언어를 관리해야 한다고 가정합니다.

### 대상 사용자

- 한국어 사용자 우선
- 클라우드 코딩 에이전트 구독 비용이 부담스러운 사용자
- 저사양 또는 중급 노트북 사용자
- 로컬/비공개 실행을 원하는 사용자
- 로컬 LLM 도구에 익숙하지 않지만 코딩 도움은 필요한 비전문가 또는 준기술 사용자

현재 하드웨어·릴리즈 기준:

- 16 GB RAM급 노트북은 계속 제품 목표지만, 정확한 model fit에는 로컬 근거가
  필요합니다.
- 공식 release artifact는 macOS arm64/x64, Linux arm64/x64, Windows x64를
  지원합니다.

### 제품 형태

주 surface:

- Claude Code / Codex / 가재코드와 비슷한 CLI 경험
- 대체 가능한 수준의 인터랙티브 사용을 위한 필수 TUI

제품 본체:

- 상태, 정책, 온톨로지, context, agent loop, evidence, stop gate를 소유하는 runtime core
- 로컬 추론을 담당하는 backend/model layer
- 모델/token/resource monitoring을 위한 observability layer
- source-backed LLM evidence와 반복 runtime observation을 위한 model knowledge base
- ledger, SQLite, ontology, model knowledge, plugin registry, evidence replay를 잇는 상태 수명주기 contract
- lifecycle control point를 위한 hook system
- 재사용 workflow를 위한 skill system
- 제한된 multi-agent 작업을 위한 subagent와 team runtime
- Claude Code/Codex형 plugin package를 runtime이 소유하는 capability로 import하는 plugin adapter layer
- 사용자 입력, streaming display, approval prompt, diff, status, final report를 표시하는 CLI/TUI surface

초기 명령 초안:

```sh
rpotato init
rpotato chat
rpotato run "이 에러 고쳐줘"
rpotato intent classify "테스트 실패 고쳐줘"
rpotato intent routes
rpotato tui
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
rpotato skill run fix-test "실패한 테스트를 고쳐줘"
rpotato policy check-command "cargo test"
rpotato hooks list
rpotato plugin import --from claude-code ./my-plugin
rpotato plugin import --from codex ./my-plugin
rpotato plugin inspect imported.example-plugin
rpotato plugin enable imported.example-plugin
rpotato team status
rpotato team admit --lanes 2
rpotato team dispatch --lanes 2 --write-owner 1:src/team.rs --write-owner 2:src/cli.rs
rpotato team dispatch --lanes 3 --write-owner 1:src/team.rs --write-owner 2:src/cli.rs --write-owner 3:src/app.rs --failed-lane 2 --failure "worker timed out"
rpotato team governor --lanes 2 --context-tokens 6000 --context-limit 4096 --model-tier standard
rpotato model list
rpotato model manifest
rpotato model inspect qwen3.5-4b
rpotato model registry
rpotato model knowledge
rpotato model knowledge inspect qwen3.5-4b
rpotato model download-plan qwen3.5-4b
rpotato model verify-file ./model.gguf --sha256 <64-hex>
rpotato model cleanup-failed qwen3.5-4b --dry-run
rpotato model install qwen3.5-4b
rpotato backend doctor
rpotato backend install-plan
rpotato backend verify-archive ./llama.cpp.zip --sha256 <64-hex>
rpotato backend health-check
rpotato cache status
rpotato monitor status
rpotato monitor models
rpotato monitor baseline
rpotato monitor optimize
rpotato monitor export --format jsonl
rpotato monitor export --format csv
rpotato monitor export --format html > rpotato-monitor.html
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

CLI는 가볍고 직접적인 느낌이어야 합니다. 다만 CLI 자체가 제품 경계는 아닙니다. 사용자가 런타임을 구동하는 첫 surface일 뿐입니다. 사용자가 로컬 LLM 도구를 자세히 이해해야만 시작할 수 있어서는 안 됩니다.

Plugin adapter 명령은 local plugin directory path만 사용합니다. `rpotato`는 외부 plugin marketplace, registry, catalog, package mirror를 연동하지 않습니다.

## 2. 런타임과 모델 기반

### 런타임 방향

기본 런타임 방향:

- `llama.cpp` backend
- GGUF model format
- 관리형 `llama-server` / `llama.cpp` runtime binary
- `rpotato`가 소유하는 local HTTP/server sidecar process

이유:

- macOS, Windows, Linux에서 동작 가능하다.
- quantized 4B 모델에 맞다.
- Mac 전용인 MLX 의존을 피한다.
- vLLM처럼 WSL/CUDA/PyTorch가 필요한 경로를 기본값으로 만들지 않는다.
- 전체 desktop app stack보다 packaging이 쉽다.

이후 선택 adapter:

- 이미 사용하는 사용자를 위한 LM Studio adapter
- 이미 모델을 설치한 사용자를 위한 Ollama adapter
- server/GPU mode를 위한 vLLM/SGLang adapter

기본값에서 제외:

- MLX: Apple Silicon 전용에 가깝다.
- vLLM: 저사양 로컬 기본 runtime보다 server/GPU backend에 가깝다.
- Tauri/Electron: 필요한 interactive surface는 GUI보다 terminal TUI가 먼저다.

### 관리형 Backend 배포

MVP 경로에서 사용자가 `llama.cpp`를 직접 설치하게 만들면 안 됩니다.

예상 backend 흐름:

1. `rpotato init`이 init 요청을 runtime core에 전달한다.
2. runtime core가 host OS, 아키텍처, RAM, disk, 기존 config를 확인한다.
3. runtime core가 현재 platform에 맞는 출처 검증 backend release를 해석한다.
4. 네트워크 다운로드가 필요하면 CLI surface가 사용자 승인을 받는다.
5. runtime core가 이어받기 가능한 방식으로 backend archive를 다운로드한다.
6. runtime core가 압축 해제 전에 archive checksum을 검증한다.
7. 추출된 backend binary를 `rpotato` app data root 아래에 저장한다.
8. `rpotato backend doctor`가 binary path, executable bit, version, port readiness, health check 동작을 진단한다.
9. `rpotato run`은 runtime core에 sidecar child process 시작 또는 재사용을 요청하고, PID/port/log path를 기록하며, reuse가 켜져 있지 않으면 소유 session 종료 시 sidecar를 종료한다.

sidecar는 소유권 측면에서 container-like하지만 Docker 기반은 아닙니다. 명시적인 path, log, port, health check, cleanup을 가진 runtime-managed child process입니다. Docker는 저사양 macOS/Windows 사용자에게 무거운 외부 선행 조건을 추가하므로 MVP 기본값이 아닙니다.

수동 backend override는 이후 config로 허용할 수 있습니다.

```sh
rpotato config set backend.llama_cpp.path /path/to/llama-server
```

override된 backend는 사용자 소유입니다. `rpotato uninstall`은 사용자 소유 backend를 삭제하면 안 됩니다.

### 초기 모델 방향

우선 평가 후보:

- `unsloth/Qwen3.5-4B-GGUF`의 `Qwen3.5-4B-Q4_K_M.gguf`

상태:

- 사용자 의도로 지정된 후보이며, 확정 기본 모델이 아니다.
- artifact URL, provider page, LFS SHA-256, file size는 source-recorded `unverified`로 기록되어 있다.
- local `llama.cpp b9878` smoke, RAM fit, text-only mmproj 필요 여부, benchmark fit은 아직 미검증이다.
- source-backed evaluation이 끝나기 전까지 한국어/코드/agent 품질, multimodal 지원, 16 GB 적합성을 사실로 쓰지 않는다.

비교 평가 후보:

- `google/gemma-4-E4B-it-qat-q4_0-gguf`의 `gemma-4-E4B_q4_0-it.gguf`

상태:

- 비교 후보일 뿐이다.
- artifact URL, provider page, LFS SHA-256, file size는 source-recorded `unverified`로 기록되어 있다.
- multimodal 지원, text-only mmproj 필요 여부, runtime fit, benchmark fit은 아직 미검증이다.
- local runtime 검증과 benchmark 실행 후에만 의미 있는 비교 후보가 된다.

기본값이 아닌 후보:

- `Qwen3.5-9B`: 더 큰 로컬 모델은 context, 검증, runtime overhead 부담을 키울 수 있다. 정확한 실행 가능성은 미확정이며 측정이 필요하다.

### 모델과 Runtime 다운로드 흐름

모델 가중치는 초기 `rpotato` release artifact에 포함하지 않습니다.

예상 흐름:

1. 사용자가 `rpotato`를 설치한다.
2. 사용자가 `rpotato init` 또는 `rpotato model install`을 실행한다.
3. CLI surface가 요청을 runtime core에 전달한다.
4. runtime core가 OS, 아키텍처, RAM, 사용 가능한 disk를 확인한다.
5. runtime core가 관리형 backend binary를 검증하거나 설치한다.
6. runtime core가 manifest 검증 후에만 source-verified model candidate를 추천한다.
7. CLI surface가 사용자에게 download 승인을 명시적으로 받는다.
8. runtime core가 이어받기 가능한 방식으로 모델을 다운로드한다.
9. runtime core가 hash를 검증한다.
10. runtime core가 local config에 모델을 등록한다.
11. runtime core가 local inference backend를 시작하거나 재사용한다.

모델 metadata는 manifest에 둡니다.

```json
{
  "id": "qwen3.5-4b-q4-k-m",
  "displayName": "Qwen3.5 4B",
  "format": "gguf",
  "backend": "llama.cpp",
  "recommendedRamGb": null,
  "license": "TODO",
  "sha256": "TODO",
  "url": "TODO"
}
```

위 JSON은 schema 초안입니다. `null`과 `TODO`는 placeholder이며 제품 사실이 아닙니다.

### 작은 모델용 Runtime 책임

runtime이 소유해야 하는 것:

- model install/cache 관리
- backend binary install/cache 관리
- model process lifecycle
- session lifecycle과 state transition
- hook lifecycle
- skill invocation과 state
- foreign plugin import와 normalized capability validation
- 모델별 prompt compilation
- 모델별 token usage accounting
- model knowledge/evidence indexing
- ontology와 context lifecycle
- context packing
- repo/file indexing
- model/runtime monitoring
- tool permission policy
- subagent lifecycle
- team coordination
- structured action schema
- 가능한 범위의 constrained output
- retry policy
- diff generation과 validation
- command/test/log feedback
- 최종 한국어 전용 응답 validation

## 3. 상태, 근거, 로컬 데이터

### 저장소 Layout

설치 자산, cache, project state는 uninstall 동작을 예측 가능하게 만들기 위해 분리합니다.

초기 logical root:

```text
rpotato app data root/
  config/
  backends/           # managed llama.cpp binaries and metadata
  models/             # GGUF model artifacts
  downloads/          # resumable partial downloads
  manifests/          # model/backend manifests
  logs/
  state/
    observability.sqlite
    runtime-ledger.jsonl
    transcripts/<project-id>/<session-id>/*.json
  plugins/
    imported/
    data/
  cache/

project root/
  .rpotato/           # optional project-local state, indexes, evidence
```

platform path는 Phase 1에서 결정하지만 경계는 안정적으로 유지해야 합니다.

- `backends/`와 `rpotato` launcher는 program/runtime asset입니다.
- `models/`, `downloads/`, `manifests/`, generated context index, SQLite monitoring store, log는 cache/data asset입니다.
- project-local `.rpotato/`는 사용자 project state이며, 사용자가 해당 project에서 cleanup을 명시적으로 요청하지 않는 한 global uninstall로 삭제하면 안 됩니다.

### Observability와 Monitoring

모델 monitoring은 나중에 붙이는 analytics 기능이 아니라 필수 runtime capability입니다.

기본 결정:

- SQLite를 local query/index/reporting store로 사용한다.
- append-only ledger/JSONL은 audit trail과 crash recovery source로 유지한다.
- token, latency, backend, guard, tool, evidence, stop-gate metric과 재생성 가능한 transcript projection을 session/workflow/model별로 저장한다.
- resume를 위해 local user turn과 visible/normalized model/tool/evidence turn을 저장한다. 전체 backend prompt, hidden/raw model response, raw source body, credential이 포함될 수 있는 command output은 저장하지 않는다.
- `rpotato monitor ...`, `doctor`, benchmark report, TUI view로 monitoring을 노출한다.

### Model Knowledge Base

LLM wiki는 model knowledge base로 도입합니다. 즉 manifest record, benchmark result, observability metric, source-backed claim 위에 놓이는 evidence index입니다.

자동 관리는 유용하지만 gate가 필요합니다.

- Agent는 반복 runtime evidence에서 `observed` 또는 `candidate` note를 자동으로 추가할 수 있다.
- 빈도는 우선순위를 높이고 조사를 시작하는 trigger가 될 수 있다.
- 빈도만으로 model quality, license, backend compatibility, RAM fit, default-model status를 확정할 수 없다.
- `measured-locally`는 benchmark/run id, artifact hash, environment, prompt/runtime version, scoring evidence가 필요하다.
- source/license/artifact 확정은 model manifest와 model source policy 경계 아래에 남긴다.
- raw prompt와 raw source text는 기본적으로 model knowledge base에 저장하지 않는다.

필수 model metric:

- prompt tokens
- completion tokens
- total tokens
- context tokens used and dropped
- ontology/tool-summary token budget
- first token latency
- total latency
- tokens per second
- backend startup time
- peak memory
- retry/regeneration count
- Korean guard rejection count
- stop gate pass/fail

runtime은 model-level token total, failure rate, latency percentile, benchmark와 실제 run 비교 같은 cross-session query가 필요하므로 SQLite가 적합합니다. append-only ledger는 event source로 남고, SQLite는 빠른 local query를 위한 projection입니다.

`benchmark run`은 첫 executable benchmark slice입니다. Project-local prompt artifact를 읽고 active backend sidecar를 호출한 뒤 local `measured-locally` 0-3 product score, `model_run_id` link, token/latency/resource summary, redacted reproducibility metadata를 저장합니다. SQLite에는 raw prompt/source text를 저장하지 않고 public benchmark parity도 주장하지 않습니다.

### 삭제와 Cache 정책

삭제는 CLI surface로 제공해야 하며, 어떤 것도 삭제하기 전에 dry-run summary를 보여줘야 합니다.

명령:

```sh
rpotato uninstall --keep-cache
rpotato uninstall --purge-cache
rpotato uninstall --dry-run --purge-cache
rpotato cache status
rpotato cache clean --models
rpotato cache clean --downloads
```

동작:

- `--keep-cache`: `rpotato`가 관리하는 program/runtime asset과 launcher registration은 제거하되, downloaded model, partial download, manifest, log, project-local `.rpotato/` state는 유지한다.
- `--purge-cache`: program/runtime asset과 app-level cache인 model, download, backend archive, manifest, log, generated index를 함께 제거한다.
- `--purge-cache`도 source repository나 project file은 삭제하지 않는다. project-local cleanup은 `rpotato project clean --dry-run` 같은 별도 project-scoped command가 필요하다.
- CLI가 package manager로 설치되었다면 `rpotato uninstall`은 app-owned data를 정리하고, package manager의 binary까지 삭제할 수 있다고 가장하지 말고 정확한 package-manager removal command를 출력해야 한다.
- 현재 실행 중인 binary 삭제가 안전하지 않거나 불가능한 platform에서는 `rpotato uninstall`이 post-exit cleanup script를 쓰거나 마지막 manual command를 한국어로 출력해야 한다.
- 모든 delete path는 실행 전 `--dry-run`, path listing, 한국어 confirmation text를 지원해야 한다.

## 4. 에이전트 동작과 안전

### Agent 전략

작은 작업은 기본적으로 sequential agent로 처리합니다. 실제로 parallel 또는 staged work가 도움이 되는 작업에 한해 subagent와 team execution을 지원합니다.

초기 role:

- planner: 짧은 구조화 plan 생성
- executor: 작은 action 또는 patch 제안
- verifier: command/test/log output 검토
- reporter: 최종 한국어 전용 사용자 응답 생성

기본적으로 피할 것:

- 제한 없는 parallel decoding
- 모델 여러 개 동시 로딩
- 큰 context dump
- 제한 없는 shell access
- 긴 자유 형식 reasoning output

필수 고급 runtime capability:

- lifecycle hooks
- reusable skills
- bounded subagents
- team orchestration
- TUI surface

### 한국어 전용 요구사항

사용자에게 보이는 출력은 code나 정확한 파일 내용이 명시적으로 필요한 경우를 제외하고 한국어만 사용해야 합니다.

runtime guard:

- 영어, 중국어, 일본어 누수를 감지한다.
- 혼합 언어 final answer를 거부한다.
- 더 엄격한 지시로 1회 regenerate한다.
- 그래도 실패하면 한국어 전용 오류로 fail closed한다.
- code block과 자연어 output을 분리한다.

### CLI 안전 모델

CLI surface는 표시하고 묻습니다. runtime core는 판단하고 강제합니다. 기본 동작은 보수적이어야 합니다.

- 선택된 project 안에서는 file read를 자유롭게 허용한다.
- file write 전에는 confirmation을 요구한다.
- side effect가 있는 command 실행 전에는 confirmation을 요구한다.
- 변경 적용 전 diff를 보여준다.
- operation log를 남긴다.
- local runtime/model diagnostics를 위한 `doctor`를 제공한다.

이 정책은 나중에 trust mode로 완화할 수 있습니다.

## 5. 배포와 열린 결정

### 배포 방향

초기 배포:

- GitHub repository
- binary용 GitHub Releases. v0.24.2 workflow는 macOS Apple Silicon, macOS Intel,
  Windows x86_64 asset, per-asset `.sha256` file, aggregate checksums file을 만든다.
- repository 또는 release asset 안의 model manifest

가능성이 높은 package channel:

- macOS/Linux용 Homebrew
- Windows용 Scoop 또는 winget
- JavaScript ecosystem adoption이 중요할 때만 npm wrapper

구현 언어 후보:

- Rust: single-binary distribution, process control, packaging, cross-platform reliability에 유리하므로 선호
- TypeScript/Node: prototype은 빠르지만 self-contained distribution에는 약함

현재 방향:

- CLI surface를 가진 Rust runtime core
- 필수 제품 surface인 terminal TUI
- 관리형 `llama.cpp` sidecar
- future backend를 위한 adapter boundary

### MVP 정의

첫 유용한 버전은 다음을 만족해야 합니다.

1. `rpotato` CLI surface로 설치하고 실행할 수 있다.
2. download가 필요하면 사용자 동의 후 관리형 `llama.cpp` backend를 설치하거나 검증한다.
3. 사용자 동의 후 권장 GGUF 모델 하나를 다운로드한다.
4. local inference backend를 시작한다.
5. 한국어로 대화한다.
6. local repo를 inspect한다.
7. 작은 patch를 제안한다.
8. 적용 전에 diff를 보여준다.
9. 승인 시 verification command를 실행한다.
10. 한국어 전용 final report를 생성한다.
11. keep-cache와 purge-cache 경로를 CLI에서 노출하고 runtime을 통해 managed runtime asset을 uninstall한다.

replacement-level beta는 추가로 다음을 만족해야 합니다.

1. TUI surface를 제공한다.
2. hook-attached policy와 evidence gate를 가진 skill을 실행한다.
3. bounded subagent를 지원한다.
4. runtime-owned merge와 stop gate를 가진 team execution을 지원한다.
5. Claude Code/Codex형 plugin package를 adapter validation과 runtime policy gate를 통해서만 import한다.
6. approval, diff, tool output, subagent/team status, plugin permission review, evidence를 TUI에서 보여준다.

### 확정된 결정

- Runtime core와 CLI는 Rust로 구현합니다.
- SQLite projection ownership은 `rusqlite`를 사용하고 canonical ledger가
  권위를 유지합니다.
- 관리형 backend artifact는 `llama.cpp b9982`에 source-pin합니다.
- Command approval, native hook, built-in skill, bounded subagent, team
  execution, local plugin adapter에는 runtime-owned policy 경계가 있습니다.
- Plugin import는 local directory만 지원하며 remote marketplace, registry,
  catalog는 지원 경계 밖입니다.
- Monitoring은 CLI/TUI와 같은 SQLite/ledger data를 사용하는 선택형
  self-contained local HTML export를 제공합니다.

### 열린 질문

- 정확한 `b9982`, 16 GB RAM-fit, mmproj, 품질, benchmark evidence를 통과해
  지원 기본값이 될 source-backed 후보는 무엇인가?
- Image 또는 screenshot understanding을 이후 버전에 포함할 것인가?
- 현재 std-only line controller보다 풍부한 TUI에 framework를 도입할 것인가?
- 기본 monitoring retention period는 얼마인가?
- 16 GB machine에서 측정된 안전한 subagent/team lane과 context budget은
  얼마인가?
- 이후 버전에서 non-code general automation을 지원할 것인가?

### 현재 문서

장별 [문서 인덱스](README.md)가 탐색의 source of truth입니다.
[현재 기능 안내](current-capabilities.md)는 구현 surface와 알려진 경계를
여기서 중복하지 않고 정리합니다.

Project-local automation과 contribution policy는
[AGENTS.md](../../AGENTS.md)에 기록합니다. 모델 관련 claim은 명시적 출처가
필요하며 [모델 출처 정책](model-source-policy.md)을 따라야 합니다.

현재 `v0.41.0` 이후 버전은 정의되어 있지 않습니다. 열린 질문을 구현 작업으로
전환하기 전에 [로드맵](ROADMAP.md)에 구체적인 버전 행을 추가해야 합니다.
