# Working on Gitbot

Gitbot is a small Rust CLI that dispatches a trusted GitHub Actions workflow
to post disclosed comments through the Gitbot GitHub App.

## Rules

- Keep the GitHub App private key and installation tokens out of the CLI,
  local files, logs, and command output.
- Keep the command surface small and focused on comments.
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
