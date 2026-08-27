mod commit;
mod completion;
mod local;
mod manual;
mod media;
mod model;
mod release_asset;
mod update;
mod workflow;
mod workflow_run;

use std::collections::BTreeMap;
use std::io::{self, IsTerminal, Read, Write};
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{fs, process};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;
use model::{
    CommentDocument, MAX_BODY_BYTES, Reaction, ReleaseLatest, Repository, Request, ReviewEvent,
};
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
    Wiki {
        #[command(subcommand)]
        command: WikiCommand,
    },
    Repository {
        #[command(subcommand)]
        command: RepositoryCommand,
    },
    Ref {
        #[command(subcommand)]
        command: GitRefCommand,
    },
    Tag {
        #[command(subcommand)]
        command: TagCommand,
    },
    Release {
        #[command(subcommand)]
        command: ReleaseCommand,
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
    UpdateBranch(PullRequestUpdateBranchArgs),
}

#[derive(Debug, Subcommand)]
enum CommitCommand {
    Create(CommitCreateArgs),
}

#[derive(Debug, Subcommand)]
enum WikiCommand {
    Publish(WikiPublishArgs),
}

#[derive(Debug, Subcommand)]
enum RepositoryCommand {
    Dispatch(RepositoryDispatchArgs),
}

#[derive(Debug, Subcommand)]
enum GitRefCommand {
    Create(GitRefCreateArgs),
    Delete(GitRefDeleteArgs),
}

#[derive(Debug, Subcommand)]
enum TagCommand {
    Create(TagCreateArgs),
    Delete(TagDeleteArgs),
}

#[derive(Debug, Subcommand)]
enum ReleaseCommand {
    Create(ReleaseCreateArgs),
    Edit(ReleaseEditArgs),
    Delete(ReleaseDeleteArgs),
    UploadAsset(ReleaseAssetUploadArgs),
}

#[derive(Debug, Subcommand)]
enum WorkflowCommand {
    Dispatch(WorkflowDispatchArgs),
    Cancel(WorkflowRunArgs),
    Rerun(WorkflowRerunArgs),
    Enable(WorkflowTargetArgs),
    Disable(WorkflowTargetArgs),
    Status(WorkflowInspectArgs),
    Watch(WorkflowWatchArgs),
    Logs(WorkflowLogsArgs),
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
    as_app: bool,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct WikiPublishArgs {
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long, requires = "source_path")]
    source_ref: Option<String>,
    #[arg(long, requires = "source_ref")]
    source_path: Option<String>,
    #[arg(long = "delete", value_name = "PATH")]
    delete: Vec<String>,
    #[arg(long)]
    replace: bool,
    #[command(flatten)]
    content: MessageArgs,
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
    #[arg(long)]
    watch: bool,
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u64).range(1..=60))]
    interval: u64,
}

#[derive(Debug, Args)]
struct RepositoryDispatchArgs {
    #[arg(value_name = "EVENT_TYPE")]
    event_type: String,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long, value_name = "JSON", conflicts_with = "client_payload_file")]
    client_payload: Option<String>,
    #[arg(long, value_name = "FILE", conflicts_with = "client_payload")]
    client_payload_file: Option<PathBuf>,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct GitRefCreateArgs {
    #[arg(value_name = "REF")]
    reference: String,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long, value_name = "SHA")]
    sha: String,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct GitRefDeleteArgs {
    #[arg(value_name = "REF")]
    reference: String,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long)]
    yes: bool,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct TagCreateArgs {
    #[arg(value_name = "TAG")]
    tag: String,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long, value_name = "SHA")]
    target: String,
    #[arg(long, conflicts_with = "message_file")]
    message: Option<String>,
    #[arg(long, value_name = "FILE", conflicts_with = "message")]
    message_file: Option<PathBuf>,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct TagDeleteArgs {
    #[arg(value_name = "TAG")]
    tag: String,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long)]
    yes: bool,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct ReleaseCreateArgs {
    #[arg(value_name = "TAG")]
    tag: String,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long)]
    name: Option<String>,
    #[command(flatten)]
    content: BodyArgs,
    #[arg(long, value_name = "BRANCH_OR_SHA")]
    target: Option<String>,
    #[command(flatten)]
    flags: ReleaseCreateFlags,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct ReleaseCreateFlags {
    #[arg(long)]
    draft: bool,
    #[arg(long)]
    prerelease: bool,
    #[arg(long)]
    generate_notes: bool,
}

