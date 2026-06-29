# Architecture

## MVP 결정

MVP는 Rust runtime core, CLI surface, 관리형 `llama.cpp` sidecar로 시작합니다.

이 결정은 다음 제약을 우선합니다.

- 16 GB RAM 노트북에서 실행 가능해야 한다.
- macOS와 Windows를 먼저 지원해야 한다.
- 설치물이 가능한 한 단순해야 한다.
- 모델 가중치를 `rpotato` release binary에 포함하지 않아야 한다.
- 작은 모델의 취약성을 프롬프트가 아니라 런타임 정책으로 줄여야 한다.

## 왜 Rust인가

Rust를 기본 구현 언어로 선택합니다.

선택 이유:

- 단일 binary 배포에 유리하다.
- cross-platform process control이 안정적이다.
- 파일, diff, 명령 실행, 설정 관리 같은 로컬 작업에 적합하다.
- Node 런타임 설치를 사용자에게 요구하지 않아도 된다.
- 추후 native backend 통합을 검토하기 쉽다.

TypeScript/Node는 프로토타입 속도는 빠르지만 MVP의 핵심 가치인 가벼운 배포와 로컬 프로세스 제어에는 덜 맞습니다.

## 왜 `llama.cpp` sidecar인가

MVP는 native binding이 아니라 `llama-server` 또는 동등한 `llama.cpp` 실행 파일을 sidecar로 관리합니다.

선택 이유:

- GGUF 생태계와 가장 직접적으로 연결된다.
- macOS, Windows, Linux에 모두 맞출 수 있다.
- native binding보다 packaging risk가 낮다.
- 백엔드 장애를 CLI 프로세스와 분리할 수 있다.
- 추후 LM Studio, Ollama, vLLM, SGLang adapter를 붙일 경계가 선명하다.

native binding은 다음 조건이 필요할 때 다시 검토합니다.

- sidecar startup latency가 제품 경험을 크게 해친다.
- 배포 대상별 binary 관리가 지나치게 복잡해진다.
- streaming, cancellation, token accounting에서 HTTP boundary가 병목이 된다.

사용자는 `llama.cpp`를 전역 설치하지 않아도 됩니다. MVP 기본 경로는 runtime core가 platform별 backend binary를 다운로드하고 checksum을 검증한 뒤 앱 데이터 디렉터리에서 child process로 실행하는 방식입니다. 사용자가 직접 설치한 backend path를 config로 지정한 경우 그 binary는 사용자 소유로 보고 `rpotato uninstall`이 삭제하지 않습니다.

## 구성 요소

```text
user
  ├─ CLI surface: rpotato
  └─ TUI surface: rpotato tui
       ├─ command parser
       ├─ prompt/approval renderer
       ├─ diff/result display
       ├─ subagent/team status display
       └─ Korean final report display

runtime core
  ├─ config manager
  ├─ model manager
  ├─ backend manager
  ├─ state and ledger
  ├─ observability store
  ├─ hook engine
  ├─ skill registry
  ├─ ontology and context plane
  ├─ repo indexer
  ├─ agent loop
  ├─ subagent runtime
  ├─ team runtime
  ├─ tool policy
  ├─ patch manager
  ├─ verifier
  ├─ evidence and stop gate
  ├─ token and resource monitor
  └─ Korean response guard

managed backend
  └─ llama.cpp sidecar
       └─ GGUF model
```

## 저장소와 캐시 경계

삭제와 재설치를 안전하게 만들기 위해 program/runtime asset, cache/data, project-local state를 분리합니다.

```text
rpotato app data root/
  config/
  backends/      # 관리형 llama.cpp binaries
  models/        # GGUF model artifacts
  downloads/     # 이어받기 가능한 임시 다운로드
  manifests/     # model/backend manifests
  logs/
  state/
    observability.sqlite
    runtime-ledger.jsonl
  cache/

project root/
  .rpotato/      # project-local index/state/evidence, 명시적 project cleanup 전에는 보존
```

`rpotato uninstall --keep-cache`는 프로그램/관리형 runtime asset만 제거하고 모델, 다운로드, manifest, 로그, project-local state는 남깁니다.

`rpotato uninstall --purge-cache`는 app-level cache와 모델까지 제거하지만 source repository나 project-local `.rpotato/`는 자동 삭제하지 않습니다. 모든 삭제 경로는 `--dry-run`으로 삭제 예정 path를 먼저 보여줘야 합니다.

## 책임 경계

### CLI/TUI surfaces

