# Pukbot

[![CI](https://github.com/pulkitxm/pukbot/actions/workflows/ci.yml/badge.svg)](https://github.com/pulkitxm/pukbot/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/pukbot.svg)](https://crates.io/crates/pukbot)
[![release](https://img.shields.io/github/v/release/pulkitxm/pukbot)](https://github.com/pulkitxm/pukbot/releases)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Pukbot is an agent-first Rust CLI for typed GitHub operations through the
Pukbot GitHub App.

Comment, issue, commit, repository dispatch, and workflow operations run
through the App. The CLI never receives the GitHub App private key or an
installation token. A protected GitHub Actions environment mints a short-lived,
repository-scoped token, performs the operation, and discards the token.

Pull request operations run through your own authenticated GitHub CLI session
instead, so GitHub records you as the pull request author. See
[Attribution](#attribution).

## Install

Install the public GitHub App on the repositories where comments may be posted:

[Install Pukbot on GitHub](https://github.com/apps/pukbot)

Grant access only to the repositories that need Pukbot. The App requests issue,
repository content, and Actions write access so the protected workflow can
post comments, create commits, and dispatch workflows.

Linux and macOS:

```bash
curl --proto '=https' --tlsv1.2 -fsSL https://gitbot.pulkit.page/install.sh | sh
```

Windows PowerShell:

```powershell
irm https://gitbot.pulkit.page/install.ps1 | iex
```

Cargo:

```bash
cargo install pukbot --locked
```

The installers download the matching binary from GitHub Releases and verify
its SHA-256 checksum before installation.

Update an installed release binary with the same checksum verification:

```bash
pukbot update
pukbot update --check --json
```

Generate or install shell completions for Bash, Zsh, Fish, Elvish, and
PowerShell:

```bash
pukbot completions zsh
pukbot completions --install
```

Print a manual page or generate one page per command:

```bash
pukbot man
pukbot man --dir ./man
```

## Use

Create a comment on an issue or pull request:

```bash
pukbot comment create 123 --repo owner/repository --body "the release is ready"
```

Read a multiline comment from a file or standard input:

```bash
pukbot comment create 123 --repo owner/repository --body-file comment.md
pukbot comment create 123 --repo owner/repository <comment.md
```

Comment, issue, pull request, and review bodies are GitHub-flavored Markdown
posted verbatim, so code fences, tables, task lists, collapsible sections,
alerts, and Mermaid diagrams all render out of the box:

````bash
pukbot comment create 123 --repo owner/repository --body-file - <<'EOF'
### CI result

| Check  | Status |
| ------ | ------ |
| fmt    | passed |
| clippy | passed |

```text
Finished `release` profile [optimized] target(s) in 12.4s
```
EOF
````

Fenced code blocks must be closed; the CLI rejects an open fence before
dispatch. See [Markdown](docs/Markdown.md) for the full syntax reference.

Edit, delete, or react to a comment by its database ID:

```bash
pukbot comment edit 456 --repo owner/repository --body "updated"
pukbot comment delete 456 --repo owner/repository --yes
pukbot comment react 456 --repo owner/repository --reaction eyes
```

For named inline media, provide one JSON request:

```json
{
  "operation": "comment_create",
  "repository": "owner/repository",
  "number": 123,
  "body": "{IMG1} Testing inline media. {VIDEO1}",
  "media": [
    {
      "name": "IMG1",
      "path": "/absolute/path/to/image.png",
      "alt": "result"
    },
    {
      "name": "VIDEO1",
      "path": "/absolute/path/to/demo.mp4",
      "alt": "demo"
    }
  ]
}
```

Apply it:

```bash
pukbot apply --input request.json
pukbot apply --input request.json --json
pukbot apply --input request.json --dry-run
```

Each media object accepts exactly one `path` or `url`. Pukbot replaces every
named placeholder in place. Local files up to 100 MiB support PNG, JPEG, GIF,
WebP, BMP, TIFF, AVIF, SVG, MP4, MOV, WebM, MKV, M4V, MP3, WAV, OGG, M4A,
FLAC, AAC, PDF, text, Markdown, JSON, CSV, ZIP, Gzip, Tar, and 7-Zip.

Local media is uploaded as a content-addressed public asset in Pukbot's
`comment-assets` prerelease. Images render inline. Video, audio, documents, and
archives render as links. Do not upload secrets or private media.

Inspect the stable machine-readable feature inventory:

```bash
pukbot capabilities --json
```

Issue operations include create, edit, close, reopen, labels, assignees, and
reactions. Pull request operations include create, edit, close, reopen,
squash-merge, ready, draft, review, labels, assignees, reactions, and branch
updates. Every mutation is also accepted by `pukbot apply` as typed JSON.

```bash
pukbot issue create --repo owner/repository --title "bug" --label bug
pukbot issue labels 123 --repo owner/repository --add urgent --remove stale
pukbot pr review 456 --repo owner/repository --event approve --body "looks good"
pukbot pr merge 456 --repo owner/repository --yes
pukbot pr create --repo owner/repository --title "automated update" \
  --head automation --base main --as-app
```

Commit staged local changes atomically through the GitHub Git Data API. The
requester is the author by default, while `--as-app` records Pukbot as both
author and committer:

```bash
git add data/members.json
pukbot commit create --repo owner/repository --branch main --message "data: update roster"
pukbot commit create --repo owner/repository --branch main \
  --message "data: automated update" --as-app
```

Only text content is supported today; staged binary files are rejected
before dispatch.

Create and delete Git refs, lightweight tags, and annotated tags through the
App:

```bash
pukbot ref create refs/heads/release --repo owner/repository --sha COMMIT_SHA
pukbot tag create v1.2.3 --repo owner/repository --target COMMIT_SHA
pukbot tag create v1.2.3 --repo owner/repository --target COMMIT_SHA \
  --message "Release 1.2.3"
```

Manage releases and upload assets as Pukbot:

```bash
pukbot release create v1.2.3 --repo owner/repository --name "Release 1.2.3"
pukbot release edit 123 --repo owner/repository --prerelease false
pukbot release upload-asset 123 checksums.txt --repo owner/repository
pukbot release delete 123 --repo owner/repository --yes
```

Typed release asset uploads preserve binary content and MIME type up to 40,000
bytes per asset.

Dispatch a workflow as the Pukbot App and receive the created run URL:

```bash
pukbot workflow dispatch release.yml \
  --repo owner/repository \
  --ref main \
  --input release=true \
  --watch
```

The target workflow must support `workflow_dispatch`. Repeat `--input` for
each `KEY=VALUE` input, or omit it when the workflow has no inputs. `--watch`
reports workflow and job state changes, prints the final job summary, includes
actor identities, shows failed logs, and returns a failing exit code when the
target workflow fails.

Inspect any run or read its logs later:

```bash
pukbot workflow status 123 --repo owner/repository
pukbot workflow watch 123 --repo owner/repository
pukbot workflow logs 123 --repo owner/repository --failed
```

Send a repository dispatch with an optional JSON payload:

```bash
pukbot repository dispatch apt-release \
  --repo owner/repository \
  --client-payload '{"version":"1.2.3"}'
```

See [Operations](docs/Operations.md) for the complete command and JSON
contracts, and [Markdown](docs/Markdown.md) for the body rendering contract.

Every comment ends with:

```markdown
---

*Automated comment posted by Pukbot from an agent-assisted workflow.*

from: @authenticated-user
```

For App operations the CLI follows the workflow, streams its status, shows
failed logs, returns a failing exit code on failure, and prints the posted
comment URL on success.

GitHub CLI must be installed and authenticated. The authenticated user needs
permission to dispatch the Operation workflow in `pulkitxm/pukbot`. Local
media uploads also require permission to upload release assets there.

## Attribution

Every operation is either authored by you or authored by the App, and the
result reports which:

```json
{
  "operation": "pull_request_create",
  "authoredBy": "user",
  "workflowUrl": null,
  "resourceUrl": "https://github.com/owner/repository/pull/7"
}
```

Authored by you, executed through your local authenticated GitHub CLI session:

- every `pr` operation, including `create`, `review`, and `merge`

The pull request author, the reviewer, the merge event, and the squash commit
on the base branch are all yours, so the work lands in your GitHub
contribution history. `workflowUrl` is `null` because no workflow runs.

Pull request create, edit, review, update-branch, and merge accept `--as-app`.
That explicit mode executes through the protected workflow, records Pukbot as
the GitHub actor, and returns the workflow URL. App merges remain squash-only.

Authored by the Pukbot App, executed inside the protected workflow:

- every `comment` and `issue` operation
- `commit create`
- `repository dispatch`
- ref and tag create and delete
- release create, edit, delete, and asset upload
- workflow dispatch, cancel, rerun, enable, and disable

Pukbot can replace the actor that requests a workflow run or performs a GitHub
mutation. It cannot replace GitHub's internal Actions identity. Workflow jobs
still execute as GitHub Actions, and steps using the automatic `GITHUB_TOKEN`
remain attributable to `github-actions[bot]`.

`commit create` records you as the commit author and the App as the committer,
so the commit shows as authored by you and committed by Pukbot, and it counts
toward your contributions. With `--as-app`, Pukbot is both author and
committer. Identities are derived inside the workflow and cannot be set to an
arbitrary name or email from the CLI.

## Agent instructions

Add this to `AGENTS.md`:

```text
Perform supported GitHub mutations through Pukbot. Do not call GitHub mutation
commands directly.

pukbot comment create <number> --repo <owner/repository> --body "<markdown>"

Bodies are GitHub-flavored Markdown posted verbatim: fence code, logs, and
command output in code blocks with a language identifier, and use headings,
tables, task lists, and `<details>` sections instead of plain text. Close every
code fence. Pass multiline bodies with `--body-file <file>` or stdin. Use a
JSON request with `pukbot apply --input <file>` for named inline media. Use
`--json` when consuming output. Local media is uploaded publicly. Pukbot
appends the required disclosure footer to comments.
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
