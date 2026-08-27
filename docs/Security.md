# Security

The App private key remains in a protected GitHub Actions environment. The CLI,
release binaries, installers, Pages artifact, and crate contain no App key or
installation token.

Installation tokens are short-lived and scoped to one repository. Release
installers verify SHA-256 checksums. Crate publication uses GitHub OIDC through
crates.io Trusted Publishing.

The operation workflow requests Actions write access only for workflow
dispatches and controls. Other operations continue to mint tokens with their
existing issue, pull request, or repository content permissions.

Local image paths are uploaded as public release assets. Never attach secrets,
private repository content, personal data, or other non-public images.

Use GitHub private vulnerability reporting for suspected security problems.
