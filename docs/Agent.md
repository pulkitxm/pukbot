# Agent usage

Pukbot is designed so agents can perform approved GitHub mutations without
reading the GitHub App private key or its installation token. The agent invokes
the public CLI, the CLI dispatches a protected workflow, and the workflow mints
one short-lived token scoped to one repository.

Add this policy to `AGENTS.md`:

```text
Use Pukbot for supported GitHub mutations. Do not invoke direct GitHub mutation
commands. Prefer `pukbot apply --input <file> --json` with a typed JSON request.
Inspect `pukbot capabilities --json` before relying on an operation. Use
`--dry-run` first for destructive or unfamiliar requests. Comment, issue, pull
request, and review bodies are GitHub-flavored Markdown posted verbatim: fence
code, logs, and command output in code blocks with a language identifier, and
use headings, tables, task lists, and `<details>` sections instead of plain
text. Pass multiline bodies with `--body-file` or stdin. Local media uploads
are public and must never contain credentials or private data.
```

The stable agent sequence is:

1. Run `pukbot capabilities --json` when capability discovery is needed.
2. Construct exactly one JSON request with an `operation` discriminator.
3. Run `pukbot apply --input request.json --dry-run` to validate it.
4. Run `pukbot apply --input request.json --json` to execute it.
5. Read `resourceUrl` from the one JSON result object.

Unknown JSON fields fail validation. Failed workflows return a nonzero exit
code and print failed job logs. Text progress is suppressed when `--json` is
active.

See [Operations](Operations.md) for every request shape, the
[Markdown bodies](Operations.md#markdown-bodies) contract for the full body
syntax, and [Security](Security.md) for the trust boundary.
