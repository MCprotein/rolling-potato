# 코드 아키텍처

English: [Code architecture](../code-architecture.md)

마이그레이션 원장: [architecture-migration-map.json](../architecture-migration-map.json)

이 문서는 동작을 보존하는 v0.37.x 리팩터링의 코드 소유권과 의존 방향을
정의하는 정본입니다. [architecture.md](architecture.md)는 제품 아키텍처를
계속 설명하고, 이 문서는 Rust 구현을 나누고 책임을 이전하는 방식을 규정합니다.

## 결과와 불변 조건

v0.38.0 개발을 시작하기 전에 모든 운영 코드와 테스트 책임을 응집도 있는
도메인 경계가 소유해야 합니다. 완료 기준은 파일 길이나 미리 정한 마지막 patch
번호가 아니라, 마이그레이션 원장의 전 범위 완료와 compatibility facade 0개입니다.

리팩터링은 다음 계약을 보존해야 합니다.

- CLI 명령, 인자, 출력, 종료 코드
- 정본 영속 바이트, 레코드 순서, 해시, event identity
- append, mutation, projection-lag, recovery 순서
- 재생성 가능한 projection에 대한 ledger의 정본 권한
- 승인, 보안, 권한, fail-closed 동작
- backend, model, benchmark, resource 동작
- release, packaging, checksum, asset 계약

별도 변경 승인이 없으면 새 dependency, async runtime, actor system, 공개 API,
사용자 노출 동작을 이 리팩터링에 추가하지 않습니다.

## 소유권 트리

```text
src/
  main.rs
  composition/                 wiring, startup/shutdown, command dispatch
  surfaces/
    cli/                       parse, command DTO, presentation
    tui/                       controller, view model, rendering, input
  runtime_core/
    workflow/                  state, transition, storage, recovery coordinator
    patch/                     intent, proposal, approval, apply, verify, rollback
    inference/                 backend, model, benchmark, resource policy
    extensions/                skill, hook, plugin lifecycle
    collaboration/             subagent, team lifecycle
    knowledge/                 context, evidence, ontology
    observability/             event projection, query, monitoring
    policy/                    approval, fail-closed decision
    reporting/                 surface-neutral report DTO와 invariant
  adapters/
    filesystem/                layout, atomic file, lease, cache, replace
    process/                   child-process lifecycle
    sqlite/                    재생성 가능한 observability projection
    llama_cpp/                 managed backend protocol과 process adapter
    terminal/                  native terminal 구현
  foundation/                  capability와 독립된 typed primitive
```

모듈은 기본적으로 private입니다. 경계는 필요한 가장 좁은 consumer에게만
노출합니다. v0.37.1 트리는 문서와 소유권 예약만 포함했습니다. v0.37.2부터는
ledger slice, targeted test, legacy path 제거가 함께 닫히는 경우에만 운영 동작을
이동합니다.
v0.37.3 inference 경계는 backend/model/benchmark/resource 규칙, durable
inference codec, llama.cpp/process/filesystem 구현을 이제 이 private root에서
소유합니다. 남은 최상위 inference command/report module은 v0.37.13의 최종
composition cleanup 대상으로 명시된 compatibility facade입니다.

## 의존 방향

허용하는 방향은 다음과 같습니다.

```text
main -> composition
composition -> surfaces + runtime_core facade + concrete adapter
surfaces -> runtime_core use-case/query DTO + foundation
runtime_core application -> owning domain + consumer-owned port
runtime_core domain -> foundation
adapters -> consumer-owned port + foundation
cross-context -> owning facade 또는 canonical DTO/event
foundation -> std + 이미 존재하는 low-level crate
```

다음 edge는 금지합니다.

- surface가 concrete adapter를 import
- domain code가 filesystem, process, SQLite, terminal, CLI, TUI를 import
- adapter가 surface 또는 report renderer를 호출
- 한 context가 다른 context의 file 또는 table을 직접 읽음
- `utils`, `services`, `managers` 같은 일반화된 소유권 묶음
- substitution 또는 invariant 근거가 없는 blanket trait
- 측정된 concurrent I/O 필요가 없는 async, Tokio, actor 도입

아키텍처 계약 테스트는 새 root 내부 import를 검사합니다. 임시 예외에는 원장에
owner, rationale, v0.37.x expiry release를 기록해야 합니다.

## Rust 설계 규칙

- 서로 바뀌기 쉬운 identifier, hash, version, validated path에는 newtype을 씁니다.
- 닫힌 command, event, state 집합에는 enum과 exhaustive match를 씁니다.
- typestate는 안정적인 process-local construction에만 씁니다. 영속화되거나
  복구된 불완전 상태는 validation이 붙은 enum 또는 record로 유지합니다.
- trait는 consumer 옆에 두고 실제 substitution seam 또는 invariant 경계에만 둡니다.
- 일반 service/repository 추상화보다 소유 도메인의 vocabulary를 우선합니다.
- 이 리팩터링 동안 runtime은 synchronous로 유지합니다.
- private rule test는 owner 옆에, boundary contract는 integration test에 둡니다.

## 영속 workflow 경계

바이트 호환 `WorkflowRecord`와 관련 영속 aggregate는 앞으로 생길
`runtime_core/workflow/storage_compat` 경계에서 하나의 canonical codec owner를
유지합니다. Domain view와 command는 이 record를 검증해서 사용하며 train 도중
독립적으로 다시 정의하거나 직렬화하지 않습니다.

하나의 workflow application transaction coordinator가 다음 cross-store 순서를
소유합니다.

1. canonical append
2. state mutation
3. project/global log convergence
4. projection-lag barrier
5. recovery와 cleanup

Port와 adapter는 작업에 참여하지만 순서를 결정하지 않습니다.

## 마이그레이션 방식

모든 관리 대상 파일은 [마이그레이션 원장](../architecture-migration-map.json)에
정확히 하나의 file record를 가집니다. 한 파일은 서로 겹치지 않는 여러 책임
slice를 가질 수 있습니다. 각 slice는 하나의 responsibility, 정확한 target 하나,
v0.37.x release 하나, state 하나, evidence 목록을 가집니다.

별도 responsibility inventory가 완전성 oracle 역할을 합니다. 모든 inventory
responsibility는 정확히 하나의 slice가 소유해야 하고, 모든 slice는 inventory의
responsibility 하나와 일치해야 합니다. Complete evidence는 존재하는 proof path 또는
선언된 logical proof ID를 가리켜야 합니다.

허용 state는 `planned`, `migrating`, `compatibility-facade`, `complete`,
`exception`입니다. 예외에는 owner, rationale, expiry release가 추가로 필요합니다.
원장의 `current_release`를 기준으로 만료된 예외는 계약에 실패합니다.
`train_completion`을 `true`로 바꾸면 `complete` 외의 모든 state를 거부하므로 planned,
migrating, exception, compatibility facade가 release blocker가 됩니다. Targeted test
통과, legacy facade 제거, PR/release evidence 기록이 끝나야 마이그레이션이 완료됩니다.

각 논리 단위는 다음 순서로 닫습니다.

```text
누락 동작 고정 -> 책임 하나 이동 -> targeted 검증 -> commit -> push
```

안정화된 patch release에는 범위를 제한한 독립 review를 한 번만 실행합니다.
전체 format, test, clippy, release build, release-policy 검증은 PR CI가 정확한
candidate commit에 한 번 수행합니다. 플랫폼 packaging과 release asset smoke는
tag 이후 배포 검증으로 유지합니다.
