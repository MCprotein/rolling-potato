# Roadmap

이 로드맵은 `rolling-potato`를 단순한 prompt harness가 아니라 작은 모델용 로컬 coding agent runtime으로 만들기 위한 순서입니다.

참고 문서:

- `/Users/sys/Desktop/codes/harness/HARNESS-OPERATING-MODELS.md`
- `/Users/sys/Desktop/codes/anamnesis/docs/ONTOLOGY-BOOTSTRAP.md`
- `/Users/sys/Desktop/codes/anamnesis/docs/CONTEXT-INDEX-DESIGN.md`
- `/Users/sys/Desktop/codes/anamnesis/docs/RUNTIME-EVIDENCE.md`
- `/Users/sys/Desktop/codes/anamnesis/docs/README-CLAIMS.md`

반영한 원칙:

- 모델을 더 똑똑하게 만든다고 가정하지 않는다.
- instruction, permission, state, ledger, transcript, evidence gate를 runtime이 소유한다.
- 온톨로지는 작은 모델이 매번 추론으로 복구하면 안 되는 프로젝트 기억 substrate로 취급한다.
- runtime core가 증명할 수 있는 Layer A 사실과 agent/runtime이 보강하되 출처를 남겨야 하는 Layer B 의미를 분리한다.
- context는 무작정 주입하지 않고 source pointer, stable ref, digest, resume bundle로 좁혀서 공급한다.
- snippet은 authoritative source가 아니다. 중요한 판단은 원본 파일을 다시 읽은 뒤 해야 한다.
- 작은 모델에게 큰 자유도를 주기보다 작은 vertical slice를 확실히 끝낸다.
- 완료 판정은 모델의 말이 아니라 검증 evidence와 Stop gate가 결정한다.
- 현재 상태 view와 append-only ledger를 분리한다.
- SQLite는 monitoring/query projection으로 두고, append-only ledger는 audit trail로 유지한다.
- mode 전환은 모델의 즉흥 판단이 아니라 deterministic rule과 runtime state로 처리한다.
- hooks, skills, subagents, team runtime, TUI는 replacement-level runtime의 1급 capability다.
- Claude Code/Codex형 플러그인은 직접 실행하지 않고 `rpotato` capability로 import/validate/enable하는 adapter 경계를 둔다.
- Plugin adapter는 Codex를 먼저 구현하고 Claude Code를 그 다음에 구현한다.
- 외부 plugin의 shell, `bin/`, MCP server, background process, remote connector, file write path는 기본 차단하고 사용 시 별도 승인으로 푼다.
- 외부 marketplace, registry, catalog, mirror는 연동하지 않는다.
- 플러그인은 사용자가 직접 가진 local directory를 import하는 방식으로만 받는다.
- monitoring은 SSH/Linux server에서 TUI로 먼저 보여주고, HTML은 후속 optional local report/dashboard로 둔다.
- compaction, resume, cancel, corrupt state fallback은 초기 runtime 설계에 포함한다.
- 공개 claim과 모델 claim은 evidence보다 넓게 쓰지 않는다.

## Phase 0: 프로젝트 정의

- [x] 제품 포지셔닝
- [x] MVP 인수 기준
- [x] Rust + `llama.cpp` sidecar 방향
- [x] 한국어 전용 출력 요구사항
- [x] 오픈소스 운영 문서
- [x] 모델 출처 정책
- [x] 하네스 운영 모델 검토
- [x] `anamnesis` 온톨로지/context lifecycle 검토
- [x] runtime surface/core/backend 경계 문서화
- [x] hooks/skills/subagents/team/TUI 필수 capability 반영
- [x] plugin adapter 경계 문서화
- [x] Codex-first, Claude Code-second plugin adapter 우선순위 반영
- [x] marketplace 미연동 및 local directory import 정책 반영

## Phase 1: Runtime Entrypoint And CLI Surface

목표: `rpotato`가 안정적인 첫 surface로 실행되고, 실제 상태/정책/오류 경계는 runtime core가 소유하게 만든다.

- [x] Rust workspace scaffold
- [x] `rpotato` 명령 router
- [x] CLI surface와 runtime core module 경계
- [x] `rpotato init`
- [x] `rpotato doctor`
- [x] `rpotato config`
- [x] `rpotato state`
- [x] `rpotato cancel`
- [x] `rpotato cache status`
- [x] `rpotato monitor status`
- [x] `rpotato monitor models`
- [x] `rpotato uninstall --keep-cache`
- [x] `rpotato uninstall --purge-cache`
- [x] config 경로 결정
- [x] app data/cache/project state 경계 결정
- [x] 구조화된 한국어 오류 보고
- [x] 명령 출력 스타일 가이드
- [x] operation log path 결정
- [x] setup/upgrade/uninstall 경계 결정
- [x] uninstall dry-run path listing
- [x] 기본 명령 smoke test

