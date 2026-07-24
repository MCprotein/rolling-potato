# 터미널 UI Surface

TUI는 Claude Code/Codex replacement experience의 기본 product surface입니다.

사용자는 `rpotato`만 실행해 TUI에 진입합니다. 기본 frame은 짧은 welcome, 하단
composer, status line 하나를 가진 대화 transcript입니다. Raw revision, hash, ledger
counter, projection freshness, workflow record는 진단 정보이며 최초 화면을 채우지
않습니다. `rpotato tui`와
`rpotato tui interactive`는 기존 자동화와 테스트를 위한 호환 alias이며 기본 사용법이
아닙니다. 일반 텍스트 입력은 shell command가 아니라 에이전트 코딩 요청입니다.

TUI design source of truth는 [DESIGN.md](../../DESIGN.md)입니다. 특히 monitoring 화면은 SSH/Linux server에서도 쓸 수 있어야 하므로 browser나 GUI를 전제하지 않습니다.

## 현재 Surface

`v0.5.0`은 dependency-free, read-only ASCII TUI beta를 추가합니다.

- `rpotato tui`
- `rpotato tui monitor`
- `rpotato tui sessions`

이 beta는 기존 runtime state와 SQLite observability projection을 읽습니다. Project/session 상태, model/token summary, session history, resume hint, read-only boundary를 보여줍니다. Approval, patch apply, resume, cancel, workflow mutation은 수행하지 않습니다.

`v0.6.0`은 같은 read-only beta에 patch proposal inspection을 추가합니다.

- `rpotato tui approvals`
- `rpotato tui diff <proposal-id>`

초기 approval view는 project-local `.rpotato/patch-proposals/` record를 읽었습니다. v0.34.0부터 두 one-shot command 모두 interactive controller와 같은 bounded canonical runtime facade를 사용합니다. Approvals는 canonical team-admission event와 active workflow에 bound된 proposal만 보여주고, diff는 unbound 또는 oversized directory-only record를 읽거나 표시하지 않으며 patch approve/apply도 수행하지 않습니다.

`v0.7.0`은 read-only beta에 선택한 session의 event inspection을 추가합니다.

- `rpotato tui transcript <session-id>`

v0.32.0부터 Transcript view는 canonical durable user/visible-model/tool/evidence artifact를 검증하고 해당 turn을 ledger 순서의 event timeline과 함께 보여줍니다. 계속 read-only이며 hidden model response, source file body, patch fragment, verification command 원문, raw event detail은 표시하지 않습니다.

`v0.8.0`은 read-only beta에 evidence/stop-gate status inspection을 추가합니다.

- `rpotato tui evidence`

Evidence view는 runtime evidence JSONL path, project evidence directory, SQLite `evidence_records` count, SQLite `stop_gate_results` count, stale policy summary를 읽습니다. Workflow를 pass/fail 판정하지는 않으며, terminal stop-gate evaluation은 runtime-core 후속 작업입니다.

`v0.10.0`은 read-only monitor view에 resource pressure와 token throughput을 추가합니다.

- `rpotato tui monitor`

Monitor view는 SQLite model summary와 최신 `resource_samples` row를 읽습니다. Model run count, token record, average latency, average tokens per second, resource sample count, 최신 pressure status, CPU percent, average/peak RSS, disk bytes, recorded timestamp를 보여줍니다. 계속 read-only이며 export, prune, governor behavior는 TUI beta 밖에 둡니다.

첫 beta의 framework decision은 dependency-free terminal-safe surface로 시작하는 것입니다. Interaction requirement가 안정된 뒤 더 풍부한 TUI crate가 필요한지 결정합니다.

`v0.34.0`은 dependency를 추가하지 않고 terminal surface를 read-only inspection에서
runtime-owned line-oriented interactive controller로 올립니다.

- `rpotato tui`는 input과 output이 모두 terminal에 연결되면 controller를 시작하고,
  redirect된 사용에서는 read-only overview를 유지합니다.
- `rpotato tui interactive`는 같은 controller를 명시적으로 시작하며 deterministic
  piped-input test도 지원합니다.
- `view`, `next`, `prev`, `select <canonical-id>`는 canonical runtime state를
  이동합니다. `select session <session-id>`는 확인 후 runtime lease 경계에서 canonical
  session 선택을 보냅니다. 예약된 TUI 명령과 일치하지 않는 입력은 agent 요청으로
  전달하며 shell command로 직접 실행하지 않습니다.
