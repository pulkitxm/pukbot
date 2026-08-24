# Architecture

Pukbot separates public commands from private GitHub App credentials.

The public Rust CLI validates typed input, resolves the authenticated GitHub
CLI user, and dispatches the repository's `operation.yml` workflow. It never
receives the GitHub App private key or an installation token.

The workflow reads the private key from the protected `pukbot-production`
environment, creates a short-lived installation token scoped to the requested
repository, performs one validated operation, and discards the token.

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
