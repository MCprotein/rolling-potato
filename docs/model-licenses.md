# Model Licenses

`rolling-potato`의 코드 라이선스와 모델 라이선스는 별개입니다.

- 프로젝트 코드: Apache-2.0
- 모델 가중치: 각 upstream model과 GGUF artifact provider license를 따름

## 원칙

- 모델 license를 확인하기 전에는 기본 추천 모델로 확정하지 않습니다.
- GGUF 변환본은 upstream model license와 artifact provider terms를 모두 확인합니다.
- manifest에는 모델별 license를 반드시 표시합니다.
- 재배포 가능 여부가 불명확하면 CLI가 직접 bundle하지 않습니다.

## 초기 후보

| Candidate | Role | Status | Notes |
| --- | --- | --- | --- |
| `Qwen3.5-4B` GGUF | 기본 후보 | 미확정 | 정확한 artifact, license, hash 확인 필요 |
| `Gemma 4 E4B` | 비교 후보 | 미확정 | license와 on-device 사용 조건 확인 필요 |
| `Qwen3.5-9B` GGUF | 품질 참고 후보 | 보류 | 16 GB RAM 기본값으로는 부담 큼 |

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
