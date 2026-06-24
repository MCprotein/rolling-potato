# Model Licenses

`rolling-potato`의 코드 라이선스와 모델 라이선스는 별개입니다.

- 프로젝트 코드: Apache-2.0
- 모델 가중치: 각 upstream model과 GGUF artifact provider license를 확인한 뒤 기록해야 함

이 문서는 `Qwen`과 `Gemma` 후보만 다룹니다. `llama.cpp`는 추론 backend 이름일 뿐 모델 후보가 아니며, `llama.cpp`를 쓴다는 이유로 Meta Llama 계열 모델이나 그 라이선스를 이 프로젝트 후보에 끌어오지 않습니다.

## 원칙

- 모델 license를 확인하기 전에는 기본 추천 모델로 확정하지 않습니다.
- GGUF 변환본은 upstream model license와 artifact provider terms를 모두 확인합니다.
- manifest에는 모델별 license를 반드시 표시합니다.
- 재배포 가능 여부가 불명확하면 CLI가 직접 bundle하지 않습니다.
- 모든 모델 관련 claim은 [model-source-policy.md](model-source-policy.md)를 따릅니다.

## 초기 후보

| Candidate | Role | Status | Notes |
| --- | --- | --- | --- |
| `Qwen3.5-4B` GGUF | 우선 평가 후보 | upstream license 확인, GGUF 미선정 | 정확한 artifact, hash, runtime fit 확인 필요 |
| `Gemma 4 E4B` GGUF | 비교 후보 | upstream license 확인, GGUF 미선정 | 정확한 artifact, hash, runtime fit 확인 필요 |
| `Qwen3.5-9B` GGUF | 품질 참고 후보 | upstream license 확인, 제품 기본값 보류 | RAM 영향과 runtime fit은 측정 전 미확정 |

## 확인된 upstream 출처

아래는 upstream 모델 자체에 대한 확인이며, GGUF 변환본의 배포자, checksum, file size, `llama.cpp` 호환성, 16 GB 실행 가능성, 제품 기본 모델 적합성까지 확정하지는 않습니다.

| Claim | Source | Checked-at | Status |
| --- | --- | --- | --- |
| `Qwen/Qwen3.5-4B`의 Hugging Face model card license field는 `apache-2.0`이다. | https://huggingface.co/Qwen/Qwen3.5-4B | 2026-06-24 | confirmed |
| `Qwen/Qwen3.5-9B`의 Hugging Face model card license field는 `apache-2.0`이다. | https://huggingface.co/Qwen/Qwen3.5-9B | 2026-06-24 | confirmed |
| `google/gemma-4-E4B`의 Hugging Face model card license field는 `apache-2.0`이고, Google AI for Developers의 Gemma 4 license 문서는 Apache License 2.0을 게시한다. | https://huggingface.co/google/gemma-4-E4B, https://ai.google.dev/gemma/apache_2 | 2026-06-24 | confirmed |

## 아직 확정하지 않은 것

- 기본 추천 모델
- 사용할 GGUF artifact URL
- GGUF artifact provider의 license/terms
- SHA-256과 file size
- `llama.cpp` 실제 호환성
- 16 GB RAM 환경의 실제 성능과 안정성
- 한국어 출력 guard 통과율

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

정확한 `Qwen3.5-4B` artifact는 아직 선택하지 않았습니다. 선택 전 [docs/model-eval.md](model-eval.md)와 [docs/benchmarks.md](benchmarks.md)의 평가를 먼저 수행합니다.
