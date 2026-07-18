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
6. matching remote release branch를 보존한 상태로 tag를 push한다.
7. 해당 tag로 GitHub Release를 만든다. Release가 publish되면 `release-binaries`
   workflow가 지원 binary asset을 build/upload하고 published set을 다시 내려받아
   정확한 11개 regular non-symlink asset과 모든 checksum을 검증한 뒤에만 merge된
   matching remote release branch를 삭제한다.
8. Published workflow와 asset을 확인한 뒤 local release branch를 삭제한다.

`main`은 integration branch입니다. Release branch는 장기 support branch가 아니며, release 이후의 unrelated work를 쌓으면 안 됩니다.

## Release Policy 강제

저장소에는 release policy check가 포함됩니다.

```sh
scripts/release/verify-release-policy.sh
```

이 check가 강제하는 것:

- release branch 이름이 `Cargo.toml` package version과 일치
- release tag가 `Cargo.toml` package version과 일치
- package version을 변경하는 PR과 `release/*` 이름의 branch는 일치하는
  `release/v...` branch를 사용하며, 일반 기능·문서 PR은 release 작업으로
  오인하지 않음
- release tag가 `origin/main` 위에 있음
- 대응되는 remote release branch가 아직 남아 있으면 release tag 시 direct ancestor이거나
  정확히 같은 tree의 squash merge 상태여야 함
- tag-policy validation은 remote release branch를 삭제하지 않으며, cleanup owner는
  published-asset verification success DAG 안에 하나만 존재

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

현재 release asset set:

- `rpotato-vX.Y.Z-aarch64-apple-darwin.tar.gz`
- `rpotato-vX.Y.Z-aarch64-apple-darwin.tar.gz.sha256`
- `rpotato-vX.Y.Z-x86_64-apple-darwin.tar.gz`
- `rpotato-vX.Y.Z-x86_64-apple-darwin.tar.gz.sha256`
- `rpotato-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`
- `rpotato-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz.sha256`
- `rpotato-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz`
- `rpotato-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz.sha256`
- `rpotato-vX.Y.Z-x86_64-pc-windows-msvc.zip`
- `rpotato-vX.Y.Z-x86_64-pc-windows-msvc.zip.sha256`
- `rpotato-vX.Y.Z-checksums.txt`

Workflow는 먼저 `ubuntu-24.04`에서 `release test gate` job을 실행해
`cargo test --locked -- --test-threads=1`와
`scripts/release/verify-release-target-matrix.sh`를 통과시킵니다. Serialized gate에는 backend
sidecar timeout/stale-record lifecycle test가 포함됩니다.
Target build job은 이 gate에 의존한 뒤 release binary를 build하고,
`scripts/release/verify-release-binary-smoke.sh`로 built binary를 smoke test하며 archive와
checksum을 GitHub Release에 upload합니다. Windows job은
`scripts/release/verify-uninstall-smoke.sh`도 실행해 `--keep-cache`와 `--purge-cache`
dry-run plan이 packaged binary에서도 보이고 non-destructive인지 확인합니다. `rpotato
doctor`는 package version, target OS/arch, binary suffix, backend/model/cache summary를
표시하고 model download, backend install, sidecar start, network access를 하지 않기 때문에
release-smoke command로 씁니다.

모든 target artifact가 build된 뒤 `checksums` job은 per-asset `.sha256` file을 합쳐
`rpotato-vX.Y.Z-checksums.txt`를 publish합니다. 모든 checksum entry는 build-directory
path가 아니라 release asset basename을 가리켜야 하며 Unix와 Windows 모두에서 aggregate
file을 검증할 수 있도록 LF line ending을 사용해야 합니다.
`scripts/release/verify-release-target-matrix.sh`는 workflow matrix가 지원 target을 계속
포함하는지 검사합니다.

Workflow는 2026-07-13 확인한 최신 안정 Node.js 24 action release인
`actions/checkout` v7.0.0, `actions/upload-artifact` v7.0.1,
`actions/download-artifact` v8.0.1의 immutable commit을 고정합니다. Workflow의
각 full commit SHA 옆에는 version comment를 유지합니다.
`scripts/release/verify-toolchain-pins.sh`는 PR과 release test gate에서 Rust,
Action SHA, GA runner pin의 일관성을 강제합니다.

Runner label은 현재 GA target image에 고정합니다.

- macOS Apple Silicon: `macos-26` / `aarch64-apple-darwin`
- macOS Intel: `macos-26-intel` / `x86_64-apple-darwin`
- Linux x86_64: `ubuntu-24.04` / `x86_64-unknown-linux-gnu`
- Linux ARM64: `ubuntu-24.04-arm` / `aarch64-unknown-linux-gnu`
- Windows x86_64: `windows-2025` / `x86_64-pc-windows-msvc`

GitHub runner-images reference는 2026-07-13 확인 시 위 label을 GA image로
명시했습니다: https://github.com/actions/runner-images

## artifact 목표

현재 archive target:

- macOS Apple Silicon
- macOS Intel
- Linux x86_64
- Linux ARM64
- Windows x86_64

준비한 package-manager target:

- Homebrew: macOS arm64/x64, Linux arm64/x64
- Scoop: Windows x64
- winget: Windows x64 portable ZIP

## Package Manager 배포

Package-manager manifest는 파생 artifact입니다. Channel마다 release URL과
checksum을 별도로 수동 관리하지 않습니다.

Release workflow는 다음 순서로 실행합니다.