## Phase 2: Runtime State, Ledger, And Observability

목표: 세션이 바뀌어도 runtime이 현재 작업 상태, 감사 기록, 모델별 monitoring 지표를 복원하고 질의할 수 있게 한다.

- [x] local state root layout
- [x] repo-root state와 session-scoped state 우선순위
- [x] project/session identity
- [x] workflow manifest와 transition table
- [x] terminal state 정의
- [x] state read/write/cancel API
- [x] append-only ledger format
- [x] ledger replay
- [x] session transcript JSONL format
- [x] transcript parent/branch pointer
- [x] transcript metadata entry
- [x] operation log format
- [x] state owner 규칙
- [x] stale state reconcile
- [x] corrupt state fallback
- [x] checkpoint record schema
- [x] evidence record schema
- [x] runtime evidence JSONL store
- [x] evidence stale 판정 기준
- [x] evidence artifact pointer validation
- [x] current state view와 event/evidence ledger 경계
- [x] local SQLite observability store 결정
- [x] SQLite schema migration
- [x] ledger to SQLite projection
- [x] token usage record schema
- [x] model run metric schema
- [x] backend health metric schema
- [x] tool/command metric schema
- [x] guard/stop-gate metric schema
- [x] prompt/source raw text 미저장 기본값
- [x] credential redaction before persistence
- [x] corrupt SQLite fallback
- [x] monitoring export: JSONL/CSV
- [x] monitoring retention/prune dry-run
- [x] 중단된 실행의 resume 동작
- [x] compaction boundary marker
- [x] compacted summary 보존 정책

## Phase 3: Mode And Constraint Recognition

목표: 사용자 입력을 모델에게 넘기기 전에 runtime이 mode, 제약, 완료 기준을 먼저 정규화한다.

- [x] explicit invocation 우선순위
- [x] skill invocation grammar
- [x] `rpotato skill list`
- [x] `rpotato skill run <id>`
- [x] `rpotato plugin import --from codex <local-path>`
- [x] `rpotato plugin import --from claude-code <local-path>`
- [x] `rpotato plugin import --dry-run`
- [x] `rpotato plugin list`
- [x] `rpotato plugin inspect <id>`
- [x] `rpotato plugin validate <id>`
- [x] `rpotato plugin enable <id>`
- [x] `rpotato plugin disable <id>`
- [x] `rpotato plugin remove <id> --keep-data`
- [x] `rpotato plugin remove <id> --purge-data`
- [x] source runtime namespace rule: native, codex, claude-code
- [ ] active workflow 귀속 규칙
- [x] deterministic keyword/phrase rule table
- [x] structural signal extraction: read-only, plan-only, review-only, test-spec
- [x] optional classifier 사용 기준
- [x] user constraint record schema
- [x] repo instruction loading boundary
- [x] generated artifact requirement 추출
- [x] nested/subagent prompt keyword non-activation
- [ ] TUI command palette routing
- [x] intent fixture test

## Phase 4: Permission And Tool Boundary

목표: 파일 쓰기, command 실행, 다운로드 같은 side effect를 모델 출력이 아니라 runtime policy로 통제한다.

- [ ] tool/action schema
- [ ] hook/control-point input/output schema
- [ ] hook registry
- [ ] lifecycle hooks: session_start, user_request_received, pre_context_pack, post_context_pack
- [ ] lifecycle hooks: pre_model_request, post_model_response, pre_action_parse, post_action_parse
- [ ] lifecycle hooks: pre_tool_call, post_tool_result, pre_patch_apply, post_patch_apply
- [ ] lifecycle hooks: pre_command_run, post_command_run, pre_final_report, stop_gate, session_end
- [ ] hook ordering: runtime, project, skill, session, observer
- [ ] hook conflict rule: deny > ask > modify > allow > observe
- [ ] project boundary checker
- [ ] 파일 읽기 allow/exclude 규칙
- [ ] rule source: user, project, local, session, policy
- [ ] allow/ask/deny decision model
- [ ] diff-before-write gate
- [ ] managed artifact manifest/hash tracking
- [ ] `create`/`update`/`noop`/`user-modified`/`blocked` action status
- [ ] user-modified file/region 보존 규칙
- [ ] command classifier
- [ ] command 승인 prompt
- [ ] destructive command deny/high-confirm policy
- [ ] 로그 credential redaction
- [ ] network/download 승인 policy
- [ ] permission decision audit record
- [ ] foreign plugin import permission report
- [ ] foreign plugin shell/background process approval gate
- [ ] foreign plugin local path canonicalization
- [ ] foreign plugin symlink boundary check
- [ ] plugin import/enable/remove ledger record
- [ ] policy fixture test
- [ ] hook JSON input/output fixture test
- [ ] hook fail-closed fixture test

