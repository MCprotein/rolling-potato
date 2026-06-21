# Korean Output Guard

사용자에게 보이는 자연어 최종 출력은 한국어여야 합니다.

## 목표

작은 모델은 한국어 지시를 받아도 영어, 중국어, 일본어를 섞을 수 있습니다. 이 요구사항은 모델 선택만으로 해결하지 않고 runtime guard로 강제합니다.

## 적용 범위

반드시 적용:

- final report
- error message
- safety warning
- model install 안내
- doctor 결과 설명

완화 가능:

- raw command output
- code block
- file path
- package name
- model name
- exact upstream license name

## 처리 단계

1. 응답을 Markdown block 단위로 분리한다.
2. fenced code block은 검사에서 제외한다.
3. inline code, path, command token은 허용 목록으로 처리한다.
4. 자연어 문장에서 영어, 중국어, 일본어 누수를 탐지한다.
5. 누수가 있으면 stricter Korean-only instruction으로 한 번 재생성한다.
6. 다시 실패하면 한국어 오류 메시지로 fail closed한다.

## 허용 예외

허용 가능한 예:

- `cargo test`
- `README.md`
- `Qwen3.5-4B`
- `Apache-2.0`
- `llama.cpp`
- 원문 error log 인용

허용하지 않는 예:

- 설명문 전체가 영어로 전환됨
- 불필요한 중국어/일본어 문장 혼입
- "Summary", "Next steps" 같은 heading을 최종 보고에 사용하는 경우

## 실패 메시지

guard 실패 시 사용자에게는 한국어로만 보고합니다.

예시:

```text
응답 언어 검증에 실패했습니다. 출력이 한국어 기준을 만족하지 않아 결과를 표시하지 않았습니다.
```

## 테스트 요구

테스트 fixture:

- 순수 한국어 통과
- 코드 블록 내 영어 허용
- 파일 경로 허용
- 영어 설명문 차단
- 중국어 문장 차단
- 일본어 문장 차단
- 재생성 후 통과
- 재생성 후 실패 시 fail closed
