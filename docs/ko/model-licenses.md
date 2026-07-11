# 모델 License

`rolling-potato`의 코드 라이선스와 모델 라이선스는 별개입니다.

- 프로젝트 코드: Apache-2.0
- 모델 가중치: 각 upstream model과 GGUF artifact provider license를 확인한 뒤 기록해야 함

이 문서는 `Qwen`과 `Gemma` 후보만 다룹니다. `llama.cpp`는 추론 backend 이름일 뿐 모델 후보가 아니며, `llama.cpp`를 쓴다는 이유로 Meta Llama 계열 모델이나 그 라이선스를 이 프로젝트 후보에 끌어오지 않습니다.

## 원칙

- 모델 license를 확인하기 전에는 기본 추천 모델로 확정하지 않습니다.
- GGUF 변환본은 upstream model license와 artifact provider terms를 모두 확인합니다.
- manifest에는 모델별 license를 반드시 표시합니다.
- 재배포 가능 여부가 불명확하면 `rpotato` 배포물에 직접 bundle하지 않습니다.
- 모델과 GGUF artifact를 프로젝트 코드처럼 소유한 것으로 표시하지 않습니다.
- 모든 모델 관련 claim은 [model-source-policy.md](model-source-policy.md)를 따릅니다.

## 포함/재배포 판단

결론: 원 라이선스 조건을 지키는 범위에서는 앱에 포함하거나 설치 흐름에 연결할 수 있지만, 모델 가중치나 변환본을 `rolling-potato`의 독점 소유물처럼 취급하면 안 됩니다.

현재 정책:

- 프로젝트 소스 저장소에는 모델 가중치를 커밋하지 않습니다.
- 기본 배포는 manifest 기반 다운로드를 우선합니다.
- 모델 다운로드 전 license, source, artifact provider, file size, checksum을 사용자에게 표시합니다.
- bundle 배포는 upstream model license, GGUF artifact provider terms, NOTICE/attribution 요구사항, checksum을 모두 확인한 뒤에만 허용합니다.
- `rolling-potato`의 Apache-2.0 라이선스는 프로젝트 코드에 적용하고, 포함되는 third-party 모델과 artifact는 각자의 원 라이선스를 유지합니다.
- 수정한 모델 파일이나 변환본을 배포하는 경우 수정 사실과 원 출처를 문서화합니다.

## 초기 후보

| Candidate | Role | Status | Notes |
| --- | --- | --- | --- |
| `Qwen3.5-4B` GGUF | 우선 평가 후보 | static `unverified`, local v0.30.0 adoption gate 실패 | Pinned Q4_K_M bytes와 license source를 검증했지만 기록된 64GB macOS 실행에서 지시문 한 줄을 추가해 exact-response equality를 실패함 |
| `Gemma 4 E4B` GGUF | 비교 후보 | static `unverified`, local v0.30.0 promotion 통과 | Pinned q4_0 bytes와 license source를 검증했고 기록된 host에서 canonical local adoption gate를 통과해 지속 기본 모델로 선택함 |
| `Qwen3.5-9B` GGUF | 품질 참고 후보 | upstream license 확인, 제품 기본값 보류 | RAM 영향과 runtime fit은 측정 전 미확정 |

## 확인된 source ledger

아래 source ledger는 source-recorded artifact field와 runtime claim을 분리합니다. Source-recorded URL, size, LFS oid만으로 local `llama.cpp` 호환성, 16 GB 실행 가능성, 제품 기본 모델 적합성이 확정되지는 않습니다.