1. 정확한 11-file GitHub Release asset set을 검증합니다.
2. 검증된 aggregate checksum file에서 archive 5개의 hash를 읽습니다.
3. Homebrew formula 하나, Scoop manifest 하나, winget manifest 3개를 생성합니다.
4. 별도 verifier가 정확한 path, version, URL, archive 이름, hash를 검증합니다.
5. Native 6개 lane에서 clean install, previous-stable-to-current upgrade,
   `rpotato doctor` version 출력, package-manager uninstall, command 부재를
   검증합니다.
6. 현재 version의 생성 결과를 publication-candidate workflow artifact로
   upload합니다.

Package-manager workflow는 GitHub Release에 파일을 추가하지 않습니다. 기존
archive 5개, sidecar checksum 5개, aggregate checksum 1개가 계속 전체 release
asset contract입니다.

### Channel 상태

다음 상태를 일관되게 사용합니다.

- `Generated`: 검증된 aggregate checksum에서 deterministic manifest를 생성함
- `Validated`: static verifier와 해당 native lifecycle lane이 모두 통과함
- `Published`: 권한 있는 외부 쓰기를 완료하고 public URL 또는 upstream review
  상태를 기록함
- `Unpublished`: 생성·검증 artifact가 있어도 live 외부 channel이라고 주장하지 않음

2026-07-18 기준 저장소 구현은 `Generated` 상태이고 static contract check가
통과했습니다. Native qualification은 아직 대기 중이므로 `Validated`에 도달한
channel은 없으며, 외부 channel 3개는 모두 `Unpublished`입니다. Homebrew tap,
Scoop bucket, winget community-manifest PR을 만들거나 갱신하는 작업은 maintainer가
별도로 승인해야 하는 외부 작업입니다.

### 무결성과 고정 prerequisite

`scripts/release/verify-release-assets.sh`가 통과한
`rpotato-vX.Y.Z-checksums.txt`만 package-manager manifest hash의 출처로
사용합니다. 모든 download URL은 대응되는 GitHub Release 아래의 immutable
versioned HTTPS URL을 유지합니다.

2026-07-18 기록한 native validation prerequisite는 다음과 같습니다.

- `Homebrew/actions/setup-homebrew` commit
  `df4b09108a1de9d6f995fe68f302b3f68bd6d2ef`
- Scoop source와 schema commit
  `b588a06e41d920d2123ec70aee682bae14935939`
- winget client release `v1.29.280` 및
  `scripts/release/verify-package-manager-prerequisites.sh`가 강제하는 bundle과
  dependency archive SHA-256
- winget manifest schema `1.12.0`

Workflow log는 각 native lane에서 실제 사용한 manager/client version을
기록합니다. Pin 없는 remote bootstrap pipeline은 금지합니다.

### Qualification과 recovery

v0.40.0 tag 전 `package-manager-distribution`을 다음 input으로 수동
실행합니다.

```text
mode=qualification
previous_tag=v0.38.0
current_tag=v0.39.0
```

Qualification은 이미 게시된 release로 pinned setup, manifest format,
install/upgrade/uninstall lifecycle을 Homebrew 4개 lane과 격리된 Scoop, winget
lane에서 검증합니다. Cargo build, release upload, tag 생성, 외부 publication,
branch cleanup은 수행하지 않습니다.

Stable tag를 게시한 뒤 package-manager 단계만 실패했다면 `current_tag`만 넣고
`mode=recovery`를 실행합니다. Recovery는 가장 큰 ancestral stable predecessor를
직접 구하고, 두 exact release asset set을 다시 검증한 뒤 manifest 준비와 native
6개 lifecycle lane만 재실행합니다. Release asset을 덮어쓰거나 새 patch tag를
만들지 않습니다. 모든 lane이 통과할 때만 matching release branch를 삭제하며,
실패하면 branch를 보존합니다.

### 설치와 제거 경계

외부 channel이 `Published`가 된 뒤에는 영문·한국어 README의 명령으로 package
manager 소유 binary를 설치, 갱신, 제거합니다. Package manager 제거는 `rpotato`
application data, model, cache를 제거하지 않습니다. 실행 파일을 제거하기 전에
`rpotato uninstall --dry-run`으로 별도 cleanup plan을 확인합니다.

### Release evidence

Release record에는 다음 항목이 있어야 합니다.

- candidate commit SHA와 exact-HEAD CI run
- 정확한 11-asset 검증
- tag 전 v0.38.0에서 v0.39.0으로 올리는 qualification run
- 고정 prerequisite와 실제 manager/client version
- 결정한 previous stable tag와 ancestry 판정
- 생성한 publication artifact ID와 hash
- native 6개 lane의 clean-install과 upgrade 결론
- 일반 deployment 또는 same-tag recovery run
- Homebrew tap URL/commit/status
- Scoop bucket URL/commit/status
- winget PR/merge/package status
- release-branch cleanup 결과

확정 evidence가 없는 외부 항목은 `Unpublished` 또는
`Pending external review`로 남기며 완료로 보고하지 않습니다.

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
10. repository toolchain pin guard
11. release target matrix guard
12. binary checksum 생성
13. GitHub Release publish 후 `release-binaries` workflow가 모든 target archive와
    대응 `.sha256` file, aggregate `checksums.txt` file을 upload했는지 확인
14. v0.40.0 이후 package-manager release는 tag 전 qualification 성공 run 기록
15. package-manager lifecycle 6개 lane이 모두 통과했는지 확인하거나 각 외부
    channel을 `Unpublished`로 명시

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
