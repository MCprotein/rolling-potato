# Development

이 문서는 `rolling-potato` 개발 환경과 검증 흐름을 정의합니다.

## 현재 상태

현재 저장소는 제품 정의와 문서화 단계입니다. Rust scaffold는 아직 생성하지 않았습니다.

## 예정 기술 스택

- Language: Rust
- CLI parser: undecided
- Backend: managed `llama.cpp` sidecar
- Model format: GGUF
- Primary OS targets: macOS, Windows

## 개발 환경

필수 도구:

- Git
- Rust stable toolchain
- platform-specific C/C++ runtime needed by `llama.cpp`

권장 도구:

- `rustfmt`
- `clippy`
- GitHub CLI

## scaffold 이후 기본 명령

Rust workspace가 생기면 다음 명령을 기본 검증으로 사용합니다.

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

CLI smoke test 예시:

```sh
cargo run -- doctor
cargo run -- model list
```

최종 binary 명령은 `rpotato`입니다.

## 코드 구조 예정

예상 module 경계:

- `cli`: command parsing and output
- `config`: local config paths and serialization
- `model`: manifest, download, checksum, registry
- `backend`: backend adapter trait and `llama.cpp` implementation
- `repo`: project file discovery and context packing
- `agent`: planner/executor/verifier/reporter loop
- `policy`: command and file permission rules
- `patch`: diff rendering and apply flow
- `guard`: Korean output validation

## 문서 변경 검증

문서만 변경한 경우:

```sh
rg -n "<확인할-오타-패턴>" README.md docs *.md
```

링크가 추가되면 파일 존재 여부를 확인합니다.

## 커밋과 푸시

작업 단위가 검증되면 Conventional Commit 형식으로 커밋합니다.

```text
docs(project): add open source operating docs
feat(cli): scaffold command router
fix(model): reject checksum mismatch
```

이 저장소의 기본 원격은 `origin`이고 기본 branch는 `main`입니다.
