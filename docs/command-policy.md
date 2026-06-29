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

## Plugin Import And Capability Policy

Plugin import는 local path만 허용합니다.

허용:

- 사용자가 직접 지정한 local plugin directory 읽기
- `.codex-plugin/plugin.json` 또는 `.claude-plugin/plugin.json` manifest parse
- dry-run inspect/validate report 생성
- 승인 후 app data root로 plugin source snapshot 생성

거부:

- remote URL import
- 외부 marketplace import
- 외부 registry/catalog import
- third-party package mirror
- path traversal
- symlink를 통한 project/app data boundary 우회

Plugin import는 실행 권한을 주지 않습니다. 다음 capability는 기본 차단하고, 사용자가 실제로 enable 또는 run하려 할 때 capability별 승인 prompt를 표시해야 합니다.

- shell command
- `bin/` executable
- MCP server
- background process
- remote connector
- file write path
- download path

Plugin enable 전에는 다음을 한국어로 보여줘야 합니다.

- source runtime: Codex or Claude Code
- source manifest path
- source manifest hash
- imported capability list
- unsupported capability list
- required permissions
- copied app data path
- plugin data path

Plugin이 가져온 skill, hook, subagent, MCP capability도 runtime tool policy, hook policy, ledger, evidence gate를 우회할 수 없습니다.

## Korean reporting

명령 실행 결과는 한국어로 요약합니다. 단, error code, command, file path, log line은 원문 보존이 가능합니다.

## MVP 테스트 요구

command policy는 fixture test로 검증해야 합니다.

- destructive command 차단
- write approval requirement
- project boundary enforcement
- credential redaction
- verification command approval
- local plugin import only
- remote plugin URL rejection
- plugin marketplace/registry/catalog rejection
- plugin path traversal rejection
- blocked plugin capability approval prompt
