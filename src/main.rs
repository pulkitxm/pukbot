mod commit;
mod completion;
mod local;
mod manual;
mod media;
mod model;
mod update;
mod workflow;

use std::collections::BTreeMap;
use std::io::{self, IsTerminal, Read, Write};
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::{fs, process};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;
use model::{CommentDocument, MAX_BODY_BYTES, Reaction, Repository, Request, ReviewEvent};
use serde::Serialize;

const ABOUT: &str = "Pukbot performs typed GitHub operations through the Pukbot GitHub App.

Every body is GitHub-flavored Markdown and is posted verbatim: headings, fenced
code blocks with syntax highlighting, tables, task lists, blockquote alerts,
collapsed details sections, footnotes, mentions, references, emoji, math, and
mermaid diagrams all render. Run `pukbot capabilities --json` for the machine
readable inventory.";

const BODY_LONG_HELP: &str = "GitHub-flavored Markdown body, posted verbatim.

Every syntax GitHub renders is supported with no flag and no escaping: headings,
bold, italic, strikethrough, inline code, fenced code blocks with a language,
ordered, unordered, and task lists, tables, blockquotes, [!NOTE] and [!WARNING]
alerts, collapsed <details> sections, footnotes, links, images, @mentions, #123
references, emoji, math, and mermaid diagrams.

Put command output, logs, diffs, and JSON inside a fenced code block with a
language. Every fence must be closed. Prefer --body-file for anything longer
than one line, because a shell mangles backticks and newlines.";

const BODY_FILE_LONG_HELP: &str =
    "Read the GitHub-flavored Markdown body from a file, or from standard input when the path is -.

This is the reliable way to pass multiline Markdown, because the file content is
sent verbatim without shell quoting.";

#[derive(Debug, Parser)]
#[command(name = "pukbot", version, about, long_about = ABOUT)]
pub(crate) struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Apply(ApplyArgs),
    Comment {
        #[command(subcommand)]
        command: CommentCommand,
    },
    Issue {
        #[command(subcommand)]
        command: IssueCommand,
    },
    Pr {
        #[command(subcommand)]
        command: PullRequestCommand,
    },
    Commit {
        #[command(subcommand)]
        command: CommitCommand,
    },
    Workflow {
        #[command(subcommand)]
        command: WorkflowCommand,
    },
    Completions(CompletionsArgs),
    Man(ManArgs),
    Update(UpdateArgs),
    Capabilities,
}

#[derive(Debug, Args)]
struct ApplyArgs {
    #[arg(long, value_name = "FILE")]
    input: PathBuf,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Clone, Copy, Debug, Args)]
struct CompletionsArgs {
    #[arg(value_enum, required_unless_present = "install")]
    shell: Option<Shell>,
    #[arg(long)]
    install: bool,
}

#[derive(Debug, Args)]
struct ManArgs {
    #[arg(long, value_name = "DIR")]
    dir: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Args)]
struct UpdateArgs {
    #[arg(long)]
    check: bool,
}

#[derive(Debug, Subcommand)]
enum CommentCommand {
    Create(CommentCreateArgs),
    Edit(CommentEditArgs),
    Delete(CommentDeleteArgs),
    React(CommentReactArgs),
}

#[derive(Debug, Subcommand)]
enum IssueCommand {
    Create(IssueCreateArgs),
    Edit(IssueEditArgs),
    Close(TargetArgs),
    Reopen(TargetArgs),
    Labels(ListEditArgs),
    Assignees(ListEditArgs),
    React(ReactArgs),
}

#[derive(Debug, Subcommand)]
enum PullRequestCommand {
    Create(PullRequestCreateArgs),
    Edit(PullRequestEditArgs),
    Close(TargetArgs),
    Reopen(TargetArgs),
    Merge(ConfirmedTargetArgs),
    Ready(TargetArgs),
    Draft(TargetArgs),
    Review(PullRequestReviewArgs),
    Labels(ListEditArgs),
    Assignees(ListEditArgs),
    React(ReactArgs),
    UpdateBranch(TargetArgs),
}

#[derive(Debug, Subcommand)]
enum CommitCommand {
    Create(CommitCreateArgs),
}

