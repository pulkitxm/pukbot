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
`workflowUrl`. See [Attribution](#attribution).

Pull request descriptions and review bodies receive no disclosure footer. Task
lists in a description feed the pull request task counter.

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
```

The commit records the requesting GitHub account as the commit author and the
Pukbot App as the committer, so it appears as authored by you and committed by
Pukbot and counts toward your contributions. Both identities are resolved
inside the protected workflow from the requesting account and cannot be set
from the CLI or your local git config. The branch ref update is never forced:
if the branch has moved since the commit was built, the operation fails
instead of overwriting the newer history.

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

`authoredBy` is `user` for every `pr` operation, which executes locally under
the authenticated GitHub CLI session. It is `pukbot` for every `comment`,
`issue`, and `commit` operation, which executes inside the protected workflow
under a short-lived App installation token; those results also carry a
`workflowUrl`.

`pukbot capabilities --json` reports the same split under `attribution`.

The JSON operation is `commit_create` and accepts `branch`, `message`, and a
`files` array of `{path, content, delete}` objects. Set `delete: true` to
remove a path, otherwise provide `content` as UTF-8 text. At most 50 files,
60,000 bytes per file, and 120,000 bytes combined.

Only text content is supported. Staged binary files (images, video, and
other non-UTF-8 content) are rejected before any request is sent; commit
those with local git for now.

## Permissions

The protected operation workflow requests issue, pull request, and repository
content write permissions from the Pukbot GitHub App. The App installation must
include the target repository and grant those permissions. The CLI never
receives the App private key or its short-lived installation token.