#[derive(Debug, Args)]
struct ReleaseEditArgs {
    #[arg(value_name = "RELEASE_ID")]
    release_id: NonZeroU64,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long)]
    tag: Option<String>,
    #[arg(long)]
    name: Option<String>,
    #[command(flatten)]
    content: BodyArgs,
    #[arg(long, value_name = "BOOL")]
    draft: Option<bool>,
    #[arg(long, value_name = "BOOL")]
    prerelease: Option<bool>,
    #[arg(long, value_enum)]
    make_latest: Option<ReleaseLatest>,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct ReleaseDeleteArgs {
    #[arg(value_name = "RELEASE_ID")]
    release_id: NonZeroU64,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long)]
    yes: bool,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct ReleaseAssetUploadArgs {
    #[arg(value_name = "RELEASE_ID")]
    release_id: NonZeroU64,
    #[arg(value_name = "PATH")]
    path: PathBuf,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    label: Option<String>,
    #[arg(long)]
    content_type: Option<String>,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct WorkflowRunArgs {
    #[arg(value_name = "RUN_ID")]
    run_id: NonZeroU64,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct WorkflowRerunArgs {
    #[command(flatten)]
    run: WorkflowRunArgs,
    #[arg(long)]
    failed_only: bool,
}

#[derive(Debug, Args)]
struct WorkflowTargetArgs {
    #[arg(value_name = "WORKFLOW")]
    workflow: String,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct WorkflowInspectArgs {
    #[arg(value_name = "RUN_ID")]
    run_id: NonZeroU64,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
}

#[derive(Debug, Args)]
struct WorkflowWatchArgs {
    #[command(flatten)]
    run: WorkflowInspectArgs,
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u64).range(1..=60))]
    interval: u64,
}

