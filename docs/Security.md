# Security

The App private key remains in a protected GitHub Actions environment. The CLI,
release binaries, installers, Pages artifact, and crate contain no App key or
installation token.

Installation tokens are short-lived and scoped to one repository. Release
installers verify SHA-256 checksums. Crate publication uses GitHub OIDC through
crates.io Trusted Publishing.

Use GitHub private vulnerability reporting for suspected security problems.