- `view tool-output <artifact-id>`는 ledger binding, owner/path/hash, 크기를 검증한
  sanitized tool artifact를 엽니다. Session/transcript page의 authority는 canonical
  ledger와 durable artifact이며 SQLite를 정본으로 사용하지 않습니다.
- `approve <proposal>`, `approve verification <proposal>`, `deny`, `resume`, `cancel`은
  선택된 workflow, fresh runtime selection lease, 명시적 `yes` 확인을 요구합니다.
  Credential은 terminal echo를 끈 상태에서 한 번만 읽고, SIGINT/SIGTERM 또는 Windows
  console 종료 시 process 종료 전에 캡처한 input mode를 복원합니다.

현재 기본 진입 계약은 다음과 같습니다.

- attached terminal에서 인자 없는 `rpotato`는 controller를 시작합니다.
- redirect된 인자 없는 실행은 read-only overview를 출력하고 종료합니다.
- TUI 예약 명령과 일치하지 않는 일반 텍스트는 user turn으로 표시한 뒤 agent runtime에 전달합니다.
- 인사와 대화형 입력은 비변경 conversation path를 사용하며 patch proposal을 만들지 않습니다.
- 결과는 assistant turn으로 표시하고, 상세 runtime report는 명시적인 진단 view에 둡니다.
- shell text처럼 보이는 입력도 직접 실행하지 않고 model/runtime policy 경계를 통과합니다.
- 최초 실행의 backend/model 선택과 설치는 이 TUI 안에서 실행합니다. 기본 흐름에서는
  `llama.cpp` executable이나 GGUF 경로를 직접 입력하지 않습니다.

### 최초 실행

기본 model이 설정되지 않았으면 `rpotato`와 attached `rpotato init`이 대화 전에 설정
흐름을 엽니다.

1. 출처 기반 후보의 model ID/version, quantization, download size, context limit,
   RAM 상태, license, 근거 note를 보여줍니다.
2. 키보드 선택기에서 ↑↓와 Enter로 model을 고른 뒤 두 번째 선택형 화면에서
   download를 확인합니다. 번호/ID 입력은 plain terminal fallback으로 유지합니다.
3. 고정 버전 managed backend를 설치하거나 기존 설치를 재사용합니다.
4. 선택 artifact를 내려받아 size와 SHA-256을 검증하고, 명시적인 사용자 선택으로
   등록한 뒤 candidate에 출처와 함께 기록된 manifest context limit으로 시작합니다.

측정하지 않은 RAM 적합성과 capability는 `미확정`으로 유지합니다. Setup은 source
manifest를 benchmark evidence로 둔갑시키지 않습니다. `/model`은 현재/권장 상태를
표시하는 같은 선택형 catalog를 열고, `/model <id>`는 automation-compatible shortcut으로
유지합니다.

### Composer 상태 line

ANSI attached terminal의 빈 대화는 compact welcome frame으로 시작하고 첫 turn 뒤에는
한 줄 identity header로 전환합니다. Bordered composer 아래에는 의미별 색상을 적용한
안정된 status line을 두며 cursor는 input row로 되돌립니다.

```text
╭─ rpotato vX.Y.Z · 로컬 코딩 에이전트 ──────────────────────────────╮
│ model    gemma-4-E4B_q4_0-it                                      │
│ project  ~/codes/rolling-potato                                   │
╰─ /help 명령 · /model 변경 ────────────────────────────────────────╯

╭─ 요청 ────────────────────────────────────────────────────────────╮
│ › _                                                               │
╰───────────────────────────────────────────────────────────────────╯
model gemma-4-E4B_q4_0-it | ctx 812/131072 (1%) | compact auto@75% | backend ready | session 01J…
```

Field 순서는 항상 `model | context | compaction | backend | session`입니다. 전체
status row를 green으로 칠하지 않고 model/focus는 cyan, healthy는 green,
due/degraded는 yellow, failed/stale은 red, secondary identity는 muted로 표시합니다.
긴 user/assistant turn은 terminal display cell 기준으로 줄바꿈하므로 한국어와 wide
character도 버리지 않습니다.
`compact auto@75%`는 아직 checkpoint가 없다는 뜻이고, `compact due`는 checkpoint가
없는 session의 측정 사용량이 자동 압축 임계값에 도달했다는 뜻이며, `compact saved`는
active session에 검증된 checkpoint가 있다는 뜻입니다. 최신 model-run projection, managed backend
sidecar, active canonical session에서 읽으며, 없는 값과 stale backend 상태는 명확히
표시합니다. `NO_COLOR`, `TERM=dumb`, redirected/scripted 실행은 ANSI control sequence
없이 plain text를 사용합니다.

