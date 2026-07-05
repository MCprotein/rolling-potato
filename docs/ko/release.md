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

MVP 전에는 `0.x.y`를 사용합니다.

- `0.x.0`: 기능 단위 release
- `0.x.y`: bugfix 또는 문서/packaging 수정

안정화 후 SemVer를 따릅니다.

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
