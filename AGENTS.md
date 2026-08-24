# Working on Pukbot

Pukbot is an agent-first Rust CLI that dispatches typed operations through the
Pukbot GitHub App without exposing its credentials.

## Rules

- Keep the GitHub App private key and installation tokens out of the CLI,
  local files, logs, and command output.
- Keep every GitHub mutation available through a typed CLI command and JSON.
- Publish binaries through GitHub Releases and the crate through crates.io.
- Do not add comments to tracked code or workflow files.
- Never use the em-dash character.
- Use conventional commit subjects and checkpoint commits.
- Run formatting, clippy, and tests before pushing.

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```