#[derive(Debug, Args)]
struct WorkflowLogsArgs {
    #[command(flatten)]
    run: WorkflowInspectArgs,
    #[arg(long)]
    failed: bool,
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
    as_app: bool,
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
    as_app: bool,
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
    as_app: bool,
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
    as_app: bool,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct PullRequestUpdateBranchArgs {
    #[command(flatten)]
    target: TargetArgs,
    #[arg(long)]
    as_app: bool,
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
struct WorkflowDispatchWatchResult {
    mutation: MutationResult,
    run: workflow_run::WorkflowRun,
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
    modes: Modes,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Attribution {
    user: Vec<&'static str>,
    app: Vec<&'static str>,
    read_only: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Modes {
    app_authored: Vec<&'static str>,
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
        Commands::Wiki { command } => run_wiki(command, cli.json),
        Commands::Repository { command } => run_repository(command, cli.json),
        Commands::Ref { command } => run_git_ref(command, cli.json),
        Commands::Tag { command } => run_tag(command, cli.json),
        Commands::Release { command } => run_release(command, cli.json),
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
                as_app: args.as_app,
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
                as_app: args.as_app,
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
            execute(
                Request::PullRequestMerge {
                    repository: args.target.repo,
                    number: args.target.number,
                    as_app: args.as_app,
                },
                args.target.dry_run,
                json,
            )
        }
        PullRequestCommand::Ready(args) => execute_pull_request_target(args, json, "ready"),
        PullRequestCommand::Draft(args) => execute_pull_request_target(args, json, "draft"),
        PullRequestCommand::Review(args) => execute(
            Request::PullRequestReview {
                repository: args.repo,
                number: args.number,
                event: args.event,
                body: read_optional_body(&args.content)?,
                as_app: args.as_app,
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
        PullRequestCommand::UpdateBranch(args) => execute(
            Request::PullRequestUpdateBranch {
                repository: args.target.repo,
                number: args.target.number,
                as_app: args.as_app,
            },
            args.target.dry_run,
            json,
        ),
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
        "ready" => Request::PullRequestReady {
            repository: args.repo,
            number: args.number,
        },
        "draft" => Request::PullRequestDraft {
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
                    as_app: args.as_app,
                },
                args.dry_run,
                json,
            )
        }
    }
}

fn run_wiki(command: WikiCommand, json: bool) -> Result<()> {
    match command {
        WikiCommand::Publish(args) => execute(
            Request::WikiPublish {
                repository: args.repo,
                message: read_message(&args.content)?,
                source_ref: args.source_ref,
                source_path: args.source_path,
                delete: args.delete,
                replace: args.replace,
            },
            args.dry_run,
            json,
        ),
    }
}

fn run_repository(command: RepositoryCommand, json: bool) -> Result<()> {
    match command {
        RepositoryCommand::Dispatch(args) => execute(
            Request::RepositoryDispatch {
                repository: args.repo,
                event_type: args.event_type,
                client_payload: read_client_payload(
                    args.client_payload.as_deref(),
                    args.client_payload_file.as_deref(),
                )?,
            },
            args.dry_run,
            json,
        ),
    }
}

fn run_git_ref(command: GitRefCommand, json: bool) -> Result<()> {
    match command {
        GitRefCommand::Create(args) => execute(
            Request::RefCreate {
                repository: args.repo,
                reference: args.reference,
                sha: args.sha,
            },
            args.dry_run,
            json,
        ),
        GitRefCommand::Delete(args) => {
            if !args.yes && !args.dry_run {
                bail!("ref deletion requires --yes");
            }
            execute(
                Request::RefDelete {
                    repository: args.repo,
                    reference: args.reference,
                },
                args.dry_run,
                json,
            )
        }
    }
}

fn run_tag(command: TagCommand, json: bool) -> Result<()> {
    match command {
        TagCommand::Create(args) => execute(
            Request::TagCreate {
                repository: args.repo,
                tag: args.tag,
                target: args.target,
                message: read_optional_message(
                    args.message.as_deref(),
                    args.message_file.as_deref(),
                )?,
            },
            args.dry_run,
            json,
        ),
        TagCommand::Delete(args) => {
            if !args.yes && !args.dry_run {
                bail!("tag deletion requires --yes");
            }
            execute(
                Request::TagDelete {
                    repository: args.repo,
                    tag: args.tag,
                },
                args.dry_run,
                json,
            )
        }
    }
}

fn run_release(command: ReleaseCommand, json: bool) -> Result<()> {
    match command {
        ReleaseCommand::Create(args) => execute(
            Request::ReleaseCreate {
                repository: args.repo,
                tag: args.tag,
                name: args.name,
                body: read_optional_body(&args.content)?,
                target: args.target,
                draft: args.flags.draft,
                prerelease: args.flags.prerelease,
                generate_notes: args.flags.generate_notes,
            },
            args.dry_run,
            json,
        ),
        ReleaseCommand::Edit(args) => execute(
            Request::ReleaseEdit {
                repository: args.repo,
                release_id: args.release_id,
                tag: args.tag,
                name: args.name,
                body: read_optional_body(&args.content)?,
                draft: args.draft,
                prerelease: args.prerelease,
                make_latest: args.make_latest,
            },
            args.dry_run,
            json,
        ),
        ReleaseCommand::Delete(args) => {
            if !args.yes && !args.dry_run {
                bail!("release deletion requires --yes");
            }
            execute(
                Request::ReleaseDelete {
                    repository: args.repo,
                    release_id: args.release_id,
                },
                args.dry_run,
                json,
            )
        }
        ReleaseCommand::UploadAsset(args) => {
            let asset =
                release_asset::prepare(&args.path, args.name, args.label, args.content_type)?;
            execute(
                Request::ReleaseAssetUpload {
                    repository: args.repo,
                    release_id: args.release_id,
                    name: asset.name,
                    label: asset.label,
                    content_type: asset.content_type,
                    content_base64: asset.content_base64,
                },
                args.dry_run,
                json,
            )
        }
    }
}

fn run_workflow(command: WorkflowCommand, json: bool) -> Result<()> {
    match command {
        WorkflowCommand::Dispatch(args) => {
            let repository = args.repo.clone();
            let request = Request::WorkflowDispatch {
                repository: args.repo,
                workflow: args.workflow,
                reference: args.reference,
                inputs: parse_workflow_inputs(&args.inputs)?,
            };
            if args.watch {
                if args.dry_run {
                    bail!("--watch cannot be combined with --dry-run");
                }
                execute_workflow_dispatch(
                    request,
                    &repository,
                    Duration::from_secs(args.interval),
                    json,
                )
            } else {
                execute(request, args.dry_run, json)
            }
        }
        WorkflowCommand::Cancel(args) => execute(
            Request::WorkflowCancel {
                repository: args.repo,
                run_id: args.run_id,
            },
            args.dry_run,
            json,
        ),
        WorkflowCommand::Rerun(args) => execute(
            Request::WorkflowRerun {
                repository: args.run.repo,
                run_id: args.run.run_id,
                failed_only: args.failed_only,
            },
            args.run.dry_run,
            json,
        ),
        WorkflowCommand::Enable(args) => execute(
            Request::WorkflowEnable {
                repository: args.repo,
                workflow: args.workflow,
            },
            args.dry_run,
            json,
        ),
        WorkflowCommand::Disable(args) => execute(
            Request::WorkflowDisable {
                repository: args.repo,
                workflow: args.workflow,
            },
            args.dry_run,
            json,
        ),
        WorkflowCommand::Status(args) => {
            let run = workflow_run::inspect(&args.repo, args.run_id)?;
            workflow_run::emit_status(&run, json)
        }
        WorkflowCommand::Watch(args) => {
            let output = if json {
                workflow_run::WatchOutput::Json
            } else {
                workflow_run::WatchOutput::Text
            };
            workflow_run::watch(
                &args.run.repo,
                args.run.run_id,
                Duration::from_secs(args.interval),
                output,
            )?;
            Ok(())
        }
        WorkflowCommand::Logs(args) => {
            workflow_run::emit_logs(&args.run.repo, args.run.run_id, args.failed, json)
        }
    }
}

fn execute_workflow_dispatch(
    request: Request,
    repository: &Repository,
    interval: Duration,
    json: bool,
) -> Result<()> {
    let operation = request.prepare(false)?;
    let mutation = perform_operation(&operation, !json)?;
    let resource_url = mutation
        .resource_url
        .as_deref()
        .context("workflow dispatch did not return a run URL")?;
    let run_id = workflow_run::run_id_from_url(repository, resource_url)?;
    if !json {
        emit_text_result(&mutation)?;
    }
    let output = if json {
        workflow_run::WatchOutput::Quiet
    } else {
        workflow_run::WatchOutput::Text
    };
    let run = workflow_run::watch(repository, run_id, interval, output)?;
    if json {
        emit_json(&WorkflowDispatchWatchResult { mutation, run })?;
    }
    Ok(())
}

fn read_client_payload(
    inline: Option<&str>,
    path: Option<&Path>,
) -> Result<BTreeMap<String, serde_json::Value>> {
    let contents = if let Some(inline) = inline {
        inline.to_owned()
    } else if let Some(path) = path {
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?
    } else {
        "{}".to_owned()
    };
    serde_json::from_str(&contents).context("client payload must be a JSON object")
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
    let output = perform_operation(&operation, !json)?;
    if json {
        emit_json(&output)
    } else {
        emit_text_result(&output)
    }
}

fn perform_operation(operation: &model::Operation, stream: bool) -> Result<MutationResult> {
    if local::runs_locally(operation) {
        Ok(MutationResult {
            operation: operation.name().to_owned(),
            authored_by: "user",
            workflow_url: None,
            resource_url: Some(local::execute(operation)?),
        })
    } else {
        let result = workflow::dispatch(operation, stream)?;
        Ok(MutationResult {
            operation: operation.name().to_owned(),
            authored_by: "pukbot",
            workflow_url: Some(result.workflow_url),
            resource_url: result.resource_url,
        })
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

fn read_optional_message(inline: Option<&str>, path: Option<&Path>) -> Result<Option<String>> {
    if let Some(message) = inline {
        return Ok(Some(message.to_owned()));
    }
    if let Some(path) = path {
        if path == Path::new("-") {
            return read_stdin_body().map(Some);
        }
        return fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))
            .map(Some);
    }
    Ok(None)
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
    let capabilities = capabilities();
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
        writeln!(
            stdout,
            "read only: {}",
            capabilities.attribution.read_only.join(", ")
        )?;
        writeln!(
            stdout,
            "app-authored mode: {}",
            capabilities.modes.app_authored.join(", ")
        )?;
        Ok(())
    }
}

fn capabilities() -> Capabilities {
    Capabilities {
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
            "wiki.publish",
            "repository.dispatch",
            "ref.create",
            "ref.delete",
            "tag.create",
            "tag.delete",
            "release.create",
            "release.edit",
            "release.delete",
            "release.upload-asset",
            "workflow.dispatch",
            "workflow.cancel",
            "workflow.rerun",
            "workflow.enable",
            "workflow.disable",
            "workflow.status",
            "workflow.watch",
            "workflow.logs",
            "completions",
            "man",
            "update",
        ],
        media: media::supported_extensions(),
        markdown: markdown_capabilities(),
        output: vec!["text", "json"],
        attribution: attribution_capabilities(),
        modes: mode_capabilities(),
    }
}

