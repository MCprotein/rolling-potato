# Governance

`rolling-potato` is a public open-source repository, but it does not currently accept external code contributions or external pull requests.

## Maintainer Model

- Current maintainer: `MCprotein`
- Product direction, releases, license decisions, and security policy are managed by the maintainer.
- There is no current plan to delegate external maintainer authority.

## External Participation

Accepted:

- bug reports
- usability feedback
- security vulnerability reports
- model artifact or license evidence

Not accepted:

- external code pull requests
- external documentation pull requests
- external maintainer applications
- requests for roadmap decision authority

When useful, the maintainer may apply changes directly based on issue reports or evidence submitted by users.

## Automated Commit And Push Operation

When Codex works in this repository, the default flow after each safe and verifiable work unit is:

1. Review the change scope.
2. Run available verification.
3. Commit using Conventional Commits.
4. Push to `origin main`.

Unless the user explicitly stops this behavior, the flow continues without asking for separate commit or push confirmation each time.
