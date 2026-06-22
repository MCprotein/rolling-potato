# Roadmap

이 로드맵은 `rolling-potato`를 단순한 prompt harness가 아니라 작은 모델용 로컬 coding agent runtime으로 만들기 위한 순서입니다.

참고 문서:

- `/Users/sys/Desktop/codes/harness/HARNESS-OPERATING-MODELS.md`

반영한 원칙:

- 모델을 더 똑똑하게 만든다고 가정하지 않는다.
- instruction, permission, state, ledger, transcript, evidence gate를 runtime이 소유한다.
- 작은 모델에게 큰 자유도를 주기보다 작은 vertical slice를 확실히 끝낸다.
- 완료 판정은 모델의 말이 아니라 검증 evidence와 Stop gate가 결정한다.

## Phase 0: 프로젝트 정의

- [x] 제품 포지셔닝
- [x] MVP 인수 기준
- [x] Rust + `llama.cpp` sidecar 방향
- [x] 한국어 전용 출력 요구사항
- [x] 오픈소스 운영 문서
- [x] 모델 출처 정책
- [x] 하네스 운영 모델 검토

## Phase 1: CLI 기반

목표: `rpotato`가 안정적인 단일 CLI entrypoint로 실행되고, 모든 후속 기능이 같은 설정, 출력, 오류 경계를 사용한다.

- [ ] Rust workspace scaffold
- [ ] `rpotato` 명령 router
- [ ] `rpotato init`
- [ ] `rpotato doctor`
- [ ] `rpotato config`
- [ ] config 경로 결정
- [ ] 구조화된 한국어 오류 보고
- [ ] 명령 출력 스타일 가이드
- [ ] operation log path 결정
- [ ] 기본 명령 smoke test

## Phase 2: Runtime State And Ledger

목표: 세션이 바뀌어도 runtime이 현재 작업 상태와 감사 기록을 복원할 수 있게 한다.

- [ ] local state root layout
- [ ] project/session identity
- [ ] append-only ledger format
- [ ] session transcript JSONL format
- [ ] operation log format
- [ ] state owner 규칙
- [ ] stale state reconcile
- [ ] checkpoint record schema
- [ ] evidence record schema
- [ ] 중단된 실행의 resume 동작

## Phase 3: Permission And Tool Boundary

목표: 파일 쓰기, command 실행, 다운로드 같은 side effect를 모델 출력이 아니라 runtime policy로 통제한다.

- [ ] tool/action schema
- [ ] project boundary checker
- [ ] 파일 읽기 allow/exclude 규칙
- [ ] diff-before-write gate
- [ ] command classifier
- [ ] command 승인 prompt
- [ ] destructive command deny/high-confirm policy
- [ ] 로그 credential redaction
- [ ] network/download 승인 policy
- [ ] permission decision audit record
- [ ] policy fixture test

## Phase 4: Model Manifest And Install

목표: 출처와 checksum이 검증된 모델 후보만 설치하고 registry에 등록한다.

- [ ] 출처 기반 model manifest schema
- [ ] 모델 후보 상태: `candidate`, `unverified`, `verified`
- [ ] `rpotato model list`
- [ ] `rpotato model install <id>`
- [ ] 이어받기 가능한 모델 다운로드
- [ ] SHA-256 verification
- [ ] 다운로드 전 license/source 표시
- [ ] 검증 실패 artifact 정리
- [ ] local model registry
- [ ] manifest test

## Phase 5: Backend Runtime

목표: `llama.cpp` sidecar를 CLI가 관리하고, backend 상태를 진단 가능하게 만든다.

- [ ] `llama.cpp` sidecar discovery
- [ ] backend binary path config
- [ ] port 선택
- [ ] sidecar process lifecycle
- [ ] health check
- [ ] startup timeout 처리
- [ ] streaming response path
- [ ] cancellation path
- [ ] stderr/stdout capture
- [ ] backend diagnostics in `doctor`
- [ ] backend adapter trait

## Phase 6: Instruction And Context Plane

목표: 작은 모델에 필요한 지시, context, 출력 형식을 매번 임기응변으로 넣지 않고 runtime이 조립한다.

- [ ] prompt compiler
- [ ] role templates: planner, executor, verifier, reporter
- [ ] 한국어 final-response instruction
- [ ] 구조화된 action output format
- [ ] repository file discovery
- [ ] context packing budget
- [ ] generated/vendor exclusion rules
- [ ] command/log summarization input format
- [ ] prompt fixture test

## Phase 7: 첫 Agent Vertical Slice

목표: 작은 fixture 저장소에서 읽기, 계획, patch 제안, 승인, 적용, 검증, 한국어 보고까지 한 번에 끝낸다.

- [ ] `rpotato run "<task>"`
- [ ] planner step
- [ ] executor step
- [ ] verifier step
- [ ] reporter step
- [ ] unified diff rendering
- [ ] approved patch apply
- [ ] patch apply rollback/failure handling
- [ ] approved verification command execution
- [ ] verification output interpretation
- [ ] final Korean report

## Phase 8: Stop Gate And Reliability Gates

목표: 끝났다는 말이 아니라 evidence가 있어야 완료로 인정한다.

- [ ] Stop gate completion contract
- [ ] command별 required evidence
- [ ] pending action detection
- [ ] 검증 실패 시 continuation
- [ ] invalid diff rejection
- [ ] Korean output guard
- [ ] mixed-language regeneration
- [ ] fail-closed Korean error
- [ ] destructive command policy tests
- [ ] fixture benchmark suite
- [ ] regression test report format

## Phase 9: Packaging And Release

목표: 사용자가 모델 가중치 없이 CLI를 설치하고, 첫 모델 설치를 안전하게 진행할 수 있게 한다.

- [ ] release build pipeline
- [ ] binary checksum 생성
- [ ] macOS Apple Silicon artifact
- [ ] macOS Intel artifact
- [ ] Windows x86_64 artifact
- [ ] release notes template
- [ ] `rpotato doctor` release smoke test
- [ ] GitHub Releases distribution
- [ ] post-MVP Homebrew/Scoop decision

## MVP 전 비범위

- GUI 앱
- parallel agents
- 여러 모델 동시 로딩
- remote GPU/server mode 기본 지원
- MCP server ecosystem
- team orchestration
- destructive command 자동 실행
- 외부 코드 PR workflow
- 출처 없는 모델 추천