일반 interactive 명령은 `/model`, `/compact`, `/search <질문>`, `/open <URL>`,
`/find <텍스트>`, `/attach <경로>`, `/update`, `/status`, `/sessions`, `/doctor`,
`/more`, `/back`, `/clear`, `/help`, `/quit`입니다. `/more`와 `/back`은 화면 밖의
긴 응답 line을 버리지 않고 page 단위로 이동합니다. `/`를 입력하면 Enter 전에 `/help`와
같은 command registry를 사용하는 live command palette가 열립니다. ↑↓ 또는
`Ctrl+P`/`Ctrl+N`으로 항목을 고르고 Enter로 적용하며 Esc로 닫습니다. Composer는
Left/Right, Home/End, `Ctrl+A`/`Ctrl+E`, Option/Alt+Left/Right 단어 이동과 terminal이
해당 escape sequence를 보내는 경우 Command/Meta+Left/Right 줄 이동을 지원합니다.
`/update`는 확인을 받은 뒤 현재
platform의 정확한 GitHub Release asset을 내려받고 대응 SHA-256 sidecar를 검증한
경우에만 관리형 설치본을 교체합니다. 시작 check는 짧은 timeout과 cache를 사용하며
network 실패는 TUI를 막지 않습니다. `/compact`는 incremental typed checkpoint를 만들고
가장 최근 transcript record 4개를 보존합니다. 자동 압축도 측정된 context 사용량
75%에서 active session의 같은 경로를 사용합니다. Model 변경은 새 backend start가
성공한 뒤에만 기본값을 확정하고, 실패하면 이전 ready backend를 복구합니다. 세부 backend, registry, benchmark, policy,
inspection 명령은 `rpotato debug --help` 아래의 진단용 surface로 유지합니다.

Bracketed paste는 하나의 입력으로 처리합니다. 절대 이미지/text 경로를 붙여넣거나
`/attach <경로>`를 사용하면 `/` 명령으로 오판하지 않고 regular non-symlink 파일을
local app data에 캡처해 첨부 badge로 표시합니다. UTF-8 text/code 파일은 256 KiB까지
허용하되 응답·runtime 공간을 예약한 뒤 선택 model의 manifest context limit 안에
들어오는 경우에만 다음 요청에 포함합니다. PNG/JPEG 이미지는 최대 4개, 합계
20 MiB까지 허용하며 dispatch 시 한 번의 bounded file read로 size, signature,
SHA-256을 다시 검증합니다. 선택한 model의 별도 고정
`mmproj` bytes가 검증되고 managed `llama-server` sidecar가 vision-ready일 때만
image inference를 사용합니다. Status line과 `/status`는 `vision ready`와
`vision text-only`를 구분합니다. Projector 준비가 실패해도 검증된 text model과
현재 선택은 유지하며, image 요청에는 해결 방법이 있는 capability 안내를 표시합니다.

`mmproj`는 또 하나의 언어 모델이 아닙니다. Image feature를 짝이 맞는 language-model
GGUF가 기대하는 embedding 공간으로 변환하는 model 전용 visual encoder/projector
GGUF입니다. 다른 model이나 revision의 projector와 호환된다고 가정할 수 없습니다.
따라서 `rpotato`는 source revision, size, SHA-256을 별도로 고정·검증하고
`llama-server`를 시작할 때 `--mmproj`로 전달합니다. 정확히 검증된 cache hit는
다시 다운로드하지 않으며, artifact가 없거나 partial/corrupt이거나 고정 revision이
바뀐 경우에만 다운로드합니다. Projector 준비 실패는 기본 model이나 ready backend를
몰래 바꾸지 않습니다.

