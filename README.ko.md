# rolling-potato

<p align="center">
  <a href="README.md">ENGLISH</a> |
  <strong>한국어</strong>
</p>

> 똥컴에서도 굴러가는 로컬 코딩 에이전트

`rolling-potato`는 저사양·중급 PC를 위한 로컬 우선 코딩 에이전트
런타임입니다. 이 프로젝트는 다음 전제에서 출발합니다.

> 작은 모델에는 작은 프롬프트가 아니라 작은 모델용 런타임이 필요합니다.

| 프로젝트 요약 | |
| --- | --- |
| 현재 릴리즈 | `v0.47.1` |
| CLI | `rpotato` |
| 런타임 | Rust, 관리형 `llama.cpp`, GGUF |
| 주요 화면 | CLI와 TUI |
| 릴리즈 플랫폼 | macOS arm64/x64, Linux arm64/x64, Windows x64 |
| 사용자 표시 언어 | 한국어 |

[설치](#설치) · [작동 방식](#작동-방식) · [현재 기능](#현재-기능) ·
[안전 모델](#아키텍처와-안전) · [문서 안내](#문서-안내)

---

## 개요

### 왜 런타임인가?

작은 로컬 모델은 유용하지만 취약합니다. 선택지를 좁히고 정책을 강제하며,
근거를 보존하고 실패를 관리하는 런타임이 필요합니다. 따라서
`rolling-potato`는 모델 추론만 감싸지 않고 다음 영역을 소유합니다.

- 모델과 백엔드 수명주기
- 크기가 제한된 저장소 컨텍스트
- 권한, 승인, 훅, 중단 게이트
- 영속 세션, 대화 기록, 워크플로, 근거
- 패치 미리보기, 적용, 검증
- 스킬, 플러그인 어댑터, 서브에이전트, 팀
- 토큰, 지연 시간, CPU, 메모리, 디스크, 벤치마크 모니터링
- CLI, TUI, 선택 가능한 로컬 정적 HTML 보고서
- 한국어 최종 응답 검증

CLI와 TUI는 사용자 화면입니다. 세션 상태, 도구 권한, 컨텍스트,
에이전트 루프, 검증은 런타임 코어가 책임집니다.

### 누구를 위한 프로젝트인가?

첫 대상은 다음과 같습니다.

- 한국어 사용자
- 구독형 코딩 에이전트 비용이 부담스러운 사용자
- 16 GB RAM 수준의 저사양 또는 중급 노트북 사용자
- 코드와 모델 실행을 로컬에서 처리하려는 사용자
- 로컬 LLM 도구를 먼저 공부하지 않고 코딩 도움을 받고 싶은 사용자

장기 목표는 약한 모델로 Claude Code/Codex를 흉내 내는 얇은 wrapper가
아니라, 실제 작업에 사용할 수 있는 로컬 대안입니다.

---

## 설치

공식 바이너리는
[GitHub Releases](https://github.com/MCprotein/rolling-potato/releases)에서만
배포합니다. 플랫폼에 맞는 archive를 내려받고 대응하는 `.sha256` file 또는
aggregate checksum file로 검증하십시오. 이 프로젝트는 Homebrew, Scoop,
winget을 비롯한 package-manager channel을 운영하지 않습니다.

`v0.42.0`부터는 압축을 푼 binary를 한 번 실행해 사용자 전용 CLI
directory에 설치하고 감지한 shell profile 또는 Windows 사용자 PATH를 자동
등록할 수 있습니다.

```sh
./rpotato install
# 새 terminal을 연 뒤:
rpotato init
```

Windows에서는 `.\rpotato.exe install`을 실행한 뒤
`& "$env:LOCALAPPDATA\Programs\rpotato\bin\rpotato.exe" init`을 실행합니다.
영구 PATH는 새 terminal부터 자동 적용되며, 두 command 모두 현재
terminal에서 바로 쓸 수 있는 한 줄 활성화 명령을 출력합니다.
`RPOTATO_*` 환경 변수는 선택적 override이므로 전역 값을 강제하지 않습니다.

`v0.44.0`부터 TUI 시작 시 이 저장소의 최신 stable GitHub Release를 확인합니다.
짧은 timeout과 6시간 cache를 사용하므로 offline이거나 network가 느려도 시작을
막지 않습니다. 새 버전이 있으면 TUI의 `/update` 또는 아래 CLI 명령으로 바로
확인하고 업데이트할 수 있습니다.

```sh
rpotato update --check
rpotato update
```

Self-update는 rpotato가 관리하는 사용자 전용 설치본에만 적용합니다. 현재 지원
platform의 정확한 archive와 대응하는 `.sha256` sidecar만 내려받아 검증한 뒤
설치 binary를 교체합니다. Windows에서는 현재 process가 종료된 뒤 교체를
완료합니다.

전역 application data와 현재 project의 `.rpotato` state만 지우고 다시
설치하려면 먼저 범위를 확인한 뒤 명시적으로 승인합니다.

```sh
./rpotato install --clean --dry-run
./rpotato install --clean --yes
```

관리형 backend 또는 generation이 실행 중이면 clean install은 차단됩니다.
dry-run은 binary와 PATH 등록이 생성·갱신·유지 중 무엇인지도 표시합니다.
runtime 게시와 삭제는 cross-process guard를 공유하며, process 생존 확인을
수행할 수 없는 경우에도 삭제를 차단합니다.

프로그램과 모든 관리형 상태를 함께 지우려면 삭제 범위를 먼저 확인하고
명시적으로 승인합니다.

```sh
rpotato uninstall --clean --dry-run
rpotato uninstall --clean --yes
```

Clean uninstall은 설치 binary, rpotato가 소유한 PATH block, 전역 application
data, 현재 project의 `.rpotato`를 제거합니다. 압축을 푼 원본 binary처럼
사용자가 직접 보관한 file과 source repository는 삭제하지 않습니다.
Windows에서 설치된 binary가 자기 자신을 지우는 경우에는 현재 process가
종료된 직후 삭제를 완료합니다.

지원 release target과 checksum 검증 방법은
[docs/ko/release.md](docs/ko/release.md)에 정리되어 있습니다.

---

## 작동 방식

의도한 로컬 설정 흐름은 다음과 같습니다.

1. `rpotato`를 실행합니다.
2. 최초 설정에 표시되는 model/version, quantization, download size, context,
   RAM 상태, license, 출처 근거를 확인합니다.
3. model을 선택하고 download를 승인합니다.
4. `rpotato`가 managed `llama.cpp` backend를 설치하거나 재사용하고 artifact
   size와 SHA-256을 검증한 뒤 model을 시작하도록 둡니다.
5. 같은 TUI에서 코딩 요청을 입력합니다.

모델 가중치는 `rpotato`에 포함하지 않습니다. 관리형 경로에서는 사용자가
`llama.cpp`를 전역으로 설치할 필요가 없습니다.

다음 명령 하나로 시작합니다.

```sh
rpotato
```

인자가 없는 `rpotato`가 기본 TUI를 시작합니다. 일반 텍스트로 범용 LLM 질문과
코딩 에이전트 요청을 모두 처리합니다. 인터넷 검색을 명시하거나 최신 정보가
필요한 질문은 API key, MCP process, provider SDK 없이 공개 검색 HTML을 직접
요청·파싱하는 제한된 읽기 전용 검색을 사용하고 runtime이 출처 link를 표시합니다.
`/doctor`는 별도 credential 없이 검색 transport 준비 상태를 보여줍니다. Composer
아래 line에는 현재 model, context 사용량,
compaction checkpoint, backend 상태, session이 표시됩니다. 일반 TUI 동작은
`/model`, `/compact`, `/search`, `/update`, `/status`, `/sessions`, `/doctor`, `/more`,
`/back`, `/clear`, `/help`, `/quit`을 사용합니다. 긴 응답은 viewport 밖의 line을
버리지 않고 `/more`와 `/back`으로 이동합니다. Context 사용량 75%에서 자동 압축하며 `/compact`는
immutable transcript를 정본으로 보존한 채 수동 checkpoint를 만듭니다.

간소화한 public CLI surface는 다음과 같습니다.

```sh
rpotato doctor
rpotato init
rpotato run "이 저장소의 테스트 실패 원인을 찾아줘"
rpotato debug --help
```

`rpotato debug --help`는 세부 호환·진단 명령을 보여줍니다. 일반 설정에서는 backend
executable이나 GGUF 경로가 필요하지 않습니다.

상세 MVP 인수 기준은 [docs/ko/mvp.md](docs/ko/mvp.md)에 있습니다.

---

## 현재 기능

`v0.47.1` 릴리즈는 제품 정의만 있는 scaffold가 아니라 실제 기능을 가진
pre-1.0 런타임입니다.

| 영역 | 현재 제공 기능 |
| --- | --- |
| 에이전트 루프 | 의도 routing, 제한된 context, typed model action, 한국어 최종 보고 |
| 영속 상태 | session, transcript, workflow, ledger, evidence, resume와 continue |
| 패치 워크플로 | 미리보기, 적용 승인, 별도 검증 승인, rollback 기록 |
| 백엔드와 모델 | 관리형 sidecar 수명주기, 출처 기반 후보, local 승격·설치 gate |
| 확장 | native hook/skill, local Codex/Claude Code plugin adapter |
| 협업 | 제한된 subagent 하나와 runtime-owned team execution |
| 모니터링 | CLI/TUI metric, SQLite projection, benchmark record, static HTML export |
| 화면 | 기본 대화 TUI, 자동화·진단 CLI, self-contained local HTML report |
| 범용 답변과 웹 | 일반 지식·계산 답변, 최신성 자동 routing, `/search`, API key 없는 직접 HTML 검색과 runtime 소유 출처 link |

장별 기능 지도, 대표 명령, 아직 완성되지 않은 경계는
[docs/ko/current-capabilities.md](docs/ko/current-capabilities.md)에
정리되어 있습니다. 설치된 버전의 정확한 문법은 `rpotato --help`와 각
subcommand help를 기준으로 확인하십시오.

---

## 아키텍처와 안전

런타임은 모델 출력을 신뢰하지 않는 입력으로 취급합니다. 모델이 작성한
텍스트가 도구를 직접 실행하지 않습니다. 지원하는 모든 side effect는 runtime
policy, 필요한 승인, evidence 기록, 검증을 통과해야 합니다.

주요 제약은 다음과 같습니다.

- 사용자에게 보이는 자연어 출력은 한국어이며 언어가 아닌 숫자와 수식은 그대로 허용합니다.
- 코드 블록, 경로, 명령, 인용 로그는 필요하면 원문을 유지합니다.
- 혼합 언어 최종 응답은 한국어 재작성 한 번 뒤 유효한 한국어 line을 안전하게
  투영하고, 보존할 수 있는 응답이 없을 때만 차단합니다.
- 가져온 plugin instruction은 runtime 권한을 넓힐 수 없습니다.
- shell, background, remote connector, 민감한 설정, write capability는
  지원하는 policy 경로가 허용하기 전까지 차단합니다.
- 복구할 때 결과가 불확실한 backend request나 verification command를
  자동으로 다시 실행하지 않습니다.
- 모델, license, 성능, memory, compatibility claim에는 출처나 로컬 근거가
  필요합니다.

아키텍처 상세 문서:

- [코드 아키텍처](docs/ko/code-architecture.md)
- [런타임 아키텍처](docs/ko/runtime-architecture.md)
- [상태 수명주기](docs/ko/state-lifecycle.md)
- [명령 정책](docs/ko/command-policy.md)
- [위협 모델](docs/ko/threat-model.md)

Qwen과 Gemma 항목은 평가 후보이며 기본 모델로 가정하지 않습니다.
`llama.cpp`는 backend이지 모델 후보가 아닙니다.
[모델 출처](docs/ko/model-source-policy.md),
[모델 라이선스](docs/ko/model-licenses.md),
[로컬 평가](docs/ko/model-eval.md)를 함께 확인하십시오.

---

## 프로젝트 상태

게시된 `v0.47.1`까지의 release history는 완료되었습니다. 최신 release는 로컬
coding-agent workflow를 유지하면서 일반 설명, runtime이 표시하는 출처 link가
있는 제한된 웹 근거, 혼합 언어 누출 방지를 안정화합니다.
[docs/ko/ROADMAP.md](docs/ko/ROADMAP.md)를
참고하십시오.

---

## 문서 안내

주제별 문서는 [한국어 문서 인덱스](docs/ko/README.md)에서 찾을 수 있습니다.

| 먼저 볼 문서 | 내용 |
| --- | --- |
| [계획](docs/ko/PLAN.md) | 제품 의도, 대상 사용자, 제품 형태 |
| [로드맵](docs/ko/ROADMAP.md) | 버전별 출시 기록과 다음 버전 규칙 |
| [디자인](docs/ko/DESIGN.md) | CLI, TUI, monitoring UX 기준 |
| [현재 기능](docs/ko/current-capabilities.md) | 구현 영역, 진입점, 알려진 경계 |
| [개발](docs/ko/development.md) | 로컬 개발과 검증 흐름 |
| [릴리즈](docs/ko/release.md) | 버전, branch, artifact, publication 정책 |

영문 문서는 [README.md](README.md)와
[영문 문서 인덱스](docs/README.md)를 사용하십시오.

---

## 공개 저장소 운영

이 저장소는 Apache-2.0 공개 오픈소스 프로젝트지만 현재 외부 코드 기여와
외부 PR을 받지 않습니다. 버그 리포트, 사용성 피드백, 보안 제보,
모델·라이선스 근거는 받을 수 있습니다.

[운영 정책](docs/ko/GOVERNANCE.md),
[보안 정책](docs/ko/SECURITY.md),
[프라이버시 정책](docs/ko/PRIVACY.md)을 확인하십시오.
