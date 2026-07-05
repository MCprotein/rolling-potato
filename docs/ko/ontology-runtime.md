# 온톨로지 Runtime

온톨로지는 `rolling-potato`의 핵심 runtime memory입니다.

작은 모델은 프로젝트 구조, 용어, 책임 경계, workflow, invariant를 매번 prompt에서 복구하기 어렵습니다. Runtime core가 이를 온톨로지로 관리해 모델의 자유도를 줄이고 반복 실수를 막습니다.

## Canonical Store

canonical ontology는 사람이 직접 수정하는 YAML 파일이 아니라 runtime data model입니다.

`anamnesis`는 library/config lifecycle tool이라 사람이 project memory를 review, 수정, commit해야 하므로 YAML을 사용합니다. `rolling-potato`는 agent runtime입니다. Runtime은 ontology를 provenance와 query index가 붙은 typed graph record로 저장하고, 사람이 확인해야 할 때만 그 graph를 TUI, HTML, JSON, YAML 같은 view로 렌더링해야 합니다.

여기서 canonical은 runtime이 계획, context retrieval, invariant 검사, session resume, prompt compile에 신뢰하는 data model과 storage layer를 뜻합니다. `rolling-potato`의 목표 canonical shape는 다음입니다.

- entity, relationship, flow, invariant, ownership, decision, open question을 표현하는 typed graph record
- 모든 semantic claim에 연결되는 source reference와 evidence record
- audit/replay를 위한 append-only ledger event
- 빠른 local query, TUI view, diagnostics, benchmark report를 위한 SQLite 기반 index/projection
- JSON/YAML fixture와 향후 interchange format을 위한 import/export serializer

YAML과 JSON은 직렬화 포맷이지 ontology 자체가 아닙니다. fixture seed, debug snapshot export, migration 보조 수단으로는 쓸 수 있지만 runtime reasoning의 source of truth가 되면 안 됩니다.

RDF, OWL, JSON-LD, Turtle, SHACL도 현재 기본 canonical store가 아닙니다. 나중에 semantic-web tooling, external reasoner, 표준 ontology exchange가 실제 요구사항이 되면 import/export 또는 interoperability target으로 추가할 수 있습니다. 그 요구가 증명되기 전까지 runtime은 local agent-loop latency, source-backed claim, 예측 가능한 query에 최적화된 compact typed graph를 우선합니다.

## 소형 모델 적합성 Gate

온톨로지 표현 방식은 형식적으로 더 풍부한 포맷이 소형 모델에 도움이 될 것이라고 가정하지 말고, 2B-4B 모델의 실제 행동을 측정해서 결정해야 합니다.

Prompt에 들어가는 ontology view를 고정하기 전에 같은 canonical store에서 최소한 다음 표현을 비교합니다.

- compact typed graph summary
- source-pointer-first JSON slice
- 짧은 triple-style relationship list
- exporter가 생긴 경우 RDF/OWL/JSON-LD export view
- ontology 없이 repository search만 쓰는 baseline

각 후보 view는 소형 모델이 다음을 할 수 있는지로 평가합니다.

- task에 맞는 entity와 relationship 식별
- invariant와 ownership boundary 준수
- action 전에 source pointer를 원본 파일 read로 승격
- weak claim이나 superseded claim을 confirmed fact처럼 취급하지 않음
- 최종 한국어 응답 품질 유지
- token, latency, memory budget 준수

formal export format이 같은 budget에서 모델 행동을 개선하면 supported view가 될 수 있습니다. 외부 tool interchange에만 유리하다면 runtime canonical store가 아니라 import/export surface로 남깁니다.

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

## Core Graph Shape

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

이 JSON 모양은 runtime graph contract를 설명하기 위한 문서 형태이지 canonical storage format이 아닙니다. 구현은 stable ID, provenance, replayability를 보존하는 한 table이나 graph index로 normalize할 수 있습니다.

각 semantic assertion은 다음을 가져야 합니다.