| Claim | Source | Checked-at | Status |
| --- | --- | --- | --- |
| `Qwen/Qwen3.5-4B`의 Hugging Face model card license field는 `apache-2.0`이다. | https://huggingface.co/Qwen/Qwen3.5-4B | 2026-06-29 | confirmed |
| `Qwen/Qwen3.5-9B`의 Hugging Face model card license field는 `apache-2.0`이다. | https://huggingface.co/Qwen/Qwen3.5-9B | 2026-06-29 | confirmed |
| `google/gemma-4-E4B`의 Hugging Face model card license field는 `apache-2.0`이고, Google AI for Developers의 Gemma 4 license 문서는 Apache License 2.0을 게시한다. | https://huggingface.co/google/gemma-4-E4B, https://ai.google.dev/gemma/apache_2 | 2026-06-29 | confirmed |
| Apache License 2.0은 조건을 지키는 경우 사용, 복제, 수정, sublicensing, 배포를 허용하며, license 사본 제공, 수정 표시, 기존 attribution/NOTICE 보존, 상표 제한을 요구한다. | https://ai.google.dev/gemma/apache_2 | 2026-06-25 | confirmed |
| `Qwen/Qwen3.5-4B` Hugging Face API는 `license:apache-2.0`을 보고하고, `unsloth/Qwen3.5-4B-GGUF` artifact card는 `license:apache-2.0`, base model `Qwen/Qwen3.5-4B`, upstream Qwen license link를 보고한다. | https://huggingface.co/api/models/Qwen/Qwen3.5-4B, https://huggingface.co/api/models/unsloth/Qwen3.5-4B-GGUF | 2026-07-06 | source field는 confirmed, runtime fit은 unverified |
| `Qwen3.5-4B-Q4_K_M.gguf` artifact entry는 size `2740937888`과 LFS oid `00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4`를 표시한다. | https://huggingface.co/api/models/unsloth/Qwen3.5-4B-GGUF/tree/main?recursive=1 | 2026-07-06 | expected hash로 source-recorded, 다운로드 bytes 검증은 필요 |
| `google/gemma-4-E4B-it-qat-q4_0-gguf` Hugging Face API는 `license:apache-2.0`을 보고하고 Google의 현재 Gemma page는 Apache License 2.0을 게시한다. | https://huggingface.co/api/models/google/gemma-4-E4B-it-qat-q4_0-gguf, https://ai.google.dev/gemma/apache_2 | 2026-07-11 | source field confirmed, license와 host-specific runtime fit은 별도 |
| `gemma-4-E4B_q4_0-it.gguf` artifact entry는 size `5154939136`과 LFS oid `e8b6a059ba86947a44ace84d6e5679795bc41862c25c30513142588f0e9dba1d`를 표시한다. | https://huggingface.co/api/models/google/gemma-4-E4B-it-qat-q4_0-gguf/tree/main?recursive=1 | 2026-07-06 | expected hash로 source-recorded, 다운로드 bytes 검증은 필요 |

## 아직 확정하지 않은 것

- 16 GB RAM 환경의 실제 성능과 안정성
- 5-marker adoption smoke보다 넓은 한국어 output guard 통과율
- 기록된 text-only mmproj 결과가 multimodal 사용에도 일반화되는지 여부

## artifact 선정 체크리스트

- upstream model card 확인
- license 확인
- GGUF 변환자 신뢰성 확인
- SHA-256 기록
- file size 기록
- context length 확인
- `llama.cpp` 호환성 확인
- Windows 실행 확인

## 문서화해야 할 항목

각 모델을 manifest에 넣을 때 다음 정보를 함께 남깁니다.

- upstream model name
- upstream URL
- artifact URL
- license
- redistribution policy
- quantization
- SHA-256
- evaluation result

## 미정 사항

두 static entry는 계속 `unverified`입니다. v0.30.0은 artifact, backend chat provenance, RAM/mmproj evidence, canonical benchmark linkage가 재검증되는 동안에만 host-local promotion을 허용하며 기록된 machine은 Qwen의 exact-response equality 실패 후 Gemma를 local default로 선택했습니다. 이는 두 모델을 bundle/재배포하거나 보편적 기본 모델을 확정하는 동작이 아닙니다.
