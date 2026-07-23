# Privacy

`rolling-potato` is local-first by default. User code, command output, and conversation content should be processed locally unless the user explicitly selects an external adapter later.

## Principles

- Default inference runs through a local model and local backend.
- Project files are read only inside the selected working directory.
- Telemetry is not part of the MVP default.
- Model weights are downloaded only after user approval.
- External backend adapters require explicit user configuration.

## Local Data

The following information may be stored in local config or logs:

- installed model IDs
- model file paths
- backend configuration
- approval records
- diagnostic results
- error logs
- per-model token usage and runtime metrics
- backend health metrics
- user-attached local file copies under the app-data attachment directory

The following must not be stored by default:

- API keys
- access tokens
- passwords
- private keys
- command output containing raw credentials
- complete backend prompts, hidden reasoning/raw model responses, or complete source-file bodies stored solely for monitoring

## Network Use

Allowed MVP network use:

- user-approved model manifest lookup
- user-approved model download
- optional release update checks, if users can disable them
- explicit or freshness-sensitive read-only web search; only the current question is
  sent to the fixed Exa MCP endpoint

Disallowed default behavior:

- automatic user-code upload
- automatic conversation upload
- attachment upload to the web-search provider
- command-output telemetry
- automatic fallback to an external LLM API

## Monitoring

`rolling-potato` may store local monitoring metrics such as per-model token usage, latency, backend health, and guard results.

Principles:

- monitoring is local-first
- external telemetry is not part of the MVP default
- durable local resume stores user turns and visible/normalized model, tool, and evidence turns; normalized patch actions store paths, action metadata, and hashes instead of find/replace or verification-command text
- the complete backend prompt, hidden/raw model response, complete source-file body, and credential-bearing command output are excluded from transcript storage
- SQLite may project those durable transcript records for local queries and can be rebuilt from canonical ledger/artifact state
- exports run only when the user invokes an export command

## External Adapters

Adapters such as LM Studio, Ollama, vLLM, and SGLang are used only when explicitly configured by the user.

The CLI must clearly display whether an adapter is local or remote.
