# Model Manifest

모델 manifest는 `rpotato`가 설치 가능한 모델과 로컬 모델 상태를 이해하기 위한 신뢰 경계입니다.

## 목표

- 모델 artifact를 명시적으로 식별한다.
- 다운로드 전 크기와 license를 보여준다.
- 다운로드 후 SHA-256을 검증한다.
- backend 호환성을 확인한다.
- 모델별 prompt/runtime option을 분리한다.

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
      "upstreamModel": "TODO",
      "artifactName": "TODO.gguf",
      "format": "gguf",
      "backend": "llama.cpp",
      "quantization": "Q4_K_M",
      "recommendedRamGb": 16,
      "contextLength": 8192,
      "license": "TODO",
      "url": "TODO",
      "sha256": "TODO",
      "sizeBytes": 0
    }
  ]
}
```

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
- 사용자 승인 없는 자동 다운로드
- CLI binary에 모델 가중치 번들링
- 임의 URL을 silent fallback으로 사용하는 동작

## open questions

- manifest signing을 MVP에 넣을지 여부
- artifact mirror를 허용할지 여부
- quantization별 prompt preset을 manifest에 넣을지 여부
- provider trust policy를 어디까지 자동화할지 여부
