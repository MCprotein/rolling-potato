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

초기 manifest source는 repository 또는 GitHub Release asset에 둡니다.

로컬 cache 위치는 platform directory 규칙을 따릅니다.

- macOS: `~/Library/Application Support/rpotato/manifests/`
- Windows: `%LOCALAPPDATA%\rpotato\manifests\`
- Linux: `~/.local/share/rpotato/manifests/`

정확한 경로 구현은 Rust scaffold에서 `directories` 계열 crate 사용 여부를 결정한 뒤 확정합니다.

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
      "url": "TODO",
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
- `rpotato model verify-file <path> --sha256 <hash>`
- `rpotato model cleanup-failed <id> --dry-run`
- `rpotato model install <id>`

후보 상태:

- `candidate`: upstream model/source/license만 후보로 추적하고, 설치할 GGUF artifact는 아직 고르지 않은 상태
- `unverified`: GGUF artifact 후보는 있지만 checksum, provider terms, backend compatibility, RAM 근거 중 하나 이상이 빠진 상태
- `verified`: 설치 전 표시해야 할 source, license, artifact URL, provider terms, SHA-256, file size, backend compatibility, RAM 근거가 모두 채워진 상태

`model download-plan`은 실제 다운로드 전에 사용자에게 보여야 할 source, license source, artifact provider, artifact terms, file size, SHA-256, resume path, final path를 렌더링합니다.

`model verify-file`은 로컬 파일을 streaming으로 읽어 SHA-256을 계산하고 expected hash와 비교합니다. 성공과 실패 모두 ledger event를 남기며, 실패 시 registry 등록은 차단되어야 합니다.

`model cleanup-failed`는 app data의 `downloads/`와 `models/` 아래에 있는 app-managed partial/failed artifact path만 대상으로 합니다. 삭제는 `--delete`가 명시된 경우에만 실행하고, 기본 검증과 문서 smoke는 `--dry-run`을 사용합니다.

`model install`은 `verified`가 아닌 항목을 차단하고 ledger event를 남깁니다. 현재 구현은 실제 다운로드를 수행하지 않으며, local registry는 `models/registry/<model-id>.json` 위치에 verified artifact만 기록하는 경계로 준비되어 있습니다.

## 필수 검증

모델 설치 시:

1. manifest schema version을 확인한다.
2. OS/backend 호환성을 확인한다.
3. 다운로드 크기를 사용자에게 보여준다.
4. 사용자가 승인해야 다운로드한다.
5. 다운로드 후 SHA-256을 검증한다.
6. 검증 실패 시 모델 registry에 등록하지 않는다.

## 금지 사항

- checksum 없는 모델 설치
- license 미표기 모델 추천
- 출처 없는 license, RAM, context length, backend 호환성 claim
- 사용자 승인 없는 자동 다운로드
- `rpotato` release binary에 모델 가중치 번들링
- 임의 URL을 silent fallback으로 사용하는 동작

## 열린 질문

- manifest signing을 MVP에 넣을지 여부
- artifact mirror를 허용할지 여부
- quantization별 prompt preset을 manifest에 넣을지 여부
- provider trust policy를 어디까지 자동화할지 여부
