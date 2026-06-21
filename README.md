# rolling-potato

`rolling-potato`는 저사양 PC에서도 굴러가는 로컬 우선 CLI 코딩 에이전트 런타임입니다.

- CLI 명령어: `rpotato`
- 태그라인: `Local coding agents for potato PCs.`
- 한국어 포지셔닝: `똥컴에서도 굴러가는 로컬 코딩 에이전트`

## 핵심 방향

`rolling-potato`는 Claude Code나 Codex를 작은 모델로 흉내 내는 도구가 아닙니다.

핵심 가정은 반대입니다. 작은 로컬 모델은 유용하지만 취약하므로, 런타임이 선택지를 좁히고 실패를 관리해야 합니다.

> 작은 모델에는 작은 프롬프트가 아니라 작은 모델용 런타임이 필요합니다.

따라서 이 프로젝트의 주요 제품은 프롬프트 묶음이 아니라 다음을 책임지는 로컬 런타임입니다.

- 모델 설치와 캐시 관리
- 로컬 추론 백엔드 실행
- 저장소 컨텍스트 압축
- 도구 권한 정책
- 패치 생성과 검증
- 재시도 제어
- 한국어 전용 최종 응답 검증

## 대상 사용자

- 한국어 사용자
- Claude Code / Codex 같은 구독형 코딩 에이전트가 부담스러운 사용자
- 16 GB RAM 수준의 저사양 또는 중급 노트북 사용자
- 코드와 모델 실행을 로컬에서 처리하고 싶은 사용자
- 로컬 LLM 도구를 자세히 모르지만 코딩 도움은 받고 싶은 사용자

초기 지원 우선순위는 macOS와 Windows입니다. Linux는 이후 지원하거나 기여 기반으로 확장합니다.

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

## CLI 초안

```sh
rpotato init
rpotato chat
rpotato run "이 에러 고쳐줘"
rpotato model list
rpotato model install qwen3.5-4b
rpotato doctor
rpotato config
```

초기 흐름은 명확해야 합니다.

1. 사용자가 `rpotato init` 또는 `rpotato model install`을 실행한다.
2. CLI가 OS, 아키텍처, RAM, 디스크 여유 공간을 확인한다.
3. 적합한 모델을 추천한다.
4. 사용자가 다운로드를 승인한다.
5. CLI가 이어받기 가능한 방식으로 모델을 다운로드한다.
6. 해시를 검증한다.
7. 로컬 설정에 모델을 등록한다.
8. 추론 백엔드를 시작하거나 기존 프로세스를 재사용한다.

모델 가중치는 CLI 설치 파일에 포함하지 않습니다.

## 기술 방향

MVP의 기본 결정은 다음과 같습니다.

- 구현 언어: Rust
- 기본 백엔드: `llama.cpp`
- 모델 포맷: GGUF
- 실행 방식: 관리형 `llama-server` sidecar
- 기본 모델 후보: `Qwen3.5-4B` 계열 quantized GGUF
- 비교 모델 후보: `Gemma 4 E4B`

자세한 구조는 [docs/architecture.md](docs/architecture.md)를 따릅니다.

## 중요 제약

사용자에게 보이는 자연어 출력은 한국어여야 합니다. 코드 블록, 파일 경로, 명령어, 원문 로그처럼 의미상 보존해야 하는 내용은 예외로 다루되, 최종 설명문이 영어, 중국어, 일본어를 무작위로 섞어서는 안 됩니다.

모델 선택만으로 이 요구사항을 만족할 수 없으므로 런타임에서 다음을 강제합니다.

- 최종 응답의 언어 누수 검사
- 코드 블록과 자연어 설명 분리
- 혼합 언어 감지 시 1회 재생성
- 재생성 실패 시 한국어 오류로 fail closed

## 현재 상태

이 저장소는 아직 제품 정의와 구조 설계 단계입니다. 구현 scaffold는 아직 만들지 않았습니다.

다음 구현 전 작업:

- 신뢰할 GGUF 모델 artifact 확정
- `Qwen3.5-4B` 후보와 `Gemma 4 E4B` 후보 벤치마크
- `rpotato init` 설정 파일 형식 확정
- sidecar 프로세스 생명주기 설계 상세화

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
- [docs/release.md](docs/release.md): 릴리즈 정책
- [docs/model-manifest.md](docs/model-manifest.md): 모델 manifest schema
- [docs/model-licenses.md](docs/model-licenses.md): 모델 라이선스 추적
- [docs/backend-adapters.md](docs/backend-adapters.md): backend adapter 경계
- [docs/command-policy.md](docs/command-policy.md): 파일/명령 승인 정책
- [docs/korean-output-guard.md](docs/korean-output-guard.md): 한국어 출력 guard
- [docs/threat-model.md](docs/threat-model.md): 위협 모델
- [docs/benchmarks.md](docs/benchmarks.md): benchmark 실행 계획