#[derive(Debug, Subcommand)]
enum WorkflowCommand {
    Dispatch(WorkflowDispatchArgs),
}

#[derive(Debug, Args)]
struct CommentCreateArgs {
    number: NonZeroU64,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[command(flatten)]
    content: BodyArgs,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct CommentEditArgs {
    comment_id: NonZeroU64,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[command(flatten)]
    content: BodyArgs,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct CommentDeleteArgs {
    comment_id: NonZeroU64,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long)]
    yes: bool,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct CommentReactArgs {
    comment_id: NonZeroU64,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long, value_enum)]
    reaction: Reaction,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct BodyArgs {
    #[arg(
        long,
        conflicts_with = "body_file",
        help = "GitHub-flavored Markdown body, posted verbatim (code fences, tables, task lists, and every other GitHub syntax render as written)",
        long_help = BODY_LONG_HELP
    )]
    body: Option<String>,
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with = "body",
        help = "Read the GitHub-flavored Markdown body from FILE, or from stdin with -; preferred for multiline bodies",
        long_help = BODY_FILE_LONG_HELP
    )]
    body_file: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct MessageArgs {
    #[arg(long, conflicts_with = "message_file")]
    message: Option<String>,
    #[arg(long = "message-file", value_name = "FILE", conflicts_with = "message")]
    message_file: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct CommitCreateArgs {
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long)]
    branch: String,
    #[command(flatten)]
    content: MessageArgs,
    #[arg(value_name = "PATH")]
    paths: Vec<PathBuf>,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct WorkflowDispatchArgs {
    #[arg(value_name = "WORKFLOW")]
    workflow: String,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long = "ref", value_name = "BRANCH_OR_TAG")]
    reference: String,
    #[arg(long = "input", value_name = "KEY=VALUE")]
    inputs: Vec<String>,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct TargetArgs {
    number: NonZeroU64,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct ConfirmedTargetArgs {
    #[command(flatten)]
    target: TargetArgs,
    #[arg(long)]
    yes: bool,
}

#[derive(Debug, Args)]
struct IssueCreateArgs {
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long)]
    title: String,
    #[command(flatten)]
    content: BodyArgs,
    #[arg(long = "label")]
    labels: Vec<String>,
    #[arg(long = "assignee")]
    assignees: Vec<String>,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct IssueEditArgs {
    number: NonZeroU64,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long)]
    title: Option<String>,
    #[command(flatten)]
    content: BodyArgs,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct PullRequestCreateArgs {
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long)]
    title: String,
    #[arg(long)]
    head: String,
    #[arg(long)]
    base: String,
    #[command(flatten)]
    content: BodyArgs,
    #[arg(long)]
    draft: bool,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct PullRequestEditArgs {
    number: NonZeroU64,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long)]
    title: Option<String>,
    #[arg(long)]
    base: Option<String>,
    #[command(flatten)]
    content: BodyArgs,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct PullRequestReviewArgs {
    number: NonZeroU64,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long, value_enum)]
    event: ReviewEvent,
    #[command(flatten)]
    content: BodyArgs,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct ListEditArgs {
    number: NonZeroU64,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long = "add")]
    add: Vec<String>,
    #[arg(long = "remove")]
    remove: Vec<String>,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct ReactArgs {
    number: NonZeroU64,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long, value_enum)]
    reaction: Reaction,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MutationResult {
    operation: String,
    authored_by: &'static str,
    workflow_url: Option<String>,
    resource_url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Capabilities {
    protocol_version: u8,
    commands: Vec<&'static str>,
    media: Vec<&'static str>,
    markdown: Markdown,
    output: Vec<&'static str>,
    attribution: Attribution,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Attribution {
    user: Vec<&'static str>,
    app: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Markdown {
    flavor: &'static str,
    verbatim: bool,
    max_body_bytes: usize,
    footer_operations: Vec<&'static str>,
    features: Vec<&'static str>,
}

fn main() {
    if let Err(error) = run() {
        drop(writeln!(io::stderr().lock(), "error: {error:#}"));
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Apply(args) => {
            let request = read_request(&args.input)?;
            execute(request, args.dry_run, cli.json)
        }
        Commands::Comment { command } => run_comment(command, cli.json),
        Commands::Issue { command } => run_issue(command, cli.json),
        Commands::Pr { command } => run_pull_request(command, cli.json),
        Commands::Commit { command } => run_commit(command, cli.json),
        Commands::Workflow { command } => run_workflow(command, cli.json),
        Commands::Completions(args) => run_completions(args, cli.json),
        Commands::Man(args) => run_manual(args, cli.json),
        Commands::Update(args) => run_update(args, cli.json),
        Commands::Capabilities => emit_capabilities(cli.json),
    }
}

fn run_completions(args: CompletionsArgs, json: bool) -> Result<()> {
    if args.install {
        let installed = completion::install(args.shell)?;
        if json {
            return emit_json(&installed);
        }
        let mut stdout = io::stdout().lock();
        writeln!(
            stdout,
            "Installed {} completions to {}",
            installed.shell,
            installed.path.display()
        )?;
        return Ok(());
    }
    let shell = args.shell.context("completion shell is required")?;
    let script = completion::script(shell)?;
    if json {
        emit_json(&serde_json::json!({"shell": shell.to_string(), "script": script}))
    } else {
        let mut stdout = io::stdout().lock();
        write!(stdout, "{script}")?;
        Ok(())
    }
}

fn run_manual(args: ManArgs, json: bool) -> Result<()> {
    if let Some(directory) = args.dir {
        let paths = manual::write_all(&directory)?;
        if json {
            return emit_json(&serde_json::json!({"paths": paths}));
        }
        let mut stdout = io::stdout().lock();
        for path in paths {
            writeln!(stdout, "{path}")?;
        }
        return Ok(());
    }
    let page = manual::render()?;
    if json {
        emit_json(&serde_json::json!({"manual": page}))
    } else {
        let mut stdout = io::stdout().lock();
        write!(stdout, "{page}")?;
        Ok(())
    }
}

fn run_update(args: UpdateArgs, json: bool) -> Result<()> {
    let result = update::run(args.check)?;
    if json {
        emit_json(&result)
    } else {
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "{}", result.text())?;
        Ok(())
    }
}

fn run_comment(command: CommentCommand, json: bool) -> Result<()> {
    match command {
        CommentCommand::Create(args) => execute(
            Request::CreateComment {
                repository: args.repo,
                number: args.number,
                document: CommentDocument {
                    body: read_body(&args.content)?,
                    media: Vec::new(),
                },
            },
            args.dry_run,
            json,
        ),
        CommentCommand::Edit(args) => execute(
            Request::EditComment {
                repository: args.repo,
                comment_id: args.comment_id,
                document: CommentDocument {
                    body: read_body(&args.content)?,
                    media: Vec::new(),
                },
            },
            args.dry_run,
            json,
        ),
        CommentCommand::Delete(args) => {
            if !args.yes && !args.dry_run {
                bail!("comment deletion requires --yes");
            }
            execute(
                Request::DeleteComment {
                    repository: args.repo,
                    comment_id: args.comment_id,
                },
                args.dry_run,
                json,
            )
        }
        CommentCommand::React(args) => execute(
            Request::ReactToComment {
                repository: args.repo,
                comment_id: args.comment_id,
                reaction: args.reaction,
            },
            args.dry_run,
            json,
        ),
    }
}

fn run_issue(command: IssueCommand, json: bool) -> Result<()> {
    match command {
        IssueCommand::Create(args) => execute(
            Request::IssueCreate {
                repository: args.repo,
                title: args.title,
                body: read_optional_body(&args.content)?,
                labels: args.labels,
                assignees: args.assignees,
            },
            args.dry_run,
            json,
        ),
        IssueCommand::Edit(args) => execute(
            Request::IssueEdit {
                repository: args.repo,
                number: args.number,
                title: args.title,
                body: read_optional_body(&args.content)?,
            },
            args.dry_run,
            json,
        ),
        IssueCommand::Close(args) => execute_issue_target(args, json, true),
        IssueCommand::Reopen(args) => execute_issue_target(args, json, false),
        IssueCommand::Labels(args) => execute(
            Request::IssueLabels {
                repository: args.repo,
                number: args.number,
                add: args.add,
                remove: args.remove,
            },
            args.dry_run,
            json,
        ),
        IssueCommand::Assignees(args) => execute(
            Request::IssueAssignees {
                repository: args.repo,
                number: args.number,
                add: args.add,
                remove: args.remove,
            },
            args.dry_run,
            json,
        ),
        IssueCommand::React(args) => execute(
            Request::IssueReact {
                repository: args.repo,
                number: args.number,
                reaction: args.reaction,
            },
            args.dry_run,
            json,
        ),
    }
}

fn execute_issue_target(args: TargetArgs, json: bool, close: bool) -> Result<()> {
    let request = if close {
        Request::IssueClose {
            repository: args.repo,
            number: args.number,
        }
    } else {
        Request::IssueReopen {
            repository: args.repo,
            number: args.number,
        }
    };
    execute(request, args.dry_run, json)
}

fn run_pull_request(command: PullRequestCommand, json: bool) -> Result<()> {
    match command {
        PullRequestCommand::Create(args) => execute(
            Request::PullRequestCreate {
                repository: args.repo,
                title: args.title,
                body: read_optional_body(&args.content)?,
                head: args.head,
                base: args.base,
                draft: args.draft,
            },
            args.dry_run,
            json,
        ),
        PullRequestCommand::Edit(args) => execute(
            Request::PullRequestEdit {
                repository: args.repo,
                number: args.number,
                title: args.title,
                body: read_optional_body(&args.content)?,
                base: args.base,
            },
            args.dry_run,
            json,
        ),
        PullRequestCommand::Close(args) => execute_pull_request_target(args, json, "close"),
        PullRequestCommand::Reopen(args) => execute_pull_request_target(args, json, "reopen"),
        PullRequestCommand::Merge(args) => {
            if !args.yes && !args.target.dry_run {
                bail!("pull request merge requires --yes");
            }
            execute_pull_request_target(args.target, json, "merge")
        }
        PullRequestCommand::Ready(args) => execute_pull_request_target(args, json, "ready"),
        PullRequestCommand::Draft(args) => execute_pull_request_target(args, json, "draft"),
        PullRequestCommand::Review(args) => execute(
            Request::PullRequestReview {
                repository: args.repo,
                number: args.number,
                event: args.event,
                body: read_optional_body(&args.content)?,
            },
            args.dry_run,
            json,
        ),
        PullRequestCommand::Labels(args) => execute(
            Request::PullRequestLabels {
                repository: args.repo,
                number: args.number,
                add: args.add,
                remove: args.remove,
            },
            args.dry_run,
            json,
        ),
        PullRequestCommand::Assignees(args) => execute(
            Request::PullRequestAssignees {
                repository: args.repo,
                number: args.number,
                add: args.add,
                remove: args.remove,
            },
            args.dry_run,
            json,
        ),
        PullRequestCommand::React(args) => execute(
            Request::PullRequestReact {
                repository: args.repo,
                number: args.number,
                reaction: args.reaction,
            },
            args.dry_run,
            json,
        ),
        PullRequestCommand::UpdateBranch(args) => {
            execute_pull_request_target(args, json, "update_branch")
        }
    }
}

fn execute_pull_request_target(args: TargetArgs, json: bool, action: &str) -> Result<()> {
    let request = match action {
        "close" => Request::PullRequestClose {
            repository: args.repo,
            number: args.number,
        },
        "reopen" => Request::PullRequestReopen {
            repository: args.repo,
            number: args.number,
        },
        "merge" => Request::PullRequestMerge {
            repository: args.repo,
            number: args.number,
        },
        "ready" => Request::PullRequestReady {
            repository: args.repo,
            number: args.number,
        },
        "draft" => Request::PullRequestDraft {
            repository: args.repo,
            number: args.number,
        },
        "update_branch" => Request::PullRequestUpdateBranch {
            repository: args.repo,
            number: args.number,
        },
        _ => bail!("unsupported pull request target action"),
    };
    execute(request, args.dry_run, json)
}

fn run_commit(command: CommitCommand, json: bool) -> Result<()> {
    match command {
        CommitCommand::Create(args) => {
            let files = commit::staged_files(&args.paths)?;
            execute(
                Request::CommitCreate {
                    repository: args.repo,
                    branch: args.branch,
                    message: read_message(&args.content)?,
                    files,
                },
                args.dry_run,
                json,
            )
        }
    }
}

fn run_workflow(command: WorkflowCommand, json: bool) -> Result<()> {
    match command {
        WorkflowCommand::Dispatch(args) => execute(
            Request::WorkflowDispatch {
                repository: args.repo,
                workflow: args.workflow,
                reference: args.reference,
                inputs: parse_workflow_inputs(&args.inputs)?,
            },
            args.dry_run,
            json,
        ),
    }
}

fn parse_workflow_inputs(values: &[String]) -> Result<BTreeMap<String, String>> {
    let mut inputs = BTreeMap::new();
    for value in values {
        let Some((key, value)) = value.split_once('=') else {
            bail!("workflow inputs must use KEY=VALUE");
        };
        if inputs.insert(key.to_owned(), value.to_owned()).is_some() {
            bail!("duplicate workflow input: {key}");
        }
    }
    Ok(inputs)
}

fn execute(request: Request, dry_run: bool, json: bool) -> Result<()> {
    let operation = request.prepare(dry_run)?;
    if dry_run {
        return emit_json(&operation);
    }
    let output = if local::runs_locally(&operation) {
        MutationResult {
            operation: operation.name().to_owned(),
            authored_by: "user",
            workflow_url: None,
            resource_url: Some(local::execute(&operation)?),
        }
    } else {
        let result = workflow::dispatch(&operation, !json)?;
        MutationResult {
            operation: operation.name().to_owned(),
            authored_by: "pukbot",
            workflow_url: Some(result.workflow_url),
            resource_url: result.resource_url,
        }
    };
    if json {
        emit_json(&output)
    } else {
        emit_text_result(&output)
    }
}

fn read_request(path: &Path) -> Result<Request> {
    let contents = if path == Path::new("-") {
        let mut contents = String::new();
        io::stdin()
            .read_to_string(&mut contents)
            .context("failed to read request JSON from stdin")?;
        contents
    } else {
        fs::read_to_string(path)
            .with_context(|| format!("failed to read request JSON from {}", path.display()))?
    };
    serde_json::from_str(&contents).context("failed to decode request JSON")
}

fn read_body(args: &BodyArgs) -> Result<String> {
    if let Some(body) = &args.body {
        return Ok(body.clone());
    }
    if let Some(path) = &args.body_file {
        if path == Path::new("-") {
            return read_stdin_body();
        }
        return fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()));
    }
    if io::stdin().is_terminal() {
        bail!("provide --body, --body-file, or pipe the body through stdin");
    }
    read_stdin_body()
}

