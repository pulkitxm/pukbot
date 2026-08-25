mod commit;
mod completion;
mod manual;
mod media;
mod model;
mod update;
mod workflow;

use std::io::{self, IsTerminal, Read, Write};
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::{fs, process};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;
use model::{CommentDocument, Reaction, Repository, Request, ReviewEvent};
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(name = "pukbot", version, about)]
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
    #[arg(long, conflicts_with = "body_file")]
    body: Option<String>,
    #[arg(long, value_name = "FILE", conflicts_with = "body")]
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
    workflow_url: String,
    resource_url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Capabilities {
    protocol_version: u8,
    commands: Vec<&'static str>,
    media: Vec<&'static str>,
    output: Vec<&'static str>,
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

fn execute(request: Request, dry_run: bool, json: bool) -> Result<()> {
    let operation = request.prepare(dry_run)?;
    if dry_run {
        return emit_json(&operation);
    }
    let result = workflow::dispatch(&operation, !json)?;
    let output = MutationResult {
        operation: operation.name().to_owned(),
        workflow_url: result.workflow_url,
        resource_url: result.resource_url,
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
    writeln!(stdout, "Workflow run: {}", result.workflow_url)?;
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
            "completions",
            "man",
            "update",
        ],
        media: media::supported_extensions(),
        output: vec!["text", "json"],
    };
    if json {
        emit_json(&capabilities)
    } else {
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "protocol: {}", capabilities.protocol_version)?;
        writeln!(stdout, "commands: {}", capabilities.commands.join(", "))?;
        writeln!(stdout, "media: {}", capabilities.media.join(", "))?;
        writeln!(stdout, "output: {}", capabilities.output.join(", "))?;
        Ok(())
    }
}
