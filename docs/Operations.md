# Operations

Every Pukbot mutation has a command and a typed JSON operation. Commands are
convenient for interactive use. `pukbot apply --input request.json --json` is
the canonical agent interface.

## Common contracts

- Repositories use `owner/name`.
- Issue and pull request numbers start at 1.
- Comment IDs are GitHub database IDs, not issue numbers.
- Comment, issue, pull request, and review bodies are GitHub-flavored
  Markdown posted verbatim. See [Markdown bodies](#markdown-bodies).
- `--dry-run` validates input and prints the final operation without dispatching.
- `--json` emits one result object and suppresses workflow progress.
- Destructive comment deletion and pull request merging require `--yes`.
- Pull request merges always use squash.
- Every JSON object rejects unknown fields.

Successful JSON output contains the operation name, workflow URL, and resource
URL:

```json
{
  "operation": "issue_create",
  "workflowUrl": "https://github.com/pulkitxm/pukbot/actions/runs/123",
  "resourceUrl": "https://github.com/owner/repository/issues/456"
}
```

## Markdown bodies

Every body field accepts the complete GitHub-flavored Markdown syntax:
comment bodies, issue bodies, pull request descriptions, and review bodies.
Pukbot posts them verbatim. Nothing is escaped, stripped, or rewritten, so
everything GitHub renders works out of the box. Format bodies instead of
posting plain text walls:

- Fenced code blocks with a language identifier, such as `rust`, `bash`,
  `json`, `diff`, or `text`. Always fence code, logs, and command output.
- Inline code spans for identifiers, flags, and paths.
- Headings, emphasis, blockquotes, and horizontal rules.
- Tables, ordered and unordered lists, and task lists.
- Links, images, and references GitHub autolinks: `#123`,
  `owner/repository#123`, commit SHAs, and `@login` mentions. Mentions
  notify people, so mention deliberately.
- Collapsible `<details>` and `<summary>` sections for long output.
- Alerts inside blockquotes: `> [!NOTE]`, `> [!TIP]`, `> [!IMPORTANT]`,
  `> [!WARNING]`, and `> [!CAUTION]`.
- Mermaid diagrams in `mermaid` code blocks, math in `$` and `$$`
  delimiters, footnotes, emoji shortcodes such as `:rocket:`, and `<kbd>`
  key labels.

Close every code fence you open. Pukbot appends its attribution footer after
comment bodies, and an unclosed fence swallows it, so the CLI rejects a body
with an open fence before dispatch.

See [Markdown](Markdown.md) for the complete syntax reference with examples.

Multiline bodies are easiest to pass with `--body-file` or stdin. When a
body must be inline, quote it with single quotes so the shell does not
expand backticks or `$`.

````bash
pukbot comment create 123 --repo owner/repository --body-file - <<'EOF'
### Release check failed

| Check  | Status |
| ------ | ------ |
| fmt    | passed |
| clippy | failed |

```text
error: unused variable: `token`
```
EOF
````

In JSON requests, encode newlines as `\n`:

````json
{
  "operation": "comment_create",
  "repository": "owner/repository",
  "number": 123,
  "body": "### Result\n\n```text\nall 42 checks passed\n```"
}
````

## Comments

```bash
pukbot comment create 123 --repo owner/repository --body "message"
pukbot comment edit 456 --repo owner/repository --body "updated"
pukbot comment delete 456 --repo owner/repository --yes
pukbot comment react 456 --repo owner/repository --reaction eyes
```

JSON operations are `comment_create`, `comment_edit`, `comment_delete`, and
`comment_react`. Comment create and edit accept `body` plus an optional `media`
array. Media names replace matching `{NAME}` placeholders inline.

The disclosure footer is appended to comment bodies after a blank line and a
thematic break, so it never joins the last block of the body.

```json
{
  "operation": "comment_create",
  "repository": "owner/repository",
  "number": 123,
  "body": "{SCREENSHOT} result {DEMO}",
  "media": [
    {
      "name": "SCREENSHOT",
      "path": "/absolute/result.png",
      "alt": "result"
    },
    {
      "name": "DEMO",
      "url": "https://example.com/demo.mp4",
      "alt": "demo"
    }
  ]
}
```

Reactions are `+1`, `-1`, `laugh`, `confused`, `heart`, `hooray`, `rocket`,
and `eyes`.

## Issues

```bash
pukbot issue create \
  --repo owner/repository \
  --title "title" \
  --body "body" \
  --label bug \
  --assignee octocat

pukbot issue edit 123 --repo owner/repository --title "new title"
pukbot issue close 123 --repo owner/repository
pukbot issue reopen 123 --repo owner/repository
pukbot issue labels 123 --repo owner/repository --add urgent --remove stale
pukbot issue assignees 123 --repo owner/repository --add octocat
pukbot issue react 123 --repo owner/repository --reaction rocket
```

JSON operation names are:

- `issue_create`
- `issue_edit`
- `issue_close`
- `issue_reopen`
- `issue_labels`
- `issue_assignees`
- `issue_react`

Create accepts `title`, optional `body`, and optional `labels` and `assignees`
arrays. Edit accepts `number` and at least one of `title` or `body`. Label and
assignee operations accept `add` and `remove` arrays.

Issue bodies receive no disclosure footer.

## Pull requests

```bash
pukbot pr create \
  --repo owner/repository \
  --title "title" \
  --body-file description.md \
  --head feature \
  --base main \
  --draft

pukbot pr edit 123 --repo owner/repository --base develop
pukbot pr close 123 --repo owner/repository
pukbot pr reopen 123 --repo owner/repository
pukbot pr ready 123 --repo owner/repository
pukbot pr draft 123 --repo owner/repository
pukbot pr review 123 --repo owner/repository --event approve --body "looks good"
pukbot pr labels 123 --repo owner/repository --add ready --remove draft
pukbot pr assignees 123 --repo owner/repository --add octocat
pukbot pr react 123 --repo owner/repository --reaction heart
pukbot pr update-branch 123 --repo owner/repository
pukbot pr merge 123 --repo owner/repository --yes
pukbot pr create --repo owner/repository --title "automated update" \
  --head automation --base main --as-app
pukbot pr review 123 --repo owner/repository --event approve --as-app
pukbot pr merge 123 --repo owner/repository --as-app --yes
```

JSON operation names are:

- `pull_request_create`
- `pull_request_edit`
- `pull_request_close`
- `pull_request_reopen`
- `pull_request_merge`
- `pull_request_ready`
- `pull_request_draft`
- `pull_request_review`
- `pull_request_labels`
- `pull_request_assignees`
- `pull_request_react`
- `pull_request_update_branch`

Create accepts `title`, optional `body`, `head`, `base`, and optional `draft`.
Edit accepts `number` and at least one of `title`, `body`, or `base`. Review
events are `approve`, `request_changes`, and `comment`. Requesting changes
requires a body.

Pull request operations run through the local authenticated GitHub CLI session
rather than the Pukbot App, so the pull request author, the review, the merge
event, and the squash commit on the base branch all belong to the
authenticated user. Results report `"authoredBy": "user"` and a `null`
`workflowUrl`. Create, edit, merge, review, and update-branch accept `--as-app`
to run through the protected workflow as Pukbot instead. The matching JSON
operations accept `"as_app": true`. App merges always use squash merge. See
[Attribution](#attribution). Pukbot supplies the squash title and an empty
message explicitly so GitHub does not add generated attribution trailers.

Pull request descriptions and review bodies receive no disclosure footer. Task
lists in a description feed the pull request task counter.

## Batch issue and pull request mutations

```bash
pukbot issue batch 101 102 103 \
  --repo owner/repository \
  --comment "closing completed work" \
  --add-label complete \
  --remove-assignee octocat \
  --close \
  --lock \
  --lock-reason resolved \
  --yes

pukbot pr batch 201 202 \
  --repo owner/repository \
  --add-label ready \
  --add-assignee octocat \
  --allow-partial \
  --yes
```

Batch commands combine a comment, label additions and removals, assignee
additions and removals, close, lock, and unlock into one typed operation for up to 50
unique targets. Lock reasons are `off-topic`, `too-heated`, `resolved`, and
`spam`. Lock and unlock cannot be requested together. Execution requires
`--yes`; `--dry-run` validates and prints the complete operation without
confirmation.

The JSON operations are `issue_batch` and `pull_request_batch`. They accept
`numbers`, optional `comment`, `add_labels`, `remove_labels`,
`add_assignees`, `remove_assignees`, `close`, `lock`,
`unlock`, `lock_reason`, and `allow_partial`. At least one mutation is required.
Duplicate targets and values, conflicting additions and removals, empty
comments, and issue or pull request type mismatches are rejected.

Every target emits its status, URL, and any failure reason in the workflow log,
followed by a compact `pukbot-batch-result` JSON array and success and failure
counts. The default fails the workflow after processing every target if any
target failed. `--allow-partial` preserves the same per-target results while
letting successful targets produce a successful workflow. Batch comments
receive the normal Pukbot disclosure footer and requester line.

## Commits

```bash
git add data/members.json
pukbot commit create --repo owner/repository --branch main --message "data: update roster"
```

`pukbot commit create` reads whatever is already staged in the local git
index (`git diff --cached`) and commits exactly that content to the given
branch of the target repository, atomically, through the GitHub Git Data API.
Pass one or more pathspecs to restrict the commit to a subset of the staged
changes:

```bash
pukbot commit create data/members.json --repo owner/repository --branch main --message "data: update roster"
pukbot commit create data/members.json --repo owner/repository --branch main \
  --message "data: refresh roster" --as-app
```

The commit records the requesting GitHub account as the commit author and the
Pukbot App as the committer, so it appears as authored by you and committed by
Pukbot and counts toward your contributions. Pass `--as-app`, or set
`"as_app": true` in the typed JSON operation, to record Pukbot as both author
and committer for a fully automated commit. Identities are resolved inside the
protected workflow and cannot be supplied as arbitrary names or emails. The
branch ref update is never forced: if the branch has moved since the commit was
built, the operation fails instead of overwriting the newer history.

## Repository dispatches

```bash
pukbot repository dispatch apt-release \
  --repo owner/repository \
  --client-payload '{"version":"1.2.3","channel":"stable"}'
```

Use `--client-payload-file` for a JSON object stored in a file. The payload is
optional and defaults to an empty object. The JSON operation is
`repository_dispatch`:

```json
{
  "operation": "repository_dispatch",
  "repository": "owner/repository",
  "event_type": "apt-release",
  "client_payload": {
    "version": "1.2.3",
    "channel": "stable"
  }
}
```

Event types are 1 to 100 bytes. GitHub accepts at most 10 top-level payload
properties and 65,535 encoded characters. Pukbot enforces these limits before
dispatch. The receiving workflow must declare a matching
`repository_dispatch` event type.

## Workflow operations

```bash
pukbot workflow dispatch release.yml \
  --repo owner/repository \
  --ref main \
  --input release=true \
  --input channel=stable \
  --watch
```

The workflow argument is a numeric workflow ID or workflow file name. The
target workflow must declare a `workflow_dispatch` trigger on its default
branch. `--ref` selects the branch or tag used for the run. Repeat `--input`
for each `KEY=VALUE` input. Pukbot returns the created workflow run as
`resourceUrl`.

The JSON operation is `workflow_dispatch`:

```json
{
  "operation": "workflow_dispatch",
  "repository": "owner/repository",
  "workflow": "release.yml",
  "ref": "main",
  "inputs": {
    "release": "true",
    "channel": "stable"
  }
}
```

Inputs are optional. GitHub accepts at most 25 inputs with a combined encoded
payload of 65,535 characters. Pukbot enforces both limits before dispatch.

Use `--watch` to follow the target run after dispatch. Pukbot reports workflow
and job state changes, prints final job URLs and conclusions, identifies the
actor and triggering actor, prints failed logs, and returns a failing exit code
for an unsuccessful target run. `--interval` accepts 1 to 60 seconds and
defaults to 3.

Control existing workflow runs and workflows:

```bash
pukbot workflow cancel 123 --repo owner/repository
pukbot workflow rerun 123 --repo owner/repository
pukbot workflow rerun 123 --repo owner/repository --failed-only
pukbot workflow enable ci.yml --repo owner/repository
pukbot workflow disable ci.yml --repo owner/repository
```

The typed operations are `workflow_cancel`, `workflow_rerun`,
`workflow_enable`, and `workflow_disable`. Run IDs start at 1. Rerun accepts
the optional Boolean `failed_only`. Enable and disable accept a numeric
workflow ID or workflow file name.

Workflow inspection is read-only and does not dispatch a Pukbot operation:

```bash
pukbot workflow status 123 --repo owner/repository
pukbot workflow watch 123 --repo owner/repository --interval 5
pukbot workflow logs 123 --repo owner/repository
pukbot workflow logs 123 --repo owner/repository --failed
```

`status` returns the run, actor identities, jobs, steps, conclusions, and URLs.
`watch` emits only state changes while a run is active and then prints the
final summary. `logs` prints all logs, or only failed job logs with `--failed`.
Global `--json` produces structured output and suppresses streaming progress.

The Pukbot App can be the actor and triggering actor for dispatches and can
perform mutations under its installation token. It cannot replace GitHub's
internal Actions identity. Jobs still run on GitHub Actions infrastructure,
and steps using the automatic `GITHUB_TOKEN` remain attributable to
`github-actions[bot]`.

## Attribution

Results report who GitHub records as the actor:

```json
{
  "operation": "pull_request_create",
  "authoredBy": "user",
  "workflowUrl": null,
  "resourceUrl": "https://github.com/owner/repository/pull/7"
}
```

`authoredBy` is `user` for ordinary `pr` operations, which execute locally
under the authenticated GitHub CLI session. It is `pukbot` for pull request
create, edit, merge, review, and update-branch with `--as-app`, and for every
`comment`, `issue`, `commit`, repository dispatch, and workflow mutation.
App operations execute inside the protected workflow under a short-lived App
installation token and carry a `workflowUrl`.

`pukbot capabilities --json` reports the same split under `attribution`.

The JSON operation is `commit_create` and accepts `branch`, `message`, an
optional `as_app` Boolean, and a `files` array of `{path, content, delete}`
objects. Set `delete: true` to remove a path, otherwise provide `content` as
UTF-8 text. At most 50 files, 60,000 bytes per file, and 120,000 bytes combined.

Only text content is supported. Staged binary files (images, video, and
other non-UTF-8 content) are rejected before any request is sent; commit
those with local git for now.

## Wiki publishing

```bash
pukbot wiki publish \
  --repo owner/repository \
  --source-ref main \
  --source-path wiki \
  --message "docs: publish wiki"

pukbot wiki publish \
  --repo owner/repository \
  --source-ref wiki-output \
  --source-path . \
  --replace \
  --message "docs: sync wiki"

pukbot wiki publish \
  --repo owner/repository \
  --delete Obsolete.md \
  --message "docs: remove obsolete wiki page"
```

Wiki publishing copies tracked files from a repository ref into the repository's
GitHub wiki. The source ref may be a branch, tag, or full commit SHA. Source
paths are repository-relative directories. Add `--replace` for a complete
mirror, or repeat `--delete` to remove selected wiki paths while retaining
everything else.

The App authors and pushes the wiki commit. A publish accepts at most 500 source
files totaling 20,000,000 bytes and at most 500 deleted paths. It rejects
symlinks, unsafe paths, empty changes, incomplete source pairs, and replacing
while also listing deleted paths. The JSON operation is `wiki_publish` with
`message`, optional paired `source_ref` and `source_path`, `delete`, and
`replace`.

## Git refs and tags

```bash
pukbot ref create refs/heads/release --repo owner/repository --sha COMMIT_SHA
pukbot ref delete refs/heads/release --repo owner/repository --yes
pukbot tag create v1.2.3 --repo owner/repository --target COMMIT_SHA
pukbot tag create v1.2.3 --repo owner/repository --target COMMIT_SHA \
  --message "Release 1.2.3"
pukbot tag delete v1.2.3 --repo owner/repository --yes
```

Ref operations require the complete `refs/...` name and a 40-character Git
object SHA. Tag names omit the `refs/tags/` prefix. `tag create` makes a
lightweight tag when no message is present and an annotated tag when a message
is present. Annotated tag objects record `pukbot[bot]` as the tagger.

The typed operations are `ref_create`, `ref_delete`, `tag_create`, and
`tag_delete`. Ref and tag deletion require confirmation in command mode.

## Releases

```bash
pukbot release create v1.2.3 \
  --repo owner/repository \
  --name "Release 1.2.3" \
  --body-file notes.md \
  --target main \
  --generate-notes

pukbot release edit 123 \
  --repo owner/repository \
  --name "Release 1.2.3" \
  --draft false \
  --prerelease false \
  --make-latest true

pukbot release upload-asset 123 checksums.txt \
  --repo owner/repository \
  --label "SHA-256 checksums" \
  --content-type text/plain

pukbot release delete 123 --repo owner/repository --yes
```

The typed operations are `release_create`, `release_edit`, `release_delete`,
and `release_asset_upload`. Edit addresses a release by its numeric database
ID and requires at least one changed field. `make_latest` accepts `true`,
`false`, or `legacy`.

Asset upload reads the local file, infers its MIME type when none is supplied,
and carries its base64-encoded bytes through the protected typed operation.
Assets must be between 1 and 40,000 bytes so the complete operation remains
within GitHub's workflow input limit. The JSON field is `content_base64`.
Pukbot returns the release URL or uploaded asset download URL.

## Deployments

```bash
pukbot deployment create main \
  --repo owner/repository \
  --environment staging \
  --description "staging deployment" \
  --payload '{"version":"1.2.3"}' \
  --required-context ci

pukbot deployment status 123 \
  --repo owner/repository \
  --state in-progress \
  --log-url https://example.com/deployments/123/logs

pukbot deployment status 123 \
  --repo owner/repository \
  --state success \
  --environment-url https://staging.example.com \
  --auto-inactive
```

Deployment creation accepts a branch, tag, or commit ref, an environment, an
optional task and description, a JSON object from `--payload` or
`--payload-file`, repeated `--required-context` values, and the
`--auto-merge`, `--transient-environment`, and `--production-environment`
flags. Pukbot validates refs, unique contexts, and the 65,535-character payload
limit before dispatch.

Deployment status states are `error`, `failure`, `inactive`, `in-progress`,
`queued`, `pending`, and `success`. Statuses can include target, log, and
environment URLs, a description, and `--auto-inactive`. The matching JSON
values use `in_progress` and the typed operations are `deployment_create` and
`deployment_status`.

Workflow output reports the created deployment or status ID and its API URL,
then returns the repository deployment list as `resourceUrl`. The operation
workflow URL remains available for complete status and logs.

## Permissions

The protected operation workflow requests only the permissions required for
each operation. Workflow dispatch and controls use Actions write access, and
deployment operations use Deployments write access. Other App operations use
issue, pull request, or repository content write access. The App installation
must include the target repository and grant the required permission. The CLI
never receives the App private key or its short-lived installation token.
