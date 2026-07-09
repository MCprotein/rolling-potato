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
- `0.2.0`: 첫 `rpotato run` context-aware model-response skeleton, model-run metric, 실행 없는 model action parsing 추가
- `0.3.0`: patch diff display와 approval gate 추가
- `0.4.0`: 승인된 patch apply와 verification command flow 추가
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

## Release Branch 정책

출시되는 모든 version은 짧게 쓰고 제거하는 version branch를 사용해야 합니다.

Branch 이름:

- stable release: `release/vX.Y.Z`
- preview release: `release/vX.Y.Z-alpha.N`, `release/vX.Y.Z-beta.N`, `release/vX.Y.Z-rc.N`

규칙:

1. `main`에서 release branch를 만든다.
2. `Cargo.toml` version을 branch 이름과 정확히 같은 version으로 올린다.
3. release note, docs, manifest, release check를 해당 branch에서 마무리한다.
4. release checklist가 통과한 뒤에만 release branch를 `main`에 merge한다.
5. merge commit에 `vX.Y.Z` 또는 대응되는 prerelease tag를 붙인다.
6. tag를 push한다. matching remote release branch가 아직 남아 있으면
   `release-policy` workflow가 `main`에 merge됐는지 확인한 뒤 삭제한다.
7. 해당 tag로 GitHub Release를 만든다. Release가 publish되면 `release-binaries`
   workflow가 지원 binary asset을 build/upload한다.
8. Release asset과 checksum을 확인한 뒤 local release branch를 삭제한다.

`main`은 integration branch입니다. Release branch는 장기 support branch가 아니며, release 이후의 unrelated work를 쌓으면 안 됩니다.

## Release Policy 강제

저장소에는 release policy check가 포함됩니다.

```sh
scripts/release/verify-release-policy.sh
```

이 check가 강제하는 것:

- release branch 이름이 `Cargo.toml` package version과 일치
- release tag가 `Cargo.toml` package version과 일치
- `main`으로 들어오는 release PR은 `release/v...` branch에서만 허용
- release tag가 `origin/main` 위에 있음
- 대응되는 remote release branch가 아직 남아 있으면 release tag 시 merge 상태여야 함
- release tag check가 성공하면 아직 남아 있는 merged remote release branch를 삭제

필수 GitHub repository setting:

- `main` 보호
- release work는 `main` merge 전에 pull request 요구
- merge 전 `release-policy` status check 필수
- `main` force push 금지
- release workflow가 merged `release/v...` branch를 삭제할 수 있도록 GitHub Actions `contents: write` 허용

GitHub repository setting은 source tree 밖에 있으므로 maintainer가 GitHub에서 설정해야 합니다. Workflow와 script는 repo-local enforcement surface입니다.

## Binary Release Workflow

`release-binaries`는 GitHub Release가 publish될 때 release asset을 build합니다.
Workflow 검증용으로 `release_tag` input을 넣어 수동 실행할 수도 있습니다.

현재 v0.26.0 asset:

- `rpotato-vX.Y.Z-aarch64-apple-darwin.tar.gz`
- `rpotato-vX.Y.Z-aarch64-apple-darwin.tar.gz.sha256`
- `rpotato-vX.Y.Z-x86_64-apple-darwin.tar.gz`
- `rpotato-vX.Y.Z-x86_64-apple-darwin.tar.gz.sha256`
- `rpotato-vX.Y.Z-x86_64-pc-windows-msvc.zip`
- `rpotato-vX.Y.Z-x86_64-pc-windows-msvc.zip.sha256`
- `rpotato-vX.Y.Z-checksums.txt`

Workflow는 `cargo test --locked`를 실행하고 release binary를 build한 뒤,
`scripts/release/verify-release-binary-smoke.sh`로 built binary를 smoke test하고 archive와
checksum을 GitHub Release에 upload합니다. Windows job은
`scripts/release/verify-uninstall-smoke.sh`도 실행해 `--keep-cache`와 `--purge-cache`
dry-run plan이 packaged binary에서도 보이고 non-destructive인지 확인합니다. `rpotato
doctor`는 package version, target OS/arch, binary suffix, backend/model/cache summary를
표시하고 model download, backend install, sidecar start, network access를 하지 않기 때문에
release-smoke command로 씁니다.

모든 target artifact가 build된 뒤 `checksums` job은 per-asset `.sha256` file을 합쳐
`rpotato-vX.Y.Z-checksums.txt`를 publish합니다. 모든 checksum entry는 build-directory
path가 아니라 release asset basename을 가리켜야 합니다.

Runner label은 첫 지원 target에 맞춰 고정합니다.

- `macos-14`: macOS Apple Silicon / `aarch64-apple-darwin`
- `macos-15-intel`: macOS Intel / `x86_64-apple-darwin`
- `windows-latest`: Windows x86_64 / `x86_64-pc-windows-msvc`

Reference: GitHub-hosted runners reference, checked 2026-07-09:
https://docs.github.com/en/actions/reference/runners/github-hosted-runners

## artifact 목표

초기 목표:

- macOS Apple Silicon
- macOS Intel
- Windows x86_64

추후 목표:

- Linux x86_64
- Linux ARM64
- package manager channel: Homebrew, Scoop, winget

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
11. GitHub Release publish 후 `release-binaries` workflow가 모든 target archive와
    대응 `.sha256` file, aggregate `checksums.txt` file을 upload했는지 확인

새 release note entry는 [release-notes-template.md](release-notes-template.md)를 사용합니다.

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
