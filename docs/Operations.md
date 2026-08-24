# Operations

Every Pukbot mutation has a command and a typed JSON operation. Commands are
convenient for interactive use. `pukbot apply --input request.json --json` is
the canonical agent interface.

## Common contracts

- Repositories use `owner/name`.
- Issue and pull request numbers start at 1.
- Comment IDs are GitHub database IDs, not issue numbers.
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

## Pull requests

```bash
pukbot pr create \
  --repo owner/repository \
  --title "title" \
  --body "one line" \
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

## Permissions

The protected operation workflow requests issue, pull request, and repository
content write permissions from the Pukbot GitHub App. The App installation must
include the target repository and grant those permissions. The CLI never
receives the App private key or its short-lived installation token.
