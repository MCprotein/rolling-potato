# Development

이 문서는 `rolling-potato` 개발 환경과 검증 흐름을 정의합니다.

## 현재 상태

현재 저장소는 제품 정의와 초기 Rust runtime/CLI scaffold 단계입니다.

구현된 첫 경계:

- `rpotato doctor`
- `rpotato backend doctor`
- `rpotato cache status`
- `rpotato config`
- `rpotato init`
- `rpotato state`
- `rpotato cancel`
- `rpotato monitor status`
- `rpotato monitor models`
- `rpotato model list`
- `rpotato model install <id>`
- `rpotato plugin import --from codex <local-path> --dry-run`
- `rpotato plugin import --from claude-code <local-path> --dry-run`
- `rpotato plugin list`
- `rpotato uninstall --keep-cache`
- `rpotato uninstall --purge-cache`

모델/backend 다운로드는 아직 활성화하지 않았습니다. 검증된 manifest가 없으면 runtime core가 다운로드를 차단하고 CLI surface가 그 이유를 표시해야 합니다.

Plugin source snapshot, persistent registry, enable/disable/remove는 아직 활성화하지 않았습니다. 현재 plugin path는 local directory dry-run 검사와 remote source rejection만 수행합니다.

## 기술 스택

- Language: Rust
- CLI parser: std 기반 수동 parser
- Runtime: CLI surface와 runtime core를 분리하는 방향
- Required capabilities: hooks, skills, subagents, team runtime, TUI, local plugin adapter
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

## 기본 검증 명령

다음 명령을 기본 검증으로 사용합니다.

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

CLI smoke test 예시:

```sh
cargo run -- doctor
cargo run -- init
cargo run -- config
cargo run -- state
cargo run -- monitor status
cargo run -- monitor models
cargo run -- model list
cargo run -- uninstall --dry-run --purge-cache
```

최종 binary 명령은 `rpotato`입니다.

## 코드 구조 방향

현재 scaffold와 예정 module 경계:

- `cli`: command parsing and output
- `runtime`: state, policy, ontology, agent loop orchestration
- `hooks`: lifecycle control points
- `skills`: reusable runtime capabilities
- `plugins`: local Codex/Claude Code plugin import, inspect, validate, enable/disable
- `config`: local config paths and serialization
- `model`: manifest, download, checksum, registry
- `backend`: backend adapter trait and `llama.cpp` implementation
- `repo`: project file discovery and context packing
- `ontology`: Layer A facts and Layer B semantic ontology
- `agent`: planner/executor/verifier/reporter loop
- `subagent`: bounded worker lifecycle
- `team`: staged multi-agent coordination
- `tui`: terminal interactive surface
- `policy`: command and file permission rules
- `patch`: diff rendering and apply flow
- `evidence`: ledger, verification evidence, stop gate
- `observability`: SQLite projection, token/resource metrics, monitoring export
- `guard`: Korean output validation

## 문서 변경 검증

문서만 변경한 경우:

```sh
rg -n "<확인할-오타-패턴>" README.md docs *.md
```

링크가 추가되면 파일 존재 여부를 확인합니다.

Plugin adapter 변경 시 추가로 확인합니다.

- local directory import만 허용되는지
- remote URL, marketplace, registry, catalog, mirror source가 거부되는지
- shell, `bin/`, MCP, background, remote connector, file write capability가 기본 차단되는지
- import/enable/remove 이벤트가 ledger에 기록되는지

## 커밋과 푸시

작업 단위가 검증되면 Conventional Commit 형식으로 커밋합니다.

```text
docs(project): add open source operating docs
feat(cli): scaffold command router
fix(model): reject checksum mismatch
```

이 저장소의 기본 원격은 `origin`이고 기본 branch는 `main`입니다.
