# 문서

이 디렉터리에는 `rolling-potato`의 한국어 문서를 보관합니다.

[한국어 README](../../README.ko.md) · [English documentation](../README.md)

## 목적별 읽기 순서

| 목적 | 읽기 순서 |
| --- | --- |
| 제품 이해 | [계획](PLAN.md) → [로드맵](ROADMAP.md) → [현재 기능](current-capabilities.md) |
| 코드 이해 | [코드 아키텍처](code-architecture.md) → [런타임 아키텍처](runtime-architecture.md) → [상태 수명주기](state-lifecycle.md) |
| 개발·릴리즈 | [개발](development.md) → [릴리즈](release.md) → [릴리즈 열차](release-train.md) |
| 모델 평가 | [출처 정책](model-source-policy.md) → [Manifest](model-manifest.md) → [평가](model-eval.md) → [벤치마크](benchmarks.md) |

## 제품과 사용 경험

- [제품 계획](PLAN.md) — 의도, 대상 사용자, 제품 형태, MVP 방향
- [버전 로드맵](ROADMAP.md) — 릴리즈 기록과 다음 버전 규칙
- [디자인 기준](DESIGN.md) — CLI, TUI, monitoring 사용 경험
- [현재 기능](current-capabilities.md) — 구현 영역, 진입점, 알려진 경계
- [MVP 인수 기준](mvp.md) — 첫 유효 제품 계약
- [CLI 출력 스타일](cli-output-style.md) — 간결하고 근거 중심인 terminal 출력
- [용어집](glossary.md) — 프로젝트 표준 용어

## 아키텍처와 상태

- [아키텍처](architecture.md) — 전체 제품·런타임 경계
- [코드 아키텍처](code-architecture.md) — module ownership과 dependency 방향
- [런타임 아키텍처](runtime-architecture.md) — surface, core, adapter, artifact
  계층
- [상태 수명주기](state-lifecycle.md) — canonical state, projection, recovery,
  resume
- [세션 메모리와 context 계획](session-memory-context-plan.md) — canonical
  conversation 소유권, model-window budget, recall, compaction
- [온톨로지 런타임](ontology-runtime.md) — typed project knowledge와
  source-pointer reread
- [관측성](observability.md) — ledger, SQLite projection, metric, retention

## 런타임 기능

- [백엔드 어댑터](backend-adapters.md)
- [명령 정책](command-policy.md)
- [훅](hooks.md)
- [스킬](skills.md)
- [서브에이전트](subagents.md)
- [팀 런타임](team-runtime.md)
- [TUI](tui.md)
- [플러그인 어댑터](plugin-adapters.md)
- [한국어 출력 guard](korean-output-guard.md)

## 모델과 평가

- [모델 출처 정책](model-source-policy.md)
- [모델 manifest](model-manifest.md)
- [모델 knowledge base](model-knowledge-base.md)
- [모델 라이선스](model-licenses.md)
- [모델 평가](model-eval.md)
- [벤치마크](benchmarks.md)

모델 이름, 라이선스, 성능, memory 적합성, backend compatibility, multimodal
claim에는 인용하거나 로컬에서 측정한 근거가 필요합니다. 확인되지 않은
claim은 명시적으로 `unverified` 상태를 유지합니다.

## 개발과 릴리즈

- [개발](development.md)
- [릴리즈 정책과 workflow](release.md)
- [릴리즈 열차](release-train.md)
- [릴리즈 노트](RELEASE_NOTES.md)
- [릴리즈 노트 template](release-notes-template.md)
- [v0.29 보정 계획](v0.29-correction-plan.md) — 과거 release-blocking 보정
  기록

## 보안, 프라이버시, 운영

- [위협 모델](threat-model.md)
- [보안 정책](SECURITY.md)
- [프라이버시 정책](PRIVACY.md)
- [운영 정책](GOVERNANCE.md)
- [Maintainer](MAINTAINERS.md)

## Maintainer 참고 문서

- [Agent handoff](../../HANDOFF.md)
- [Agent 실행 회고](../agent-retrospectives.md)
- [저장소 agent 지침](../../AGENTS.md)
