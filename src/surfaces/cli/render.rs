pub(crate) const HELP: &str = "\
rpotato

사용법:
  rpotato doctor
  rpotato install
  rpotato install --clean --dry-run
  rpotato install --clean --yes
  rpotato init
  rpotato run \"<request>\"
  rpotato intent classify \"<request>\"
  rpotato intent routes
  rpotato config
  rpotato state
  rpotato state reconcile
  rpotato state resume
  rpotato session list
  rpotato session history
  rpotato session resume <session-id>
  rpotato session new
  rpotato team status
  rpotato team plan --manifest <project-relative-json>
  rpotato team execute --team <team-id>
  rpotato team reconcile --team <team-id>
  rpotato team cancel --team <team-id>
  rpotato team admit --lanes <count> [--write <path>] [--write-owner <lane:path>] [--command <command>]
  rpotato team dispatch --lanes <count> --write-owner <lane:path> [--failed-lane <lane>] [--failure <reason>]
  rpotato team governor --lanes <count> --context-tokens <tokens> [--context-limit <tokens>] [--model-tier small|standard|large]
  rpotato subagent launch --role <role> --task <text> --tool <tool> --read <path> [--tool <tool>] [--read <path>] [--write <path>] [--timeout-ms <ms>] [--max-tokens <tokens>]
  rpotato subagent status [subagent-id]
  rpotato subagent cancel <subagent-id>
  rpotato resume [session-id]
  rpotato continue [session-id]
  rpotato tui
  rpotato tui interactive
  rpotato tui monitor
  rpotato tui sessions
  rpotato tui transcript <session-id>
  rpotato tui approvals
  rpotato tui diff <proposal-id>
  rpotato tui evidence
  rpotato cancel
  rpotato evidence validate <artifact-pointer>
  rpotato skill list
  rpotato skill run <id> \"<request>\"
  rpotato policy schema
  rpotato policy check-command <command>
  rpotato policy check-path --read <path>
  rpotato policy check-path --write <path>
  rpotato policy redact <text>
  rpotato hooks list
  rpotato hooks validate-result <json>
  rpotato patch preview --path <path> --find <text> --replace <text>
  rpotato patch approve <proposal-id> --token <token> [--dry-run]
  rpotato patch verify <proposal-id> --token <token>
  rpotato patch token-rotate <proposal-id>
  rpotato backend doctor
  rpotato backend install-plan
  rpotato backend install
  rpotato backend start --model <path> [--ctx-size <tokens>]
  rpotato backend status
  rpotato backend stop
  rpotato backend cancel
  rpotato backend verify-archive <path> --sha256 <hash>
  rpotato backend health-check
  rpotato backend chat --prompt <text> [--max-tokens <tokens>] [--stream] [--timeout-ms <ms>]
  rpotato cache status
  rpotato monitor status
  rpotato monitor models
  rpotato monitor baseline
  rpotato monitor optimize
  rpotato monitor export --format jsonl
  rpotato monitor export --format csv
  rpotato monitor export --format html
  rpotato monitor prune --before 30d --dry-run
  rpotato ontology status
  rpotato ontology seed
  rpotato ontology inspect
  rpotato ontology context --query <text>
  rpotato ontology reread <source-pointer>
  rpotato ontology export --format json
  rpotato ontology export --format jsonl
  rpotato ontology import --file <path> --dry-run
  rpotato benchmark validate <fixture.json>
  rpotato benchmark record --fixture <fixture.json>
  rpotato benchmark run --fixture <fixture.json> --prompt <artifact> [--max-tokens <tokens>]
  rpotato benchmark report --format jsonl
  rpotato model list
  rpotato model manifest
  rpotato model inspect <id>
  rpotato model registry
  rpotato model download-plan <id>
  rpotato model eval-plan <id>
  rpotato model benchmark-plan <id>
  rpotato model fetch-candidate <id> --for-evaluation
  rpotato model verify-file <path> --sha256 <hash>
  rpotato model promote <id> --evidence <file>
  rpotato model cleanup-failed <id> --dry-run
  rpotato model install <id>
  rpotato plugin import --from codex <local-path> --dry-run
  rpotato plugin import --from claude-code <local-path> --dry-run
  rpotato plugin list
  rpotato plugin inspect <id>
  rpotato plugin validate <id>
  rpotato plugin enable <id>
  rpotato plugin disable <id>
  rpotato plugin remove <id> --keep-data
  rpotato plugin remove <id> --purge-data
  rpotato uninstall --keep-cache
  rpotato uninstall --purge-cache
  rpotato uninstall --dry-run --purge-cache
  rpotato uninstall --clean --dry-run
  rpotato uninstall --clean --yes

patch workflow 규칙:
  run이 만든 proposal은 verification plan을 미리 binding합니다.
  patch approve는 patch만 적용하고 patch verify는 별도 승인 후 command를 실행합니다.
  state resume은 pending approval에서 backend를 다시 호출하지 않습니다.
  verification command는 proposal에 binding되며 CLI에서 바꿀 수 없습니다.

현재 상태:
  install은 사용자 전용 binary와 PATH를 멱등 등록하고, clean install/uninstall은 dry-run/명시적 확인 및 active runtime 차단을 요구합니다.
  backend install은 source-backed manifest와 SHA-256 검증을 거친 뒤 관리형 release payload를 배치합니다.
  backend start/status/stop/chat/cancel은 managed sidecar lifecycle, SSE chat streaming, generation 취소를 다룹니다.
  team status는 최신 resource sample 기준의 read-only admission preview와 sequential fallback 결정을 표시합니다.
  team plan은 canonical team manifest를 active parent workflow에 binding하고 durable team-plan state를 기록합니다.
  team execute는 durable team plan의 모든 member를 resource pressure에 따라 병렬 또는 순차 실행합니다.
  team reconcile은 complete worker set과 evidence를 검증해 parent에 원자적으로 merge하고 stop gate를 통과시킵니다.
  team cancel은 durable marker를 기록해 active team worker 전체에 취소를 전파합니다.
  team admit은 dispatcher 진입 전 resource/policy/file-ownership admission gate를 강제하고 결과를 ledger에 기록합니다.
  team dispatch는 dispatch 직전 file ownership을 다시 강제하고 failed-worker continuation 상태를 ledger에 기록합니다.
  team governor는 dispatcher 진입 전 context/model budget clamp와 downgrade/escalation hint를 기록합니다.
  benchmark record는 metadata-only not-comparable run을 기록하고, benchmark run은 실행 중인 backend sidecar로 local measured run을 기록합니다.
  monitor optimize는 측정된 local metric과 benchmark evidence만으로 context/lane/fallback/model route hint를 추천합니다.
  ontology store는 project-local typed graph JSONL을 canonical runtime store로 두고, source-pointer-first compact context view와 원문 reread rule을 제공합니다.
  모델 registry install은 source-backed manifest와 local promotion evidence가 검증되기 전까지 차단되며, 검증용 artifact fetch는 --for-evaluation을 요구합니다.";