fn read_optional_body(args: &BodyArgs) -> Result<Option<String>> {
    if args.body.is_none() && args.body_file.is_none() {
        return Ok(None);
    }
    read_body(args).map(Some)
}

fn read_message(args: &MessageArgs) -> Result<String> {
    if let Some(message) = &args.message {
        return Ok(message.clone());
    }
    if let Some(path) = &args.message_file {
        if path == Path::new("-") {
            return read_stdin_body();
        }
        return fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()));
    }
    if io::stdin().is_terminal() {
        bail!("provide --message, --message-file, or pipe the message through stdin");
    }
    read_stdin_body()
}

fn read_stdin_body() -> Result<String> {
    let mut body = String::new();
    io::stdin()
        .read_to_string(&mut body)
        .context("failed to read body from stdin")?;
    Ok(body)
}

fn emit_text_result(result: &MutationResult) -> Result<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "Authored by: {}", result.authored_by)?;
    if let Some(url) = &result.workflow_url {
        writeln!(stdout, "Workflow run: {url}")?;
    }
    if let Some(url) = &result.resource_url {
        writeln!(stdout, "Result: {url}")?;
    }
    Ok(())
}

fn emit_json(value: &impl Serialize) -> Result<()> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, value)?;
    writeln!(stdout)?;
    Ok(())
}

