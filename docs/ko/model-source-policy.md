# 모델 출처 정책

모델 관련 정보는 추측으로 기록하지 않습니다.

모델 이름, 라이선스, 성능, context length, GGUF artifact, quantization, backend 호환성, RAM 요구량, multimodal 지원 여부, 한국어/코드 성능 평가는 반드시 명확한 근거와 출처가 있어야 합니다.

## 원칙

- 출처가 없으면 `미확정`으로 표시한다.
- 출처가 없는 모델은 기본 추천 모델로 확정하지 않는다.
- "더 좋다", "지원한다", "실행 가능하다", "라이선스가 무엇이다" 같은 표현은 근거 없이 쓰지 않는다.
- 사용자 의도로 받은 모델명도 제품 사실이 아니라 `후보`로만 기록한다.
- 모델 관련 문서에는 출처 URL, 확인 날짜, 확인한 claim을 함께 남긴다.
- backend 이름과 모델 family를 섞지 않는다. `llama.cpp`는 backend이며, 이 프로젝트의 모델 후보는 별도로 기록된 `Qwen`과 `Gemma` 후보만 의미한다.

## 허용 출처

우선순위가 높은 출처:

- upstream 공식 model card
- upstream 공식 repository
- upstream 공식 license 문서
- artifact provider의 공식 배포 페이지
- release asset checksum
- `llama.cpp` 공식 호환성 문서 또는 issue/release note
- 이 저장소에서 실행한 benchmark 결과

보조 출처:

- 신뢰 가능한 maintainer의 GGUF 변환 repo
- 재현 가능한 benchmark log
- checksum이 포함된 release note

허용하지 않는 근거:

- 모델 이름만 보고 한 추측
- leaderboard만 보고 제품 적합성을 단정하는 표현
- 출처 없는 블로그/커뮤니티 요약
- "대체로 그럴 것" 수준의 추정
- 다른 모델 family의 정보를 끌어온 추정
- backend 이름만 보고 Meta Llama 같은 다른 모델 family의 license나 정책을 적용한 추정

## 문서 형식

모델 관련 claim을 확정할 때는 다음 정보를 남깁니다.

```text
Claim: <확정하려는 내용>
Source: <URL>
Checked-at: <YYYY-MM-DD>
Evidence: <확인한 문서/필드/결과 요약>
Status: confirmed | rejected | superseded
```

예시:

```text
Claim: <model-id> artifact의 license는 <license-id>이다.
Source: <official-model-card-or-artifact-url>
Checked-at: 2026-06-22
Evidence: model card의 license field와 artifact page의 license field가 일치함.
Status: confirmed
```

## manifest 요구사항

manifest에 들어가는 모델 항목은 최소한 다음 필드를 source-backed 상태로 채워야 합니다.

- upstream model name
- upstream URL
- artifact URL
- artifact provider
- license
- SHA-256
- file size
- quantization
- backend compatibility
- recommended RAM 근거

하나라도 확인되지 않으면 해당 모델은 `recommended`가 아니라 `candidate` 또는 `unverified` 상태로 둡니다.

## 금지 표현

출처 없이 쓰면 안 되는 표현:

- "기본 모델"
- "권장 모델"
- "한국어/코드에 더 적합"
- "multimodal 지원"
- "vision 가능"
- "Apache-2.0"
- "16 GB에서 실행 가능"
- "llama.cpp에서 지원"

필요하면 다음처럼 낮춰 씁니다.

- "평가 후보"
- "사용자 의도에 따라 우선 검토할 후보"
- "출처 확인 전 미확정"
- "benchmark 전까지 기본값으로 확정하지 않음"
