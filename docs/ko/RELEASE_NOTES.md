# 릴리즈 노트

## v0.43.1 - 릴리스 게이트 복구

예정 릴리즈 날짜: 2026-07-21

이 patch는 v0.43.0의 안내형 TUI와 context compaction 동작을 그대로 유지하면서
완료되지 못한 binary publication을 복구합니다.

### 포함한 것

- Packaged binary smoke가 현재 안내형 interactive header를 검사하도록 맞춥니다.
- Windows ConPTY echo 복원 probe가 exact echo mode 검증을 약화하지 않으면서
  terminal control sequence와 prompt prefix를 처리합니다.
- 새 tag 전에 최종 PR candidate에서 packaged release-binary smoke와 Windows
  native terminal lifecycle을 실행합니다.

### 호환성 경계

- Runtime 기능 동작, persisted state, model policy, public command semantic은
  v0.43.0과 같습니다.
- v0.43.0 source tag는 immutable하게 유지하지만 5-platform asset set은 게시되지
  않았습니다. v0.43.1이 지원 replacement가 됩니다.

## v0.43.0 - 안내형 로컬 대화와 컨텍스트 압축

릴리즈 날짜: 2026-07-21

이 릴리즈는 안내형 로컬 대화를 기본 제품 surface로 만들고, 작은 로컬
모델에 맞춘 bounded context compaction을 추가합니다.

### 포함한 것

- 인자 없는 `rpotato`가 기본 TUI를 시작하고 일반 텍스트를 코딩 요청으로
  처리합니다. 세부 호환·진단 명령은 `rpotato debug --help` 아래에 유지합니다.
- 최초 model 선택, 검증된 artifact download, managed `llama.cpp` 설치 또는
  재사용, backend start를 GGUF 경로 입력 없이 TUI 안에서 처리합니다.
- Composer 아래에 model, 측정된 context 사용량, compaction 상태, backend 상태,
  session을 구조화되고 color-aware한 terminal 출력으로 표시합니다.
- 측정된 context 사용량 75%에서 자동 compaction을 실행하고 `/compact`를
  제공합니다. Active model context limit의 40%를 목표로 최근 transcript record
  최대 4개를 보존합니다.
- 자동 trigger metric은 active session으로 제한하고, 40% 상한은 checkpoint,
  resume turn, resume source snippet을 합친 실제 resume prompt 전체에 적용합니다.
- Typed incremental checkpoint를 정확한 project, session, transcript boundary에
  binding된 immutable hash-chain artifact로 저장합니다.
- Resume 시 bounded full hash chain과 monotonic boundary를 검증하고 session writer
  lease와 current-pointer CAS로 동시 compaction fork를 막습니다.
- Deterministic pruning과 typed extraction을 먼저 수행한 뒤 local model rationale
  시도는 한 번으로 제한하며, 실패해도 deterministic fallback으로 계속합니다.
- 긴 TUI 응답은 `/more`, `/back`으로 모두 다시 볼 수 있으며 model start 실패 시
  이전 기본 모델과 ready backend를 복구합니다.

### 호환성 경계

- Canonical transcript, ledger, instruction, source artifact가 계속 정본입니다.
  Compacted checkpoint field는 명시적으로 신뢰하지 않는 과거 데이터입니다.
- Compaction은 원본 transcript를 다시 쓰거나 삭제하지 않습니다. 파생 artifact가
  유효하지 않으면 기존 bounded recent-turn resume 경로로 fallback합니다.
- 공식 binary 배포 channel은 이 저장소의 GitHub Releases 하나뿐입니다. 이
  source tag는 게시됐지만 binary workflow가 complete asset set 업로드 전에
  실패했으며, v0.43.1이 이 publication을 대체합니다.

## v0.42.0 - User-local Install and Clean Uninstall

릴리즈 날짜: 2026-07-19

이 릴리즈는 GitHub Release archive에서 실행한 binary를 사용자 전용 CLI로
설치하고, 필요할 때 rpotato가 관리하는 설치와 상태를 안전하게 전부 지우는
대칭적인 lifecycle을 추가합니다.

### 포함한 것

- `rpotato install`의 사용자 전용 binary 배치와 zsh/bash/fish 또는 Windows
  사용자 PATH 멱등 등록
- 설치된 binary로 실행한 `rpotato init`의 PATH 환경 보정과 현재 terminal용
  활성화 안내
- 전역 application data와 현재 project `.rpotato`를 초기화한 뒤 다시
  설치하는 `install --clean --dry-run|--yes`
- 설치 binary, rpotato 소유 PATH 등록, 전역 application data, 현재 project
  `.rpotato`를 제거하는 `uninstall --clean --dry-run|--yes`
- 관리형 backend/generation이 실행 중이거나 생존 확인을 할 수 없을 때
  destructive clean operation을 차단하는 cross-process runtime guard
- Windows에서 실행 중인 설치 binary를 process 종료 뒤 제거하는 post-exit
  cleanup

### 호환성 경계

- 압축을 푼 실행 원본, source repository, 사용자 소유 backend override는
  자동 삭제하지 않습니다.
- 기존 `uninstall --keep-cache|--purge-cache`는 호환성 plan mode로 유지합니다.
- 공식 binary 배포 channel은 이 저장소의 GitHub Releases 하나뿐이며
  package-manager 저장소는 추가하지 않습니다.

## v0.41.0 - Local HTML Monitor Report

릴리즈 날짜: 2026-07-19

이 릴리즈는 기존 local monitoring export에 선택형 static HTML view를
추가합니다. Server, external telemetry, 별도 monitoring source of truth는
추가하지 않습니다.

### 포함한 것

- `rpotato monitor export --format html`이 완전한 self-contained HTML document
  하나를 standard output에 기록합니다. Local file 생성은 사용자의 명시적 shell
  redirection으로 남습니다.
- Store summary, resource pressure, model metric, optimization policy를 기존
  bounded SQLite/ledger monitor query로 읽습니다. Query 하나가 실패하면 전체
  report를 숨기지 않고 해당 section만 unavailable로 표시합니다.
- JavaScript, external asset, network request, local HTTP server 없이 semantic
  responsive layout, light/dark color, restrictive content security policy를
  제공합니다.
- 모든 dynamic text를 render하기 전에 기존 credential redaction boundary를
  적용하고 absolute local path token을 제거한 뒤 HTML escape합니다.
- CLI parser, runtime port, partial availability, redaction, architecture
  contract 회귀 테스트와 한/영 사용법·observability 문서를 추가했습니다.

### 호환성 경계

- 기존 JSONL/CSV monitor export, persisted schema, dependency, runtime policy,
  exit semantic은 변경하지 않습니다.
- Report는 특정 시점의 local snapshot입니다. 자동 refresh, browser 열기,
  shell redirection 없는 file 생성, raw prompt/source text 노출을 수행하지
  않습니다.
- 5개 platform, 정확한 11-file GitHub Release asset contract는 변경하지
  않습니다. GitHub Releases만 지원하는 배포 channel로 남기고 package-manager
  실험과 관련 자동화는 중단했습니다.

## v0.40.0 - Package Manager 배포

릴리즈 날짜: 2026-07-18

Tag 시점 package-manager channel 상태: `Unpublished`. 검증된 v0.40.0 GitHub
Release asset을 기준으로 게시합니다.

이 릴리즈는 기존 binary와 정확한 11-asset GitHub Release contract를 바꾸지
않고 deterministic package-manager 배포 계약을 추가합니다.

### 포함한 것

- Strict stable tag, canonical package metadata, 검증된 aggregate GitHub Release
  checksum file에서 정확한 Homebrew, Scoop, winget manifest tree 생성
- Stale version, 예상하지 않은 path/asset, 잘못되거나 중복된 hash, 해결되지 않은
  template marker, non-canonical URL을 별도 verifier로 거부
- Current clean install, previous-to-current upgrade, `rpotato doctor` version
  증명, package-manager uninstall, command 부재를 검증하는 Homebrew native 4개
  lane과 격리된 Scoop, winget Windows lane 추가
- v0.38.0에서 v0.39.0으로 올리는 고정 pre-tag qualification mode와 strict
  ancestral predecessor를 구하는 same-tag recovery mode 추가
- Package-manager binary 제거와 non-destructive
  `rpotato uninstall --dry-run` application-data plan을 분리

### 과거 Evidence 상태

- Manifest는 `Generated` 상태이고 저장소 fixture/workflow contract check가 통과
- Tag 전 v0.38.0→v0.39.0 qualification과 release mode v0.39.0→v0.40.0
  lifecycle이 Homebrew 4개 lane과 Scoop, winget에서 모두 통과해
  lifecycle contract가 `Validated`에 도달
- 실험용 Homebrew tap은 commit
  `bf50499674dcbf46ce7e36260a8a6b3cf0c6b49e`에서 `Published`에 도달
- 실험용 Scoop bucket은 commit
  `2e881e23456ae818d00ae63a1059bd870fc914de`에서 `Published`에 도달
- winget community manifest: 생성과 lifecycle 검증은 통과했지만 진행 중인
  upstream 제출이 없는 `Unpublished` 상태
- Public URL 또는 upstream review 상태를 기록하기 전에는 어떤 외부 channel도
  완료로 보지 않음

### 호환성 경계

- Runtime 동작, persisted schema, Cargo dependency, archive 내용, 정확한 11-file
  release asset set은 변경하지 않음
- 게시된 channel file은 검증된 v0.40.0 asset set에서 생성한 workflow artifact와
  byte 단위로 일치하며, winget 제출의 최종 상태는 외부 upstream이 소유
- 이 실험은 2026-07-19에 중단했습니다. Package-manager 저장소와 manifest는
  더 이상 지원하는 배포 channel이 아니며 GitHub Releases만 사용합니다.

## v0.39.0 - 통합 Workflow 성능 최적화

릴리즈 날짜: 2026-07-18

이 릴리즈는 완료된 agent, subagent, team workflow에 결정적 성능 contract를
추가하고, 측정으로 확인한 불필요한 runtime 작업 두 곳을 제거합니다. 근거는
저장소 fake sidecar와 local runtime projection에서 얻었으며 실제 모델 품질이나
공개 benchmark 성능 claim이 아닙니다.

### 포함한 것

- 완료된 agent, subagent, 2-member team CLI 경로를 실행하고 request 수와 byte,
  projection token 합계, 영속 runtime byte, 필수 completion marker를 gate하는
  `workflow-performance-v1` 추가
- 각 fixture의 wall time, process peak CPU, peak RSS를 측정하되 hardware-dependent
  값은 report-only로 유지
- Locked test, clippy, release build 뒤 exact PR candidate에서 release-mode
  evaluator 실행
- Performance marker는 request 수와 body byte 크기만 기록하고, declared-context
  worker의 source-context byte가 project나 app-state artifact에 영속되지 않음을
  검증. 일반 visible user transcript 영속화는 변경하지 않음

### 측정한 최적화

- Canonical ledger append가 반환한 ordinal을 production SQLite projection call
  site 12곳에서 재사용합니다. 결정적 regression fixture에서 projected append당
  canonical full-ledger read를 1회에서 0회로 줄였습니다.
- Fixed binding, evidence-only result contract, patch restriction, safety marker를
  보존하면서 bounded subagent worker context를 압축했습니다.
- 같은 fake-sidecar harness에서 aggregate request payload는 subagent fixture
  3,813 byte에서 3,730 byte, 2-member team fixture 5,231 byte에서 5,065 byte로
  감소했습니다.
- Deterministic fake response는 각각 projection token 20개로 고정되어 있습니다.
  따라서 request byte 감소는 context-envelope 근거이며 실제 모델 token 사용량이나
  출력 품질 향상 claim이 아닙니다.

### 호환성 경계

- Public CLI command, flag, exit-code contract, dependency, synchronous execution,
  default-deny action boundary는 변경하지 않음
- Ledger order는 계속 canonical이며 SQLite는 재생성 가능한 derived projection으로
  유지하고, recovery가 필요하면 canonical ledger를 다시 읽음
- 성능 fixture는 local에서 재현 가능하지만 wall, CPU, RSS 관측값은 cross-machine
  threshold나 public benchmark 결과가 아님

## v0.38.0 - Claude Code Plugin 실행 Adapter

릴리즈 날짜: 2026-07-18

이 릴리즈는 안전한 local Claude Code plugin instruction을 기존 runtime-owned
plugin capability contract에 mapping합니다. 외부 plugin content는 계속 신뢰하지
않으며 read-only action, tool, evidence, approval boundary를 넓힐 수 없습니다.

### 포함한 것

- Local import한 Claude Code plugin의 canonical
  `skills/<name>/SKILL.md`와 `commands/<name>.md` 탐색
- Enable된 instruction을
  `imported.claude-code.<plugin>.<skill-or-command>` namespace로 노출하고
  imported Codex skill과 같은 native read-only workflow, ledger, hook,
  evidence, stop gate로 실행
- Admission과 completion 시 normalized source runtime, adapter policy, 전체
  imported snapshot, capability metadata, instruction file size, frontmatter,
  source path, source SHA-256 재검증
- Claude Code의 기본 skill 우선/command 차선 규칙은 유지하되 source manifest가
  아직 지원하지 않는 custom `commands` path를 선언하면 기본 `commands/`를
  active하다고 잘못 처리하지 않음
- Claude Code manifest, frontmatter, substitution, agent, hook, MCP, LSP,
  monitor, `bin/`, settings, theme, output style, root skill, custom layout의
  미지원 semantic을 조용히 mapping하지 않고 명시적으로 보고

### Default-Deny 경계

- Claude Code dynamic shell interpolation과 skill script는
  `blocked-by-default`
- `allowed-tools`, fork/agent/model 선택, hook, path activation, argument
  substitution 등 source-runtime semantic은 `rpotato` 권한을 부여하지 않음
- Plugin import와 enable은 계속 shell, MCP, background, remote,
  sensitive-config, runtime-setting, file-write 실행 승인이 아님
- 이전 adapter version으로 import한 plugin은 신뢰하는 local directory에서
  다시 import해야 함

### Targeted 검증

- Plugin adapter unit test가 안전한 Claude skill/command, custom-command
  replacement, dynamic shell 거부, unsupported 명시 보고, source drift,
  normalized metadata 변조를 검증
- CLI lifecycle test가 Claude command를 read-only runtime으로 실행하고 기존
  Codex 실행/completion recovery contract를 보존
- Extension ownership contract가 Claude mapping, generic execution, scanner,
  import normalization, domain parsing을 bounded owner에 유지

## v0.37.14 - Windows 릴리즈 복구

릴리즈 날짜: 2026-07-18

이 patch는 완결되지 못한 v0.37.13 binary publication을 대체합니다.
v0.37.13 source tag는 immutable하게 유지하지만, architecture ownership
migration 이후 Unix 전용 source-install 함수 두 개를 Windows test binary가
import하는 문제가 Windows release preflight에서 드러났습니다.

### 수정한 것

- Source-recovery test import를 `test`와 `unix` 조건에 함께 제한
- Unix 전용 initial source-install admission re-export를 `unix`로 제한하면서
  Windows의 기존 unsupported-platform 동작은 유지
- 수동 실행 Windows targeted workflow가 release job과 같은 backend lifecycle
  4개 slice를 compile하고 실행하도록 확장
- Targeted workflow가 release compile surface를 다시 누락하지 못하도록 해당
  Windows preflight command를 release workflow contract에 고정

### Targeted 검증

