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

The following must not be stored by default:

- API keys
- access tokens
- passwords
- private keys
- command output containing raw credentials
- raw source code or raw prompts in the monitoring database

## Network Use

Allowed MVP network use:

- user-approved model manifest lookup
- user-approved model download
- optional release update checks, if users can disable them

Disallowed default behavior:

- automatic user-code upload
- automatic conversation upload
- command-output telemetry
- automatic fallback to an external LLM API

## Monitoring

`rolling-potato` may store local monitoring metrics such as per-model token usage, latency, backend health, and guard results.

Principles:

- monitoring is local-first
- external telemetry is not part of the MVP default
- raw prompts, raw source code, and command output containing credentials are not stored in the monitoring database by default
- exports run only when the user invokes an export command

## External Adapters

Adapters such as LM Studio, Ollama, vLLM, and SGLang are used only when explicitly configured by the user.

The CLI must clearly display whether an adapter is local or remote.
