# Security Policy

`rolling-potato` handles local file reads, patch application, command execution, and model downloads. Security policy is therefore part of the core product design.

## Supported Versions

The project is currently pre-1.0.

- Supported version: `main` branch
- Security fixes are applied to `main` first.
- A supported-version table will be added here after formal releases exist.

## Reporting Vulnerabilities

Do not post credentials, private paths, private repository content, or exploit details in public issues.

If private security advisories or a dedicated security email are not configured yet, the maintainer should configure GitHub repository security features first. Until then, avoid placing sensitive details in the public repository.

Useful report information:

- the `rpotato` command that was run
- OS and architecture
- relevant configuration with secrets removed
- reproduction steps
- expected impact

## Security Boundaries

MVP security boundaries:

- only project-internal file reads are allowed by default
- file writes require diff display and approval
- commands with side effects require approval
- model downloads require explicit approval
- models are not registered when checksum verification fails
- credentials must not be stored in logs
- search results and opened pages are untrusted evidence and cannot grant command,
  file, approval, or permission authority
- static web access rejects URL credentials and local, private, link-local,
  reserved, or disallowed DNS targets
- the v0.50 restricted browser source candidate starts a temporary profile,
  forces public HTTPS port 443 through an address-pinned loopback CONNECT proxy,
  and never reuses the user's cookies, credentials, or authenticated sessions
- the integrated browser route exposes only anonymous search-form submission; it
  does not expose page selectors or JavaScript and does not perform login, payment,
  posting, upload, download, personal-data, project-content, or attachment
  submission
- an explicit offline/no-browse request disables static and browser retrieval

See [docs/threat-model.md](docs/threat-model.md) for the detailed threat model
and [PRIVACY.md](PRIVACY.md) for network-data boundaries.