fn attribution_capabilities() -> Attribution {
    Attribution {
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
            "pr.create",
            "pr.edit",
            "pr.merge",
            "pr.review",
            "pr.update-branch",
            "commit.create",
            "wiki.publish",
            "repository.dispatch",
            "ref.create",
            "ref.delete",
            "tag.create",
            "tag.delete",
            "release.create",
            "release.edit",
            "release.delete",
            "release.upload-asset",
            "workflow.dispatch",
            "workflow.cancel",
            "workflow.rerun",
            "workflow.enable",
            "workflow.disable",
        ],
        read_only: vec!["workflow.status", "workflow.watch", "workflow.logs"],
    }
}

fn mode_capabilities() -> Modes {
    Modes {
        app_authored: vec![
            "commit.create",
            "pr.create",
            "pr.edit",
            "pr.merge",
            "pr.review",
            "pr.update-branch",
        ],
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
    use std::path::Path;

    use super::{parse_workflow_inputs, read_client_payload};

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

    #[test]
    fn parses_inline_repository_dispatch_payload() {
        let payload = read_client_payload(Some(r#"{"version":"1.2.3","retry":false}"#), None)
            .expect("client payload should parse");
        assert_eq!(payload.get("version"), Some(&serde_json::json!("1.2.3")));
        assert_eq!(payload.get("retry"), Some(&serde_json::json!(false)));
    }

    #[test]
    fn defaults_repository_dispatch_payload_to_empty_object() {
        let payload = read_client_payload(None, None).expect("client payload should default");
        assert!(payload.is_empty());
    }

    #[test]
    fn rejects_non_object_repository_dispatch_payload() {
        assert!(read_client_payload(Some("[]"), None).is_err());
    }

    #[test]
    fn rejects_missing_repository_dispatch_payload_file() {
        assert!(read_client_payload(None, Some(Path::new("missing-payload.json"))).is_err());
    }
}