## Phase 5: Model Manifest And Install

목표: 출처와 checksum이 검증된 모델 후보만 설치하고 registry에 등록한다.

- [ ] 출처 기반 model manifest schema
- [ ] 모델 후보 상태: `candidate`, `unverified`, `verified`
- [ ] 모델 후보별 공식 model card/source/license 조사
- [ ] 모델 후보별 공개 benchmark claim/source ledger
- [ ] 공개 benchmark 재현 가능성 평가: harness, dataset, prompt, scoring, hardware/backend 조건
- [x] `rpotato model list`
- [ ] `rpotato model install <id>`
- [x] 미검증 artifact 설치 차단
- [ ] 이어받기 가능한 모델 다운로드
- [ ] SHA-256 verification
- [ ] 다운로드 전 license/source 표시
- [ ] 검증 실패 artifact 정리
- [ ] local model registry
- [ ] manifest test

## Phase 6: Backend Runtime

목표: `llama.cpp` sidecar를 runtime core가 관리하고, backend 상태를 CLI surface에서 진단 가능하게 만든다.

- [ ] `llama.cpp` sidecar discovery
- [ ] managed backend binary download/install
- [ ] backend archive checksum verification
- [ ] backend binary version detection
- [ ] backend binary path config
- [ ] port 선택
- [ ] sidecar process lifecycle
- [ ] health check
- [ ] startup timeout 처리
- [ ] streaming response path
- [ ] cancellation path
- [ ] stderr/stdout capture
- [ ] backend diagnostics in `doctor`
- [ ] backend diagnostics in `rpotato backend doctor`
- [ ] managed backend removal during uninstall
- [ ] backend adapter trait

## Phase 7: Tool Runtime

목표: shell, file, patch, verifier 같은 도구 실행을 일관된 schema와 result format 뒤에 둔다.

- [ ] file read tool contract
- [ ] patch apply tool contract
- [ ] shell command tool contract
- [ ] PTY/non-PTY 실행 기준
- [ ] streaming output 처리
- [ ] command cancellation
- [ ] output truncation과 artifact spill
- [ ] tool result shaping
- [ ] command failure classification
- [ ] unattended environment hardening

## Phase 8: Instruction, Ontology, And Context Plane

목표: 작은 모델에 필요한 지시, 온톨로지, context, 출력 형식을 매번 임기응변으로 넣지 않고 runtime이 조립한다.

- [ ] prompt compiler
- [ ] skill manifest schema
- [ ] skill registry
- [ ] normalized plugin manifest schema
- [ ] foreign plugin parser: Codex `.codex-plugin/plugin.json`
- [ ] Codex plugin inspect/validate dry-run
- [ ] Codex skill import
- [ ] Codex MCP import with default disabled server command
- [ ] foreign plugin parser: Claude Code `.claude-plugin/plugin.json`
- [ ] Claude Code plugin inspect/validate dry-run
- [ ] Claude Code skill/command import
- [ ] Claude Code agent import as subagent role
- [ ] Claude Code hook import
- [ ] Claude Code LSP/monitor/bin/settings/theme import policy
- [ ] plugin capability mapping: skill, hook, subagent, MCP, unsupported
- [x] plugin import dry-run report
- [ ] plugin enable/disable scope policy
- [ ] plugin source snapshot and manifest hash record
- [ ] plugin data path separation
- [ ] unsupported plugin capability ledger record
- [ ] default-block policy for shell/bin/MCP/background/remote/file-write capability
- [x] reject remote plugin URL
- [x] reject plugin marketplace source
- [x] reject plugin registry/catalog source
- [ ] local plugin directory path traversal test
- [ ] skill context requirements
- [ ] skill allowed tools
- [ ] skill evidence requirements
- [ ] skill stop criteria
- [ ] role templates: planner, executor, verifier, reporter
- [ ] 한국어 final-response instruction
- [ ] 구조화된 action output format
- [ ] project ontology root layout
- [ ] ontology schema: entities, relationships, flows, invariants, ownership, open_questions, source_refs
- [ ] Layer A deterministic repo facts schema
- [ ] Layer A fact generators: files, package/build/test signals, symbols, entrypoints
- [ ] Layer A fact freshness/hash tracking
- [ ] Layer B semantic ontology schema
- [ ] Layer B source/ref/confidence requirements
- [ ] Layer B merge/supersede/open-question lifecycle
- [ ] ontology gap diagnostics in `doctor`
- [ ] ontology drift detection
- [ ] source-backed ontology claim rule
- [ ] repository file discovery
- [ ] context index JSONL: source_path, source_hash, stable_ref, snippet, freshness
- [ ] context query contract
- [ ] source pointer first retrieval rule
- [ ] compact resume bundle
- [ ] context packing budget
- [ ] generated/vendor exclusion rules
- [ ] command/log summarization input format
- [ ] static ontology vs Layer A vs Layer B context fixture
- [ ] prompt fixture test

