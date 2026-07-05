# 릴리즈

이 문서는 `rolling-potato` 릴리즈 정책 초안입니다.

## 릴리즈 원칙

- GitHub Releases를 기본 배포 채널로 사용합니다.
- 모델 가중치는 `rpotato` release binary에 포함하지 않습니다.
- 외부 plugin package는 `rpotato` release binary에 포함하지 않습니다.
- 외부 plugin marketplace, registry, catalog, mirror는 배포 채널로 사용하지 않습니다.
- 각 binary는 checksum과 함께 배포합니다.
- 릴리즈 노트에는 지원 OS, backend version, known issues를 포함합니다.

## 버전 정책

`rolling-potato`는 SemVer 정신을 따르지만, 1.0 이전 버전은 실용적으로 운용합니다. 기능 하나를 추가할 때마다 major version을 올리지 않습니다.

MVP 전, 그리고 첫 stable contract 전에는 `0.x.y`를 사용합니다.

- `0.x.0`: 하나의 응집된 feature 또는 milestone release
- `0.x.y`: bugfix, 문서, packaging, manifest, policy, test 수정

`0.x` 단계에서 breaking change는 MVP runtime contract로 가기 위해 필요할 때만 허용합니다. 보통 minor version을 올리고, release note에 `Breaking before 1.0`으로 명시합니다. Stable user-facing contract가 준비되기 전까지 `1.0`처럼 말하지 않습니다.

예시:

- `0.1.1`: `doctor` 수정, 문서 수정, release note 수정, packaging fix
- `0.2.0`: managed backend install plan을 사용 가능한 release surface로 추가
- `0.3.0`: verified model install flow 추가
- `0.4.0`: 첫 `rpotato run` vertical slice 추가
- `0.5.0`: TUI beta surface 추가

`1.0.0` 이후에는 SemVer를 엄격히 따릅니다.

- Patch: backward-compatible fix, 문서, packaging, manifest update, security hardening, diagnostics, test fix
- Minor: backward-compatible feature, 새 command, 새 optional flag, 새 adapter, 새 TUI panel, stable API를 제거하지 않는 fail-closed policy check
- Major: stable public contract와 호환되지 않는 변경

Stable public contract에 포함되는 것:

- CLI command 이름, flag, exit semantics, 문서화된 output schema
- config file schema
- model/backend manifest schema
- plugin manifest schema와 adapter behavior
- ledger, session, evidence, SQLite migration compatibility
- approval, resume, monitoring에 사용자가 의존하는 TUI workflow

Major release는 묶어서 드물게 냅니다. Stable behavior를 제거하기 전에 deprecation warning, alias, migration command를 우선합니다. `preview`로 명시한 experimental command, unstable schema, preview feature는 stable contract로 승격되기 전까지 major bump를 요구하지 않습니다.

문서만 바뀐 경우 release artifact를 만들지 않는 한 version bump가 필요하지 않습니다. Model metadata correction은 source-backed manifest fact만 갱신한다면 patch release로 처리합니다. 단, benchmark/runtime evidence 없이 새 model recommendation을 암시하면 안 됩니다.

## artifact 목표

초기 목표:

- macOS Apple Silicon
- macOS Intel
- Windows x86_64

추후 목표:

- Linux x86_64
- Linux ARM64

## 릴리즈 체크리스트

릴리즈 전 확인:

1. `cargo fmt --check`
2. `cargo test`
3. `cargo clippy --all-targets -- -D warnings`
4. `rpotato doctor` smoke test
5. model manifest checksum verification
6. Korean output guard tests
7. destructive command policy tests
8. plugin adapter가 포함된 release라면 local import only와 remote source rejection smoke test
9. release notes 작성
10. binary checksum 생성

## 모델 manifest 배포

모델 manifest는 repo 또는 release asset으로 배포할 수 있습니다.

manifest에는 최소한 다음 정보가 필요합니다.

- model id
- upstream model
- quantization
- GGUF artifact URL
- SHA-256
- license
- recommended RAM
- context length

모델 가중치를 직접 재배포할지 여부는 모델별 license와 artifact provider 정책을 확인한 뒤 결정합니다.

## Plugin 배포 경계

`rpotato` release는 plugin adapter만 배포합니다. 외부 plugin package는 배포하지 않습니다.

허용:

- local plugin directory import 기능
- Codex/Claude Code plugin manifest parser
- inspect/validate/enable/disable/remove command

비허용:

- plugin marketplace 연동
- plugin registry/catalog 연동
- third-party plugin package mirror
- remote URL plugin install
- official Codex/Claude Code compatibility claim
