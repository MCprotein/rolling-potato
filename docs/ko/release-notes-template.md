# 릴리즈 노트 템플릿

모든 `rpotato` 릴리즈 노트 항목은 이 템플릿을 기준으로 작성합니다.

```markdown
## vX.Y.Z - 짧은 릴리즈 제목

릴리즈 날짜: YYYY-MM-DD

사용자가 체감할 수 있는 릴리즈 결과를 짧은 문단으로 설명합니다.

### 포함된 것

- 추가된 구체적인 동작 또는 artifact.
- 변경된 구체적인 command, workflow, document.

### 이 릴리즈에서 검증한 것

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- release policy 검사
- 관련 smoke test

### 경계

이 릴리즈가 의도적으로 포함하지 않는 것을 명시합니다.
```
