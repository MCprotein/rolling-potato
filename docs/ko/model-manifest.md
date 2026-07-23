# 모델 Manifest

모델 manifest는 `rpotato`가 설치 가능한 모델과 로컬 모델 상태를 이해하기 위한 신뢰 경계입니다.

## 목표

- 모델 artifact를 명시적으로 식별한다.
- 다운로드 전 크기와 license를 보여준다.
- 다운로드 후 SHA-256을 검증한다.
- backend 호환성을 확인한다.
- 모델별 prompt/runtime option을 분리한다.
- 모델 관련 claim은 출처 기반으로만 확정한다.

## 위치

초기 manifest source는 repository 또는 GitHub Release asset에 둡니다. 설치 가능한 artifact 다운로드 URL은 Rust 소스 코드 상수가 아니라 manifest 데이터로 관리합니다.

로컬 cache 위치는 platform directory 규칙을 따릅니다.

- macOS: `~/Library/Application Support/rpotato/manifests/`
- Windows: `%LOCALAPPDATA%\rpotato\manifests\`
- Linux: `~/.local/share/rpotato/manifests/`

정확한 경로 구현은 Rust scaffold에서 `directories` 계열 crate 사용 여부를 결정한 뒤 확정합니다.

## 다운로드 URL 관리

설치 가능한 다운로드 URL 필드는 `artifactUrl`입니다.

- 실제 배포용 다운로드 URL은 Rust 코드에 하드코딩하지 않고 versioned model manifest에 고정한다.
- 현재 `src/model.rs`의 static 후보 table은 초기 scaffold용 후보 metadata입니다. CLI inspection을 위해 source-backed artifact 후보 사실을 담을 수는 있지만, production manifest 배포 surface는 아닙니다.
- `status != "verified"`인 항목은 artifact URL, checksum, file size가 이미 알려져 있어도 설치할 수 없습니다.
- `rpotato model install`은 사용자가 임의로 넣은 URL을 모델 source로 받으면 안 된다.
- `latest`, branch, 움직이는 redirect URL을 신뢰 근거로 쓰지 않는다. provider가 immutable revision 또는 release URL을 지원하면 `artifactUrl`은 그 구체 artifact를 가리켜야 한다.
- 모든 `artifactUrl`은 `artifactProvider`, `artifactTermsUrl`, `artifactName`, `sha256`, `sizeBytes`, 출처/확인 날짜 evidence와 함께 기록해야 한다.
- URL이 바뀌면 checksum, size, provider evidence, manifest entry를 같이 업데이트해야 한다.

## 현재 source-backed artifact 후보

아래 항목은 후보 사실이며 install-ready 기본값이 아닙니다. Hugging Face LFS `oid` 값은 expected SHA-256으로 기록하되, registry 등록 전 다운로드된 bytes를 다시 검증해야 합니다.

| Model ID | Artifact provider | Artifact | Revision | Quantization | Size bytes | SHA-256 | Source status |
| --- | --- | --- | --- | --- | ---: | --- | --- |
| `qwen3.5-4b` | `unsloth/Qwen3.5-4B-GGUF` | `Qwen3.5-4B-Q4_K_M.gguf` | `e87f176479d0855a907a41277aca2f8ee7a09523` | `Q4_K_M` | `2740937888` | `00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4` | Static `unverified`; 기록된 host-local smoke/RAM/mmproj evidence는 있지만 exact-response equality를 실패해 local promotion은 무효 |
| `gemma-4-e4b` | `google/gemma-4-E4B-it-qat-q4_0-gguf` | `gemma-4-E4B_q4_0-it.gguf` | `bb3b92e6f031fa438b409f898dd9f14f499a0cb0` | `QAT q4_0` | `5154939136` | `e8b6a059ba86947a44ace84d6e5679795bc41862c25c30513142588f0e9dba1d` | Static `unverified`; 기록된 host-local smoke/RAM/mmproj evidence가 통과해 재검증되는 local promotion/default selection을 지원 |

Main GGUF는 언어 모델을 포함합니다. Model 전용 `mmproj`는 image feature를 해당
언어 모델의 embedding 공간으로 변환하는 visual encoder/projector입니다. 다른
model이나 revision 사이에서 두 artifact가 호환된다고 가정할 수 없습니다.
따라서 vision-ready 상태에는 main artifact뿐 아니라 별도로 고정·검증한 projector
bytes가 필요하고, text-ready 상태에는 projector를 요구하지 않습니다.

| Model ID | Vision artifact | Revision | Size bytes | SHA-256 |
| --- | --- | --- | ---: | --- |
| `qwen3.5-4b` | `unsloth/Qwen3.5-4B-GGUF/mmproj-F16.gguf` | `e87f176479d0855a907a41277aca2f8ee7a09523` | `672423616` | `cd88edcf8d031894960bb0c9c5b9b7e1fea6ebee02b9f7ce925a00d12891f864` |
| `gemma-4-e4b` | `google/gemma-4-E4B-it-qat-q4_0-gguf/gemma-4-E4B-it-mmproj.gguf` | `bb3b92e6f031fa438b409f898dd9f14f499a0cb0` | `991551904` | `c6398448d84a4836fdedf58f9775979e69ae0cc4dfdf4d697b5597693a555b12` |

Registry schema v2는 `visionStatus`와 검증된 projector path, hash, size를
기록합니다. Schema v1 registry는 legacy text-only 상태로 읽으며 vision claim으로
자동 승격하지 않습니다. 명시적 model 준비를 수행할 때만 v2를 기록합니다.

2026-07-23 확인 source:

- https://huggingface.co/api/models/Qwen/Qwen3.5-4B
- https://huggingface.co/api/models/unsloth/Qwen3.5-4B-GGUF
- https://huggingface.co/api/models/unsloth/Qwen3.5-4B-GGUF/tree/e87f176479d0855a907a41277aca2f8ee7a09523?recursive=1
- https://huggingface.co/api/models/google/gemma-4-E4B-it-qat-q4_0-unquantized
- https://huggingface.co/api/models/google/gemma-4-E4B-it-qat-q4_0-gguf
- https://huggingface.co/api/models/google/gemma-4-E4B-it-qat-q4_0-gguf/tree/bb3b92e6f031fa438b409f898dd9f14f499a0cb0?recursive=1
- https://github.com/ggml-org/llama.cpp/blob/master/docs/multimodal.md

## schema 초안

```json
{
  "schemaVersion": 1,
  "models": [
    {
      "id": "qwen3.5-4b-q4-k-m",
      "displayName": "Qwen3.5 4B Q4_K_M",
      "status": "candidate",
      "upstreamModel": "TODO",
      "upstreamUrl": "TODO",
      "artifactName": "TODO.gguf",
      "artifactProvider": "TODO",
      "artifactTermsUrl": "TODO",
      "format": "gguf",
      "backend": "llama.cpp",
      "quantization": "TODO",
      "recommendedRamGb": null,
      "recommendedRamEvidence": "TODO",
      "contextLength": null,
      "license": "TODO",
      "licenseSource": "TODO",
      "licenseCheckedAt": "TODO",
      "artifactUrl": "TODO",
      "sha256": "TODO",
      "sizeBytes": null,
      "backendCompatibility": "TODO",
      "publishedBenchmark": {
        "source": "TODO",
        "checkedAt": "TODO",
        "claimStatus": "source-listed-unreproduced",
        "parityStatus": "not-run"
      }
    }
  ]
}
```

이 예시는 schema shape만 보여줍니다. `TODO`와 `null` 값은 제품 사실이 아니며, 실제 manifest에는 [model-source-policy.md](model-source-policy.md)에 따른 출처 확인 후 값을 채웁니다.

현재 CLI surface:

- `rpotato model list`
- `rpotato model manifest`
- `rpotato model inspect <id>`
- `rpotato model registry`
- `rpotato model download-plan <id>`
- `rpotato model eval-plan <id>`
- `rpotato model benchmark-plan <id>`
- `rpotato model fetch-candidate <id> --for-evaluation`
- `rpotato model verify-file <path> --sha256 <hash>`
- `rpotato model promote <id> --evidence <file>`
- `rpotato model cleanup-failed <id> --dry-run`
- `rpotato model install <id>`
- `rpotato model default [<id>]`

후보 상태:

- `candidate`: upstream model/source/license만 후보로 추적하고, 설치할 GGUF artifact는 아직 고르지 않은 상태
- `unverified`: GGUF artifact 후보는 있지만 byte-level checksum 검증, provider/source review, backend smoke, RAM-fit evidence, mmproj 필요 여부 검토, benchmark evidence 같은 install 승격 조건 중 하나 이상이 남은 상태
- `verified`: 설치 전 표시해야 할 source, license, artifact URL, provider terms, SHA-256, file size, backend compatibility, RAM 근거가 모두 채워진 상태

`model download-plan`은 실제 다운로드 전에 사용자에게 보여야 할 source, license source, artifact provider, artifact terms, file size, SHA-256, resume path, final path를 렌더링합니다.

`model eval-plan <id>`은 read-only입니다. source-backed artifact field가 있는지, app-data의 expected artifact file이 missing인지 size/SHA-256 검증 상태인지, 다음 local smoke/benchmark 단계가 무엇인지 보고합니다.

`model benchmark-plan <id>`은 read-only입니다. 공개 benchmark 재현 조건과 로컬 제품 benchmark fixture를 분리하고, artifact, quantization, backend, context length, prompt/template, dataset version, scoring method가 함께 기록되기 전까지 score parity claim을 차단합니다.

`model fetch-candidate <id> --for-evaluation`은 `unverified` source-backed artifact를 다운로드할 수 있는 유일한 경로입니다. 명시적인 평가 플래그를 요구하고, app-managed partial/final artifact file만 쓰며, 성공 전에 file size와 SHA-256을 검증하고 ledger event를 남깁니다. 이 명령은 `models/registry/<model-id>.json`을 만들지 않습니다.

`model verify-file`은 로컬 파일을 streaming으로 읽어 SHA-256을 계산하고 expected hash와 비교합니다. 성공과 실패 모두 ledger event를 남기며, 실패 시 registry 등록은 차단되어야 합니다.

`model promote <id> --evidence <file>`은 registry 등록 전에 local promotion evidence를 검증합니다. 이 명령은 다음을 요구합니다.

- app-managed artifact file size와 SHA-256이 source-backed manifest와 일치해야 한다.
- backend binary, model artifact, context, sampling, mmproj 상태, OS, architecture provenance가 후보와 일치하는 `backend.chat.completed` event가 있어야 한다.
- `ramFit=observed-within-local-host`, 실제 `peakRssBytes`, `ceil(peak RSS GiB) + 2 GiB`와 같은 `recommendedRamGb`가 있어야 한다.
- `mmproj` evidence는 `not-required-text-only`, `not-required`, `required` 중 하나여야 한다.
- SQLite row의 fixture ID, fixture SHA-256, prompt SHA-256, benchmark name, dataset reference가 release-pinned canonical adoption contract와 일치하고, requested/effective max token이 모두 192이며, quantization이 source-backed model manifest에서 resolve되어야 한다. 또한 chat event를 `model_run_id`로 직접 가리키고, `claim_state=measured-locally`, `local_pass=true`, 후보 backend, 동일한 `peak_rss_bytes`를 가져야 한다.

Promotion evidence JSON schema:

```json
{
  "schemaVersion": 1,
  "modelId": "gemma-4-e4b",
  "artifactSha256": "e8b6a059ba86947a44ace84d6e5679795bc41862c25c30513142588f0e9dba1d",
  "artifactSizeBytes": 5154939136,
  "backendId": "llama.cpp",
  "backendVersion": "b9878",
  "backendSmokeEventId": "event-...",
  "ramFit": "observed-within-local-host",
  "recommendedRamGb": 8,
  "peakRssBytes": 5521932288,
  "mmproj": "not-required-text-only",
  "benchmarkRunId": "benchmark-event-...",
  "recordedAt": "2026-07-11T00:00:00Z"
}
```

승격이 성공하면 정규화된 evidence를 `models/evidence/<model-id>.promotion.json`에 기록합니다. 이 evidence는 local evidence일 뿐 public benchmark parity claim이 아닙니다.

`model cleanup-failed`는 app data의 `downloads/`와 `models/` 아래에 있는 app-managed partial/failed artifact path만 대상으로 합니다. 삭제는 `--delete`가 명시된 경우에만 실행하고, 기본 검증과 문서 smoke는 `--dry-run`을 사용합니다.

`model install`은 source-backed manifest entry가 정적으로 `verified`이거나 `models/evidence/<model-id>.promotion.json`이 local artifact, ledger, RAM/mmproj, benchmark evidence와 다시 일치할 때만 모델을 등록합니다. Registry entry는 `models/registry/<model-id>.json`에 기록합니다.

Interactive 최초 설정은 사용자가 명시적으로 선택하는 별도 실행 lane입니다. 이 lane은 고정 source, artifact size와 SHA-256, 확인된 license, backend-compatibility source가 재검증될 때만 `source-backed-manifest` registry entry를 기록할 수 있습니다. 이 entry는 RAM 적합성, capability 품질, benchmark parity를 주장하지 않으며 더 엄격한 `model install` promotion gate를 충족한 것으로 보지 않습니다.

`model default <id>`는 artifact와 evidence status가 재검증되는 registry 모델만 선택합니다. 허용되는 evidence는 현재 local promotion evidence 또는 위에서 제한한 정확한 `source-backed-manifest` setup 상태입니다. `model default`는 현재 선택을 보여줍니다. `backend start`에서 `--model`을 생략하면 지속 기본 모델을 재검증해 사용하며 실패 시 fail-closed합니다.

## v0.30.0 로컬 도입 기록

2026-07-11 Apple M5 Pro와 64GB RAM의 MacBook Pro, `llama.cpp b9878`, context 4096, temperature 0.1, top-p 0.8 조건에서 두 pinned artifact에 같은 hash-pinned 5-line local adoption contract를 실행했습니다. 최신 Gemma 실행은 `1686ms`, `61.6845 tokens/s`, peak RSS `5521932288` bytes와 `3/3` 통과를 기록했습니다. Qwen은 `1680ms`, `61.9048 tokens/s`, peak RSS `3296378880` bytes였지만 필수 marker 5개 앞에 지시문을 한 줄 추가해 `2/3`으로 실패했습니다. 두 모델 모두 금지 marker는 0건이었습니다.

Gemma는 exact-response equality를 통과해 local promotion, registry 등록, 지속 기본 선택을 완료했습니다. Qwen의 낮은 RSS는 instruction contract 실패보다 우선하지 않습니다. 이는 보편적 model ranking, 16GB evidence, public benchmark parity가 아닙니다. Model weight는 repository와 release asset에 포함하지 않습니다.

## 필수 검증

모델 설치 시:

1. manifest schema version을 확인한다.
2. OS/backend 호환성을 확인한다.
3. 다운로드 크기를 사용자에게 보여준다.
4. 사용자가 승인해야 다운로드한다.
5. 다운로드 후 SHA-256을 검증한다.
6. 검증 실패 시 모델 registry에 등록하지 않는다.
7. unverified source-backed candidate를 설치하기 전에 local promotion evidence를 요구한다.

## 금지 사항

- checksum 없는 모델 설치
- license 미표기 모델 추천
- 출처 없는 license, RAM, context length, backend 호환성 claim
- 사용자 승인 없는 자동 다운로드
- `rpotato` release binary에 모델 가중치 번들링
- 설치 가능한 artifact URL을 Rust source에 하드코딩하는 동작
- 임의 URL을 silent fallback으로 사용하는 동작
- `latest`나 움직이는 URL을 검증된 artifact로 취급하는 동작

## 열린 질문

- manifest signing을 MVP에 넣을지 여부
- artifact mirror를 허용할지 여부
- quantization별 prompt preset을 manifest에 넣을지 여부
- provider trust policy를 어디까지 자동화할지 여부
