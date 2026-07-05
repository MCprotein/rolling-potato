# MVP 인수 기준

## 목표

MVP는 "작은 로컬 모델로도 안전하게 작은 코딩 작업을 끝낼 수 있다"를 증명해야 합니다.

범위는 의도적으로 좁힙니다. 좋은 대화형 코딩 에이전트 전체가 아니라, CLI surface로 시작해 runtime core가 로컬 모델 설치부터 작은 patch 검증까지 한 흐름을 완성합니다.

단, 전체 제품 목표는 Claude Code/Codex를 대신할 수 있는 runtime입니다. Hooks, skills, local plugin adapter, subagents, team runtime, TUI는 MVP 이후 replacement-level beta에 필요한 필수 기능입니다.

## 사용자 시나리오

대표 시나리오:

1. 사용자가 `rpotato init`을 실행한다.
2. CLI surface가 init 요청을 runtime core에 전달한다.
3. runtime core가 환경을 점검한다.
4. runtime core가 관리형 `llama.cpp` sidecar를 설치 또는 확인한다.
5. runtime core가 출처와 checksum이 검증된 모델 후보를 제안한다.
6. CLI surface가 다운로드 승인을 받는다.
7. runtime core가 모델을 다운로드하고 hash를 검증한다.
8. 사용자가 프로젝트에서 `rpotato run "테스트 실패 고쳐줘"`를 실행한다.
9. runtime core가 관련 파일과 온톨로지/context를 조회하고 작은 수정안을 만든다.
10. CLI surface가 diff를 보여주고 적용 승인을 받는다.
11. 사용자가 승인하면 runtime core가 patch를 적용한다.
12. CLI surface가 검증 명령 실행 승인을 받는다.
13. runtime core가 검증 결과를 해석하고 reporter가 한국어로 최종 보고한다.

## 기능 기준

### 설치와 초기화

- `rpotato` 명령이 macOS와 Windows에서 실행된다.
- `rpotato init`은 OS, CPU architecture, RAM, disk 여유 공간을 확인한다.
- 지원 불가 환경에서는 한국어로 명확한 이유를 보여준다.
- 모델 가중치는 `rpotato` 설치물에 포함하지 않는다.
- `llama.cpp` backend는 사용자가 직접 전역 설치하지 않아도 되는 관리형 sidecar로 설치 또는 확인된다.
- `rpotato uninstall --keep-cache`와 `rpotato uninstall --purge-cache`는 삭제 예정 path를 먼저 보여준다.
- CLI surface는 runtime core의 정책 결정을 우회하지 않는다.

### 모델 관리

- `rpotato model list`가 설치 가능 모델과 설치된 모델을 구분한다.
- `rpotato model install <id>`는 runtime core가 검증한 다운로드 전 크기와 license 정보를 보여준다.
- 다운로드는 중단 후 재개할 수 있다.
- 다운로드 완료 후 SHA-256 hash를 검증한다.
- 검증 실패 시 모델을 등록하지 않는다.

### backend 관리

- runtime core는 `llama.cpp` sidecar를 시작하거나 기존 프로세스를 재사용한다.
- `rpotato doctor`는 backend binary, model file, port, health check 상태를 보여준다.
- `rpotato backend doctor`는 관리형 backend binary, version, 실행 권한, health check를 별도로 점검한다.
- backend 시작 실패 시 원인을 한국어로 좁혀서 보고한다.
- runtime core는 model run별 token, latency, backend health metric을 로컬 monitoring store에 기록한다.

### 저장소 작업

- runtime core는 현재 프로젝트 내부 파일을 읽을 수 있다.
- 기본 동작은 필요한 파일만 좁혀 읽는 것이다.
- 프로젝트 외부 파일은 기본적으로 작업 범위에 포함하지 않는다.
- generated/vendor 대용량 디렉터리는 기본 index 대상에서 제외한다.
- runtime core는 source pointer 없이 snippet만 근거로 patch를 만들지 않는다.

### patch 흐름

- 수정은 unified diff 또는 내부 patch format으로 표시한다.
- 적용 전 사용자 승인을 요구한다.
- 승인 전에는 파일을 쓰지 않는다.
- patch 적용 실패 시 원본 파일을 보존한다.
- 관련 없는 formatting churn을 만들지 않는다.

### 검증 흐름

- 검증 명령 실행 전 사용자 승인을 요구한다.
- 명령 출력은 요약하되, 실패 원인 판단에 필요한 핵심 줄은 보존한다.
- 실패하면 다음 action을 하나로 좁힌다.
- 성공하면 어떤 검증이 통과했는지 최종 보고에 포함한다.

### 한국어 출력

- 최종 자연어 보고는 한국어만 사용한다.
- 코드, 명령어, 파일 경로, package 이름, 원문 로그는 예외로 허용한다.
- 영어, 중국어, 일본어 누수가 감지되면 한 번 재생성한다.
- 재생성 후에도 실패하면 한국어 오류 메시지로 종료한다.

## 첫 vertical slice 비범위

첫 vertical slice에서 하지 않습니다. 아래 항목은 거절이 아니라 단계적 구현 대상입니다.

- GUI 앱
- 멀티모달 screenshot 이해
- 여러 모델 동시 로딩
- 원격 GPU 서버 기본 지원
- 자동 destructive command 실행
- 대규모 refactor 자동 적용
- npm wrapper 또는 Homebrew/Scoop 패키지 배포
- 외부 plugin marketplace, registry, catalog, mirror 연동
- remote URL plugin install
- 외부 plugin 직접 실행

첫 vertical slice 이후 필수 구현:

- lifecycle hooks
- reusable skills
- local plugin import adapter
- bounded subagents
- team runtime
- TUI surface

## 완료 정의

MVP 완료 조건:

- macOS와 Windows에서 `rpotato init`, `model install`, `chat`, `run`, `doctor`가 동작한다.
- 한 개의 출처 검증된 GGUF 모델 후보가 manifest로 설치된다.
- sidecar backend health check가 통과한다.
- 작은 fixture 저장소에서 patch 제안, 승인, 적용, 검증이 끝난다.
- runtime core가 상태, 권한, context, tool result, evidence를 소유한다.
- 모델별 token/latency/resource metric이 로컬 store에 기록된다.
- 최종 보고의 한국어 guard가 테스트로 검증된다.
- destructive action policy 위반 테스트가 0건이다.
- uninstall keep-cache와 purge-cache smoke test가 통과한다.

## 남은 결정

- 정확한 `Qwen3.5-4B` GGUF artifact
- 기본 quantization level
- config file 경로와 형식
- operation log 보관 위치
- Windows binary packaging 방식
- fixture benchmark 저장소 구성
