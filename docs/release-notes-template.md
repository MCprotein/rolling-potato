# Release Notes Template

Use this template for every `rpotato` release note entry.

```markdown
## vX.Y.Z - Short Release Title

Release date: YYYY-MM-DD

One short paragraph describing the user-visible release outcome.

### Included

- Concrete behavior or artifact added.
- Concrete command, workflow, or document changed.

### Verified In This Release

- `cargo fmt --check`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- release policy check
- relevant smoke tests

### Boundary

State what this release intentionally does not include.
```
