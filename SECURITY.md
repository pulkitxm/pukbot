# Security Policy

## Supported Versions

Security fixes are applied to the latest published release and `main`.

| Version | Supported |
| --- | --- |
| Latest release | Yes |
| `main` | Yes |
| Older releases | No |

## Reporting a Vulnerability

Do not open a public issue for a suspected vulnerability. Use GitHub's
[private vulnerability reporting form](https://github.com/pulkitxm/gitbot/security/advisories/new).

Include the affected version and platform, reproduction steps, expected and
observed behavior, potential impact, and suggested mitigation. Remove
credentials, private repository contents, and personal data.

## Security Model

The CLI invokes GitHub CLI directly with argument arrays. It does not read or
store the Gitbot App private key, installation tokens, or crates.io publishing
credentials.

Local images are validated by file signature, limited to 10 MiB, assigned a
content-derived filename, and uploaded through the authenticated GitHub CLI
session to the public `comment-assets` prerelease. Never attach credentials,
private repository content, personal data, or other non-public images.

The comment workflow stores the App private key in a protected environment,
mints a short-lived installation token scoped to one repository, requests only
issue and pull request write access, validates every input independently, and
discards the token after the job.

Release installers use fixed GitHub release URLs and verify SHA-256 checksums
before installing binaries. Pages contains only static installation files.
Crates.io releases use OIDC Trusted Publishing after the first publication, so
GitHub does not store a long-lived crates.io token.

Protect `main`, require review for workflow changes, restrict the production
environment to `main`, and do not grant agents permission to change protection
rules or environment secrets.

Reports are especially useful when they demonstrate command injection,
credential exposure, workflow authorization bypass, checksum bypass, or access
outside the requested repository.

## Safe Harbor

Good-faith research that follows this policy is considered authorized. Avoid
privacy violations, service disruption, destructive testing, social
engineering, and access beyond what is necessary to demonstrate the issue.
