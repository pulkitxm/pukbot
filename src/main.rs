mod media;
mod model;
mod workflow;

use std::io::{self, IsTerminal, Read, Write};
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::{fs, process};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use model::{CommentDocument, Reaction, Repository, Request};
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(name = "pukbot", version, about)]
struct Cli {
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
    Capabilities,
}

#[derive(Debug, Args)]
struct ApplyArgs {
    #[arg(long, value_name = "FILE")]
    input: PathBuf,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Subcommand)]
enum CommentCommand {
    Create(CommentCreateArgs),
    Edit(CommentEditArgs),
    Delete(CommentDeleteArgs),
    React(CommentReactArgs),
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
        Commands::Capabilities => emit_capabilities(cli.json),
    }
}

fn run_comment(command: CommentCommand, json: bool) -> Result<()> {
    match command {
        CommentCommand::Create(args) => execute(
            Request::Create {
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
            Request::Edit {
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
                Request::Delete {
                    repository: args.repo,
                    comment_id: args.comment_id,
                },
                args.dry_run,
                json,
            )
        }
        CommentCommand::React(args) => execute(
            Request::React {
                repository: args.repo,
                comment_id: args.comment_id,
                reaction: args.reaction,
            },
            args.dry_run,
            json,
        ),
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
