# 세션 메모리와 Context 계획

상태: `feature/session-memory-context`에 구현됨. Release 게시는 범위에 포함하지 않습니다.

## 목표

`rpotato`를 재시작해도 완료된 대화를 기억하되 소형 local model이 session 전체를
매 요청마다 다시 읽지 않게 합니다. 고정 4,096-token fallback 대신 선택 모델의
선언된 context window를 사용하고, 현재 요청의 우선권과 감사 가능한 canonical
history를 함께 보존합니다.

## 범위 밖

- hidden reasoning이나 raw backend response 저장
- 생성된 summary를 source of truth로 사용
- vector database, hosted memory service, 검색 dependency 추가
- 매 요청마다 model token 전체를 채우기
- 불확실한 tool, command, backend request 자동 재실행

## 역할과 책임

### Canonical Conversation Store

TUI conversation adapter는 coding·agent workflow를 포함한 모든 성공한 대화형
요청의 완료된 user/assistant pair를 append-only로 저장합니다. Workflow
transcript는 별도의 실행 감사 stream으로 유지합니다. 복원할 때 완료되지 않은
tail은 제외하고, 매 요청마다 session 전체를 다시 읽는 대신 제한된 in-memory
prompt view를 유지합니다. `/clear`는 감사 이력을 삭제하지 않고 고유한 causal
reset boundary를 기록합니다.

### Dialogue Recall Policy

`runtime_core::knowledge::recall`은 dependency-free 정책 module입니다.

- 선호, 정정, identity statement 같은 typed user-memory 후보를 찾습니다.
- 오래된 완료 pair를 deterministic lexical overlap과 recency로 정렬합니다.
- pair integrity와 시간 순서 rendering을 보존합니다.
- 가장 최근 pair가 너무 커도 어느 한 role을 버리지 않고 함께 줄입니다.

이 정책은 소형 model에서도 예측 가능하고 저렴하게 실행되도록 설계했습니다.

### Prompt Assembly

`runtime_core::knowledge::prompt`는 token 배분과 section 순서를 소유합니다.
Stable instruction을 가장 앞에 두고 typed memory, query recall, 최근 대화,
attachment에 별도 상한을 적용합니다. 현재 사용자 요청과 response cue는 항상
가장 뒤에 둡니다.

과거 대화와 attachment payload는 신뢰하지 않는 data로 encoding합니다. Context
정보는 제공할 수 있지만 stable instruction이나 현재 요청을 덮어쓸 수 없습니다.
Agent/workflow prompt도 같은 effective runtime context를 사용하며 output/runtime
reserve를 먼저 확보한 뒤 resume·repository section을 남은 예산에 맞춥니다.

### Backend Reconciliation

Inference adapter는 설정된 runtime specification을 제공합니다. TUI backend
adapter는 desired/observed model path, context length, vision projector를
비교합니다. Drift가 있으면 통제된 restart를 수행하고 다시 정확히 일치하는지
확인한 뒤 요청을 허용합니다.

### Resume와 Compaction

Canonical transcript가 계속 권위입니다. Compaction은 typed incremental
checkpoint와 완료된 최근 exchange의 제한된 tail을 만듭니다. Record·token
상한에서는 오래된 exchange 전체를 제거하며, 최신 exchange 하나가 너무 크면
user/model 경계를 보존하고 tool detail을 줄입니다. 파생 artifact가 invalid 또는
stale이면 canonical recent history 경로로 fallback합니다.

## Model-Window 정책

- Model context length는 정확히 ready인 backend가 실행 중이면 그 runtime에서,
  그렇지 않으면 선택되고 검증된 manifest에서 가져옵니다.
- 누락된 context 값을 임의로 4,096으로 바꾸지 않습니다.
- Prompt output reserve는 명시하며 runtime reserve는 512~4,096 token으로 확장됩니다.
- Typed-memory 목표는 사용 가능한 input의 1/8, 최대 8,192 token입니다.
- Query-recall 목표는 1/4, 최대 32,768 token입니다.
- 최근 대화 목표는 1/4, 최대 16,384 token입니다.
- Resume transcript budget은 model window의 1/8이며 estimated token
  512~16,384와 turn 8~64 사이로 제한합니다.
- 자동 compaction은 측정 사용량 75%에서 시작해 model window의 40%를 목표로 합니다.
- Compaction recent tail은 완료된 exchange 2~8개와 estimated token
  512~16,384 사이에서 확장됩니다.
- Repository source retrieval은 의도적으로 더 작은 공유 budget인 pointer
  4개·3,200자를 유지합니다.

이 상한은 large-context model의 capability를 사용하면서도 소형 model prompt가
불필요하게 길고 산만해지는 것을 막습니다.

## 안전 불변식

1. 완료된 user/assistant pair만 복원하고 recall합니다.
2. 현재 사용자 요청은 prompt data section의 마지막입니다.
3. 과거 memory와 attachment는 명시적으로 신뢰하지 않습니다.
4. Compaction은 canonical transcript를 다시 쓰거나 삭제하지 않습니다.
5. 파생 state는 command, 파일 변경, factual claim을 승인하지 못합니다.
6. Runtime ready는 desired/observed specification의 정확한 일치를 요구합니다.
7. Manifest context 누락은 추측한 default가 아니라 조치 가능한 오류입니다.
8. Hidden reasoning을 memory로 저장하거나 표시하지 않습니다.

## 인수 근거

- 재시작은 완료된 pair를 복원하고 완료되지 않은 tail을 제외합니다.
- `/clear`는 audit stream을 보존하면서 이전 대화를 화면과 prompt에서 분리합니다.
- Coding·agent workflow 답변은 별도 workflow 감사 기록을 유지하면서 canonical
  conversation에도 남아 재시작 후 복원됩니다.
- 여러 reset 사이에서 같은 질문을 반복해도 causal record가 고유하며, reset 전
  orphan user와 이후 model turn을 잘못 결합하지 않습니다.
- Typed memory와 query recall은 완료된 pair와 시간 순서를 보존합니다.
- Prompt assembly는 선언된 model input budget 안에 있고 현재 요청으로 끝납니다.
- Agent/workflow prompt도 최대 resume·repository 입력이 있을 때 1,024-token
  active runtime window 안에 머뭅니다.
- 4K와 131K manifest는 서로 다른 resume·compaction limit을 만듭니다.
- Context 또는 projector가 다른 ready backend는 restart합니다.
- Compaction은 token·record·artifact ceiling 안에서 완료된 exchange를 보존합니다.
- Candidate branch에서 TUI, context, recall, compaction, backend reconciliation,
  architecture contract test가 통과합니다.

## 향후 확장 지점

- Local 측정에서 비용 대비 품질 향상이 확인될 때만 recall policy interface 뒤의
  deterministic ranking을 교체합니다.
- Canonical transcript 저장 방식을 바꾸지 않고 typed memory category를 확장합니다.
- 같은 fail-closed model-window 계약을 유지하며 budget estimator 뒤에
  tokenizer별 계산을 추가할 수 있습니다.
- 사용자에게 보이는 memory 조회와 선택적 forgetting은 silent transcript
  mutation이 아니라 명시적인 canonical event로 추가합니다.
