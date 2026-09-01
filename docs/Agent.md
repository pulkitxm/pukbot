# Agent usage

Pukbot is designed so agents can perform approved GitHub mutations without
reading the GitHub App private key or its installation token. For comment,
issue, commit, repository dispatch, and workflow mutation operations the agent
invokes the public CLI, the CLI dispatches a protected workflow, and the
workflow mints one short-lived token scoped to one repository. Pull request and
stack operations run through the user's own authenticated GitHub CLI session
so the user, not the App, is the author.

Add this policy to `AGENTS.md`:

```text
Use Pukbot for supported GitHub mutations. Do not invoke direct GitHub mutation
commands. Prefer `pukbot apply --input <file> --json` with a typed JSON request.
Inspect `pukbot capabilities --json` before relying on an operation. Use
`--dry-run` first for destructive or unfamiliar requests. Comment, issue, pull
request, and review bodies are GitHub-flavored Markdown posted verbatim: fence
code, logs, and command output in code blocks with a language identifier, and
use headings, tables, task lists, and `<details>` sections instead of plain
text. Close every code fence. Pass multiline bodies with `--body-file` or
stdin. Local media uploads are public and must never contain credentials or
private data.
```

The stable agent sequence is:

1. Run `pukbot capabilities --json` when capability discovery is needed.
2. Construct exactly one JSON request with an `operation` discriminator.
3. Run `pukbot apply --input request.json --dry-run` to validate it.
4. Run `pukbot apply --input request.json --json` to execute it.
5. Read `resourceUrl` and `authoredBy` from the one JSON result object.

`authoredBy` is `user` for locally executed pull request and stack operations
and `pukbot` for App operations. `workflowUrl` is `null` when no workflow runs.
`pukbot capabilities --json` reports the split under `attribution`.

For stacked pull requests, `pukbot stack` exposes the installed gh-stack
command surface with the current working directory, terminal, and exit status
intact. Its merge command is the squash-only exception and uses Pukbot's direct
merge API. For deterministic agent mutations, use Git for pushes and local
topology, then use Pukbot's typed operations. Create or edit the pull requests
first, disable auto-merge where necessary, and call `stack_create` or
`stack_append` with pull request numbers ordered bottom to top. Use
`stack_merge` instead of an ordinary GitHub merge command. Pukbot's existing
`pull_request_merge` operation also detects stack membership and selects the
asynchronous squash flow automatically.

Unknown JSON fields fail validation. Failed workflows return a nonzero exit
code and print failed job logs. Text progress is suppressed when `--json` is
active.

Use `pukbot workflow status`, `watch`, and `logs` for read-only workflow
inspection. A dispatched workflow can be followed immediately with
`pukbot workflow dispatch ... --watch`. The watcher reports state changes,
job conclusions, actor identities, URLs, and failed logs.

## Bodies

Comment, issue, pull request, and review bodies are GitHub-flavored Markdown.
Pukbot sends them byte for byte, so every syntax GitHub renders in the web
editor renders through Pukbot with no flag, no escaping, and no preprocessing.

The one structural rule the CLI enforces is that fenced code blocks must be
closed, because an open fence would swallow the rest of the body and the
disclosure footer.

In JSON requests, newlines are `\n`. Build the request with a serializer
rather than by hand, and run `--dry-run` to print the exact body first.

Machine readable discovery lives in the `markdown` object of
`pukbot capabilities --json`.

See [Markdown](Markdown.md) for the full syntax reference,
[Operations](Operations.md) for every request shape, the
[Markdown bodies](Operations.md#markdown-bodies) contract for the per
operation rules, and [Security](Security.md) for the trust boundary.