fn emit_capabilities(json: bool) -> Result<()> {
    let capabilities = Capabilities {
        protocol_version: 1,
        commands: vec![
            "apply",
            "comment.create",
            "comment.edit",
            "comment.delete",
            "comment.react",
            "issue.create",
            "issue.edit",
            "issue.close",
            "issue.reopen",
            "issue.labels",
            "issue.assignees",
            "issue.react",
            "pr.create",
            "pr.edit",
            "pr.close",
            "pr.reopen",
            "pr.merge",
            "pr.ready",
            "pr.draft",
            "pr.review",
            "pr.labels",
            "pr.assignees",
            "pr.react",
            "pr.update-branch",
            "commit.create",
            "workflow.dispatch",
            "completions",
            "man",
            "update",
        ],
        media: media::supported_extensions(),
        markdown: markdown_capabilities(),
        output: vec!["text", "json"],
        attribution: Attribution {
            user: vec![
                "pr.create",
                "pr.edit",
                "pr.close",
                "pr.reopen",
                "pr.merge",
                "pr.ready",
                "pr.draft",
                "pr.review",
                "pr.labels",
                "pr.assignees",
                "pr.react",
                "pr.update-branch",
            ],
            app: vec![
                "comment.create",
                "comment.edit",
                "comment.delete",
                "comment.react",
                "issue.create",
                "issue.edit",
                "issue.close",
                "issue.reopen",
                "issue.labels",
                "issue.assignees",
                "issue.react",
                "commit.create",
                "workflow.dispatch",
            ],
        },
    };
    if json {
        emit_json(&capabilities)
    } else {
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "protocol: {}", capabilities.protocol_version)?;
        writeln!(stdout, "commands: {}", capabilities.commands.join(", "))?;
        writeln!(stdout, "media: {}", capabilities.media.join(", "))?;
        writeln!(
            stdout,
            "markdown: {} verbatim, up to {} bytes",
            capabilities.markdown.flavor, capabilities.markdown.max_body_bytes
        )?;
        writeln!(
            stdout,
            "markdown features: {}",
            capabilities.markdown.features.join(", ")
        )?;
        writeln!(stdout, "output: {}", capabilities.output.join(", "))?;
        writeln!(
            stdout,
            "authored by you: {}",
            capabilities.attribution.user.join(", ")
        )?;
        writeln!(
            stdout,
            "authored by pukbot: {}",
            capabilities.attribution.app.join(", ")
        )?;
        Ok(())
    }
}

