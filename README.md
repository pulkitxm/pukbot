# Pukbot

[![CI](https://github.com/pulkitxm/pukbot/actions/workflows/ci.yml/badge.svg)](https://github.com/pulkitxm/pukbot/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/pukbot.svg)](https://crates.io/crates/pukbot)
[![release](https://img.shields.io/github/v/release/pulkitxm/pukbot)](https://github.com/pulkitxm/pukbot/releases)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Pukbot is a small Rust CLI for posting disclosed GitHub issue and pull request
comments through the Pukbot GitHub App.

The CLI never receives the GitHub App private key or an installation token. A
protected GitHub Actions environment mints a short-lived, repository-scoped
token, posts the comment, and discards the token.

## Install

Install the public GitHub App on the repositories where comments may be posted:

[Install Pukbot on GitHub](https://github.com/apps/pulkit-pukbot)

Grant access only to the repositories that need Pukbot. The App requests issue
and pull request write access so the protected workflow can post comments.

Linux and macOS:

```bash
curl --proto '=https' --tlsv1.2 -fsSL https://pukbot.pulkit.page/install.sh | sh
```

Windows PowerShell:

```powershell
irm https://pukbot.pulkit.page/install.ps1 | iex
```

Cargo:

```bash
cargo install pukbot --locked
```

The installers download the matching binary from GitHub Releases and verify
its SHA-256 checksum before installation.

## Use

Post to an issue or pull request:

```bash
pukbot comment 123 --repo owner/repository --body "the release is ready"
```

Read a multiline comment from a file or standard input:

```bash
pukbot comment 123 --repo owner/repository --body-file comment.md
pukbot comment 123 --repo owner/repository <comment.md
```

Add an image from a local path:

```bash
pukbot comment 123 \
  --repo owner/repository \
  --body "the final result" \
  --image ./result.png
```

Local PNG, JPEG, GIF, and WebP files up to 10 MiB are uploaded as public assets
to Pukbot's `comment-assets` prerelease. Do not use this option for secrets or
private images.

An existing HTTP or HTTPS image URL also works:

```bash
pukbot comment 123 \
  --repo owner/repository \
  --body "the final result" \
  --image https://example.com/result.png
```

Multiple `--image` options are supported. Dry runs validate local images and
show their final asset URLs without uploading them.

Preview the final comment without dispatching:

```bash
pukbot comment 123 --repo owner/repository --body "the release is ready" --dry-run
```

Every comment ends with:

```markdown
---

*Automated comment posted by Pukbot from an agent-assisted workflow.*

from: @authenticated-user
```

The CLI follows the workflow, streams its status, shows failed logs, returns a
failing exit code on failure, and prints the posted comment URL on success.

GitHub CLI must be installed and authenticated. The authenticated user needs
permission to dispatch the Comment workflow in `pulkitxm/pukbot`. Local image
uploads also require permission to upload release assets to that repository.

## Agent instructions

Add this to `AGENTS.md`:

```text
Post automated GitHub issue and pull request conversation comments through
Pukbot. Do not call gh issue comment or gh pr comment directly.

pukbot comment <number> --repo <owner/repository> --body "<message>"

Use --body-file or stdin for multiline comments. Use --image with a local PNG,
JPEG, GIF, or WebP path, or an existing HTTP or HTTPS image URL. Local files are
uploaded publicly. Pukbot appends the required disclosure footer.
```

## Security

The Pukbot App private key is stored only as a GitHub Actions environment
secret. It is never embedded in the crate, release binaries, Pages artifacts,
or installer scripts. Installation tokens are narrowed to one repository and
expire automatically.

The release workflow uses crates.io Trusted Publishing with GitHub OIDC after
the initial crate publication. No long-lived crates.io token is stored in the
repository.

See [SECURITY.md](SECURITY.md) for vulnerability reporting and the complete
trust model.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo package --locked
sh tests/install.sh
```

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md).

Project governance and support are documented in [GOVERNANCE.md](GOVERNANCE.md)
and [SUPPORT.md](SUPPORT.md).
