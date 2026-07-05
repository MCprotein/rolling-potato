# Command Policy

`rolling-potato` can read files, create patches, and run commands inside the user's local project. The default policy must be conservative.

## Default Rules

- Project-internal file reads are allowed.
- File reads outside the project are restricted by default.
- File writes require diff display and user approval.
- Commands with side effects require user approval.
- Destructive commands are denied by default or require strong confirmation.
- Credential-like values are not stored in logs.

## Read Policy

Allowed by default:

- source files inside the current working directory
- config files
- test files
- package manifests
- build scripts

Excluded by default:

- `.git/`
- `node_modules/`
- `target/`
- `dist/`
- `build/`
- large binaries
- model files
- credential files

## Write Policy

Required steps before writing:

1. explain the reason for the change
2. show the diff
3. receive user approval
4. apply the patch
5. confirm the application result

Files are not modified before approval.

## Command Execution Policy

Low risk:

- read-only listing
- targeted test
- formatter check
- typecheck
- lint

Approval required:

- dependency install
- package update
- file generation
- server start
- network download
- write/build artifact generation

Denied or strong confirmation required:

- recursive delete
- destructive reset/checkout operation
- credential output
- modification outside the project
- system-wide install
- production deploy

## Current Implementation

Phase 4 currently implements:

- `rpotato policy schema`
- `rpotato policy check-command <command>`
- `rpotato policy check-path --read <path>`
- `rpotato policy check-path --write <path>`
- `rpotato policy redact <text>`

The current classifier does not execute commands; it returns decisions only.

- `allow`: read/verification command or project-internal read path
- `ask`: command with possible side effects, network/download/dependency command, or project-internal write path
- `deny`: destructive command, path outside project boundary, `..` traversal, or excluded path

Every `check-command` and `check-path` result is recorded as a permission decision audit event in the ledger.

Phase 4 schema:

- action kinds: read file, write file, run command, apply patch, network download, plugin capability
- rule sources: user, project, local, session, policy
- action status: create, update, noop, user-modified, blocked
- write policy: diff-before-write and user approval required
- managed artifact policy: manifest/hash tracking required before download/install
- network policy: download/remote connector is `ask`

## Plugin Import And Capability Policy

Plugin import accepts only local paths.

Allowed:

- read local plugin directory specified by the user
- parse `.codex-plugin/plugin.json` or `.claude-plugin/plugin.json`
- generate dry-run inspect/validate report
- create a plugin source snapshot under app data root after approval

Denied:

- remote URL import
- external marketplace import
- external registry/catalog import
- third-party package mirror
- path traversal
- project/app-data boundary escape through symlinks

Plugin import does not grant execution authority. The following capabilities are blocked by default and must show per-capability approval prompts when the user enables or runs them:

- shell command
- `bin/` executable
- MCP server
- background process
- remote connector
- file write path
- download path

Before plugin enablement, the runtime should show in Korean:

- source runtime: Codex or Claude Code
- source manifest path
- source manifest hash
- imported capability list
- unsupported capability list
- required permissions
- copied app data path
- plugin data path

Imported skills, hooks, subagents, and MCP capabilities cannot bypass runtime tool policy, hook policy, ledger, or evidence gates.

## Korean Reporting

Command execution results are summarized in Korean. Error codes, commands, file paths, and log lines may preserve their original text.

## MVP Test Requirements

Command policy must be verified with fixture tests:

- destructive command rejection
- write approval requirement
- project boundary enforcement
- credential redaction
- verification command approval
- local plugin import only
- remote plugin URL rejection
- plugin marketplace/registry/catalog rejection
- plugin path traversal rejection
- blocked plugin capability approval prompt