fn markdown_capabilities() -> Markdown {
    Markdown {
        flavor: "github",
        verbatim: true,
        max_body_bytes: MAX_BODY_BYTES,
        footer_operations: vec!["comment_create", "comment_edit"],
        features: vec![
            "headings",
            "emphasis",
            "strikethrough",
            "inline-code",
            "fenced-code",
            "syntax-highlighting",
            "line-breaks",
            "lists",
            "task-lists",
            "tables",
            "blockquotes",
            "alerts",
            "details",
            "footnotes",
            "links",
            "autolinks",
            "images",
            "media-placeholders",
            "mentions",
            "references",
            "permalinks",
            "emoji",
            "math",
            "mermaid",
            "geojson",
            "topojson",
            "stl",
            "html-subset",
            "html-comments",
            "escapes",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::parse_workflow_inputs;

    #[test]
    fn parses_workflow_inputs_at_first_equals_sign() {
        let values = vec!["release=true".to_owned(), "message=a=b".to_owned()];
        let inputs = parse_workflow_inputs(&values).expect("workflow inputs should parse");
        assert_eq!(inputs.get("release").map(String::as_str), Some("true"));
        assert_eq!(inputs.get("message").map(String::as_str), Some("a=b"));
    }

    #[test]
    fn rejects_duplicate_workflow_inputs() {
        let values = vec!["release=true".to_owned(), "release=false".to_owned()];
        assert!(parse_workflow_inputs(&values).is_err());
    }

    #[test]
    fn rejects_workflow_input_without_equals_sign() {
        let values = vec!["release".to_owned()];
        assert!(parse_workflow_inputs(&values).is_err());
    }
}
