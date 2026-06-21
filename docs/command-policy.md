# Command Policy

`rolling-potato`는 사용자의 로컬 프로젝트에서 파일을 읽고, patch를 만들고, 명령을 실행할 수 있습니다. 기본 정책은 보수적이어야 합니다.

## 기본 규칙

- 프로젝트 내부 파일 읽기는 허용합니다.
- 프로젝트 외부 파일 읽기는 기본적으로 제한합니다.
- 파일 쓰기는 diff 표시 후 사용자 승인이 필요합니다.
- side effect가 있는 명령은 사용자 승인이 필요합니다.
- destructive command는 기본 거부하거나 강한 확인을 요구합니다.
- credential로 보이는 값은 로그에 저장하지 않습니다.

## 읽기 정책

기본 허용:

- 현재 작업 디렉터리 내부 source file
- config file
- test file
- package manifest
- build script

기본 제외:

- `.git/`
- `node_modules/`
- `target/`
- `dist/`
- `build/`
- 대용량 binary
- model files
- credential file

## 쓰기 정책

쓰기 전 필수 단계:

1. 변경 이유 설명
2. diff 표시
3. 사용자 승인
4. patch 적용
5. 적용 결과 확인

승인 전에는 파일을 수정하지 않습니다.

## 명령 실행 정책

낮은 위험:

- read-only listing
- targeted test
- formatter check
- typecheck
- lint

승인 필요:

- dependency install
- package update
- file generation
- server start
- network download
- write/build artifact 생성

거부 또는 강한 확인 필요:

- recursive delete
- reset/checkout destructive operation
- credential 출력
- project 밖 파일 수정
- system-wide install
- production deploy

## Korean reporting

명령 실행 결과는 한국어로 요약합니다. 단, error code, command, file path, log line은 원문 보존이 가능합니다.

## MVP 테스트 요구

command policy는 fixture test로 검증해야 합니다.

- destructive command 차단
- write approval requirement
- project boundary enforcement
- credential redaction
- verification command approval
