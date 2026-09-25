# Architecture

Pukbot separates public commands from private GitHub App credentials, and
separates operations authored by the user from operations authored by the App.

The public Rust CLI validates typed input, resolves the authenticated GitHub
CLI user, and routes the operation. It never receives the GitHub App private
key or an installation token.

By default, pull request and stack API operations execute locally through the
authenticated GitHub CLI session, so GitHub records the user as the author,
reviewer, merger, and author of the squash commit. No workflow runs and no App
token is minted. `pukbot stack` preserves the installed gh-stack extension
interface for local, interactive, and composite workflows, while routing merge
through Pukbot's direct squash-only implementation.
`pukbot stack-api` provides noninteractive create, append, unstack, inspection,
and asynchronous merge through GitHub's native stack endpoints. Pull request
merge checks stack membership and selects the asynchronous endpoint when
needed.

Commits execute locally through the authenticated GitHub CLI session by
default, so the user is the author and committer, staged modes and binary
content are preserved, and workflow files can change under the session's
workflow scope. After a commit lands, the CLI fast-forwards the local branch
with a soft reset only when the checkout is on that branch at the commit's
parent, so the index and working tree are never modified.

Comment, issue, App-authored commit, and workflow dispatch operations dispatch
the repository's `operation.yml` workflow. The workflow reads the private key from
the protected `pukbot-production` environment, creates a short-lived
installation token scoped to the requested repository and required permission,
performs one validated operation, and discards the token. App-authored commits
request the workflows permission only when they touch `.github/workflows`.
Workflow dispatches return the created target run URL.

For local media paths, the CLI validates the file and uploads it through the
authenticated GitHub CLI session. Images and video the GitHub attachment
endpoint accepts become user attachments on the repository the comment is
posted to, and every other supported type becomes a content-named public asset
in the `comment-assets` prerelease. Named placeholders decide where the
resulting Markdown is inserted in a comment.

The release workflow builds five platform artifacts, verifies them, publishes
the crate through crates.io Trusted Publishing, produces checksums and an SBOM,
attests the binaries, generates completions and a manual, and creates the GitHub
Release. GitHub Pages receives only the two installer scripts and the
custom-domain file.

The self-updater resolves a stable release tag, downloads the platform asset and
checksum document with fixed limits, verifies SHA-256, and only then replaces
the resolved executable while preserving its permissions.
