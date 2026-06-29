# CLI Output Style

`rpotato` CLI 출력은 한국어를 기본으로 합니다.

## 원칙

- 첫 줄은 명령 결과의 주제를 짧게 표시한다.
- 이후 줄은 `- label: value` 형식으로 정리한다.
- 아직 실행하지 않는 destructive action은 반드시 dry-run 또는 plan으로 표시한다.
- 파일 경로, command, model id, plugin id는 원문을 유지한다.
- 실패 이유는 한국어로 쓰되, error code와 원문 log line은 보존할 수 있다.
- 다음 단계가 있으면 마지막 줄에 하나만 적는다.

## 예시

```text
monitor 상태
- observability store: /path/to/observability.sqlite
- runtime ledger: /path/to/runtime-ledger.jsonl
- raw prompt/source 저장: 기본 비활성
- 현재 상태: SQLite projection은 Phase 2에서 생성 예정
```

## 금지

- 성공처럼 보이는 placeholder 출력
- 삭제 실행 없이 삭제된 것처럼 말하기
- 검증되지 않은 모델/license/benchmark claim
- remote plugin marketplace나 registry를 사용 가능한 것처럼 안내
