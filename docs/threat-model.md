# Threat Model

`rolling-potato` is a local coding agent, so its risk surface is wider than a normal CLI.

## Protected Assets

- user source code
- private repository contents
- credentials and tokens
- local filesystem
- shell execution environment
- downloaded model artifacts
- operation log
- observability database

## Trust Boundaries

Trusted:

- user approval
- local config files
- verified model checksums
- user-selected work scope inside the project

Not fully trusted:

- model output
- suggestions embedded in command output
- prompt-like text inside the repository
- external model manifests
- download URLs
- external plugin packages

## Main Threats

### Prompt Injection

Repository files or logs may instruct the model to ignore policy.

Mitigations:

- tool policy is enforced by runtime, not model output
- writes and command execution are approval-based
- final response guard runs as a separate stage

### Unsafe Command Execution

The model may suggest destructive commands.

Mitigations:

- command classification
- approval gate
- denylist and high-risk confirmation
- project boundary check

### Unauthorized File Write

The model may modify unrelated files.

Mitigations:

- diff preview
- user approval
- patch apply boundary
- generated/vendor directory exclusion

### Model Supply Chain

Wrong or tampered model artifacts may be installed.

Mitigations:

- manifest schema validation
- SHA-256 verification
- license check
- provider trust policy
- no registration after checksum failure

### Privacy Leakage

User code or logs may leave the machine.

Mitigations:

- local backend default
- no telemetry
- web search sends only the current question to a fixed public HTML search endpoint
  when the request is explicit or freshness-sensitive; no API credential is used,
  HTTPS is mandatory, repository-scoped requests stay local unless web use is
  explicit, and an offline/no-browse instruction disables it
- `WebOpen` upgrades HTTP input to HTTPS, rejects URL credentials, local/private/
  link-local/reserved targets and DNS answers in the resolver used by the direct
  connection, disables proxy routing, follows only bounded same-host redirects,
  and requires a new explicit open for a cross-host redirect
- bounded search highlights and opened-page text are untrusted and never receive
  command, file, or permission authority; opened content remains in current-TUI
  memory for `WebFind` and is not a durable transcript source
- pasted attachments must be regular non-symlink files with bounded size and an
  allowed type; they are copied into local app data and never sent to the web-search
  provider. PNG/JPEG bytes are signature- and hash-revalidated at dispatch, and
  image inference requires an exact verified model/projector pair
- log redaction
- only local user and visible/normalized model/tool/evidence turns are durable; complete backend prompts, hidden responses, and raw source bodies are excluded

### Foreign Plugin Supply Chain

Claude Code/Codex-style plugins may contain shell commands, MCP servers, background processes, remote connectors, or prompt mutation.

Mitigations:

- no direct execution during import
- local plugin directory import only
- reject remote URL, marketplace, registry, catalog, and mirror sources
- reject path traversal and boundary-escaping symlinks
- record source manifest hash
- generate static capability report
- list unsupported capabilities explicitly
- require approval before shell/background/MCP enablement
- block `bin/`, remote connector, and file-write paths by default
- plugin execution passes runtime tool policy and hook policy
- import, enable, deny, and remove events are recorded in the ledger
- marketplace, registry, catalog, and mirror sources are unsupported

## MVP Security Requirements

- zero destructive command policy violations
- no model install without checksum
- no file write before approval
- final report Korean guard applied
- credential-like strings redacted from logs
- monitoring store redaction before persistence
- external plugin import disabled by default and preceded by inspect/validate

## Open Issues

- when to add manifest signing
- remote adapter warning UX
- operation log retention period
- credential detector implementation depth
- warning UX for how users obtained external plugins