- [실패한 v0.37.13 release run 29603744149](https://github.com/MCprotein/rolling-potato/actions/runs/29603744149)는
  Windows `E0432` import 실패를 기록하고 release branch를 보존했으며 aggregate
  checksum과 branch cleanup job을 실행하지 않았습니다.
- [Windows targeted run 29604380487](https://github.com/MCprotein/rolling-potato/actions/runs/29604380487)는
  Windows release preflight 4개 slice와 native terminal lifecycle을 통과했고,
  macOS native job 두 개도 함께 통과했습니다.

### 호환성 경계

- CLI 동작, durable byte, runtime ordering, dependency, 완료된 v0.37.x ownership
  migration은 변경하지 않음
- v0.37.13은 immutable source history로 유지하지만 8개 non-Windows asset은
  지원 release set이 아닙니다. v0.37.14를 지원 replacement로 삼고 검증된 exact
  11-asset set을 게시합니다.

## v0.37.13 - 전체 Architecture Ownership Migration

릴리즈 날짜: 2026-07-18

이 patch는 동작을 보존하는 v0.37.x 소유권 마이그레이션을 완료합니다. 남은
최상위 compatibility facade를 제거하고 CLI/TUI 및 application integration에
명시적인 private owner를 부여하며, binary entrypoint는 사용자 노출 동작과 durable
contract를 바꾸지 않는 얇은 delegate로 남깁니다.

아래 v0.37.1-v0.37.12 row는 별도 발행하지 않은 구현 milestone입니다. 이들의
정확한 최종 tree를 하나의 v0.37.13 artifact로 통합하며, 해당 milestone의 별도
binary나 tag가 있었다고 주장하지 않습니다.

### 포함한 것

- CLI command family, dispatch, presentation과 TUI controller/view 소유권을
  `surfaces`, `composition`, focused application adapter로 분리
- Concrete runtime, workflow, patch, inference, knowledge, policy, extension,
  collaboration, observability, TUI integration을 `app` 아래로 이동
- Composition을 binary-private으로 유지: `main.rs`가 ownership root를 private으로
  등록하고 library API 없이 startup composition에 직접 위임
- `src` 최상위 production compatibility facade를 모두 제거해 `main.rs`만 유지
- Migration ledger의 모든 책임을 complete로 전환하고 planned/exception/
  compatibility-facade 0건 completion gate를 활성화

### 검증 계약

- 범위를 제한한 독립 review 한 번에서 stale durable-proof selector, 의도하지
  않은 public library target, 누락된 application dependency gate 등 High 3건을
  발견했고 두 번째 review 없이 targeted 회귀 테스트로 모두 닫았습니다.
- 최종 feature candidate는 serialized locked test suite, warning을 거부하는
  clippy, release build, release-policy check, architecture contract,
  exact-head candidate assertion을 통과했습니다.
- Release proof selection은 이제 정확히 한 test만 match해야 하며 durable
  selector 29개는 모두 현재 private module path를 가리킵니다.

### 호환성 경계

- CLI command, argument, 출력, exit code는 변경하지 않음
- Durable workflow, ledger, transcript, evidence, ontology, team, extension byte와
  ordering은 변경하지 않음
- Runtime execution, recovery, approval, default-deny policy, backend 동작,
  dependency, synchronous operation은 변경하지 않음
- Model weight와 외부 plugin package는 번들하지 않음

## v0.37.13 구현 Milestone: v0.37.12 Collaboration 경계

이 patch는 CLI 동작, durable byte, recovery semantic, 실행 순서, evidence merge와
stop gate를 바꾸지 않고 subagent와 team의 side-effect-free 규칙을 private runtime
owner로 이동합니다.

### 포함한 것

- Subagent role/tool admission, bounded launch validation, canonical result shape,
  patch proposal policy를 `runtime_core::collaboration`으로 이동
- Team manifest/state DTO와 codec, stage transition, resource/continuation decision,
  execution binding, action ownership을 collaboration owner로 이동
- Reconciliation binding, stage/ownership gate, unique evidence validation,
  deterministic reconciliation artifact rendering을 reconciliation owner로 이동
- 기존 subagent/team CLI integration 본문을 `tests/collaboration` 아래에 묶고 root
  Cargo test harness는 유지

### 호환성 경계

- Backend call, thread coordination, lease, filesystem install, snapshot recovery,
  ledger/projection event, workflow evidence checkpoint는 v0.37.13 composition
  cleanup 전까지 최상위 collaboration facade가 연결
- CLI 출력과 exit code, canonical manifest/state/result/artifact, worker recovery와
  cancellation, dependency, synchronous execution은 변경하지 않음

## v0.37.13 구현 Milestone: v0.37.11 Extension 경계

이 patch는 CLI 동작, plugin normalized manifest byte, hook ordering, skill state와
default-deny 실행 정책을 바꾸지 않고 extension의 순수 규칙을 private runtime
owner로 이동합니다.

### 포함한 것

- Hook registry, layer ordering, conflict resolution, fail-closed result와 payload
  modification 규칙을 `runtime_core::extensions::hook`으로 이동
- Built-in/imported skill manifest, lifecycle state machine, context/tool/evidence/stop
  policy를 `runtime_core::extensions::skill`로 이동
- Codex `SKILL.md` frontmatter parsing, plugin/component ID validation, normalized
  capability와 default-deny permission 규칙을 `runtime_core::extensions::plugin`으로 이동
- 기존 `hooks`, `skill`, `plugin` module은 ledger/state, workflow persistence,
  filesystem snapshot과 plugin discovery를 새 owner에 연결하는 compatibility facade로 축소

### 호환성 경계

- Plugin source scan/copy/checksum, normalized manifest persistence와 ledger event는
  concrete facade에 남고 v0.37.13 composition cleanup에서 최종 정리합니다.
- CLI 출력과 exit code, persisted manifest/workflow byte, hook/skill/plugin 실행 순서,
  dependency와 synchronous execution은 변경하지 않습니다.

## v0.37.13 구현 Milestone: v0.37.10 Runtime과 Reporting 경계

이 patch는 CLI command, 출력 byte와 field 순서, exit code, synchronous 실행을
바꾸지 않고 workflow runtime 호출 순서와 report/Korean output 규칙을 private
application/reporting owner로 이동합니다.

### 포함한 것

- Agent run, workflow/session resume, patch approve/verify 호출 순서를 explicit
  `RuntimeApplicationPort` 위의 `runtime_core::workflow::application::runner`로 이동
- Workflow/session resume, init, doctor report의 typed input과 canonical field/order
  rendering을 `runtime_core::reporting::runtime_report`로 이동
- Streaming/non-streaming 한국어 output invariant 전체를
  `runtime_core::reporting::korean_guard`로 이동
- 기존 `runtime` module은 concrete state, ledger, context, patch, backend adapter와
  TUI bridge를 조립하고 새 owner에 위임하는 facade로 축소
- 기존 `korean_guard` module은 crate 내부 호출 경로를 보존하는 5줄 re-export
  facade로 축소

### 호환성 경계

- TUI read/intent/outcome bridge는 계획대로 v0.37.13 surface/composition 단계까지
  `runtime` facade에 남습니다.
- CLI 동작과 출력, report field/order, patch guard 조건, recovery preflight 순서,
  dependency, synchronous execution은 변경하지 않습니다.

## v0.37.13 구현 Milestone: v0.37.9 Patch Workflow 경계

이 patch는 CLI 출력, approval secret 전달, source/journal 설치 순서를 바꾸지 않고
intent, proposal, approval credential, apply/rollback, verification/recovery 판단을
private patch owner로 이동합니다.

### 포함한 것

- Deterministic intent 분류, constraint 탐지, side-effect-free action candidate와
  model action fail-closed parsing을 `runtime_core::patch::intent`로 이동
- Proposal DTO, deterministic ID/diff, canonical record bytes, strict header/hex/hash
  parsing을 `runtime_core::patch::proposal`로 이동
- Entropy-derived approval token, credential hash와 constant-time 비교를
  `runtime_core::patch::approval`로 이동
- Stale source/apply admission, rollback hash, already-restored 상태, concurrent edit
  보존 판단을 `runtime_core::patch::application`으로 이동
- Policy-parsed verification plan, bounded result, test-plan 판정과 시작된 command를
  자동 재실행하지 않는 recovery admission을 `runtime_core::patch::verification`으로 이동
- 1,618줄 patch integration suite를 작은 `tests/patch_loop.rs` harness와
  `tests/patch/lifecycle.rs` contract로 분리

### 호환성 경계

- 최상위 intent/patch module은 v0.37.13 composition cleanup까지 backend, skill,
  filesystem, process, ledger, workflow transaction 조립 facade로 남습니다.
- CLI 동작과 exit code, proposal/approval byte와 hash, event/journal 순서, rollback,
  fail-closed 동작, dependency, synchronous execution은 변경하지 않습니다.

## v0.37.13 구현 Milestone: v0.37.8 Knowledge와 Policy 경계

이 patch는 filesystem, ledger, transcript, CLI 동작을 바꾸지 않고 bounded
context, evidence stop gate, typed ontology graph, approval record, tool/path policy의
판단 규칙과 DTO를 private runtime owner로 이동합니다.

### 포함한 것

- Context pack/source pointer/resume DTO, 공유 파일·문자 예산, 중복 제거,
  prompt rendering과 truncation을 `runtime_core::knowledge::context`로 이동
- Evidence DTO, artifact pointer fail-closed 규칙, phase·approval·workflow·proposal·
  action·evidence·command·source hash를 모두 묶는 stop-input validation을
  `runtime_core::knowledge::evidence`로 이동
- Typed ontology record/JSONL codec, latest-current projection, supersession,
  diagnostics, compact source-first context selection, import validation을
  `runtime_core::knowledge::ontology`로 이동
- Approval request DTO, ID 검증, redaction-aware record rendering을
  `runtime_core::policy::approval`로 이동
- Command parsing/classification, patch verification allowlist, path policy DTO와
  `PathPolicyPort`를 `runtime_core::policy::decision`으로 이동
- Migration ledger를 v0.37.8로 진행하고 architecture contract로 runtime owner,
  legacy 금지 정의, concrete adapter 역의존 부재를 고정

### 호환성 경계

- 최상위 context/evidence/ontology/approval/policy module은 v0.37.13 composition
  cleanup까지 filesystem, ledger, transcript, report 조립 facade로 남습니다.
- CLI 출력과 exit code, durable JSONL/approval/evidence byte와 hash, ledger/event
  순서, recovery 및 fail-closed 동작, dependency, synchronous execution은
  변경하지 않습니다.

## v0.37.13 구현 Milestone: v0.37.7 Projection과 Observability

이 patch는 surface-neutral observability record, projection/query port, monitor
report use case, projection-lag recovery admission을 private runtime owner로
이동합니다. 재생성 가능한 SQLite schema/replay, ledger validation, transcript row
설치는 명시적인 adapter module이 소유합니다.

### 포함한 것

- `ObservabilityProjectionPort`를 정의하고 observability record를
  `runtime_core::observability::facade`로 이동
- `MonitorQueryPort`를 정의하고 status, model, baseline, optimization, export,
  prune, 한국어 report rendering을 `runtime_core::observability::monitor`로 이동
- SQLite observability projection과 ledger/transcript projection helper를
  `adapters::sqlite` 아래로 분리
- Projection-lag recovery admission을 workflow application의
  `projection_barrier` owner로 이동
- Migration ledger를 v0.37.7로 진행하고 architecture contract로 새 소유권과
  금지된 legacy path를 고정

### 호환성 경계

- 최상위 observability와 monitor module은 v0.37.13 composition cleanup까지
  private compatibility facade로 남습니다. SQLite projection은 이 단계적 경계를
  통해 canonical ledger/transcript 권위를 계속 읽습니다.
- CLI 동작과 출력, durable schema/byte/hash, canonical event 순서, recovery와
  synchronization 동작, dependency, synchronous execution은 변경하지 않습니다.
  SQLite는 계속 재생성 가능하며 두 번째 source of truth가 아닙니다.

## v0.37.13 구현 Milestone: v0.37.6 Workflow Transaction과 Recovery

이 patch는 command, durable byte, event identity/order, crash 동작, synchronous
runtime을 바꾸지 않고 workflow transition, cross-store transaction 순서, recovery
policy를 private workflow domain/application owner로 이동합니다.

### 포함한 것

- Legal transition intent, prepared member, source install, event chain, bundle
  record를 `runtime_core::workflow::domain::transition`으로 이동
- Consumer-owned port 위에서 exact event 진행과 state, checkpoint, reconcile,
  approval, verification, terminal, projection, cleanup 순서를 선택하는 단일
  application transaction coordinator 추가
- Prepared workflow suffix recovery, current-state recovery 순서, 불확실한
  projection-lag admission을 `runtime_core::workflow::application::recovery`로 이동
- 기존 patch-loop harness를 유지하면서 restart와 preflight mutation barrier
  integration contract를 `tests/workflow/recovery.rs`로 분리
- Migration ledger를 v0.37.6으로 진행하고 예정된 transition, transaction,
  recovery, test slice를 모두 complete로 전환

### 호환성 경계

- 기존 state, ledger, transition module은 concrete filesystem, lease, event sink,
  journal, projection 연산을 유지하지만 이전된 commit/recovery 순서를 선택하지 않습니다.
- CLI 동작과 exit code, canonical schema/byte/hash, event/state 순서, recovery와
  fail-closed 동작, dependency, synchronous execution은 변경하지 않습니다.

## v0.37.13 구현 Milestone: v0.37.5 Validated Domain View

이 patch는 command, durable byte, ledger 순서, filesystem/lock/recovery 동작을
유지하면서 workflow/session/snapshot과 transcript-session validation을 private
workflow domain owner로 이동합니다.

### 포함한 것

- Current-state, lease, TUI snapshot, active-workflow binding DTO를
  `runtime_core::workflow::domain::snapshot`으로 이동
- Session resume 권위, current-state identity/lease, bounded ledger ancestor와
  workflow checkpoint validation을 해당 owner로 이동
- Canonical transcript-session filtering, ledger ordering, duplicate 거부,
  event/record/tool binding, `ToolOutputView`를 workflow transcript domain으로 이동
- Migration ledger를 v0.37.5로 진행하고 예정된 domain-view slice를 complete로 전환

### 호환성 경계

- 최상위 state와 transcript module은 이후 slice의 filesystem, lock, transaction,
  recovery, projection, command orchestration facade로 남습니다.
- CLI 동작, durable schema/byte, canonical event identity/order, recovery/projection
  순서, dependency, synchronous runtime은 변경하지 않습니다.

## v0.37.13 구현 Milestone: v0.37.4 Workflow Storage Compatibility

이 patch는 기존 command, storage schema, append 순서, recovery 동작, synchronous
runtime을 유지하면서 durable workflow, ledger, transcript byte의 canonical
compatibility owner를 하나로 확립합니다.

### 포함한 것

- Byte-compatible workflow record DTO, v2/v3/v4 snapshot codec, commit pointer
  codec, schema probe, hash validation을 `runtime_core::workflow::storage_compat`로 이동
- Ledger event DTO, strict parser, chain payload, physical/planned hash, sync를
  포함한 canonical line append를 같은 compatibility 경계로 이동하고 cross-store
  coordination 순서는 유지
- Transcript DTO, v1/v2 render/parser, source/tool binding validation, canonical
  hash, exact immutable record install을 compatibility 경계로 이동
- Snapshot/pointer byte, ledger append order/hash chain/failure 경계, transcript
  exact/idempotent/immutable install을 고정하는 focused workflow integration target 추가
- Migration ledger를 v0.37.4로 진행하고 예정된 storage compatibility slice를 모두 complete로 전환

### 호환성 경계

- 기존 최상위 state, ledger, transcript module은 이후 v0.37.x slice에 배정된 path,
  lock, transaction, recovery, projection, command orchestration facade만 유지합니다.
- CLI output과 exit code, durable schema와 byte, event identity와 순서, recovery와
  projection 순서, security policy, dependency, synchronous runtime은 변경하지 않습니다.

## v0.37.13 구현 Milestone: v0.37.3 Inference 경계

이 patch는 synchronous runtime과 기존 command 동작을 유지하면서 inference rule과
durable record codec을 llama.cpp, process, filesystem 구현에서 분리합니다.

### 포함한 것

- Backend contract, generation admission, lifecycle DTO/codec, resource policy,
  stream outcome을 `runtime_core::inference`로 이동
- Llama.cpp discovery, release/install, request, health/version, bounded SSE transport를
  OS process lifecycle과 command identity check에서 분리
- Model manifest, registry/default/promotion codec, adoption policy, artifact path
  contract와 benchmark fixture/scoring/adoption rule을 owning domain과 adapter로 이동
- Legacy backend stream/resource module과 obsolete buffered parser test를 제거하고
  migration ledger의 v0.37.3 slice 21개를 complete로 전환

### 호환성 경계

- `backend.rs`, `benchmark.rs`, `model.rs`는 private command/report와 cross-store
  orchestration facade로 남고 최종 composition 이동은 v0.37.13에 배정합니다.
- CLI output, exit code, durable record byte, event order, install/promotion gate,
  timeout/cancellation, manifest, dependency, synchronous runtime은 변경하지 않습니다.

## v0.37.13 구현 Milestone: v0.37.2 Foundation과 Platform 경계

이 patch는 동작과 synchronous runtime을 유지하면서 운영 소유권 이동을 시작합니다.
완료된 v0.37.2 legacy module path는 compatibility facade로 남기지 않고 제거합니다.

### 포함한 것

- Application error 경계, SHA-256 primitive, 기존 strict/canonical JSON 구현을 private
  `foundation` 소유자로 이동
- Path layout, cache report, configuration source discovery, recoverable filesystem lease,
  Windows file identity replacement 지원을 `adapters::filesystem`로 이동
- Terminal attachment detection과 native terminal I/O를 `adapters::terminal` 아래에서
  분리하고 terminal-backed integration contract를 `platform` test target으로 통합
- Release proof와 native build workflow가 안정된 새 owner path와 test target을 사용하도록 갱신
- Migration ledger를 v0.37.2로 진행하고 예정된 slice 14개를 모두 complete로 전환하며,
  현재 patch에 예정된 미완료 slice를 거부하는 gate 추가

### 호환성 경계

- CLI command, output, exit code, durable byte, recovery order, permission 동작, backend
  동작, direct dependency는 변경하지 않습니다.
- Runtime은 synchronous로 유지하며 async runtime, actor framework, public API,
  persisted schema, 새 dependency를 추가하지 않습니다.

## v0.37.13 구현 Milestone: v0.37.1 아키텍처 기반

이 patch는 v0.38.0 전에 완료할 동작 보존 코드 아키텍처 리팩터링을 시작합니다.
운영 로직을 옮기지 않고 소유권과 강제 계약을 먼저 정의합니다.

### 포함한 것

- 전체 Rust, test, workflow, release script, 관리 대상 문서 범위를 다루는 영문/한국어
  코드 아키텍처 정본과 machine-readable file/responsibility migration ledger
- 문서만 포함하고 private으로 compile graph에 연결된 `composition`, `surfaces`,
  `runtime_core`, `adapters`, `foundation` root
- 재귀 원장 coverage, exact target, state/release, private skeleton, dependency edge,
  bilingual link, direct dependency 불변을 검사하는 새 dependency 없는 architecture contract test
- 정확한 PR head SHA를 checkout하고 기록한 뒤 정본 full candidate gate를 한 번 실행하는
  read-only `release-candidate` PR workflow
- Release workflow가 Rust module-qualified test path를 직접 소유하지 않게 하는 stable
  durable-runtime proof entrypoint

### 경계

- v0.37.1에는 운영 로직, 공개 API, persisted schema, canonical byte, recovery order,
  CLI 동작, permission policy, backend 동작, dependency 변경이 없습니다.
- 이후 v0.37.x patch는 하나의 응집된 ownership/rollback boundary씩 이동합니다.
  Migration ledger가 모두 complete이고 compatibility facade가 0개가 되기 전에는
  v0.38.0을 시작하지 않습니다.

## v0.37.0 - 안전한 Codex Skill 실행

릴리즈 날짜: 2026-07-16

이 릴리즈는 native runtime을 유일한 policy, hook, evidence, stop-gate 권위로
유지하면서 첫 번째 실행 가능한 외부 plugin adapter를 추가합니다. Enable된 canonical
Codex skill은 기존 read-only agent loop에 bounded instruction을 제공할 수 있지만 plugin
code와 외부 capability는 계속 비활성 상태로 유지합니다.

### 포함한 것

- Enable된 canonical Codex `skills/<name>/SKILL.md` capability를
  `imported.codex.<plugin>.<skill>` namespace로 resolve하고 `rpotato skill list`와
  `rpotato skill run`에서 사용합니다.
- Admission 전과 completion 시점에 copied source snapshot, source manifest, normalized
  capability metadata, slugged plugin identity, frontmatter, instruction size, 정확한
  `SKILL.md` hash를 다시 검증합니다.
- Imported instruction을 bounded repository context, native read-only lifecycle hook, typed
  non-mutating action, 한국어 출력 guard, evidence requirement, stop criterion에 연결합니다.
  Model output은 여전히 capability를 직접 실행할 수 없습니다.
- Source에 binding된 plugin admission/completion event를 기록하고 completion event 영속화
  전 또는 active pointer 정리 전 crash window를 event 중복이나 model request replay 없이
  복구합니다.
- Lifecycle hook 전체에서 admission된 manifest를 재사용해 일반 실행의 전체 plugin
  snapshot hash는 admission과 completion 시점에만 계산합니다.

### 검증 계약

- 범위를 제한한 독립 review 한 번을 완료했습니다. High 2건과 Medium 2건을 두 번째
  review 없이 targeted 회귀 테스트로 닫았습니다.
- Regression coverage는 normalized capability 변조, script default-deny, slugged discovery
  ID, native hook/skill/intent gate, 정상 imported-skill 실행, 두 completion recovery
  window를 포함합니다.
- 최종 feature candidate는 unit test 522개와 integration test 41개, warning을 거부하는
  binary clippy, release build, formatting, release policy check를 통과했습니다.
- Release workflow는 serialized Rust release gate, 5개 native build, packaged-binary smoke,
  asset별 checksum, aggregate checksum 검증을 수행합니다.

### 경계

- Plugin import와 enable은 shell, script, hook, MCP, app, background, remote connector,
  runtime setting, sensitive configuration, file-write 권한을 부여하지 않습니다. v0.37에는
  이 capability를 위한 approval grant 또는 execution surface가 없습니다.
- Claude Code plugin은 v0.38 conformance adapter가 구현될 때까지 inspect할 수 있지만
  실행할 수 없습니다. Marketplace, registry, catalog, mirror, remote URL source도 계속
  지원하지 않습니다.
- 이 릴리즈는 managed `llama.cpp b9982` backend와 5개 platform, exact 11-asset release
  set을 유지합니다. Model weight와 외부 plugin package는 번들하지 않습니다.

## v0.36.0 - Durable Team 실행

릴리즈 날짜: 2026-07-16

이 릴리즈는 영속 team plan을 runtime이 소유하는 lane 실행과 evidence reconciliation로
확장합니다. Runtime은 manifest의 정확한 member를 admit하고 resource pressure에 따라
parallel 또는 sequential fallback으로 실행하며, deterministic receipt와 stop gate가
통과한 뒤에만 parent를 완료합니다.

### 포함한 것

- 정확한 lane/member admission, parallel 또는 sequential 실행, 영속 stage 진행,
  failed-lane 수집, idempotent completion을 위한 `rpotato team execute`와
  `rpotato team reconcile`을 추가합니다.
- Manifest write ownership을 action 시점에 다시 강제합니다. Worker patch proposal은
  실행되지 않는 artifact로 유지하며, reconciliation은 worker가 작성한 변경을 적용하지
  않고 검증된 evidence만 parent에 merge합니다.
- 하나의 영속 cancellation marker를 모든 admitted worker에 전달하고 team operation
  barrier로 cancel이 admission 경계에 끼어 orphan child를 남기지 못하게 합니다.
- 중단된 `team-dispatch` worker는 기존 identity로 재개하고, 모든 worker가 완료된 뒤
  receipt가 누락되면 이를 복원합니다. 실행 중 중단된 요청은 다시 보내지 않고
  `interrupted-no-replay`로 종료합니다.
- Worker evidence를 source path, stable reference, fingerprint에 binding합니다. Validation
  gap, 누락된 receipt, 변조된 artifact, source drift가 있으면 parent evidence checkpoint
  전에 reconciliation을 중단합니다.
- 같은 process의 병렬 backend generation 전체에 하나의 cancellation marker를 적용하고
  마지막 member가 terminal state에 도달할 때까지 영속 active-generation record를
  유지합니다.
- Rust fake sidecar를 사용해 initialization, backend start, parent run, team plan, 실행,
  reconciliation, retry, status reporting을 잇는 실제 CLI integration coverage를
  추가합니다.

### 검증 계약

- v0.36 구현에 대해 범위를 제한한 독립 리뷰 한 번을 완료했고, 발견된 release-blocker
  네 건은 두 번째 리뷰 없이 targeted 회귀 테스트로 닫았습니다.
- Targeted unit coverage는 parallel/sequential mode, critical resource pressure,
  action-time ownership, failed worker, 영속 cancellation, 중단 실행 복구, source
  freshness, validation gap, 병렬 generation-group cancellation을 포함합니다.
- Release workflow는 serialized full Rust test gate, pinned-tool/release-contract check,
  5개 native release build, packaged-binary smoke, asset별 checksum, aggregate checksum
  검증을 실행합니다.

### 경계

- Team worker는 bounded finding, evidence, 실행되지 않는 patch proposal을 반환할 수
  있습니다. Command 실행, 직접 write, patch apply, nested team, nested subagent 권한은
  받지 않습니다.
- Imported Codex/Claude Code plugin은 inspection-only로 유지하며 실행 adapter는
  v0.37.0부터 시작합니다.
- 이 릴리즈는 managed `llama.cpp b9982` backend와 5개 platform, exact 11-asset release
  set을 유지합니다. Model weight와 외부 plugin package는 번들하지 않습니다.

## v0.35.1 - Hermetic Release Contract 복구

릴리즈 날짜: 2026-07-16

이 patch release는 asset이 없는 v0.35.0 publication을 대체합니다. v0.35.0 source tag는
immutable하게 유지하지만, release test job에서 실제 release tag 환경이 일반 pull request
policy fixture로 누출되는 문제가 드러났습니다.

### 수정한 것

- 일반 PR release-policy fixture 안에서 ambient `RPOTATO_RELEASE_TAG`, `GITHUB_REF_TYPE`,
  `GITHUB_REF_NAME`을 지워 synthetic Cargo version을 contract test를 호출한 release job과
  독립적으로 평가합니다.
- Publish 전에 명시적인 simulated tag 환경으로 release workflow contract를 실행해 platform
  build나 asset 생성 없이 v0.35.0의 정확한 실패 조건을 재현·검증합니다.
- Runtime behavior를 바꾸지 않고 v0.35 bounded subagent 구현 전체와 S01-S18 acceptance
  coverage를 유지합니다.

### 릴리즈 복구

- v0.35.0은 immutable source history로 유지하지만
  [release run 29484349685](https://github.com/MCprotein/rolling-potato/actions/runs/29484349685)는
  binary build 전에 중단되어 지원 asset을 게시하지 못했습니다.
- v0.35.1을 지원 replacement로 삼고 검증된 exact 11-asset set을 게시합니다.

### 복구 중 검증

- Simulated `v0.35.1` tag 환경에서 release workflow contract
- Release policy, toolchain pin, target matrix check
- Runtime code가 바뀌지 않았으므로 v0.35.0 final candidate의 full test, clippy, release build,
  packaged-binary, uninstall smoke evidence를 그대로 적용

## v0.35.0 - Bounded Subagent 실행

릴리즈 날짜: 2026-07-16

이 릴리즈는 활성 parent workflow 아래에 runtime이 소유하는 bounded child worker 하나를
추가합니다. Runtime은 backend dispatch 전에 child의 context, tool, path, resource, result,
lifecycle 경계를 고정하고 검증한 evidence만 parent에 merge합니다.

### 포함한 것

- Strict role/tool policy, 정규화한 project-relative read/write ownership, 정확한 task/result
  상한, bounded timeout/token budget, parent당 non-terminal child 하나를 적용하는
  `rpotato subagent launch`, `status`, `cancel`을 추가합니다.
- Dispatch와 completion 직전에 선언한 source pointer를 다시 검증하고 기존 resource governor와
  bounded backend cancellation 경로로 generation을 실행합니다. Child에는 command 실행이나
  patch 직접 적용 권한을 주지 않습니다.
- Canonical `SubagentResultV1` 하나만 strict parse하고, executor가 source hash와 ownership이
  일치하는 non-executing patch proposal을 반환할 때만 허용합니다. Credential 형태의 model
  text는 result/evidence artifact 설치 전에 차단합니다.
- Requested/admitted/running/terminal의 4개 revision lifecycle을 저장하고 timeout, cancel,
  resource denial, stale context/parent, interrupted-no-replay outcome을 기록합니다. 전송한 model
  request는 재시도하지 않습니다.
- Parent merge 전에 result/evidence artifact를 다시 검증합니다. Parent checkpoint 뒤 merge
  event 전에 restart되면 두 번째 parent checkpoint 없이 누락된 event를 복구합니다. 같은
  retry는 no-op이고 서로 다른 두 번째 result는 fail-closed합니다.
- Boundary 최대값, stale/tampered state, cancellation race, restart recovery, result/evidence
  merge, raw task 비영속성, secret-safe diagnostic을 포함한 S01-S18 unit/실제 CLI process
  test를 추가합니다.

### 구현 중 검증

- v0.35 구현 계약을 범위로 제한한 독립 리뷰 한 번
- Targeted subagent unit/실제 CLI lifecycle test
- `cargo fmt --all -- --check`
- `cargo test --locked -- --test-threads=1`
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo build --release --locked`
- Release policy, toolchain pin, target matrix, packaged-binary, uninstall smoke check

### 경계

- 실행은 sequential이며 parent당 non-terminal child 하나로 제한합니다.
- Nested subagent, team lane 실행, command tool, child의 직접 write, patch apply, parallel model
  loading은 범위 밖입니다.
- 이 릴리즈는 managed `llama.cpp b9982` backend와 기존 5개 platform, exact 11-asset release
  set을 유지합니다. Model weight와 외부 plugin package는 번들하지 않습니다.

## v0.34.3 - Native Release Gate Recovery

릴리즈 날짜: 2026-07-16

이 patch release는 binary publication이 일부만 완료된 v0.34.2를 대체합니다. v0.34.2
source tag는 immutable하게 유지하지만, release gate에서 platform process sampling의
무제한 대기와 macOS native fixture의 외부 Python listener 의존이 드러났습니다.

### 수정한 것

- Platform process sampling command를 2초로 제한해 멈춘 `ps` 또는 `wmic` probe가
  backend startup이나 lifecycle reporting을 무기한 막지 못하게 합니다.
- Keep-alive server의 connection EOF를 기다리지 않고 유효한 HTTP status line을 받는 즉시
  backend health probe를 완료합니다.
- Native adapter test가 저장소의 Rust fake sidecar를 재사용하고 fixture SSE response를
  정상 TCP half-close로 끝내도록 해, macOS loopback reset과 외부 Python listener 의존을
  제거합니다.
- Native fixture port를 ephemeral port로 할당하고, bounded fixture command 실패 시
  sidecar log, runtime-ledger tail, test-only startup stage를 보존합니다.
- 실패했던 Windows와 macOS native lifecycle surface를 release gate 전에 하나의 수동
  platform-targeted workflow에서 실행합니다.

### Targeted 검증

- [Native targeted run 29475140650](https://github.com/MCprotein/rolling-potato/actions/runs/29475140650)에서
  Windows x86_64, macOS arm64, macOS x86_64의 lifecycle과 full-adapter test가 통과했습니다.

### 릴리즈 복구

- v0.34.0부터 v0.34.2까지는 immutable source history로 유지하지만, 일부만 게시된
  binary asset은 지원 release set이 아닙니다.
- v0.34.3을 지원 replacement로 삼고 검증된 exact 11-asset set을 게시합니다.

## v0.34.2 - Windows ConPTY Release Recovery

릴리즈 날짜: 2026-07-16

이 patch release는 binary publication이 완결되지 못한 v0.34.1을 대체합니다. v0.34.1
source tag는 immutable하게 유지하지만, Windows archive build 전에 native-terminal
gate가 test host lifecycle과 long-path 결함을 드러냈습니다.

### 수정한 것

- Windows pseudoconsole startup contract에 맞춰 첫 production client를 생성할 때까지
  ConPTY-side pipe handle을 유지합니다. 이후 각 production client가 재사용 console의 input
  echo를 복구했는지 post-child probe로 검증합니다.
- 각 production client를 ConPTY에 attach하기 전에 test host의 redirected standard handle을
  비우고, Enter는 carriage return으로 보내며 Ctrl+Z 뒤 Enter를 terminal EOF boundary로
  처리합니다.
- Mode probe output flush를 기다리고 ConPTY title/cursor control sequence를 정규화해,
  runner별 화면 rendering에 의존하지 않고 reusable-console lifecycle과 exact terminal
  outcome을 검증합니다.
- Fixture subprocess output을 file로 capture하고 30초 상한을 적용해, redirected pipe
  inheritance가 source-approval setup을 멈추지 않게 합니다.
- `MoveFileExW` 호출 전 destination parent를 canonicalize해 Windows atomic replacement가
  long path를 지원하도록 합니다. Native test는 새 deep target과 기존 deep target을 모두
  검증합니다.
- Fixture cleanup에서 `ClosePseudoConsole`보다 host output pipe를 먼저 닫아, pseudoconsole
  close가 대기하는 Windows version의 문서화된 deadlock 위험을 제거합니다.
- Entry/EOF lifecycle과 full adapter test를 각각 5분으로 제한한 수동 실행 Windows
  native-terminal targeted workflow를 추가합니다.

### 릴리즈 복구

- v0.34.0과 v0.34.1은 immutable source history로 유지하지만, 일부만 게시된 binary
  asset은 지원 release set이 아닙니다.
- v0.34.2는 immutable source history로 유지하지만, 일부만 게시된 binary asset은 지원
  release set이 아닙니다. v0.34.3이 이를 대체합니다.

## v0.34.1 - Portable Release Recovery

릴리즈 날짜: 2026-07-16

이 patch release는 binary publication이 완결되지 못한 v0.34.0을 대체합니다. v0.34.0
source tag는 immutable하게 유지하지만, Windows build의 unstable metadata API와 Linux
ARM64 source-recovery 경로가 Linux x86 전용 `openat` flag 값을 사용해 exact 11-asset
set을 게시하지 못했습니다.

### 수정한 것

- Unstable Windows `MetadataExt` file identifier 대신 stable
  `GetFileInformationByHandle` volume/file identity 검사를 사용합니다. Dependency를
  추가하지 않고 기존 path/open-handle fail-closed contract를 유지합니다.
- Unix 전용 source-recovery test를 Unix로 제한해 Windows test binary가 compile됩니다.
- Windows에서 지원하지 않는 transition-parent directory open을 생략해 기존 Windows
  atomic-replace durability 경로와 일치시킵니다.
- Linux ARM64의 `O_DIRECTORY`와 `O_NOFOLLOW`에 architecture-correct 값을 사용해 두 Linux
  architecture 모두에서 descriptor-relative source traversal을 fail-closed로 유지합니다.
- Release matrix에 native Windows file-identity test를 추가합니다.

### 릴리즈 복구

- v0.34.0은 immutable source history로 유지하지만, 일부만 게시된 binary asset은 지원
  release set이 아닙니다.
- v0.34.1은 immutable source history로 유지하지만, 일부만 게시된 binary asset은 지원
  release set이 아닙니다. v0.34.2가 이를 대체합니다.

## v0.34.0 - Runtime-Owned Interactive TUI

릴리즈 날짜: 2026-07-16

이 릴리스는 dependency-free terminal surface를 실제 interactive controller로 올리면서
모든 권위를 runtime core에 유지합니다. 승인된 built-in-skill patch도 서로 느슨한 여러
write가 아니라 복구 가능한 하나의 exact prepared transaction으로 만듭니다.

### 포함된 것

- `rpotato tui`의 attached-terminal 자동 선택과 명시적 `rpotato tui interactive`
  line controller를 추가합니다. Canonical view, paging, selection, patch/verification
  승인, pending-gate 거부, workflow resume, cancel을 지원합니다.
- Runtime-owned selection lease, intent ID, confirmation, no-echo one-shot secret input,
  closed 27-row outcome table을 사용합니다. Terminal output은 ANSI/OSC/control byte를
  escape하고 live terminal dimension을 따르며 SIGINT/SIGTERM 또는 Windows console
  종료 전에 echo를 복원하고 dispatch 전 frame failure와 commit 뒤 display failure를
  구분합니다.
- 모든 mutating TUI lease를 선택된 active workflow에 binding하고 approval/diff read는
  해당 workflow의 bounded canonical proposal만 허용합니다. 다른 project의 tool artifact,
  변조되거나 과대한 resume proposal, 보유 중 교체된 lock-file inode는 fail-closed하며,
  kernel-locked per-lock owner namespace와 claim이 replacement inode의 두 번째 writer 진입을 막습니다.
- 성공한 built-in-skill patch approval을 immutable exact-11 bundle 하나로 commit하고
  preplanned E0-E9 event chain을 실제 effect boundary에서 append합니다. Workflow R+1/R+2
  snapshot과 shared pointer, current-state C+1 image 하나, sanitized no-stream tool output,
  TranscriptRecord v2, rollback/source data, projection lag metadata가 hash와 identity로
  binding됩니다.
- T1-T10의 모든 중단을 저장된 journal byte에서 duplicate event/revision 없이 복구하고,
  설치된 R+2 pointer를 내리지 않으며, 반복된 committed intent에는 refresh-only receipt를
  반환합니다. Projection repair는 project ledger, operation log, SQLite가 순서대로 수렴할
  때까지 exact E9 lag marker를 보존합니다.
- Current-state를 revision/hash/ledger/workflow binding이 있는 strict canonical schema v2로
  올리고 검증된 schema-v1 promotion을 유지합니다. Runtime ledger만 event authority이며
  project/SQLite record는 재생성 가능한 projection입니다.
- Remote release branch cleanup을 published exact 11-asset set 검증 성공 뒤로 옮깁니다.
  Tag-policy와 failed/cancelled release path는 branch를 보존하고, failure diagnostic은
  실제 실패 job result와 remote branch의 preserved/missing/unverifiable 상태를 항상
  구분해 출력합니다.

### 알려진 제한

- v0.34.0의 승인된 source installation 성공 경로는 Unix만 지원합니다. 미지원 platform은
  journal commit과 target effect 전에 차단합니다.
- Interaction은 line-oriented이며 raw-key/full-screen protocol이 아닙니다.
- 마지막 pathname validation 뒤 시작해 validate-to-unlink race를 이기는 동시 외부
  writer는 지원 보장 밖입니다. 관측 가능한 conflict는 fail-closed하지만 이 관측 불가능한
  interval까지 atomic하다고 주장하지 않습니다.
- Secret value는 재표시하지 않습니다. 반복된 committed intent에는 refresh-only receipt만
  반환합니다.

### 구현 중 검증

- `cargo fmt --all -- --check`
- `cargo test --locked -- --test-threads=1`
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo build --release --locked`
- release policy, toolchain pin, target matrix, exact release-asset fixture,
  packaged-binary, uninstall smoke check

## v0.33.0 - 실행 가능한 훅과 스킬

릴리즈 날짜: 2026-07-13

이 릴리스는 built-in skill과 runtime-owned lifecycle hook을 validation/routing
기반 기능에서 영속 agent loop의 실행 기능으로 올립니다. Policy, evidence, stop
criteria는 이제 prompt 관례가 아니라 실행되는 state-machine gate입니다.

### 포함된 것

- 17개 lifecycle hook point를 runtime, project, skill, session, observer layer와
  hook id 순서로 deterministic하게 dispatch합니다. Strict JSON parsing은
  fail-closed하고 conflict rule은 더 엄격한 result를 유지합니다.
- 명시적 `rpotato skill run <id> "<request>"`와 자연어로 routing된 built-in
  skill을 동일한 context, model, typed action, approval, verification, final report,
  stop-gate loop로 실행합니다.
- State transition마다 선언된 context, allowed tool, required hook, evidence,
  stop criteria를 강제합니다. Context가 빠지면 model request 전에 실패하고,
  요구사항이 덜 채워진 terminal state는 인정하지 않습니다.
- Patch 또는 command side effect 전에 workflow phase와 skill state가 일치해야 합니다.
  `fix-test`는 승인된 patch 전후에 동일한 canonical `cargo test` command를 실행하고,
  관측한 patch 전 실패를 ledger의 workflow/command hash에 binding합니다.
- 비어 있거나 한국어 guard를 통과하지 못한 read-only model answer를 거부하고,
  guarded answer에 실제로 표시된 source, line, diagnostic, benchmark, checksum,
  ranked finding detail만 completion evidence로 인정합니다.
- Active skill, invocation, skill state, completed hook, evidence, stop criteria를
  저장하는 workflow schema v4를 추가하면서 immutable v2/v3 artifact와 단방향
  upgrade 호환성을 유지합니다. SQLite는 canonical checkpoint의 실제 active skill을
  projection합니다.
- Raw payload 대신 payload SHA-256을 hook dispatch evidence로 기록합니다. 직접
  command 실행과 project file write는 hook capability가 아닙니다. Imported/plugin
  hook executable은 계속 비활성이고 default-deny입니다.

### 1.0 이전 Breaking 변경

- `rpotato skill run`은 이제 built-in skill id와 request를 모두 요구합니다. 기존
  id-only command는 실행 전 routing state만 기록했고 agent loop를 실행할 입력이
  부족했습니다.

### 구현 중 검증

- `cargo fmt --all -- --check`
- `cargo test --locked -- --test-threads=1` (`351` unit, `1` backend
  lifecycle, `28` process test 통과)
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo build --release --locked`
- release policy, toolchain pin, target matrix, packaged-binary, uninstall smoke check

## v0.32.1 - 안정 도구 체계 갱신

릴리즈 날짜: 2026-07-13

이 maintenance release는 과거 모델 측정값을 다시 쓰지 않으면서 저장소가 관리하는
모든 active tool pin을 2026-07-13 확인한 최신 안정 upstream release로 올립니다.

### 포함된 것

- Rust `1.97.0`을 `rust-toolchain.toml`, `mise.toml`, `Cargo.toml`에 일관되게
  고정하고 Cargo lock graph의 `bytes`를 1.12.1, `cc`를 1.2.67, `memchr`를
  2.8.3으로 갱신했습니다. 직접 Cargo dependency 6개는 모두 이미 최신 안정 release였고,
  공개된 `zip 9.0.0-pre2` prerelease는 도입하지 않았습니다.
- Release workflow를 최신 Node.js 24 action인 `actions/checkout` v7.0.0,
  `actions/upload-artifact` v7.0.1, `actions/download-artifact` v8.0.1의
  immutable commit으로 갱신했습니다.
- Release job을 현재 GA hosted image인 macOS 26 arm64, macOS 26 Intel,
  Ubuntu 24.04 x64/arm64, Windows Server 2025 x64로 옮겼습니다.
- Managed backend manifest를 `llama.cpp b9878`에서 source-pinned `b9982`로
  갱신하고 6개 지원 artifact의 name, size, SHA-256을 official release API와
  일치시켰습니다. 실제 macOS arm64 archive download가 size/SHA-256/install 검증을
  통과했고 설치된 binary는 build `9982 (99f3dc322)`를 보고했습니다.
- `b9982` upstream source를 기준으로 SSE, disconnect cancellation,
  response-reader, final usage contract를 다시 확인했습니다. `b9878`에서 측정한 모델
  adoption 결과는 과거 evidence로 유지하며 `b9982` 측정값으로 바꾸지 않습니다.

### Upstream 근거

- Rust 1.97.0: https://static.rust-lang.org/dist/channel-rust-1.97.0.toml
- GitHub Actions release: https://github.com/actions/checkout/releases/tag/v7.0.0,
  https://github.com/actions/upload-artifact/releases/tag/v7.0.1,
  https://github.com/actions/download-artifact/releases/tag/v8.0.1
- Hosted runner label: https://github.com/actions/runner-images
- llama.cpp b9982: https://github.com/ggml-org/llama.cpp/releases/tag/b9982

### 구현 중 검증

- Rust 1.97.0의 `cargo update --dry-run` (추가 갱신 `0 packages`)
- `cargo fmt --all -- --check`
- `cargo test --locked -- --test-threads=1` (`331` unit, `1` backend
  lifecycle, `23` process test 통과)
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-toolchain-pins.sh`
- `scripts/release/verify-release-target-matrix.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.32.1`
- `scripts/release/verify-uninstall-smoke.sh target/release/rpotato`
- 실제 managed `llama.cpp b9982` macOS arm64 download, checksum, install,
  install record, version detection smoke

## v0.32.0 - 지속 가능한 대화 Resume

릴리즈 날짜: 2026-07-13

이 release는 local conversation state를 process restart 뒤에도 재개 가능하게 만듭니다. Immutable app-data transcript artifact에는 user, visible/normalized model, tool, evidence turn을 저장하고 canonical ledger event가 순서와 binding 권위를 가지며, SQLite migration v6의 `transcript_records`는 재생성 가능한 query projection입니다.

### 포함된 것

- `run`은 workflow 생성이나 model 실행 전에 최근 turn을 최대 8개·2,400자 안에서 재구성하고 현재 요청과 resume context 전체에 source pointer 최대 4개·3,200자의 단일 공유 budget을 적용합니다.
- `state resume`, `resume <session-id>`, 새 `continue [session-id]`는 session 선택이나 안전한 checkpoint continuation 전에 transcript artifact, 현재 source SHA-256, workflow/proposal/evidence binding을 검증합니다. Pending approval은 backend를 다시 호출하지 않고, 결과가 불확실한 backend/verification side effect는 재시도하지 않습니다.
- `tui transcript <session-id>`, `state`, `monitor status`는 hidden model response, raw source body, patch fragment, verification command 원문을 노출하지 않고 ledger 순서의 durable transcript record를 보여줍니다.
- Process-level test가 SQLite 삭제 뒤 순서가 보존되는 projection 재구축, 반복 resume의 backend 2차 호출 없음, 선택 session continuation, mutation 전 preflight, patch fragment 제외, artifact 변조 fail-closed를 검증합니다.

### Privacy 경계

Durable resume는 local user turn과 visible/normalized model/tool/evidence record를 저장합니다. 전체 backend prompt, hidden reasoning/raw backend response, raw source body, patch fragment, verification command 원문, credential 포함 command output은 저장하지 않습니다. Source context는 project-relative pointer와 SHA-256으로 남기고 resume 시 원문을 다시 읽습니다.

### 구현 중 검증한 것

- `cargo fmt --all -- --check`
- `cargo test --locked -- --test-threads=1` (`331` unit, `1` backend lifecycle, `23` process test 통과)
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-target-matrix.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.32.0`
- `scripts/release/verify-uninstall-smoke.sh target/release/rpotato`
- 독립 release-blocker 재리뷰: proposal binding, transcript root, privacy, ordering, preflight 보정 후 승인

## v0.31.1 - Windows Sidecar Stop Fallback

릴리즈 날짜: 2026-07-11

이 patch release는 불완전한 v0.31.0 release에서 누락된 Windows artifact를 복구합니다. v0.31.0 Windows release job은 새 native cancellation lifecycle을 실제로 검증했지만 non-forced `taskkill` 실패 시 기존 forced-stop branch에 도달하기 전에 반환하는 문제를 발견했습니다.

### 포함된 것

- `backend stop`은 graceful termination command 실패 뒤 process liveness를 확인하고 process가 남아 있으면 즉시 force fallback을 실행합니다.
- Force command와 자연 종료가 경합해 command가 실패해도 기록된 process가 사라졌음을 liveness로 확인한 경우에만 성공으로 처리합니다.
- Platform-independent unit test가 두 fallback path를 검증하고, Windows release job은 artifact build 전에 실제 fake-sidecar cancellation/stop lifecycle을 다시 실행합니다.

### 구현 중 검증한 것

- `cargo test --locked -- --test-threads=1` (unit test 326개, process-level integration test 20개)
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-target-matrix.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.31.1`

## v0.31.0 - Backend Streaming과 Cancellation

릴리즈 날짜: 2026-07-11

이 릴리즈는 buffered one-shot backend chat transport를 bounded SSE lifecycle로 교체합니다. Live display, 명시적 cross-process cancellation, timeout cleanup, terminal observability를 추가하면서 generation이 중단되어도 managed `llama.cpp` sidecar는 계속 실행합니다.

### 포함된 것

- `backend chat`은 항상 `stream_options.include_usage=true`인 SSE를 요청합니다. `--stream`은 filtering된 visible delta를 즉시 flush하고 기본 display는 기존처럼 모아서 출력합니다.
- `backend cancel`은 exclusive lock 뒤에서 publish된 active generation을 대상으로 chat connection만 닫고 terminal record를 기다립니다. Lease가 사라졌다는 사실로 cancellation을 추정하지 않고 기록된 `completed`, `cancelled`, `timed-out`, `failed` outcome을 보고하며 managed sidecar는 계속 실행합니다.
- 전체 request timeout 기본값은 30초이며 `--timeout-ms`로 1-300,000ms를 지정할 수 있습니다. 이 제한은 address resolution, connection, request upload, response read 전체에 적용되고 upload/read 중 cancellation은 최대 100ms 간격으로 확인합니다.
- HTTP body 전송 뒤 request retry는 0회입니다. Adapter는 normal non-resumable stream path를 사용하며 `X-Conversation-Id`를 보내지 않습니다.
- Incremental filtering은 buffered/streaming display 전에 `reasoning_content`와 split `<think>` trace를 폐기합니다. Streaming 언어 검증은 완전한 text unit을 검사할 때까지 보류하므로 금지된 model byte를 flush하지 않습니다. First-token latency는 처음 표시 가능한 filtered delta부터 측정합니다.
- SSE event, HTTP chunk, 미완성 body buffer 크기를 제한하고 누적 visible completion text는 2 MiB로 제한합니다. Upstream error payload는 display 또는 persistence 전에 고정 category로 축약합니다.
- 시작, cancellation 요청, cancellation, timeout, failure, completion, stale lease cleanup을 lifecycle evidence로 남깁니다. 모든 terminal path는 raw prompt/response text 없이 resource/model-run evidence도 기록합니다. `backend stop`은 sidecar 종료 전에 terminal acknowledgement를 최대 5초 기다리고 시간이 만료되면 forced-stop outcome을 기록합니다. 병렬 ledger reader는 recoverable writer lease를 공유하므로 진행 중인 JSONL/head 갱신을 손상으로 오판하지 않습니다.
- Final usage chunk가 도착한 경우에만 token usage를 projection합니다. 중단되거나 실패한 run의 누락 usage는 임의의 0이 아니라 unknown으로 유지합니다.

### 경계

- App-data root 하나에서는 active generation 하나만 허용합니다.
- Cancellation과 timeout은 generation을 중단하지만 backend sidecar를 종료하지 않습니다.
- Streaming은 CLI에서 사용할 수 있습니다. Interactive TUI stream 조작은 v0.34.0 범위에 남아 있습니다.
- 현재 SQLite model-run projection은 interruption boolean을 사용합니다. Cancellation과 timeout은 lifecycle ledger event type으로 구분할 수 있습니다.
- Cross-platform process test는 Rust fake sidecar를 compile해 `backend cancel`이 sidecar를 유지하고 `backend stop`이 cancellation acknowledgement 뒤 sidecar를 종료하는지 검증합니다. Unix 전용 hostile fixture는 timeout, 언어 거부, error redaction, stop ordering을 추가로 검증합니다. Windows release job은 cross-platform process test와 portable streaming/generation-state suite를 Windows에서 직접 실행합니다.

### Upstream Contract

구현은 `llama.cpp b9878`에 고정되어 있습니다. Upstream SSE, response reader 파기 시 cancellation, disconnect, final usage 동작을 2026-07-11 확인했습니다: [chat completions](https://github.com/ggml-org/llama.cpp/blob/b9878/tools/server/README.md#post-v1chatcompletions), [response-reader lifecycle](https://github.com/ggml-org/llama.cpp/blob/b9878/tools/server/server-queue.h#L168-L208), [cancellation posting](https://github.com/ggml-org/llama.cpp/blob/b9878/tools/server/server-queue.cpp#L441-L460), [disconnect handling](https://github.com/ggml-org/llama.cpp/blob/b9878/tools/server/server-http.cpp#L521-L565), [final usage chunk](https://github.com/ggml-org/llama.cpp/blob/b9878/tools/server/server-task.cpp#L526-L537).

### 구현 중 검증한 것

- `cargo test --locked -- --test-threads=1` (unit test 323개, process-level integration test 20개)
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-target-matrix.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.31.0`

## v0.30.0 - 검증된 로컬 모델 도입

릴리즈 날짜: 2026-07-11

이 릴리즈는 source-backed Qwen/Gemma 후보를 실제 측정과 fail-closed 검증을 거치는 local adoption path로 연결합니다. Model weight를 bundle하지 않고 공개 benchmark 점수를 local GGUF 실행과 동일 조건으로 취급하지 않습니다.

### 포함된 것

- Strict persistent `model default [<id>]`와 optional `backend start --model`을 추가하고 selection마다 registry, artifact bytes, promotion evidence를 재검증합니다.
- Promotion을 backend binary, model artifact, context, sampling, mmproj, OS, architecture provenance가 있는 정확한 `backend.chat.completed` event에 binding합니다.
- Canonical `model-adoption-smoke-v1` fixture와 benchmark-to-chat event 직접 linkage를 추가합니다.
- Qwen 전용 non-thinking option을 Gemma request에 보내지 않습니다.
- 손상된 project ledger mirror를 보존하고 정상 app-global canonical ledger에서 재구축하되 global ledger 검증은 약화하지 않습니다.
- 같은 local host에서 pinned artifact 두 개를 평가하고 exact-response equality를 통과한 Gemma만 promotion, install, 지속 기본 선택했습니다.

### 로컬 실측 증거

Host: MacBook Pro `Mac17,8`, Apple M5 Pro, 64GB RAM, macOS arm64, managed `llama.cpp b9878`, context 4096, temperature 0.1, top-p 0.8.

| Artifact | 결과 | Latency | Tokens/s | Peak RSS |
| --- | --- | ---: | ---: | ---: |
| Qwen3.5 4B Q4_K_M | `2/3`, exact response 실패, marker `5/5`, 금지 `0` | `1680ms` | `61.9048` | `3296378880` bytes |
| Gemma 4 E4B IT QAT q4_0 | `3/3`, exact response 통과, marker `5/5`, 금지 `0` | `1686ms` | `61.6845` | `5521932288` bytes |

Qwen은 요구된 다섯 줄 앞에 지시문 문장을 추가 출력했으므로 더 낮은 측정 RSS가 계약 실패를 덮지 않습니다. Gemma의 local `recommendedRamGb=8`은 측정 peak RSS를 GiB로 올림한 값에 2GiB headroom을 더해 계산했습니다. 이는 16GB host test, 보편적 model ranking, public benchmark parity가 아닙니다.

### 경계

- Static manifest 후보는 계속 `unverified`이며 local promotion은 evidence가 재검증되는 동안에만 유효합니다.
- Adoption smoke는 좁은 instruction/safety contract를 검사하며 전체 repository code-edit 또는 tool execution quality benchmark가 아닙니다.
- Model weight는 user app data에만 있고 commit이나 GitHub Release asset에 포함하지 않습니다.
- Windows와 16GB model runtime 검증은 아직 열려 있습니다.

## v0.29.1 - 크로스 플랫폼 Aggregate Checksum 수정

릴리즈 날짜: 2026-07-11

이 patch release는 v0.29.0의 모든 release asset을 독립적으로 내려받아 검증하는 과정에서
발견한 Windows checksum 줄바꿈 문제를 수정합니다. PowerShell이 Windows `.sha256` file을
CRLF로 기록해 aggregate checksum의 zip filename에 보이지 않는 carriage return이 남았고,
Unix `shasum -c`가 실패했습니다. v0.29.1은 v0.29.0 publication을 supersede하면서 해당
runtime 변경은 그대로 유지합니다.

### 포함된 것

- Windows checksum을 명시적인 ASCII encoding과 LF terminator로 기록합니다.
- Per-asset과 aggregate checksum file의 carriage return을 거부합니다.
- Release target guard에서 유효한 LF와 잘못된 CRLF fixture를 검증합니다.
- 5개 플랫폼 build, packaged binary smoke, Windows uninstall smoke, aggregate checksum
  publication은 그대로 유지합니다.

### 구현 중 검증한 것

- `bash -n scripts/release/verify-checksum-basenames.sh`
- `bash -n scripts/release/verify-release-target-matrix.sh`
- `scripts/release/verify-release-target-matrix.sh`
- `cargo fmt --all -- --check`
- `cargo test --locked -- --test-threads=1` (294 tests)
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh`를 통한 release binary `doctor` smoke

### 경계

이 릴리즈는 packaging과 checksum validation만 변경합니다. v0.29 runtime 동작, model
manifest, 지원 target matrix는 변경하지 않습니다.

## v0.29.0 - 지속 가능한 Single-Agent Runtime 보정

릴리즈 날짜: 2026-07-11

v0.29.0은 `run`, `state resume`, `patch approve`, `patch verify`를 하나의 restart-safe
workflow로 연결합니다. Model text는 실행 불가 typed action으로 저장되고 runtime이 소유한
ontology context/source reread, 분리된 approval binding, guarded apply, verification evidence,
stop gate만 side effect와 completion을 결정합니다.

### 포함된 것

- Patch 전용 workflow identity를 범용 workflow envelope와 typed action state로 분리하고,
  read-only `run`과 patch proposal을 독립적으로 종료할 수 있게 했습니다.
- Ontology projection을 첫 context 선택 계층으로 사용하고, source pointer를 patch proposal에
  binding하기 전에 authoritative source를 다시 읽습니다.
- Raw model response를 사용자 최종 보고로 표시하지 않고 read-only, pending approval,
  blocked, terminal 결과 모두에 deterministic 한국어 output guard를 적용합니다.
- Patch 적용 승인과 verification command 승인을 별도 일회성 credential로 분리했습니다.
  `patch approve`는 command를 실행하지 않으며 `patch verify`만 pre-bound plan을 승인합니다.
- Workflow schema v3를 추가하되 immutable v2 snapshot/hash를 보존하고, 단방향 v2에서 v3
  append migration과 strict recovery binding을 적용했습니다.
- Runtime/project ledger append를 recoverable writer lease로 직렬화하고, 손상된
  current-state mutation은 fail-closed하며 SQLite-only session은 resume 권위에서 제거했습니다.
- Approve/cancel 경쟁, 이미 복구된 rollback, 변조된 source recovery artifact 경로를
  idempotent하고 fail-closed하게 처리합니다.
- 변경 불가 versioned workflow snapshot, sync된 recovery transaction, atomic
  committed-revision pointer를 추가하고 schema/revision/hash chain이 strict parse된
  append-only ledger checkpoint와 일치하도록 했습니다.
- 모든 nonterminal artifact를 검색하고 multi-active를 fail-closed하며 검증된 terminal
  active pointer를 atomic하게 정리하는 recovery를 추가했습니다.
- Approval을 workflow/action/proposal ID, before/after hash, 정확한 policy-allowed
  verification plan에 binding합니다. OS CSPRNG nonce는 한 번만 표시하고 hash만 저장하며
  approval은 apply 전에 저장합니다. Pending token을 잃은 경우 명시적인 `patch
  token-rotate`가 안전하게 새 token으로 교체합니다.
- Substring command 판정을 하나의 shell-free parsed argv grammar로 교체하고
  classification과 execution이 같은 결과를 사용하게 했습니다. Patch verification은 `pwd`와
  현재 crate의 제한된 Cargo 검사만 허용하며 command smuggling, 외부
  manifest/package/workspace 선택, path executable, metacharacter를 거부합니다.
- Model 재호출 없는 pending approval/approved-apply resume, guarded idempotent apply,
  atomic guarded apply, hash 검증 atomic rollback과 사실에 맞는 실패 evidence, hash-only
  evidence, complete resume에도 적용되는 fresh source/evidence stop gate를 추가했습니다.
- Standalone preview를 diff-only로 제한하고 PID/nonce recoverable lease, 명시적 `cancel`
  crash reconciliation, no-clobber source guard transaction, synced truncation head를 가진
  physical ledger chain을 추가했습니다.
- Deterministic 한국어 성공/실패 보고와 happy path, restart, stale hash, token 거부/redaction,
  denied command, rollback, corrupt workflow, idempotence, complete-state tamper, hostile model
  text/path parsing subprocess test를 추가했습니다. Portable unit/state test는 checkpoint fault
  window, multi-active state, ledger partial/chain conflict, rollback tamper/failure, token recovery,
  evidence dedupe, strict malformed artifact, projection truth, 재사용 가능한 8-fixture 한국어
  output guard를 검증합니다.

### 구현 중 검증한 것

- `cargo fmt --all -- --check`
- `cargo test --locked -- --test-threads=1` (294 tests: unit 282개, Unix subprocess integration 12개)
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-target-matrix.sh`
- `scripts/release/verify-release-binary-smoke.sh`를 통한 release binary `doctor` smoke

### 경계

이 릴리즈는 전체 conversation transcript replay, streaming, subagent/team 실행,
interactive TUI mutation을 구현하지 않습니다. 해당 기능은 후속 roadmap 항목입니다.
SQLite는 canonical runtime ledger에서 rebuild 가능한 workflow/session projection이며 두 번째
권위가 아닙니다.
Mode 0600 project-local workflow/proposal artifact는 project cleanup 전까지 recovery에 필요한
snippet, proposal diff/proposed source, transaction metadata, rollback source를 보존합니다.
SQLite/monitor, ledger detail, evidence에는 raw source bytes를 저장하지 않습니다. Legacy v2
plaintext credential은 atomic hash-only scrub 뒤 새 canonical workflow preview가 필요합니다.
Portable unit/state boundary는 Windows에서 실행되지만 true fake-sidecar subprocess suite는 현재
Unix 전용이므로 v0.29.0은 Windows subprocess end-to-end coverage를 주장하지 않습니다.

## v0.28.5 - Unix PID Guard for Release Gate

릴리즈 날짜: 2026-07-10

이 패치 릴리즈는 release gate runner shutdown의 root cause를 수정합니다. Stale sidecar
test가 fake PID로 `u32::MAX`를 사용했는데, Linux에서 이 값을 process command에 넘기면
`pid_t` 의미론 때문에 wrap되어 runner process group에 signal이 갈 수 있습니다. v0.28.5는
`kill`이나 `ps`를 호출하기 전에 invalid Unix PID 값을 거부하고 full serialized release test
gate를 복구합니다.

### 포함된 것

- Unix PID argument guard를 추가해 `kill`/`ps` process check 전에 `0`과 `i32::MAX` 초과
  값을 거부합니다.
- Wrapping PID value에 대한 regression test를 추가했습니다.
- GitHub Release test gate를 전체 `cargo test --locked -- --test-threads=1` suite로
  복구했습니다.
- Release docs, README binary download 설명, roadmap entry를 v0.28.5 complete Linux
  artifact publication 기준으로 업데이트했습니다.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test --locked` (216 tests)
- `cargo test --locked -- --test-threads=1` (216 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.28.5`
- `scripts/release/verify-release-target-matrix.sh`

### 경계

이 릴리즈는 Homebrew, Scoop, winget, apt, rpm, container 배포를 추가하지 않습니다. 직접
다운로드 가능한 GitHub Release archive와 checksum publish 안정화만 다룹니다.

## v0.28.4 - Release Runner Safe Test Gate

릴리즈 날짜: 2026-07-10

이 패치 릴리즈는 hosted release runner에서 sidecar timeout fixture를 skip해 GitHub runner
shutdown을 피하려고 했습니다. Published v0.28.4 release도 stale-record test에서 Unix PID
wrap hazard가 드러나 실패했습니다. v0.28.5가 이를 supersede합니다.

### 포함된 것

- Merge 전 local release verification에서는 sidecar timeout fixture를 포함한
  `cargo test --locked` full suite를 계속 실행합니다.
- GitHub Release test gate는
  `cargo test --locked -- --test-threads=1 --skip backend::tests::start_timeout_removes_record_and_keeps_logs`
  로 바꿔 hosted runner shutdown을 반복적으로 유발하는 fixture를 release runner에서만
  제외합니다.
- Target job은 native target build, packaged-binary smoke, archive 생성, checksum 생성,
  release upload에 집중하도록 유지합니다.
- Release docs, README binary download 설명, roadmap entry를 attempted Linux artifact
  publication path 기준으로 업데이트했습니다.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test --locked` (215 tests)
- `cargo test --locked -- --test-threads=1` (215 tests)
- `cargo test --locked -- --test-threads=1 --skip backend::tests::start_timeout_removes_record_and_keeps_logs` (214 tests, 1 filtered out)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.28.4`
- `scripts/release/verify-release-target-matrix.sh`

### 경계

이 릴리즈는 Homebrew, Scoop, winget, apt, rpm, container 배포를 추가하지 않습니다. 직접
다운로드 가능한 GitHub Release archive와 checksum publish 안정화만 다룹니다.

## v0.28.3 - Serialized Release Test Gate

릴리즈 날짜: 2026-07-10

이 패치 릴리즈는 v0.28.2의 GitHub runner shutdown 이후 release test gate를 직렬화했습니다.
Published v0.28.3 release도 sidecar timeout fixture 직후 shutdown signal을 받아 binary
asset을 upload하지 못했습니다. v0.28.4가 이를 supersede합니다.

### 포함된 것

- Release test gate를 `cargo test --locked -- --test-threads=1`로 실행해 release job에서
  process-oriented backend lifecycle test가 서로 겹치지 않게 했습니다.
- Fake timeout backend fixture는 `exec sleep` 상태로 유지해 timeout cleanup 이후 wrapper
  child가 남지 않게 합니다.
- Target job은 native target build, packaged-binary smoke, archive 생성, checksum 생성,
  release upload에 집중하도록 유지합니다.
- Release docs, README binary download 설명, roadmap entry를 attempted Linux artifact
  publication path 기준으로 업데이트했습니다.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test --locked` (215 tests)
- `cargo test --locked -- --test-threads=1` (215 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.28.3`
- `scripts/release/verify-release-target-matrix.sh`

### 경계

이 릴리즈는 Homebrew, Scoop, winget, apt, rpm, container 배포를 추가하지 않습니다. 직접
다운로드 가능한 GitHub Release archive와 checksum publish 안정화만 다룹니다.

## v0.28.2 - Linux Test Gate Process Cleanup

릴리즈 날짜: 2026-07-10

이 패치 릴리즈는 v0.28.1 release test gate가 Linux에서 signal 143으로 종료된 상태를
보완하려고 했습니다. Published v0.28.2 release도 GitHub runner shutdown signal로 binary
asset upload 전에 실패했습니다. v0.28.3이 이를 supersede합니다.

### 포함된 것

- Backend sidecar startup timeout에서 wrapper child가 timeout test 이후 남지 않도록 Unix
  process-group cleanup을 추가했습니다.
- Fake timeout backend fixture가 lingering child process를 만들지 않도록 `sleep`을 spawn하지
  않고 `exec sleep`을 사용하게 했습니다.
- v0.28.1의 `release test gate` 설계는 유지합니다. Full test는 target build 전에 한 번
  실행하고, target job은 build, smoke, package, checksum, upload에 집중합니다.
- Release docs, README binary download 설명, roadmap entry를 attempted Linux artifact
  publication path 기준으로 업데이트했습니다.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test --locked` (215 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.28.2`
- `scripts/release/verify-release-target-matrix.sh`

### 경계

이 릴리즈는 Homebrew, Scoop, winget, apt, rpm, container 배포를 추가하지 않습니다. 직접
다운로드 가능한 GitHub Release archive와 checksum publish 안정화만 다룹니다.

## v0.28.1 - Release Artifact Workflow Stabilization

릴리즈 날짜: 2026-07-10

이 패치 릴리즈는 v0.28.0 GitHub Actions 실행이 Linux ARM64 runner 종료 신호로
끊긴 뒤 release test gate를 추가했습니다. Published v0.28.1 release는 Linux test gate가
여전히 signal 143으로 종료되어 binary asset을 upload하지 못했습니다. v0.28.2가 이를
supersede합니다.

### 포함된 것

- `ubuntu-24.04`에서 `cargo test --locked`를 한 번 실행하는 전용 `release test gate`
  job을 추가했습니다.
- Target build job은 native target build, packaged-binary smoke, archive 생성, checksum
  생성, release upload에 집중하도록 정리했습니다.
- `scripts/release/verify-release-target-matrix.sh`가 release test 중앙화와
  Linux/macOS/Windows matrix 고정을 함께 검증하도록 강화했습니다.
- Release docs, README binary download 설명, roadmap entry를 attempted Linux artifact
  publication path 기준으로 업데이트했습니다.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test --locked` (215 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.28.1`
- `scripts/release/verify-release-target-matrix.sh`

### 경계

이 릴리즈는 Homebrew, Scoop, winget, apt, rpm, container 배포를 추가하지 않습니다. 직접
다운로드 가능한 GitHub Release archive와 checksum publish 안정화만 다룹니다.

## v0.28.0 - Linux Release Artifacts

릴리즈 날짜: 2026-07-10

이 릴리즈는 기존 checksum과 packaged-binary smoke gate를 유지하면서 공식 GitHub Release
binary target을 Linux x86_64와 Linux ARM64까지 확장합니다.

### 포함된 것

- `ubuntu-24.04` runner에서 build하는 `x86_64-unknown-linux-gnu` release artifact를
  추가했습니다.
- `ubuntu-24.04-arm` runner에서 build하는 `aarch64-unknown-linux-gnu` release artifact를
  추가했습니다.
- macOS와 Linux가 같은 OS-neutral `tar.gz` packaging path를 쓰도록 tarball packaging step을
  정리했습니다.
- Release workflow target list를 고정하는
  `scripts/release/verify-release-target-matrix.sh`를 추가했습니다.
- Release docs, README binary download 설명, roadmap entry를 새 Linux target 기준으로
  업데이트했습니다.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test --locked` (215 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.28.0`
- `scripts/release/verify-release-target-matrix.sh`

### 경계

이 릴리즈는 Homebrew, Scoop, winget, apt, rpm, container 배포를 추가하지 않습니다. 직접
다운로드 가능한 GitHub Release archive와 checksum만 publish합니다.

## v0.27.0 - Plugin Adapter Hardening

릴리즈 날짜: 2026-07-10

이 릴리즈는 외부 plugin 실행을 열지 않고, local Codex/Claude Code형 plugin import
경로를 hash와 permission 정책 기준으로 강화합니다.

### 포함된 것

- Normalized plugin manifest를 schema version 2로 올리고 adapter version,
  permission policy, source manifest SHA-256, imported source snapshot SHA-256을
  기록합니다.
- `plugin import`가 보이는 Codex/Claude Code plugin surface를 capability summary로
  mapping하고 required/blocked permission을 함께 보고합니다.
- Shell, `bin/`, MCP, hook, LSP, monitor/background, runtime setting, remote
  connector, sensitive config, file-write permission은 기본 차단 상태로 유지합니다.
- `plugin validate`와 `plugin enable`이 imported source snapshot hash를 다시 확인하고
  drift가 있으면 plugin을 `blocked`로 표시합니다.
- Capability report, manifest hash persistence, Claude Code surface detection,
  drift blocking test를 추가했습니다.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test --locked` (215 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.27.0`

### 경계

이 릴리즈는 imported plugin code를 실행하지 않고, 외부 plugin marketplace를 연동하지
않으며, MCP/shell/background permission을 부여하지 않습니다. 외부 plugin format은 policy
approval 없이 trusted native runtime extension이 되지 않습니다.

## v0.26.0 - Ontology Runtime Store

릴리즈 날짜: 2026-07-10

이 릴리즈는 `rolling-potato`의 온톨로지를 문서상의 설계에서 실제 project-local
runtime store로 옮기는 첫 단계입니다. 작은 모델 prompt에는 source pointer 중심의 compact
view만 제공하고, 원문 판단은 다시 파일을 읽도록 강제합니다.

### 포함된 것

- `.rpotato/ontology/graph.jsonl` typed graph store와
  `.rpotato/ontology/schema.json` schema contract를 추가했습니다.
- `rpotato init`이 ontology store/schema를 만들고 source-backed Layer A fact를 seed합니다.
- `rpotato ontology status`, `seed`, `inspect`, `context --query <text>`,
  `reread <source-pointer>`, `export --format json|jsonl`,
  `import --file <path> --dry-run` 명령을 추가했습니다.
- Layer A seed는 indexed file, package manifest, entrypoint, generated-exclusion rule을
  source pointer와 SHA-256 hash로 기록하고 raw source text를 store에 장기 저장하지 않습니다.
- Import dry-run은 source pointer와 source hash 없는 confirmed Layer B semantic claim을
  차단합니다.
- `rpotato doctor`가 ontology store diagnostic을 함께 표시합니다.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test --locked` (212 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.26.0`
- isolated `rpotato init`, `ontology status`, `ontology context`, `ontology reread`,
  `ontology import --dry-run` smoke

### 경계

이 릴리즈는 model output을 confirmed ontology로 승격하지 않고, RDF/OWL을 canonical
store로 사용하지 않으며, ontology invariant check를 patch apply나 agent loop에 아직 연결하지
않습니다.

## v0.25.0 - Verified Model Install Gate

릴리즈 날짜: 2026-07-10

이 릴리즈는 출처가 기록된 model candidate를 registry에 설치하는 첫 경로를 추가합니다.
설치는 근거 없는 model claim이 아니라 local evidence로 gate합니다.

### 포함된 것

- Registry 설치 전 local promotion evidence를 검증하는
  `rpotato model promote <id> --evidence <file>`을 추가했습니다.
- Promotion은 source-backed artifact size/SHA-256 일치, backend smoke ledger event,
  RAM-fit/mmproj field, matching peak RSS를 가진 SQLite `measured-locally` benchmark row를
  요구합니다.
- `rpotato model install <id>`은 static manifest가 `verified`이거나 normalized local
  promotion evidence가 다시 검증될 때만 모델을 등록합니다.
- Model registry entry에 promotion evidence path, backend version, benchmark run id를
  기록합니다.
- README와 model manifest 문서에 fetch, benchmark, promote, install 흐름과 promotion
  evidence JSON schema를 문서화했습니다.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test --locked` (203 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.25.0`
- `RPOTATO_DATA_HOME=<tmp> RPOTATO_PROJECT_ROOT=<tmp> target/release/rpotato model install qwen3.5-4b` (promotion evidence 전 expected block)

### 경계

이 릴리즈는 Qwen 또는 Gemma public benchmark parity를 주장하지 않으며, model weight를
포함하지 않고, local evidence 없이 모델을 자동 승격하지 않습니다.

## v0.24.2 - Aggregate Checksum Checkout Fix

릴리즈 날짜: 2026-07-09

이 patch release는 aggregate checksum job이 repo-local checksum guard를 실행하기 전에
repository를 checkout하도록 수정합니다.

### 포함된 것

- `checksums` job이 workflow artifact를 download하고
  `scripts/release/verify-checksum-basenames.sh`를 실행하기 전에 `actions/checkout@v4`를
  실행합니다.
- 현재 release 문서가 v0.24.2 binary workflow를 가리키도록 갱신했습니다.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test --locked` (197 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `bash -n scripts/release/verify-checksum-basenames.sh`
- `scripts/release/verify-checksum-basenames.sh <fixture-checksum-file>`
- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/release-binaries.yml")'`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.24.2`

### 경계

이 릴리즈는 지원 binary target이나 package-manager distribution을 변경하지 않습니다.

## v0.24.1 - Checksum Workflow Fix

릴리즈 날짜: 2026-07-09

이 patch release는 GitHub Release checksum file을 download한 release asset 옆에서
바로 사용할 수 있도록 checksum publishing을 수정합니다.

### 포함된 것

- macOS `.sha256` file이 build-directory path 대신 release asset basename을 기록합니다.
- Aggregate checksum upload step에 명시적인 GitHub repository context를 추가했습니다.
- `scripts/release/verify-checksum-basenames.sh`를 추가하고 release packaging과 aggregate
  checksum generation에 연결했습니다.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test --locked` (197 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --release --locked`
- `bash -n scripts/release/verify-checksum-basenames.sh`
- `scripts/release/verify-checksum-basenames.sh <fixture-checksum-file>`
- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/release-binaries.yml")'`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.24.1`

### 경계

이 릴리즈는 지원 binary target이나 package-manager distribution을 변경하지 않습니다.

## v0.24.0 - Cross-Platform Release Hardening

릴리즈 날짜: 2026-07-09

이 릴리즈는 macOS Intel artifact, aggregate checksum publication, Windows uninstall
smoke coverage, future release용 release notes template로 공식 binary publishing
경로를 강화합니다.

### 포함된 것

- `release-binaries`가 macOS Apple Silicon과 Windows x86_64에 더해
  `x86_64-apple-darwin`도 build합니다.
- Release asset에 aggregate checksum publication file인
  `rpotato-vX.Y.Z-checksums.txt`를 추가합니다.
- Windows release job이 `rpotato uninstall --dry-run --keep-cache`와
  `rpotato uninstall --dry-run --purge-cache` smoke check를 실행합니다.
- `docs/release-notes-template.md`와 `docs/ko/release-notes-template.md`를 추가했습니다.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (197 tests)
- `cargo test --locked` (197 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --locked`
- `cargo build --release --locked`
- `cargo build --release --locked --target x86_64-apple-darwin`
- `file target/x86_64-apple-darwin/release/rpotato`
- `bash -n scripts/release/verify-uninstall-smoke.sh`
- `scripts/release/verify-uninstall-smoke.sh target/release/rpotato`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.24.0`
- `scripts/release/verify-release-binary-smoke.sh target/x86_64-apple-darwin/release/rpotato 0.24.0`

### 경계

이 릴리즈는 Linux artifact, package-manager channel, signing, notarization,
auto-update 배포를 추가하지 않습니다.

## v0.23.1 - Windows Binary Link Fix

릴리즈 날짜: 2026-07-09

이 patch release는 `rusqlite`의 `bundled-windows` feature를 켜서 Windows x86_64
release build를 수정합니다. Release workflow가 runner에 설치된 `sqlite3.lib`에
의존하지 않도록 합니다.

### 포함된 것

- Windows release binary가 system SQLite import library 대신 locked Rust dependency
  graph를 통해 SQLite를 link합니다.
- v0.23.0의 macOS packaging 동작은 그대로 유지합니다.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (197 tests)
- `cargo test --locked` (197 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build --locked`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.23.1`

## v0.23.0 - Official Binary Download Foundation

릴리즈 날짜: 2026-07-09

이 릴리즈는 첫 공식 binary release pipeline을 추가합니다. Published GitHub Release가
macOS Apple Silicon과 Windows x86_64용 managed `rpotato` binary build를 trigger하고,
checksum과 binary smoke 검증을 함께 생성합니다.

### 포함된 것

- 새 `release-binaries` GitHub Actions workflow.
- Release asset target:
  - `rpotato-vX.Y.Z-aarch64-apple-darwin.tar.gz`
  - `rpotato-vX.Y.Z-aarch64-apple-darwin.tar.gz.sha256`
  - `rpotato-vX.Y.Z-x86_64-pc-windows-msvc.zip`
  - `rpotato-vX.Y.Z-x86_64-pc-windows-msvc.zip.sha256`
- Packaged binary smoke check용 `scripts/release/verify-release-binary-smoke.sh`.
- `rpotato doctor`가 package name, package version, target OS/arch, binary suffix,
  release-smoke 가능 여부를 표시합니다.
- Release 문서가 v0.23.0 asset workflow를 정의하고, macOS Intel, Linux, package
  manager channel은 후속 작업으로 남긴다고 명시합니다.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (197 tests)
- `cargo test --locked` (197 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `cargo build --locked`
- `cargo build --release --locked`
- `scripts/release/verify-release-policy.sh`
- `scripts/release/verify-release-binary-smoke.sh target/debug/rpotato 0.23.0`
- `scripts/release/verify-release-binary-smoke.sh target/release/rpotato 0.23.0`
- `rpotato doctor`

### 경계

이 릴리즈는 macOS Intel, Linux, Homebrew, Scoop, winget, signing, notarization,
auto-update 배포를 추가하지 않습니다. 또한 model weight, backend binary, external plugin
package를 `rpotato` release binary에 포함하지 않습니다.

## v0.22.0 - Dispatcher Hardening

릴리즈 날짜: 2026-07-09

이 릴리즈는 첫 dispatch-time team hardening surface를 추가합니다. Worker를 시작하거나
team stage를 전진시키지 않고 dispatch ownership decision과 failed-worker continuation
상태를 기록합니다.

### 포함된 것

- 새 `rpotato team dispatch --lanes <count> --write-owner <lane:path>` 명령.
- Dispatch-time normalized file ownership enforcement. Cross-lane ownership
  conflict와 critical resource pressure는 blocked error를 반환하고 ledger/SQLite
  projection event를 기록합니다.
- `--failed-lane <lane> --failure <reason>`으로 failed-worker continuation을 기록합니다.
  남은 admitted lane 수와 continuation action도 표시합니다.
- `rpotato team status`가 현재 project의 최신 `team.*` runtime ledger event를 표시합니다.
- 영문/한국어 문서가 `team dispatch`를 worker launcher가 아니라 preflight/reporting
  boundary로 설명하도록 업데이트되었습니다.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (196 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `scripts/release/verify-release-policy.sh`
- `rpotato team dispatch --lanes 2 --write-owner 1:src/team.rs --write-owner 2:src/cli.rs`
- `rpotato team status`

### 경계

이 릴리즈는 subagent 시작, tool 실행, worker output merge, team stage 전진, full
dispatcher를 구현하지 않습니다. Worker launch가 생기기 전에 필요한 dispatch preflight
state를 기록하고 강제하는 기능만 제공합니다.

## v0.21.0 - Benchmark-Driven Optimization Policy

릴리즈 날짜: 2026-07-09

이 릴리즈는 첫 read-only optimization policy surface를 추가합니다. Local runtime
metric과 local benchmark evidence를 읽어 작은 모델 실행에 더 안전한 context,
team lane, fallback, model route hint를 추천합니다.

### 포함된 것

- 새 `rpotato monitor optimize` 명령.
- Resource pressure, model-run metric, context clamp count, p95 latency, average
  tokens/sec, local benchmark pass/fail evidence를 사용하는 deterministic
  optimization policy.
- `monitor optimize`는 measured benchmark run count, pass/fail count, average
  local score, 최신 measured benchmark row, recommended context tokens,
  recommended team lanes, fallback mode, model route hint를 출력합니다.
- 추천은 read-only local runtime hint입니다. 실제 model artifact를 선택하거나,
  model status를 승격하거나, raw prompt/source text를 저장하거나, public
  benchmark parity를 주장하지 않습니다.
- 영문/한국어 문서가 `monitor optimize`를 v0.21.0 benchmark-driven optimization
  policy surface로 설명하도록 업데이트되었습니다.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (189 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `scripts/release/verify-release-policy.sh`
- `rpotato monitor baseline`
- `rpotato monitor optimize`

### 경계

이 릴리즈는 dispatcher worker launch, model 승격, public benchmark parity, 자동
model 선택을 구현하지 않습니다. Local SQLite projection evidence를 보수적인 runtime
policy hint로 바꾸는 기능만 제공합니다.

## v0.20.1 - Benchmark Evidence Status

릴리즈 날짜: 2026-07-09

이 패치 릴리즈는 첫 Qwen executable smoke 실측 결과를 기록하고, model
evaluation preflight가 로컬 benchmark 측정 row를 반영하도록 수정합니다.

### 수정

- `rpotato model eval-plan qwen3.5-4b`가 더 이상 항상 `local benchmark
  status: not-run`을 표시하지 않습니다. SQLite `benchmark_runs` projection의
  최신 local `measured-locally` benchmark row를 표시합니다.
- 후보 artifact model id에 측정 row가 있으면 status가 `local-smoke-measured`로
  올라갑니다.

### 기록된 증거

- Qwen3.5-4B Q4_K_M local artifact는 이미 존재했고 SHA-256 검증을 통과했습니다.
- Managed `llama.cpp` version `9878 (2da668617)`가 `--ctx-size 4096`으로 Qwen
  sidecar를 시작했습니다.
- `rpotato benchmark run --fixture benchmarks/fixtures/executable-smoke.json
  --prompt benchmarks/prompts/executable-smoke.txt --max-tokens 32`는 score
  `3/3`, `local_pass=true`, latency `243ms`, total tokens `83`, resource
  pressure `normal`, peak RSS `3351363584` bytes를 기록했습니다.
- 측정 후 sidecar를 중지했습니다.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (186 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `rpotato model eval-plan qwen3.5-4b`
- `rpotato backend status`

### 경계

이 결과는 local smoke benchmark일 뿐입니다. Qwen3.5-4B를 `verified`로
승격하지 않으며 public benchmark parity도 주장하지 않습니다.

## v0.20.0 - Executable Benchmark Runner

릴리즈 날짜: 2026-07-09

이 릴리즈는 첫 executable local benchmark runner를 추가합니다. 여전히
source-only developer preview이며, 모델 가중치, 외부 plugin package, prebuilt
`rpotato` binary는 포함하지 않습니다.

### 포함된 것

- 새 `rpotato benchmark run --fixture <fixture.json> --prompt <artifact>
  [--max-tokens <tokens>]` 명령.
- `benchmark run`은 실행 중인 backend sidecar를 호출하고 local
  `claim_state=measured-locally` benchmark row를 기록합니다.
- Expected/forbidden response marker, abstention requirement, 비어 있지 않은 model
  output을 기준으로 deterministic 0-3 local product score를 산정합니다.
- SQLite migration v4가 `benchmark_runs`에 `model_run_id`, prompt artifact
  checksum/length, local pass flag, marker count, latency, token count, resource
  pressure, peak RSS field를 추가합니다.
- `benchmark report --format jsonl`이 executable benchmark field를 export합니다.
- `benchmarks/fixtures/executable-smoke.json`과
  `benchmarks/prompts/executable-smoke.txt`가 첫 executable smoke fixture/prompt
  pair를 제공합니다.
- Executable benchmark boundary, redaction, observability linkage에 대한
  영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (185 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `scripts/release/verify-release-policy.sh`
- `rpotato benchmark validate benchmarks/fixtures/sample.json`
- `rpotato benchmark validate benchmarks/fixtures/executable-smoke.json`
- `rpotato benchmark record --fixture benchmarks/fixtures/sample.json`
- `rpotato benchmark run --fixture benchmarks/fixtures/executable-smoke.json --prompt benchmarks/prompts/executable-smoke.txt --max-tokens 32` 실행 중인 sidecar가 없을 때 fail-closed
- `rpotato benchmark report --format jsonl`

### 알려진 제한

- `benchmark run`은 이미 실행 중인 backend sidecar와 `rpotato backend start`로
  시작한 local model file이 필요합니다. 이 릴리즈는 model weight를 bundle하거나
  자동 선택하지 않습니다.
- Executable runner는 local product score만 기록합니다. Public benchmark score와
  비교하거나 leaderboard parity를 주장하지 않습니다.
- Source-read compliance와 hallucination scoring은 아직 marker/proxy 기반입니다.
  Tool/evidence-aware scoring은 후속 범위입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.19.0 - Benchmark Harness Foundation

릴리즈 날짜: 2026-07-09

이 릴리즈는 첫 metadata-only benchmark harness surface를 추가합니다. 여전히
source-only developer preview이며, 모델 가중치, 외부 plugin package, prebuilt
`rpotato` binary는 포함하지 않습니다.

### 포함된 것

- 새 `rpotato benchmark validate <fixture.json>` 명령.
- 새 `rpotato benchmark record --fixture <fixture.json>` 명령.
- 새 `rpotato benchmark report --format jsonl` 명령.
- Project-local fixture schema 검증: runtime capability, model/runtime responsibility,
  expected route, policy decision, escalation target, required tool/source/evidence
  record, abstention requirement, ontology view, context budget, backend/model artifact
  identifier, sampling policy, raw artifact retention policy.
- SQLite migration v3가 `benchmark_runs`에 session, fixture checksum, claim state,
  reproducibility manifest, redacted report field를 추가합니다.
- Metadata-only benchmark record는 `claim_state=not-comparable`, `score=null`을
  사용합니다. Model 실행이나 public benchmark parity claim은 하지 않습니다.
- `benchmarks/fixtures/sample.json`은 raw prompt/source가 없는 CLI-contract smoke
  fixture입니다.
- Benchmark fixture contract, observability integration, v0.19.0 rollout status에 대한
  영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (183 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `scripts/release/verify-release-policy.sh`
- `rpotato benchmark validate benchmarks/fixtures/sample.json`
- `rpotato benchmark record --fixture benchmarks/fixtures/sample.json`
- `rpotato benchmark report --format jsonl`
- `rpotato monitor status`

### 알려진 제한

- Benchmark 명령은 model 실행, fixture score 산정, public benchmark와 local score 비교를
  하지 않습니다.
- Hardware/RAM/power/thermal manifest field는 executable benchmark run이 수집하기 전까지
  `not-recorded` placeholder입니다.
- Fixture suite, ontology-view scoring, public benchmark parity report,
  benchmark-driven optimization policy는 후속 범위입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.18.0 - Performance Baseline Report

릴리즈 날짜: 2026-07-08

이 릴리즈는 read-only local performance baseline report를 추가합니다. 여전히
source-only developer preview이며, 모델 가중치, 외부 plugin package, prebuilt
`rpotato` binary는 포함하지 않습니다.

### 포함된 것

- 새 `rpotato monitor baseline` 명령.
- 새 raw prompt/source store를 추가하지 않고 기존 local ledger/SQLite projection
  metric을 집계합니다.
- p50/p95 latency, average tokens/sec, context clamp count, context tokens
  dropped, peak RSS, pressure-state distribution, model/backend/session grouping을
  보고합니다.
- 이 report는 local metric evidence일 뿐이며, model artifact를 선택하거나
  source-backed model capability claim을 만들지 않습니다.
- v0.18.0 performance baseline 범위에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (172 tests)
- `cargo clippy --all-targets -- -D warnings`
- `cargo build`
- `scripts/release/verify-release-policy.sh`
- `rpotato monitor baseline`

### 알려진 제한

- `monitor baseline`은 local projection에 이미 있는 metric만 보고합니다.
  Benchmark를 실행하거나 continuous background sample을 수집하지 않습니다.
- Benchmark harness recording, redacted report export, benchmark-driven
  optimization policy는 후속 범위입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.17.0 - Team Context And Model Governor

릴리즈 날짜: 2026-07-08

이 릴리즈는 첫 team context/model governor preflight를 추가합니다. 여전히
source-only developer preview이며, 모델 가중치, 외부 plugin package, prebuilt
`rpotato` binary는 포함하지 않습니다.

### 포함된 것

- 새 `rpotato team governor --lanes <count> --context-tokens <tokens>` 명령.
- 명시적인 runtime policy simulation을 위한 선택 옵션
  `--context-limit <tokens>`와 `--model-tier small|standard|large`.
- 최신 resource sample을 사용한 admitted-lane 및 context/model governor decision.
- 설정 budget, degraded-pressure budget, local small-model soft budget에 맞춘
  effective context-token clamp.
- Local model route hint: `keep`, `downgrade`, `escalate`, `defer`.
- Team governor decision의 ledger/SQLite 기록.
- v0.17.0 governor 범위에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (170 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato team governor --lanes 2 --context-tokens 6000 --context-limit 4096 --model-tier standard`
- `rpotato team governor --lanes 2 --context-tokens 1024 --context-limit 4096 --model-tier small`
- `rpotato monitor status`

Smoke check는 `/private/tmp` 아래 scratch project root를 사용하며, normal pressure에서는
clamped context/model decision이 기록되고 critical pressure에서는 `defer` route hint로
차단되는지 확인합니다.

### 알려진 제한

- `team governor`는 preflight/reporting surface입니다. Worker를 시작하거나 실제 model
  artifact를 선택하거나 model routing을 실행하지 않습니다.
- Model route hint는 local runtime policy hint일 뿐이며, 실제 model artifact capability에
  대한 source-backed claim이 아닙니다.
- Dispatch-time ownership enforcement와 failed-worker continuation은 후속 범위입니다.
- Resource sampling은 아직 event-driven이며 continuous live polling은 아닙니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.16.0 - Team Approval Queue Integration

릴리즈 날짜: 2026-07-08

이 릴리즈는 차단된 team admission decision을 read-only approval queue에 연결합니다.
여전히 source-only developer preview이며, 모델 가중치, 외부 plugin package,
prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `.rpotato/approval-requests/` 아래 project-local approval request store를 추가했습니다.
- Blocking `team admit` policy/ownership decision은 team admission ledger event에 연결된
  redacted approval request record를 씁니다.
- `rpotato tui approvals`는 patch proposal approval 옆에 team admission approval request를
  표시합니다.
- `rpotato init`은 project runtime layout 일부로 approval request directory를 생성합니다.
- Team admission 출력은 policy 또는 ownership decision 검토가 필요할 때 approval request id와
  path를 포함합니다.
- v0.16.0 approval queue integration 범위에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (165 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato team admit --lanes 2 --command "cargo test"`
- `rpotato team admit --lanes 2 --write README.md`
- `rpotato team admit --lanes 2 --write-owner 1:README.md --write-owner 2:./README.md`
- `rpotato tui approvals`
- `rpotato monitor status`

Smoke check는 `/private/tmp` 아래 scratch project root를 사용하며, policy/ownership으로
차단된 team admission record가 read-only TUI approval queue에 표시되는지 확인합니다.

### 알려진 제한

- `tui approvals`는 read-only입니다. Team admission request를 나열하지만 approve, deny,
  dispatch resume은 수행하지 않습니다.
- `team admit`은 아직 subagent 시작, team lane dispatch, team stage 전진, 실제 worker
  execution 중 ownership enforcement를 수행하지 않습니다.
- Resource sampling은 아직 event-driven이며 continuous live polling은 아닙니다.
- Runtime context clamp와 model downgrade/escalation hint는 후속 범위입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.15.0 - Team File Ownership Preflight

릴리즈 날짜: 2026-07-08

이 릴리즈는 enforced team admission gate에 file ownership preflight를 추가합니다.
여전히 source-only developer preview이며, 모델 가중치, 외부 plugin package,
prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato team admit --lanes <count>`가 반복 가능한
  `--write-owner <lane:path>` ownership claim을 받습니다.
- Ownership path는 dispatch 전에 정규화됩니다. 예를 들어 `README.md`와
  `./README.md`는 같은 ownership key로 판정됩니다.
- Cross-lane ownership conflict는 향후 worker launch 이전 단계에서 admission을
  차단합니다.
- Owned write path도 기존 write policy preflight에 포함되므로 approval-required write는
  approval queue integration이 생기기 전까지 계속 dispatch를 차단합니다.
- Team admission 출력과 ledger event detail에 ownership claim count, ownership status,
  ownership blocked flag, owned write path, per-claim decision을 포함합니다.
- v0.15.0 ownership preflight 범위에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (163 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato team admit --lanes 2`
- `rpotato team admit --lanes 2 --command "cargo test"`
- `rpotato team admit --lanes 2 --write-owner 1:src/app.rs --write-owner 2:src/cli.rs`
- `rpotato team admit --lanes 2 --write-owner 1:README.md --write-owner 2:./README.md`
- `rpotato monitor status`

Smoke check는 `/private/tmp` 아래 scratch project root를 사용하며, 서로 다른 lane-owned
path는 allocation으로 표시되고 정규화된 cross-lane ownership conflict는 worker launch 전에
dispatch를 차단하는지 확인합니다.

### 알려진 제한

- `team admit`은 아직 subagent 시작, team lane dispatch, team stage 전진, 실제 worker
  execution 중 ownership enforcement를 수행하지 않습니다.
- Approval queue integration이 아직 후속 범위라서 `ask` decision은 dispatch를 차단합니다.
- Resource sampling은 아직 event-driven이며 continuous live polling은 아닙니다.
- Runtime context clamp와 model downgrade/escalation hint는 후속 범위입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.14.0 - Team Policy Preflight

릴리즈 날짜: 2026-07-08

이 릴리즈는 enforced team admission gate에 policy preflight를 추가합니다. 여전히
source-only developer preview이며, 모델 가중치, 외부 plugin package, prebuilt
`rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato team admit --lanes <count>`가 반복 가능한 `--write <path>`와
  `--command <command>` preflight check를 받습니다.
- 요청 write path는 `policy check-path --write`와 같은 policy engine으로 분류합니다.
- 요청 command는 `policy check-command`와 같은 policy engine으로 분류합니다.
- `allow` policy check는 admission gate를 통과할 수 있습니다.
- `ask`와 `deny` policy check는 향후 worker launch 이전 단계에서 dispatch를 차단합니다.
- Team admission 출력과 ledger event detail에 policy check count, policy status,
  policy blocked flag, requested write, redacted command, per-check decision을
  포함합니다.
- v0.14.0 policy preflight 범위에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (159 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato team admit --lanes 2`
- `rpotato team admit --lanes 2 --command "cargo test"`
- `rpotato team admit --lanes 2 --write README.md`
- `rpotato monitor status`

Smoke check는 `/private/tmp` 아래 scratch project root를 사용하며, command preflight는
통과하고 write preflight는 worker launch 전에 `approval-required`로 차단되는지
확인합니다.

### 알려진 제한

- `team admit`은 아직 subagent 시작, team lane dispatch, team stage 전진, file ownership
  allocation을 수행하지 않습니다.
- Approval queue integration이 아직 후속 범위라서 `ask` decision은 dispatch를 차단합니다.
- Resource sampling은 아직 event-driven이며 continuous live polling은 아닙니다.
- Runtime context clamp와 model downgrade/escalation hint는 후속 범위입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.13.0 - Team Admission Gate

릴리즈 날짜: 2026-07-07

이 릴리즈는 v0.12.0의 read-only team admission preview를 첫 enforced admission
gate로 바꿉니다. 여전히 source-only developer preview이며, 모델 가중치, 외부 plugin
package, prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- 새 `rpotato team admit --lanes <count>` 명령.
- Admission decision을 append-only ledger와 SQLite projection에 기록합니다.
- Normal pressure에서는 요청한 parallel lane을 허용합니다.
- Missing/unknown 또는 degraded pressure에서는 sequential lane 하나로 fallback합니다.
- Critical pressure에서는 향후 worker launch 이전 단계에서 blocked error를 반환합니다.
- `team status`는 read-only로 남고, `team admit`은 mutating gate가 됩니다.
- v0.13.0 admission gate 범위에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (157 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato team admit --lanes 2`
- `rpotato monitor status`

Smoke check는 `/private/tmp` 아래 scratch project root를 사용하며, resource sample이
없을 때 `team admit`이 ledger event를 기록하면서 sequential lane 하나로 fallback하는지
확인합니다.

### 알려진 제한

- 요청 write와 command에 대한 policy preflight는 v0.14.0에서 도입됐습니다. Full worker
  dispatch와 file ownership allocation은 후속 범위입니다.
- Resource sampling은 아직 event-driven이며 continuous live polling은 아닙니다.
- Runtime context clamp, file ownership, tool risk, approval queue, model
  downgrade/escalation hint는 후속 범위입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.12.0 - Team Admission Preview

릴리즈 날짜: 2026-07-07

이 릴리즈는 resource monitoring/governor 작업 위에 첫 read-only team admission
surface를 추가합니다. 여전히 source-only developer preview이며, 모델 가중치, 외부
plugin package, prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- 새 `rpotato team status` 명령.
- 향후 subagent/team dispatch가 재사용할 resource lane admission policy.
- Normal pressure에서는 요청한 parallel lane을 허용합니다.
- Missing/unknown 또는 degraded pressure에서는 sequential lane 하나로 fallback합니다.
- Critical pressure에서는 새 team dispatch를 차단합니다.
- `team status`가 최신 resource sample metadata, requested lane, admitted lane,
  admission, dispatch-blocked flag, fallback, reason, hint, read-only boundary를
  표시합니다.
- v0.12.0 team admission preview 범위에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (153 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato team status`
- `rpotato monitor status`

Smoke check는 `/private/tmp` 아래 scratch project root를 사용하며, resource sample이
없을 때 `team status`가 workflow state를 변경하지 않고 sequential fallback을
보고하는지 확인합니다.

### 알려진 제한

- `team status`는 admission preview일 뿐입니다. Subagent 시작, team lane dispatch,
  workflow mutation, file ownership enforcement는 아직 수행하지 않습니다.
- Resource sampling은 아직 event-driven이며 continuous live polling은 아닙니다.
- Enforced resource admission gate는 v0.13.0에서 도입됐고, 남은 dispatcher policy는
  후속 범위입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.11.0 - Backend Chat Resource Governor

릴리즈 날짜: 2026-07-07

이 릴리즈는 managed backend sidecar를 위한 첫 runtime resource governor slice를
추가합니다. 여전히 source-only developer preview이며, 모델 가중치, 외부 plugin
package, prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato backend chat`이 model 실행 전에 backend CPU/RSS/disk resource pressure를
  sampling합니다.
- Critical resource pressure에서는 `/v1/chat/completions` 요청을 보내기 전에 chat을
  차단합니다.
- Degraded resource pressure에서는 effective max-token budget을 clamp하고,
  normal/unknown pressure 요청은 유지합니다.
- `backend chat`과 `run` 출력이 requested max tokens와 effective max tokens를
  구분하고 governor admission/token action을 표시합니다.
- Redacted ledger event가 raw prompt/response 없이 governor admission, token action,
  reason, sample event id를 기록합니다.
- v0.11.0 governor 범위에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (149 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato backend chat --prompt smoke --max-tokens 256`
- `rpotato monitor status`

Smoke check는 `/private/tmp` 아래 scratch project root를 사용합니다. 실행 중인
backend sidecar가 없을 때 `backend chat`은 model 실행 전에 fail closed 해야 하며
raw prompt/response storage를 만들면 안 됩니다.

### 알려진 제한

- Resource sampling은 아직 event-driven이며 continuous live polling은 아닙니다.
- v0.11.0 governor는 backend chat에만 적용됩니다. Team admission preview와
  sequential fallback은 v0.12.0에서 도입되며, 실제 subagent/team dispatch admission은
  후속 범위입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.10.0 - TUI Resource Monitor

릴리즈 날짜: 2026-07-07

이 릴리즈는 read-only TUI beta에 managed backend sidecar resource-pressure
monitor를 추가합니다. 여전히 source-only developer preview이며, 모델 가중치, 외부
plugin package, prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato tui monitor`가 resource sample count, 최신 pressure status, CPU
  percent, average/peak RSS, disk bytes, recorded timestamp를 보여줍니다.
- Model monitoring summary가 total token과 average latency에 더해 average tokens
  per second를 표시합니다.
- Monitor layout은 dependency-free terminal-safe surface로 유지되며,
  `COLUMNS=64` 같은 좁은 terminal render도 다룹니다.
- v0.10.0 TUI monitor 범위에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (148 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato tui monitor`
- `COLUMNS=64 rpotato tui monitor`

TUI smoke는 `/private/tmp` 아래 scratch project root에서 runtime state를 초기화하고,
observability schema v2 상태에서 monitor view가 resource pressure, resource sample
count, model/token count, read-only action, beta boundary를 workflow mutation 없이
렌더링하는지 확인했습니다.

### 알려진 제한

- Resource monitor data는 event-driven이며 최신 recorded sample을 보여줍니다.
  Continuous live polling은 아닙니다.
- Runtime resource governor 동작은 v0.10.0에 포함되지 않으며, 첫 backend chat
  governor slice는 v0.11.0에서 도입됩니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.9.0 - Backend Resource Sampling

릴리즈 날짜: 2026-07-07

이 릴리즈는 관리형 `llama.cpp` sidecar를 위한 첫 backend resource monitoring
slice를 추가합니다. 여전히 source-only developer preview이며, 모델 가중치, 외부
plugin package, prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- CPU percent, average/peak RSS bytes, disk bytes, sample count, pressure
  status, recorded timestamp를 담는 `resource_samples` SQLite projection schema.
- `backend start`, already-running start 재사용, 실행 중인 sidecar의 `backend
  status`, `backend chat`에서 backend resource sampling.
- Redacted `backend.resource.sampled` ledger event. Raw prompt, response, source
  text는 기본적으로 계속 저장하지 않습니다.
- `monitor status`가 resource sample count와 최신 CPU/RSS/disk/pressure field를
  보여줍니다.
- `monitor prune --dry-run`이 resource sample row count를 포함합니다.
- v0.9.0 monitoring 범위에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (147 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato monitor status`
- `rpotato backend status`
- `rpotato monitor prune --before 30d --dry-run`

CLI smoke는 `/private/tmp` 아래 scratch project root에서 runtime state를 초기화하고,
observability schema v2와 monitor 출력의 resource sample count, latest resource
CPU/RSS/disk/pressure field를 확인했습니다.

### 알려진 제한

- Resource sampling은 event-driven이며 continuous background polling은 아닙니다.
- TUI resource-pressure 표시는 v0.9.0에 포함되지 않으며, v0.10.0에서 도입되었습니다.
- Runtime resource governor 동작은 v0.9.0에 포함되지 않으며, 첫 backend chat
  governor slice는 v0.11.0에서 도입됩니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.8.0 - TUI Evidence And Stop Gate View

릴리즈 날짜: 2026-07-07

이 릴리즈는 read-only TUI beta에 evidence/stop-gate status inspection을 추가합니다.
여전히 source-only developer preview이며, 모델 가중치, 외부 plugin package, prebuilt
`rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato tui evidence`는 runtime evidence store path, runtime evidence record
  count, project evidence artifact count, SQLite evidence record count, SQLite
  stop-gate result count, stale evidence policy summary를 보여줍니다.
- TUI overview가 evidence view를 안내합니다.
- `monitor status`가 SQLite evidence/stop-gate result count를 함께 보여줍니다.
- Project-local artifact를 세는 read-only evidence store status API.
- 확장된 TUI beta surface에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (143 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato init`
- `rpotato tui evidence`
- `COLUMNS=64 rpotato tui evidence`

TUI smoke는 `/private/tmp` 아래 scratch project root에서 runtime state를 초기화하고,
evidence view가 runtime evidence, project evidence, observability, stop-gate count,
stale policy, validation command, read-only beta boundary field를 렌더링하는지 확인했습니다.

### 알려진 제한

- TUI beta는 아직 interactive event loop가 아니라 one-shot read-only render입니다.
- Evidence view는 evidence/stop-gate status만 보고하며, workflow를 pass/fail 판정하지 않습니다.
- Terminal stop-gate evaluation, tool output viewer, subagent/team status, plugin
  permission review는 후속 작업입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.7.0 - TUI Session Transcript View

릴리즈 날짜: 2026-07-07

이 릴리즈는 read-only TUI beta에 선택한 session의 event inspection을 추가합니다.
여전히 source-only developer preview이며, 모델 가중치, 외부 plugin package, prebuilt
`rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato tui transcript <session-id>`는 선택한 session metadata와 timestamp 순
  event timeline을 보여줍니다.
- `rpotato tui sessions`가 transcript inspection command를 안내합니다.
- Session event를 읽는 SQLite observability read API.
- Transcript replay, resume, cancellation, workflow mutation을 TUI beta 밖에 두는
  read-only boundary.
- 확장된 TUI beta surface에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (140 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato session new`
- `rpotato state resume`
- `rpotato tui sessions`
- `rpotato tui transcript <session-id>`
- `COLUMNS=64 rpotato tui transcript <session-id>`

TUI smoke는 `/private/tmp` 아래 scratch project root에서 새 session을 만들고 no-op
resume event를 기록한 뒤, session list와 transcript timeline에 2개의 projected ledger
event가 보이는지 확인했습니다. Raw model transcript replay나 workflow mutation은
수행하지 않았습니다.

### 알려진 제한

- TUI beta는 아직 interactive event loop가 아니라 one-shot read-only render입니다.
- Transcript view는 projected ledger event metadata와 summary만 보여줍니다. Raw event
  detail과 model transcript replay는 후속 agent-loop 작업입니다.
- Tool output viewer, subagent/team status, plugin permission review,
  stop-gate evidence view는 후속 작업입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.6.0 - TUI Approval And Diff Views

릴리즈 날짜: 2026-07-07

이 릴리즈는 read-only TUI beta에 patch approval queue와 diff inspection view를
추가합니다. 여전히 source-only developer preview이며, 모델 가중치, 외부 plugin
package, prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato tui approvals`는 project-local patch proposal record를 나열합니다.
- `rpotato tui diff <proposal-id>`는 proposal metadata, approve/dry-run command
  hint, 저장된 unified diff를 보여줍니다.
- Patch proposal summary/detail을 읽는 read-only API.
- TUI에서 `---`, `+++`, `@@`, `-`, `+` diff line이 유지되도록 literal diff rendering.
- 확장된 TUI beta surface에 대한 영문/한국어 문서 업데이트.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (138 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato patch preview --path src/lib.rs --find 1 --replace 2`
- `rpotato tui approvals`
- `rpotato tui diff <proposal-id>`
- `COLUMNS=64 rpotato tui diff <proposal-id>`

TUI smoke는 `/private/tmp` 아래 scratch project root에서 patch proposal을 만들고,
pending approval record와 저장된 unified diff를 표시했으며 patch approve나 apply는
수행하지 않았습니다.

### 알려진 제한

- TUI beta는 아직 interactive event loop가 아니라 one-shot read-only render입니다.
- Approval queue와 diff view는 기존 patch proposal record를 inspect만 합니다.
  Approval과 apply는 여전히 `rpotato patch approve`로 수행합니다.
- Transcript view, tool output viewer, subagent/team status, plugin permission review,
  stop-gate evidence view는 후속 작업입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.5.0 - Read-Only TUI Beta

릴리즈 날짜: 2026-07-07

이 릴리즈는 terminal-only 환경을 위한 첫 read-only TUI beta surface를 추가합니다.
여전히 source-only developer preview이며, 모델 가중치, 외부 plugin package,
prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato tui` overview dashboard
- `rpotato tui monitor` model/token monitoring view
- `rpotato tui sessions` full session id와 resume hint가 있는 session-history view
- SSH/Linux server 친화적인 dependency-free ASCII layout
- approval, patch apply, resume, cancel, workflow mutation을 수행하지 않는 read-only boundary
- TUI beta surface에 대한 영문/한국어 문서 업데이트

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (133 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato tui`
- `rpotato tui monitor`
- `rpotato tui sessions`

TUI smoke는 project/session 상태, SQLite observability path, 기록된 model/token metric, session history, read-only beta boundary를 보여줬습니다.

### 알려진 제한

- TUI beta는 interactive event loop가 아니라 one-shot read-only render입니다.
- approval queue, diff viewer, transcript view, subagent/team status, plugin permission review, stop-gate evidence view는 후속 작업입니다.
- 첫 beta는 의도적으로 TUI framework dependency를 추가하지 않습니다. Interaction requirement가 안정된 뒤 더 풍부한 TUI crate가 필요한지 재검토할 수 있습니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.4.0 - Approved Patch Apply

릴리즈 날짜: 2026-07-07

이 릴리즈는 patch approval surface를 dry-run gate 확인에서 승인된 patch apply,
rollback record, 선택적 verification command 실행까지 확장합니다. 여전히
source-only developer preview이며, 모델 가중치, 외부 plugin package, prebuilt
`rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato patch approve <proposal-id> --token <token>`은 `--dry-run`이 없을 때 승인된 proposal을 적용합니다.
- apply 전 current file SHA-256 guard로 preview 이후 target file이 바뀐 stale proposal을 차단합니다.
- `.rpotato/patch-proposals/` 아래 rollback record를 생성합니다.
- write 이후 applied SHA-256을 검증합니다.
- `--verify-command <command>`는 apply 이후 allow 정책을 통과한 단순 argv verification command를 실행합니다.
- verification 실패 시 rollback을 시도하고 성공으로 보고하지 않습니다.
- 새 patch application 경계에 대한 영문/한국어 문서 업데이트

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (127 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `RPOTATO_PROJECT_ROOT=/private/tmp/rpotato-v040-smoke` scratch project smoke
- `rpotato patch preview --path README.md --find "Local coding agents for potato PCs." --replace "Local coding agents for potato PCs. Smoke"`
- `rpotato patch approve <generated-proposal-id> --token <generated-token> --verify-command "rg Smoke README.md"`

Patch smoke는 `status: applied`, rollback record 생성, `verification status:
passed`, verification exit code `0`을 반환했습니다. Smoke는 repository working
tree가 아니라 `/private/tmp` project fixture에서 실행했습니다.

### 알려진 제한

- patch preview는 여전히 project-local UTF-8 text file 하나에 대한 명시적인 단일 find/replace proposal만 지원합니다.
- verification command는 policy가 allow한 단순 argv command로 제한됩니다. Shell syntax, quoting, pipe, redirect, environment expansion은 지원하지 않습니다.
- model action output은 아직 patch preview/apply에 자동 연결되지 않습니다.
- verification output interpretation과 final Korean task reporting은 후속 작업입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.3.0 - Patch Diff Approval Preview

릴리즈 날짜: 2026-07-06

이 릴리즈는 첫 patch diff display와 approval gate surface를 추가합니다. 여전히
source-only developer preview이며, 모델 가중치, 외부 plugin package, prebuilt
`rpotato` binary는 포함하지 않습니다.

### 포함된 것

- `rpotato patch preview --path <path> --find <text> --replace <text>`
- project-local text replacement 하나에 대한 unified diff rendering
- `.rpotato/patch-proposals/` 아래 project-local proposal record
- 생성된 proposal의 approval token 표시
- `rpotato patch approve <proposal-id> --token <token> --dry-run`
- patch 적용 없는 approval gate 검증과 ledger event 기록
- 새 patch 경계에 대한 영문/한국어 문서 업데이트

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (123 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato patch preview --path RELEASE_NOTES.md --find "Run Skeleton Preview" --replace "Run Skeleton Preview Smoke"`
- `rpotato patch approve <generated-proposal-id> --token <generated-token> --dry-run`

Patch smoke는 `status: diff-ready`와 예상 unified diff를 반환했고, dry-run
approval에서는 `status: gate-passed`를 반환했습니다. Smoke 이후 target file에
Git diff가 없어 파일을 수정하지 않았음을 확인했습니다.

### 알려진 제한

- patch preview는 project-local UTF-8 text file 하나에 대해 명시적인 단일 find/replace proposal만 지원합니다.
- 이 릴리즈에서 patch approval은 dry-run 전용입니다. gate 결과를 기록하지만 patch를 적용하지 않습니다.
- model action에서 patch preview로 이어지는 agent-loop 통합은 후속 작업입니다.
- verification command execution, rollback handling, final Korean reporting은 후속 작업입니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.2.0 - Run Skeleton Preview

릴리즈 날짜: 2026-07-06

이 릴리즈는 managed `llama.cpp` sidecar 위에 첫 `rpotato run` vertical slice를
추가합니다. 여전히 source-only developer preview이며, 모델 가중치, 외부 plugin
package, prebuilt `rpotato` binary는 포함하지 않습니다.

### 포함된 것

- context-aware `rpotato run "<task>"` skeleton
- skill, mode, signal, constraint로 deterministic request routing
- source pointer가 있는 bounded repository context packing
- runtime-owned action candidate와 next gate reporting
- structured action line 또는 인식 가능한 action text에서 실행 없는 model action parsing
- local SQLite observability projection에 model/token/latency metric 기록
- intent, context pack, action candidate, model action, backend chat, model run ledger event
- versioned backend/model user agent를 쓰도록 source policy 정리
- 새 `run` 경계에 대한 영문/한국어 문서 업데이트

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test` (117 tests)
- `cargo clippy --all-targets -- -D warnings`
- `scripts/release/verify-release-policy.sh`
- `rpotato backend start --model <qwen-gguf> --ctx-size 4096`
- `rpotato run "src/intent.rs 기준으로 다음 action candidate가 무엇인지 한국어 한 문장으로 요약해."`
- `rpotato monitor models`
- `rpotato backend stop`

최신 Qwen3.5 smoke는 `model action parse: heuristic-text`, `model action kind:
patch-proposal`, `model action executable now: no`, `guard: pass`, `finish
reason: stop`을 반환했습니다. 이는 현재의 실행 없는 runtime boundary와
observability path의 증거이지, patch 품질이나 autonomous tool use 통과 증거는
아닙니다.

### 지원 환경

- 개발 및 smoke test 확인 환경: macOS Apple Silicon
- source-backed backend artifact manifest에는 계속 macOS arm64/x64, Linux
  arm64/x64, Windows arm64/x64용 `llama.cpp b9878` CPU artifact가 포함됩니다.

### 알려진 제한

- `rpotato run`은 아직 patch 적용, command 실행, model output의 승인된 action 처리를 하지 않습니다.
- model action parsing은 tolerant하고 실행하지 않습니다. 안정적인 structured action 생성과 approval UI는 후속 작업입니다.
- TUI, hooks execution, skills execution, subagents, team runtime은 아직 설계/계획 surface입니다.
- 모델 후보는 여전히 `unverified`이며 default model로 승격된 모델은 없습니다.
- Gemma local artifact fetch와 smoke는 완료되지 않았습니다.
- RAM-fit, peak memory, mmproj 필요 여부, benchmark scoring은 완료되지 않았습니다.
- streaming generation과 cancellation은 구현되지 않았습니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.

## v0.1.0 - 개발자 프리뷰

릴리즈 날짜: 2026-07-06

이 버전은 `rolling-potato`의 첫 개발자 프리뷰입니다. 초기 Rust runtime과
CLI scaffold를 위한 source-only release tag이며, stable runtime contract가
아닙니다. 모델 가중치, 외부 plugin package, prebuilt model/backend bundle은
포함하지 않습니다.

### 포함된 것

- `rpotato` Rust CLI scaffold
- project/app state 초기화
- SQLite projection 기반 session list/new/resume
- runtime ledger와 evidence validation surface
- command/path policy check와 credential redaction
- hook registry와 fail-closed hook result validation
- local plugin import/inspect/validate/enable/disable/remove surface
- monitoring status, model summary, export, dry-run prune surface
- source-backed Qwen/Gemma model candidate manifest와 evaluation gate
- size와 SHA-256 검증이 있는 evaluation-only model artifact fetch
- managed `llama.cpp b9878` backend install/start/status/stop/health surface
- `/v1/chat/completions` 기반 non-streaming backend chat smoke path
- `chat_template_kwargs.enable_thinking=false`를 쓰는 Qwen3.5 non-thinking smoke path
- 기본 영문 문서와 주요 문서의 한국어 번역

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `rpotato backend start --model <qwen-gguf> --ctx-size 4096`
- `rpotato backend health-check`
- `rpotato backend chat --prompt "한국어로 한 문장만 답해. 감자는 무엇인가?" --max-tokens 64`
- `rpotato backend stop`

Qwen chat smoke는 managed `llama.cpp` sidecar를 통해 깨끗한 한국어 응답을
반환했습니다. 이는 backend/model 연결과 non-thinking chat path의 증거이지,
전체 모델 품질 통과 증거는 아닙니다.

### 지원 환경

- 개발 및 smoke test 확인 환경: macOS Apple Silicon
- source-backed backend artifact manifest에는 macOS arm64/x64, Linux
  arm64/x64, Windows arm64/x64용 `llama.cpp b9878` CPU artifact가 포함됩니다.

### 알려진 제한

- `rpotato run`은 아직 intent normalization만 수행하며 full agent loop는
  구현되지 않았습니다.
- TUI, hooks execution, skills execution, subagents, team runtime은 아직
  설계/계획 surface입니다.
- 모델 후보는 여전히 `unverified`이며 default model로 승격된 모델은 없습니다.
- Gemma local artifact fetch와 smoke는 완료되지 않았습니다.
- RAM-fit, peak memory, mmproj 필요 여부, benchmark scoring은 완료되지 않았습니다.
- streaming generation과 cancellation은 구현되지 않았습니다.
- 이 preview release에는 prebuilt `rpotato` binary artifact를 첨부하지 않습니다.
