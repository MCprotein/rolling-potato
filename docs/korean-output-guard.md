# Korean Output Guard

User-facing final natural-language output must be Korean.

## Goal

Small models may mix English, Chinese, or Japanese even when instructed in Korean. This requirement is enforced by a runtime guard instead of model choice alone.

## Scope

Mandatory:

- final report
- error message
- safety warning
- model install guidance
- doctor result explanation

Relaxed:

- raw command output
- code block
- file path
- package name
- model name
- exact upstream license name

## Processing Steps

1. Split the response by Markdown block.
2. Exclude fenced code blocks from checks.
3. Treat inline code, paths, and command tokens through an allowlist.
4. Detect English, Chinese, or Japanese leakage in natural-language sentences.
5. If leakage is detected, regenerate once with stricter Korean-only instruction.
6. If it still fails, fail closed with a Korean error message.

## Allowed Exceptions

Allowed examples:

- `cargo test`
- `README.md`
- `Qwen3.5-4B`
- confirmed license identifier
- `llama.cpp`
- quoted original error log

Not allowed:

- whole explanation switching to English
- unnecessary Chinese/Japanese sentences
- final report headings such as "Summary" or "Next steps"

## Failure Message

When the guard fails, report to the user only in Korean.

Example:

```text
응답 언어 검증에 실패했습니다. 출력이 한국어 기준을 만족하지 않아 결과를 표시하지 않았습니다.
```

## Test Requirements

Test fixtures:

- pure Korean passes
- English inside code block allowed
- file path allowed
- English explanation blocked
- Chinese sentence blocked
- Japanese sentence blocked
- pass after regeneration
- fail closed after regeneration failure
