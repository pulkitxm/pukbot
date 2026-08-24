# Architecture

Gitbot separates public commands from private GitHub App credentials.

The public Rust CLI validates input, resolves the authenticated GitHub CLI user,
and dispatches the repository's `comment.yml` workflow. It never receives the
GitHub App private key or an installation token.

The workflow reads the private key from the protected `gitbot-production`
environment, creates a short-lived installation token scoped to the requested
repository, posts the comment, and discards the token when the job ends.

The release workflow builds five platform artifacts, verifies them, publishes
the crate through crates.io Trusted Publishing, produces checksums and an SBOM,
attests the binaries, and creates the GitHub Release. GitHub Pages receives only
the two installer scripts and the custom-domain file.
