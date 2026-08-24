# Working on Pukbot

Pukbot is a small Rust CLI that dispatches a trusted GitHub Actions workflow
to post disclosed comments through the Pukbot GitHub App.

## Rules

- Keep the GitHub App private key and installation tokens out of the CLI,
  local files, logs, and command output.
- Keep the command surface small and focused on comments.
- Publish binaries only through GitHub Releases. Never publish this crate to Cargo.
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