명확한 저장소·action signal이 없는 일반 질문은 가벼운 범용 답변 경로를 사용합니다.
Local model이 사용자 요청을 보고 `WebSearch`, `WebOpen`, `WebFind`가 필요한지
판단하며 고정된 한국어/영어 keyword 목록은 routing authority가 아닙니다.
`/search`는 명시적 fallback으로 유지합니다. Model이 선택한 검색은 공개 검색 HTML을
직접 요청·파싱해 제한된 snippet을 가져오고 출처 URL을 덧붙입니다. `/open`은
사용자가 지정한 공개 HTTP URL을 HTTPS로 승격해
HTML/plain text/JSON 문서를 제한된 크기로 읽으며, `/find`는 현재 TUI에서 마지막으로
연 문서의 정규화된 text를 literal·대소문자 무시 방식으로 찾습니다. API key, 별도
MCP process, provider SDK, background 검색 service를 사용하지 않습니다. 검색
transport는 redirect를 자동 추적하지 않습니다. `WebOpen` orchestration만 동일
scheme·port·host(`www.` 차이는 동등 취급) redirect를 최대 10회 추적하며, 다른
host redirect는 target URL을 표시하고 새로운 명시적 `/open`을 요구합니다. URL
credential, localhost, private/link-local/reserved IP와 그런 IP로 해석되는 DNS
host는 차단합니다. 검색 결과와 열린 페이지는 신뢰하지 않는 prompt context일 뿐
command 실행, 파일 수정, runtime 권한 확대를 할 수 없습니다. 해당 요청에서
offline/no-browse를 지시하면 agent-selected retrieval을 사용하지 않습니다.
Routing model에는 사용자 요청만 전달하고 local attachment 본문은 전달하지 않으며,
첨부 text는 근거 retrieval 이후 local 답변 합성에만 사용합니다. `/doctor`는
별도 credential 없이 `WebSearch`·`WebOpen`·`WebFind` 준비 상태를 표시합니다.

<!-- TUI-READ-CONTRACT:START -->
8개 view(`overview`, `monitor`, `sessions`, `transcript`, `tool-output`, `approvals`,
`diff`, `evidence`)는 view별 item, byte, scan, line, pagination 상한을 적용합니다. 모든
page는 canonical current/workflow revision과 hash, ledger sequence와 hash, 관련 content
또는 transcript hash, projection watermark, validation time, 그리고 `complete`,
`next-page`, `truncated`, `unavailable`, `redacted` 중 하나의 typed continuation을
포함합니다. SQLite는 파생된 metrics/freshness projection일 뿐이며 freshness 표기는 정확히
`fresh`, `stale`, `projection-lag`, `unavailable`입니다. 읽기 경로는 mutation lease를
획득하거나 state를 복구하거나 validation gap을 쓰지 않으며 corrupt, unbound,
SQLite-only, directory-scan-only candidate를 허용하지 않습니다.
<!-- TUI-READ-CONTRACT:END -->

모든 mutation, intent ID, immutable receipt, closed 27-row outcome table은 runtime이
소유합니다. 성공한 patch approval은 11개 ordered member와 exact E0-E9 semantic event
chain을 포함한 하나의 prepared bundle을 commit합니다. Restart recovery는 저장된 effect를
idempotent하게만 재생하고 설치된 R+2 workflow pointer를 R+1로 내리지 않으며, 같은
committed intent가 반복되면 secret을 다시 표시하지 않고 refresh-only receipt를
반환합니다. 첫 approval 성공은 새 verification credential을 terminal에 정확히 한 번
출력하고 다음 rendered notice에는 저장하지 않습니다. 읽기 surface는 새 product mutation을
만들지 않지만 command startup은 이미 commit된 transition journal을 마저 수렴하거나 지연된
derived projection을 재구축할 수 있습니다. Project ledger, operation log, SQLite는 이 순서로 파생되고 projection이
실패하면 수렴할 때까지 journal과 exact E9 lag marker를 보존합니다.

Terminal output은 ANSI/OSC와 control byte를 escape하고 width/height에 맞춘 bounded
rendering을 적용합니다. Dispatch 전 frame failure와 commit 뒤 frame failure를 구분해
후자를 새 mutation으로 재시도하지 않습니다. Tool output은 현재 project의 canonical
ledger event로 제한하고 approval/diff view는 active workflow의 bounded
workflow/action/hash-bound proposal만 노출합니다.

v0.34.0 제한:

- 승인된 source installation 성공 경로는 Unix만 지원합니다. 미지원 platform은 journal
  commit과 source effect 전에 차단합니다.
- Interaction은 line-oriented이며 raw-key/full-screen terminal protocol이 아닙니다.
- 마지막 pathname validation 뒤 시작해 validate-to-unlink race를 이기는 동시 외부
  writer는 지원 보장 밖입니다. 관측 가능한 conflict는 fail-closed하지만 관측 불가능한
  interval까지 atomic하다고 주장하지 않습니다.

## 목표

