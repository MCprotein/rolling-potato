# Glossary

이 문서는 `rolling-potato`에서 쓰는 핵심 용어를 고정합니다.

## Agent Runtime

사용자 요청을 받아 모델, context, 도구, patch, 검증, 보고를 하나의 통제된 흐름으로 실행하는 시스템입니다.

`rolling-potato`의 제품 본체입니다. CLI는 runtime을 사용하는 surface입니다.

## Surface

사용자가 runtime에 접근하는 입구입니다.

MVP surface는 `rpotato` CLI입니다. Surface는 표시와 승인을 맡고, 정책과 상태는 runtime core에 위임합니다.

## CLI Surface

`rpotato` 명령입니다.

역할:

- command parsing
- user prompt
- approval display
- diff display
- progress display
- final report display

## TUI Surface

터미널 안에서 runtime state, transcript, diff, approval, tool output, subagent/team status, evidence를 보여주는 interactive surface입니다.

TUI는 정책을 소유하지 않고 runtime core에 사용자 결정을 전달합니다.

## Runtime Core

상태, 정책, 온톨로지, context, agent loop, evidence, stop gate를 소유하는 내부 계층입니다.

역할:

- state and ledger
- hooks
- skills
- model/backend management
- ontology/context lifecycle
- subagents and teams
- tool policy
- patch and verification
- Korean output guard

## Hook

Runtime lifecycle control point입니다.

예:

- `pre_model_request`
- `pre_tool_call`
- `pre_patch_apply`
- `stop_gate`

Hook은 권한을 넓힐 수 없고, runtime policy보다 느슨해질 수 없습니다.

## Skill

재사용 가능한 runtime capability입니다.

Prompt template만이 아니라 context requirements, allowed tools, hooks, evidence requirements, stop criteria를 함께 가집니다.

## Subagent

Runtime core가 parent workflow 아래에서 실행하는 bounded worker agent입니다.

Subagent는 전역 상태를 소유하지 않고, runtime policy와 context boundary를 상속합니다.

## Team Runtime

여러 subagent를 하나의 parent workflow 아래에서 stage별로 조정하는 runtime 기능입니다.

Team runtime은 plan, dispatch, execute, review, verify, merge, report 흐름을 ledger와 evidence gate 뒤에서 관리합니다.

## Backend

모델 추론을 실행하는 엔진입니다. MVP backend는 `llama.cpp` sidecar입니다.

Backend는 coding agent 정책을 소유하지 않습니다.

## Model Artifact

GGUF 같은 모델 파일입니다. Third-party artifact이며 `rolling-potato` 코드 라이선스와 별개입니다.

## Manifest

모델 또는 backend artifact의 신뢰 정보를 담는 파일입니다.

필수 정보:

- source
- URL
- license
- checksum
- file size
- compatibility

## Agent Loop

작업을 단계적으로 진행하는 runtime 흐름입니다.

MVP 단계:

- planner
- executor
- verifier
- reporter

## Tool Policy

파일 쓰기, command 실행, 다운로드, 삭제 같은 side effect를 통제하는 runtime 정책입니다.

모델 출력은 tool policy를 우회할 수 없습니다.

## Layer A Facts

Runtime이 deterministic하게 수집할 수 있는 repo 사실입니다.

예:

- file list
- source hash
- package manifest
- test command 후보
- entrypoint 후보

## Layer B Ontology

프로젝트 의미 구조입니다. Runtime 또는 agent가 보강할 수 있지만 source ref와 confidence가 필요합니다.

예:

- domain entity
- relationship
- ownership
- invariant
- workflow
- open question

## Source Pointer

Context snippet 대신 원본 위치를 가리키는 안정적인 참조입니다.

중요 판단 전에는 source pointer를 원본 파일 read로 승격해야 합니다.

## Context Index

Runtime이 context 검색을 위해 유지하는 index입니다. Snippet은 힌트이지 authoritative source가 아닙니다.

## Evidence

완료나 claim을 뒷받침하는 검증 결과입니다.

예:

- test output
- command exit code
- file hash
- source URL
- benchmark log

## Ledger

Runtime event와 evidence를 append-only로 남기는 기록입니다.

현재 상태 view와 ledger는 분리합니다.

## Stop Gate

작업 완료 여부를 결정하는 runtime gate입니다. 모델이 "끝났다"고 말해도 evidence가 부족하면 완료가 아닙니다.