- `id`
- `kind`
- `statement`
- `sourceRefs`
- `confidence`
- `status`
- `updatedAt`
- 이전 assertion을 대체한다면 `supersedes`

Runtime record는 다음도 보존해야 합니다.

- Layer A fact의 deterministic source hash 또는 observation hash
- provenance: generator, model/backend, command, session, 관련 ledger event
- monorepo나 nested project root를 위한 scope path
- 새 source observation이 기존 claim과 충돌할 때의 conflict/drift state

## `anamnesis`에서 흡수할 개념

`rolling-potato`가 흡수해야 하는 것은 lifecycle 설계이지 YAML을 canonical storage로 쓰는 방식이 아닙니다.

- Layer A: deterministic local introspection이 검증 가능한 fact를 만든다.
- Layer B: semantic enrichment는 source evidence가 있을 때만 relationship, flow, operational rule, open question을 만든다.
- regenerable fact와 reviewed semantic은 분리해 refresh가 curated meaning을 덮어쓰지 못하게 한다.
- re-run은 stable ID 기준으로 merge하고, 새 claim은 append하며, 대체는 `supersedes`로 표시하고, 근거가 약하면 open question으로 둔다.
- context index와 resume bundle은 source pointer와 snippet을 제공할 뿐 authoritative source read를 대체하지 않는다.
- diagnostics는 missing fact, stale observation, source 없는 claim, duplicate ID, contradictory relationship, superseded entry가 current처럼 쓰이는 상태를 보고해야 한다.

Runtime 버전의 이 설계는 위 개념을 ontology graph와 ledger에 저장하고, 사람이 읽을 수 있는 view는 필요할 때 렌더링합니다.

## Runtime 사용 위치

### `rpotato init`

- project identity 생성
- ontology store/schema 생성과 Layer A fact seed 작성
- project-local `.rpotato/` state layout 준비
- ontology gap diagnostics 표시

### `rpotato run`

- user request와 관련된 ontology entry를 찾는다.
- source pointer를 원본 파일 read로 승격한다.
- prompt compiler에 필요한 최소 entry만 전달한다.
- action candidate가 invariant를 깨뜨리는지 검사한다.
- 적용/검증 evidence를 ledger에 기록한다.
- source-backed event를 통해서만 ontology observation을 갱신한다.

### `rpotato doctor`

- Layer A freshness를 확인한다.
- stale source hash를 표시한다.
- source 없는 Layer B claim을 경고한다.
- open question을 표시한다.
- graph conflict, duplicate stable ID, superseded current entry를 보고한다.

### TUI와 Report

- prompt text가 아니라 store에서 ontology summary를 렌더링한다.
- entity, relationship, flow, invariant, source ref, confidence, drift state를 보여준다.
- 사용자나 agent가 authoritative file을 열 수 있도록 source pointer를 노출한다.
- HTML report는 같은 store/export data를 읽는 local view로만 둔다.

## 금지 사항

- source ref 없는 semantic claim 확정
- snippet만 보고 patch 적용
- 모델 output을 ontology에 바로 confirmed로 기록
- 오래된 source hash를 최신 사실처럼 사용
- ontology를 거대한 prompt dump로 사용하는 방식
- YAML/JSON/RDF/OWL export를 runtime store와 ledger보다 더 authoritative하게 취급하는 방식
- regenerated deterministic fact로 reviewed semantic claim을 덮어쓰는 방식

## Stop Gate와의 관계

Stop gate는 다음을 확인합니다.

- 요청된 변경이 source file과 연결되어 있는가
- 관련 invariant가 깨지지 않았는가
- 검증 evidence가 있는가
- open question이 완료 판정을 막는가
- 최종 보고가 한국어 guard를 통과했는가

온톨로지는 모델을 똑똑하게 보이게 만드는 장식이 아니라, runtime이 작은 모델의 작업 공간을 좁히는 안전 장치입니다.
