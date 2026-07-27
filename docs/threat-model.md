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
- agent-selected web search sends only a bounded literal projection of the current
  user request to a fixed public HTML search endpoint; values found only in
  conversation history are rejected, the routing model never receives local
  attachment contents, no API credential is used, HTTPS is mandatory, and an
  offline/no-browse instruction disables retrieval
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

### Restricted Browser Abuse

A public page may attempt SSRF, DNS rebinding, prompt injection, authenticated
side effects, unintended file transfer, or process/profile leakage.

Mitigations:

- the model may request only the typed `search-form` operation; a deterministic
  fallback exists only for explicit public-search-site instructions
- every run uses a new temporary profile and process group; the integrated route
  never loads the user's browser profile, cookies, passwords, or login session
- all page traffic is forced through a repo-owned loopback HTTPS CONNECT proxy;
  only port 443 is accepted, DNS is resolved and checked for a public address,
  that address is pinned for the connection, QUIC is disabled, non-proxied WebRTC
  UDP is disabled, and no direct fallback exists
- the runtime observes a bounded accessibility tree and acts on opaque handles;
  site-specific selectors and `Runtime.evaluate` JavaScript are not exposed
- the application coordinator types only the bounded search query, submits it,
  and reads a verified public final URL plus bounded text; it exposes no login,
  payment, posting, upload, download, personal-data, project-content, or
  attachment-submission path
- navigation attempts and reads have bounded attempts, time, bytes, and element
  counts; process groups and temporary profiles are cleaned up on success,
  timeout, and error
- page content remains untrusted evidence and cannot widen command, file,
  approval, or permission authority
- an explicit offline/no-browse instruction disables the restricted browser path

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