- long-running agent session을 inspect 가능하게 만든다.
- log를 직접 뒤지지 않아도 runtime state를 보여준다.
- approval, diff, tool output, subagents, teams를 지원한다.
- plugin import/permission review를 지원한다.
- context/evidence/stop gate를 visible하게 만든다.
- model/token/resource monitoring을 terminal-only 환경에서도 사용할 수 있게 만든다.
- keyboard-first terminal workflow를 유지한다.

## 비목표

- GUI desktop app
- primary interface로서의 web dashboard
- TUI-owned policy
- runtime core 직접 우회
- monitoring 화면에서 raw prompt/source를 기본 노출하는 것

## 필수 View

최소 TUI view:

- chat/session transcript
- current plan
- context and ontology summary
- pending approvals
- diff viewer
- tool output viewer
- model/backend status
- model/token usage summary
- CPU/memory/resource-pressure summary
- subagent status
- team status와 최신 team runtime event
- plugin permission review
- evidence/stop gate status
- logs and diagnostics

## 상호작용 Model

TUI action:

- user request submit
- tool call approve 또는 deny
- patch approve 또는 deny
- command approve 또는 deny
- plugin enable 또는 disable
- blocked plugin capability를 per-capability로 approve 또는 deny
- source pointer inspect
- evidence inspect
- active view switch
- session history 열기
- 선택한 session resume
- workflow cancel
- workflow resume

모든 action은 runtime core를 통과합니다.

## Layout 방향

초기 layout:

```text
┌────────────────────────────────────────────┐
│ transcript / active task                   │
├───────────────┬────────────────────────────┤
│ plan/context  │ diff/tool/evidence detail  │
├───────────────┴────────────────────────────┤
│ approvals / status / command bar           │
└────────────────────────────────────────────┘
```

Monitoring layout direction:

```text
┌─ Monitor ──────────────────────────────────┐
│ model/backend  tokens  tps  latency  mem   │
├───────────────┬────────────────────────────┤
│ model runs    │ selected session detail    │
│ failures      │ gate/tool/evidence status  │
├───────────────┴────────────────────────────┤
│ export / prune / refresh / command bar     │
└────────────────────────────────────────────┘
```

Monitoring UI rules:

- overview first, drill-down second
- active/degraded/blocked run first in sort order
- every metric shows timestamp or stale marker
- no color-only status; include text status
- no raw prompt/source by default
- export and prune actions require dry-run summary
- narrow terminal falls back to stacked single-panel views

## Runtime 계약

TUI는 runtime state를 consume합니다.

- session status
- session history
- active workflow
- active skill
- active subagents
- active team stage
- pending approvals
- plugin capability and permission report
- ledger tail
- evidence status
- backend/model status
- token/resource metric summary
- metric freshness/staleness state

TUI는 user decision을 emit합니다.

- request
- approve
- deny
- session 선택
- cancel
- resume
- inspect

## 명령 Palette Routing

Phase 3에서 고정한 command palette routing contract:

- `request.submit` -> `rpotato run <request>`
- `intent.preview` -> `rpotato intent classify <request>`
- `skill.run` -> `rpotato skill run <id> "<request>"`
- `plugin.review` -> `rpotato plugin inspect <id>` 또는 `rpotato plugin validate <id>`
- `plugin.toggle` -> `rpotato plugin enable <id>` 또는 `rpotato plugin disable <id>`
- `workflow.cancel` -> `rpotato cancel`
- `session.history` -> `rpotato session list`
- `session.resume` -> `rpotato resume <session-id>`
- `workflow.resume` -> `rpotato state resume`
- `monitor.open` -> `rpotato monitor status`
- `evidence.inspect` -> `rpotato evidence validate <artifact-pointer>`

Active workflow는 current-state가 소유합니다. TUI action은 runtime core에 request만 emit하고, skill/plugin/subagent/team은 parent workflow pointer 없이 독립 workflow를 만들 수 없습니다.

## 접근성과 제약

- Korean user-facing label by default
- small terminal size에서도 readable
- SSH/Linux server 환경 first-class
- hidden destructive shortcut 금지
- keyboard-first
- terminal resize handling
- clear fail-closed error display

## 검증

TUI는 smoke test가 필요합니다.

- small terminal size layout render
- approval flow가 runtime policy를 bypass하지 않음
- diff view가 long file을 처리함
- cancellation이 runtime state를 update함
- team/subagent status update
- model/token/resource monitoring view update
- plugin permission review가 runtime policy를 bypass하지 않음
- shell/bin/MCP/background/remote/file-write capability가 기본 차단으로 표시됨
- Korean output guard가 final report에 visible
