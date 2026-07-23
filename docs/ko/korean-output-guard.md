# 한국어 출력 Guard

사용자에게 보이는 자연어 최종 문장은 한국어여야 합니다. 숫자와 수식처럼 언어가
아닌 답변은 그대로 유효합니다.

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
3. 한글 문자가 없어도 숫자, 수식, 문장부호, inline code, path, command token은 허용한다.
4. 한국어 문장 안의 일반적인 영어 기술 용어는 허용한다.
5. 영어, 중국어, 일본어 문장 전체가 섞이는 누수를 탐지한다.
6. 누수가 있으면 사실, code, 숫자, URL을 보존한 채 한국어로 한 번 다시 작성한다.
7. 다시 실패하면 안전한 한국어 projection을 유지하거나 한국어 오류를 표시한다.

## 허용 예외

허용 가능한 예:

- `cargo test`
- `README.md`
- `Qwen3.5-4B`
- 확인된 license identifier
- `llama.cpp`
- 원문 error log 인용
- `15`, `3.14`, `x = 3`
- 한국어 설명 안의 일반적인 기술 용어

허용하지 않는 예:

- 설명문 전체가 영어로 전환됨
- 불필요한 중국어/일본어 문장 혼입
- 한국어 답변에 외국어 문단 전체가 끼어드는 경우

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
- 숫자·수식만 있는 답변 허용
- 일반적인 기술 용어가 포함된 한국어 문장 허용
- 영어 설명문 차단
- 중국어 문장 차단
- 일본어 문장 차단
- 재생성 후 통과
- 재생성 후 실패 시 fail closed