## Phase 9: 첫 Agent Vertical Slice

목표: 작은 fixture 저장소에서 읽기, 계획, patch 제안, 승인, 적용, 검증, 한국어 보고까지 한 번에 끝낸다.

- [ ] `rpotato run "<task>"`
- [ ] run startup ontology/context retrieval
- [ ] source pointer를 원본 파일 읽기로 승격하는 step
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

## Phase 10: Stop Gate And Reliability Gates

목표: 끝났다는 말이 아니라 evidence가 있어야 완료로 인정한다.

- [ ] Stop gate completion contract
- [ ] command별 required evidence
- [ ] ontology completeness gate
- [ ] context source-read evidence gate
- [ ] source-backed public claim ledger
- [ ] README/model/benchmark claim gate
- [ ] pending action detection
- [ ] 검증 실패 시 continuation
- [ ] validation gap 기록
- [ ] invalid diff rejection
- [ ] Korean output guard
- [ ] mixed-language regeneration
- [ ] fail-closed Korean error
- [ ] cancel cleanup
- [ ] stale workflow terminal 처리
- [ ] destructive command policy tests
- [ ] fixture benchmark suite
- [ ] static-only vs Layer A vs Layer B ontology benchmark
- [ ] 모델별 product benchmark suite
- [ ] 모델별 공개 benchmark parity suite
- [ ] 공개 benchmark 점수 비교 report: published vs local, 조건 차이, 재현 실패 사유
- [ ] fake session lifecycle test: submit -> tool -> stop
- [ ] corrupt state fallback test
- [ ] resume/cancel E2E test
- [ ] regression test report format

## Phase 11: Subagents, Team Runtime, And TUI Surface

목표: Claude Code/Codex를 대신할 replacement-level runtime에 필요한 interactive surface와 bounded multi-agent 실행을 구현한다.

- [ ] subagent role schema
- [ ] subagent task slice contract
- [ ] subagent allowed tools/path/context boundary
- [ ] subagent lifecycle: start, complete, blocked, failed, cancelled
- [ ] parent cancellation propagation
- [ ] shared file conflict detection
- [ ] subagent evidence merge
- [ ] team manifest schema
- [ ] team pipeline: plan, dispatch, exec, review, verify, merge, report
- [ ] team write policy: single writer per file
- [ ] team merge policy: runtime-owned merge
- [ ] team stage ledger
- [ ] failed worker continuation policy
- [ ] `rpotato team status`
- [ ] TUI framework decision
- [ ] `rpotato tui`
- [ ] TUI monitoring UX spec from `DESIGN.md`
- [ ] TUI transcript/session view
- [ ] TUI plan/context/ontology view
- [ ] TUI approval queue
- [ ] TUI diff viewer
- [ ] TUI tool output viewer
- [ ] TUI subagent/team status view
- [ ] TUI model/token monitoring view
- [ ] TUI evidence/stop gate view
- [ ] TUI plugin permission review view
- [ ] optional local HTML report/dashboard decision
- [ ] TUI cancel/resume controls
- [ ] TUI small terminal smoke test
- [ ] TUI policy-bypass regression test

## Phase 12: Packaging And Release

목표: 사용자가 모델 가중치 없이 runtime surface를 설치하고, 첫 모델 설치를 안전하게 진행할 수 있게 한다.

- [ ] release build pipeline
- [ ] binary checksum 생성
- [ ] macOS Apple Silicon artifact
- [ ] macOS Intel artifact
- [ ] Windows x86_64 artifact
- [ ] release notes template
- [ ] `rpotato doctor` release smoke test
- [ ] uninstall keep-cache/purge-cache smoke test
- [ ] plugin local-import rejection smoke test: remote URL, marketplace, registry, catalog
- [ ] GitHub Releases distribution
- [ ] post-MVP Homebrew/Scoop decision

## MVP 전 비범위

- GUI 앱
- runtime 소유권 없는 unbounded parallel agents
- 여러 모델 동시 로딩
- remote GPU/server mode 기본 지원
- MCP server ecosystem
- 모든 MCP transport 지원
- remote bridge
- IDE/Desktop/Web surface
- 외부 runtime plugin 직접 실행
- 외부 plugin marketplace 연동
- 외부 plugin registry/catalog 연동
- license 미확인 plugin package mirror
- 공식 승인 없는 Claude Code/Codex 호환 claim
- destructive command 자동 실행
- 외부 코드 PR workflow
- 출처 없는 모델 추천
- exhaustive framework ontology parser
- 출처 없는 semantic ontology claim
- 원본 파일 확인 없는 snippet 기반 자동 수정
