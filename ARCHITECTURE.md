# Architecture

Pukbot separates public commands from private GitHub App credentials, and
separates operations authored by the user from operations authored by the App.

The public Rust CLI validates typed input, resolves the authenticated GitHub
CLI user, and routes the operation. It never receives the GitHub App private
key or an installation token.

By default, pull request and stack API operations execute locally through the
authenticated GitHub CLI session, so GitHub records the user as the author,
reviewer, merger, and author of the squash commit. No workflow runs and no App
token is minted. `pukbot stack` preserves the complete installed gh-stack
extension interface for local, interactive, and composite workflows.
`pukbot stack-api` provides noninteractive create, append, unstack, inspection,
and asynchronous merge through GitHub's native stack endpoints. Pull request
merge checks stack membership and selects the asynchronous endpoint when
needed.

Comment, issue, commit, and workflow dispatch operations dispatch the
repository's `operation.yml` workflow. The workflow reads the private key from
the protected `pukbot-production` environment, creates a short-lived
installation token scoped to the requested repository and required permission,
performs one validated operation, and discards the token. Commits carry the
requesting user as the commit author and the App as the committer. Workflow
dispatches return the created target run URL.

For local media paths, the CLI validates the file and uploads a content-named
public asset to the `comment-assets` prerelease through the authenticated
GitHub CLI session. Named placeholders decide where the resulting Markdown is
inserted in a comment.

The release workflow builds five platform artifacts, verifies them, publishes
the crate through crates.io Trusted Publishing, produces checksums and an SBOM,
attests the binaries, generates completions and a manual, and creates the GitHub
Release. GitHub Pages receives only the two installer scripts and the
custom-domain file.

The self-updater resolves a stable release tag, downloads the platform asset and
checksum document with fixed limits, verifies SHA-256, and only then replaces
the resolved executable while preserving its permissions.
