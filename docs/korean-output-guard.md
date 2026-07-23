# Korean Output Guard

User-facing final natural-language sentences must be Korean. Language-neutral
answers such as numbers and formulas remain valid.

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
3. Accept language-neutral numbers, formulas, punctuation, inline code, paths,
   and command tokens without requiring a Hangul character.
4. Allow ordinary English technical terms inside an otherwise Korean sentence.
5. Detect full English, Chinese, or Japanese sentence leakage.
6. If leakage is detected, rewrite once in Korean while preserving facts, code,
   numbers, and URLs.
7. If it still fails, keep any safe Korean projection or fail with a Korean error.

The v0.29.0 runtime implements this contract as the reusable
`korean_guard` module. Patch-loop errors and deterministic terminal reports use
the same validator. Structured IDs, hashes, paths, command lines, and captured
stdout/stderr fields are treated as technical data; natural-language lines remain
subject to the Korean/Chinese/Japanese/English leakage checks.

## Allowed Exceptions

Allowed examples:

- `cargo test`
- `README.md`
- `Qwen3.5-4B`
- confirmed license identifier
- `llama.cpp`
- quoted original error log
- `15`, `3.14`, or `x = 3`
- ordinary technical terms inside Korean prose

Not allowed:

- whole explanation switching to English
- unnecessary Chinese/Japanese sentences
- a full foreign-language paragraph inserted into a Korean answer

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
- number/formula-only answer allowed
- Korean sentence with ordinary technical terms allowed
- English explanation blocked
- Chinese sentence blocked
- Japanese sentence blocked
- pass after regeneration
- fail closed after regeneration failure
