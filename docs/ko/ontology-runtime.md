# 온톨로지 Runtime

온톨로지는 `rolling-potato`의 핵심 runtime memory입니다.

작은 모델은 프로젝트 구조, 용어, 책임 경계, workflow, invariant를 매번 prompt에서 복구하기 어렵습니다. Runtime core가 이를 온톨로지로 관리해 모델의 자유도를 줄이고 반복 실수를 막습니다.

## 목표

- 프로젝트 의미 구조를 runtime asset으로 유지한다.
- 작은 모델이 추측으로 복구해야 하는 정보를 줄인다.
- context packing을 source pointer 중심으로 좁힌다.
- agent action이 프로젝트 invariant를 깨뜨리는지 검사한다.
- 세션이 바뀌어도 작업 의도와 판단 근거를 보존한다.

## 두 계층

### 계층 A: deterministic fact

Runtime이 직접 수집할 수 있는 사실입니다.

예:

- file path
- file hash
- package manager
- test script
- build script
- public symbol
- entrypoint 후보
- generated/vendor exclusion

Layer A는 confidence 대신 freshness와 source hash를 기록합니다.

### 계층 B: semantic ontology

프로젝트 의미 구조입니다.

예:

- domain entity
- relationship
- ownership
- invariant
- workflow
- architecture boundary
- open question
- rejected decision

Layer B는 반드시 source ref, confidence, status를 가집니다.

Status 예:

- proposed
- confirmed
- superseded
- rejected
- open_question

## 최소 Schema

```json
{
  "schemaVersion": 1,
  "entities": [],
  "relationships": [],
  "flows": [],
  "invariants": [],
  "ownership": [],
  "openQuestions": [],
  "sourceRefs": []
}
```

각 semantic entry는 다음을 가져야 합니다.

- `id`
- `kind`
- `statement`
- `sourceRefs`
- `confidence`
- `status`
- `updatedAt`

## Runtime 사용 위치

### `rpotato init`

- project identity 생성
- Layer A fact seed 작성
- project-local `.rpotato/` state layout 준비
- ontology gap diagnostics 표시

### `rpotato run`

- user request와 관련된 ontology entry를 찾는다.
- source pointer를 원본 파일 read로 승격한다.
- prompt compiler에 필요한 최소 entry만 전달한다.
- action candidate가 invariant를 깨뜨리는지 검사한다.
- 적용/검증 evidence를 ledger에 기록한다.

### `rpotato doctor`

- Layer A freshness를 확인한다.
- stale source hash를 표시한다.
- source 없는 Layer B claim을 경고한다.
- open question을 표시한다.

## 금지 사항

- source ref 없는 semantic claim 확정
- snippet만 보고 patch 적용
- 모델 output을 ontology에 바로 confirmed로 기록
- 오래된 source hash를 최신 사실처럼 사용
- ontology를 거대한 prompt dump로 사용하는 방식

## Stop Gate와의 관계

Stop gate는 다음을 확인합니다.

- 요청된 변경이 source file과 연결되어 있는가
- 관련 invariant가 깨지지 않았는가
- 검증 evidence가 있는가
- open question이 완료 판정을 막는가
- 최종 보고가 한국어 guard를 통과했는가

온톨로지는 모델을 똑똑하게 보이게 만드는 장식이 아니라, runtime이 작은 모델의 작업 공간을 좁히는 안전 장치입니다.
