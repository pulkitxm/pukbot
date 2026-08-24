# Pukbot

[![CI](https://github.com/pulkitxm/Pukbot/actions/workflows/ci.yml/badge.svg)](https://github.com/pulkitxm/Pukbot/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Pukbot is a small Rust CLI for posting disclosed GitHub issue and pull request
comments through the Pukbot GitHub App.

The CLI never receives the GitHub App private key or an installation token. It
validates the request and dispatches a trusted workflow in this repository. A
protected GitHub Actions environment mints a repository-scoped token, appends
the disclosure footer, posts the comment, and discards the token.

## Usage

Post to an issue or pull request conversation:

```bash
pukbot comment 123 --repo owner/repository --body "the release is ready"
```

Read a multiline comment from a file:

```bash
pukbot comment 123 --repo owner/repository --body-file comment.md
```

Read from stdin:

```bash
pukbot comment 123 --repo owner/repository <comment.md
```

Preview the final disclosed comment without dispatching a workflow:

```bash
pukbot comment 123 --repo owner/repository --body "the release is ready" --dry-run
```

Every posted comment ends with:

```markdown
---

_Automated comment posted by Pukbot from an agent-assisted workflow._

from: @authenticated-user
```

After dispatching, the CLI finds the exact workflow run, follows its progress,
prints failed logs, returns a failing exit status when the workflow fails, and
prints the posted comment URL on success.

GitHub CLI must be installed and authenticated. The authenticated user needs
permission to dispatch the Comment workflow in `pulkitxm/Pukbot`.

## Installation

On Linux or macOS, install the latest private release with your authenticated
GitHub CLI session:

```bash
printf 'header = "Authorization: Bearer %s"\n' "$(gh auth token)" \
  | curl --proto '=https' --tlsv1.2 -LsSf --config - \
    -H 'Accept: application/vnd.github.raw' \
    -H 'X-GitHub-Api-Version: 2022-11-28' \
    https://api.github.com/repos/pulkitxm/Pukbot/contents/install.sh \
  | sh
```

The installer detects the operating system and CPU architecture, downloads the
matching binary from the latest GitHub Release, verifies its SHA-256 checksum,
and installs it to `$XDG_BIN_HOME` or `~/.local/bin`. It requires an
authenticated GitHub CLI because the repository and its releases are private.

Pass `--version` or `--bin-dir` to select a release or destination:

```bash
printf 'header = "Authorization: Bearer %s"\n' "$(gh auth token)" \
  | curl --proto '=https' --tlsv1.2 -LsSf --config - \
    -H 'Accept: application/vnd.github.raw' \
    -H 'X-GitHub-Api-Version: 2022-11-28' \
    https://api.github.com/repos/pulkitxm/Pukbot/contents/install.sh \
  | sh -s -- --version v0.2.0 --bin-dir "$HOME/.local/bin"
```

You can also download a binary manually from
[GitHub Releases](https://github.com/pulkitxm/Pukbot/releases), verify it
against `SHA256SUMS`, make it executable on Unix, and place it on `PATH`.

Pukbot is not published to Cargo and the package metadata prevents `cargo publish`.

## Trusted workflow setup

1. Install the Pukbot GitHub App on every repository where it may comment.
2. Create the `pukbot-production` Actions environment in this repository.
3. Restrict that environment to the protected `main` branch.
4. Add `PUKBOT_PRIVATE_KEY` as an environment secret.
5. Add `PUKBOT_CLIENT_ID` as an environment variable.
6. Protect `main` so workflow changes require human review.

The Comment workflow requests only issue and pull request write permissions
and narrows each token to one repository. It validates the repository, number,
body, and size independently of the CLI.

Repository secrets alone are not a sufficient boundary when an untrusted
process can modify and dispatch workflow files from arbitrary branches. The
protected environment and protected `main` branch keep the private key
available only to the reviewed workflow.

The caller still needs a normal GitHub credential to dispatch the workflow.
This design keeps the Pukbot private key and installation tokens away from the
caller. It does not hide capabilities already available through the caller's
own GitHub account.

## Repository instructions

Add this to a repository's `AGENTS.md`:

```text
Post automated GitHub issue and pull request conversation comments through
Pukbot. Do not call `gh issue comment` or `gh pr comment` directly.

pukbot comment <number> --repo <owner/repository> --body "<message>"

Use `--body-file` or stdin for multiline comments. Pukbot appends the required
disclosure footer.
```

## Releases

Releases are created only from version tags that match `Cargo.toml`:

```bash
git tag v0.1.0
git push origin v0.1.0
```

GitHub Actions builds Linux x86-64 and ARM64, macOS x86-64 and ARM64, and
Windows x86-64 binaries. The release contains those binaries and
`SHA256SUMS`.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
actionlint
```

## Possible next additions

- Wait for the dispatched workflow and return its final result
- Update the last Pukbot comment instead of adding another one
- Repository and organization allowlists
- Named comment templates
- A local confirmation mode for sensitive repositories
- Structured output for orchestration
- Release update checks without self-installation
