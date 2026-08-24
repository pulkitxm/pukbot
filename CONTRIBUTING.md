# Contributing to Gitbot

Bug reports, design feedback, documentation, tests, and code are welcome.

## Before You Start

- Search existing issues and pull requests before opening a duplicate.
- Open an issue before a substantial feature or architecture change.
- Keep pull requests focused.
- Never include credentials, private repository data, or build artifacts.

## Development Setup

Gitbot requires Rust 1.85 or newer and GitHub CLI.

```bash
git clone https://github.com/pulkitxm/Gitbot.git
cd Gitbot
cargo test --locked
```

## Required Checks

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo package --locked
sh tests/install.sh
```

Add focused tests for behavior changes. Keep the command surface small and
pass arguments directly to subprocesses without building shell commands from
user input.

## Pull Requests

Use conventional commit subjects. Keep the pull request description to one
line. After verification, post one concise evidence comment showing the final
result when there is something meaningful to demonstrate.

By contributing, you agree that your contribution is licensed under the MIT
License.

## Security Reports

Do not publish exploitable security issues in a public issue. Follow
[SECURITY.md](SECURITY.md) and use GitHub private vulnerability reporting.