Surface는 사용자 경험을 소유하지만, 로컬 작업 정책을 직접 집행하지 않습니다. 사용자의 요청, 승인, diff 표시, 진행 상태, 최종 보고를 runtime core에 연결합니다.

- 명령어 파싱
- 사용자 입력 전달
- 승인 prompt 표시
- 모델 다운로드 진행 상태 표시
- diff 표시와 적용 승인 전달
- 검증 명령 승인 전달
- subagent/team 상태 표시
- evidence/stop gate 상태 표시
- 최종 한국어 응답 표시

### Runtime core

Runtime core는 Claude Code/Codex류 agent 경험의 본체입니다. CLI surface, future TUI/IDE surface, test harness가 붙더라도 같은 정책과 상태를 사용해야 합니다.

- 설정 파일 읽기와 쓰기
- session state와 append-only ledger 관리
- SQLite monitoring projection 관리
- hook lifecycle 관리
- skill registry와 invocation 관리
- 모델 manifest 해석
- 모델 다운로드, hash 검증, registry 등록
- sidecar 시작, 재시작, 종료
- 프로젝트 파일 읽기와 context packing
- ontology lifecycle 관리
- subagent lifecycle 관리
- team runtime 조정
- tool permission policy 집행
- diff 생성과 patch 적용
- 검증 명령 분류와 실행
- evidence 수집과 stop gate 판정
- token 사용량, latency, memory, backend health metric 수집
- 최종 한국어 응답 검증

### Backend adapter

Backend adapter는 추론 백엔드 차이를 숨깁니다.

공통 인터페이스는 다음 수준이면 충분합니다.

- health check
- model metadata
- chat completion
- streaming tokens
- cancellation
- context length reporting
- backend diagnostics

MVP adapter는 `llama.cpp`만 구현합니다.

### Agent loop

첫 vertical slice의 agent loop는 순차 실행입니다.

기본 단계:

1. planner: 짧은 작업 계획 생성
2. executor: 작은 패치 또는 명령 제안
3. verifier: diff, 명령 출력, 로그 확인
4. reporter: 한국어 최종 보고 생성

작은 모델에서는 무조건적인 병렬 agent보다 다음이 더 중요합니다.

- 각 단계의 출력 형식 제한
- context 크기 제한
- 실패 시 짧은 재시도
- 검증 가능한 단일 action

서브에이전트와 팀 실행은 replacement-level runtime의 필수 기능이지만, parent workflow, hook, policy, evidence, stop gate를 공유해야 합니다.

## 안전 모델

기본 정책은 보수적입니다.

- 프로젝트 내부 파일 읽기는 허용한다.
- 파일 쓰기는 diff 표시 후 사용자 승인을 요구한다.
- side effect가 있는 명령은 사용자 승인을 요구한다.
- 모델 다운로드는 CLI surface가 사용자 승인을 받은 뒤 runtime core가 수행한다.
- operation log를 남긴다.
- `doctor` 명령으로 환경, backend, 모델 상태를 점검한다.

추후 trust mode를 둘 수 있지만 MVP 기본값으로 만들지 않습니다.

## 한국어 응답 guard

한국어 전용 출력은 모델 프롬프트만으로 처리하지 않습니다.

guard 단계:

1. 응답을 코드 블록과 자연어 블록으로 분리한다.
2. 자연어 블록에서 영어, 중국어, 일본어 누수를 감지한다.
3. 허용 목록을 적용한다.
   - 명령어
   - 파일 경로
   - 패키지명
   - 코드 식별자
   - 원문 로그 인용
4. 누수가 있으면 더 강한 지시로 한 번 재생성한다.
5. 다시 실패하면 한국어 오류 메시지로 종료한다.

이 guard는 최종 reporter 출력에 반드시 적용합니다. 중간 모델 출력에는 더 느슨하게 적용할 수 있지만, 사용자에게 그대로 노출하지 않습니다.

## 모델 manifest

모델 metadata는 manifest로 관리합니다.

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

`null`과 `TODO`는 schema placeholder이며 제품 사실이 아닙니다.

정확한 artifact URL, hash, license, backend 호환성은 벤치마크와 출처 확인 후 확정합니다. 모델 관련 claim은 [model-source-policy.md](model-source-policy.md)를 따릅니다.

## 후순위 adapter

다음 backend는 MVP 이후 검토합니다.

- LM Studio: 이미 설치한 사용자와 데모 흐름에 유리하다.
- Ollama: 사용자 기반은 크지만 core runtime으로는 무겁고 opaque하다.
- vLLM/SGLang: GPU 또는 server mode에 유리하다.

이 adapter들은 기본 경험이 아니라 확장 경로입니다.
