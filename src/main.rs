use std::ffi::OsString;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use serde::Serialize;

const WORKFLOW_REPOSITORY: &str = "pulkitxm/Pukbot";
const WORKFLOW_FILE: &str = "comment.yml";
const WORKFLOW_REF: &str = "main";
const MAX_BODY_BYTES: usize = 40_000;
const FOOTER: &str = "_Automated comment posted by Pukbot from an agent-assisted workflow._";

#[derive(Debug, Parser)]
#[command(name = "pukbot", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Comment(CommentArgs),
}

#[derive(Debug, Args)]
struct CommentArgs {
    number: NonZeroU64,
    #[arg(long, value_name = "OWNER/REPOSITORY")]
    repo: Repository,
    #[arg(long, conflicts_with = "body_file")]
    body: Option<String>,
    #[arg(long, value_name = "FILE", conflicts_with = "body")]
    body_file: Option<PathBuf>,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Repository {
    owner: String,
    name: String,
}

impl FromStr for Repository {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some((owner, name)) = value.split_once('/') else {
            return Err("repository must use OWNER/REPOSITORY".to_owned());
        };
        if owner.is_empty() || name.is_empty() || name.contains('/') {
            return Err("repository must use OWNER/REPOSITORY".to_owned());
        }
        if !owner.chars().all(is_repository_character) || !name.chars().all(is_repository_character)
        {
            return Err("repository contains unsupported characters".to_owned());
        }
        Ok(Self {
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }
}

#[derive(Debug, Serialize)]
struct WorkflowInputs<'a> {
    owner: &'a str,
    repository: &'a str,
    number: String,
    body: &'a str,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Comment(args) => run_comment(&args),
    }
}

fn run_comment(args: &CommentArgs) -> Result<()> {
    let body = read_body(args.body.as_deref(), args.body_file.as_deref())?;
    validate_body(&body)?;
    if args.dry_run {
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "{body}\n\n---\n\n{FOOTER}")?;
        return Ok(());
    }
    dispatch_workflow(&args.repo, args.number, &body)
}

fn read_body(body: Option<&str>, body_file: Option<&Path>) -> Result<String> {
    if let Some(body) = body {
        return Ok(body.to_owned());
    }
    if let Some(path) = body_file {
        if path == Path::new("-") {
            return read_stdin();
        }
        return fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()));
    }
    if io::stdin().is_terminal() {
        bail!("provide --body, --body-file, or pipe the comment through stdin");
    }
    read_stdin()
}

fn read_stdin() -> Result<String> {
    let mut body = String::new();
    io::stdin()
        .read_to_string(&mut body)
        .context("failed to read comment from stdin")?;
    Ok(body)
}

fn validate_body(body: &str) -> Result<()> {
    if body.trim().is_empty() {
        bail!("comment body cannot be empty");
    }
    if body.len() > MAX_BODY_BYTES {
        bail!("comment body exceeds {MAX_BODY_BYTES} bytes");
    }
    Ok(())
}

fn dispatch_workflow(repository: &Repository, number: NonZeroU64, body: &str) -> Result<()> {
    let inputs = WorkflowInputs {
        owner: &repository.owner,
        repository: &repository.name,
        number: number.to_string(),
        body,
    };
    let mut child = Command::new("gh")
        .args(workflow_arguments())
        .stdin(Stdio::piped())
        .spawn()
        .context("failed to launch gh; install and authenticate GitHub CLI")?;
    let mut stdin = child
        .stdin
        .take()
        .context("failed to open gh workflow input")?;
    serde_json::to_writer(&mut stdin, &inputs).context("failed to encode workflow input")?;
    drop(stdin);
    let status = child.wait().context("failed to wait for gh")?;
    if !status.success() {
        bail!("GitHub rejected the Pukbot workflow dispatch");
    }
    Ok(())
}

fn workflow_arguments() -> [OsString; 8] {
    [
        OsString::from("workflow"),
        OsString::from("run"),
        OsString::from(WORKFLOW_FILE),
        OsString::from("--repo"),
        OsString::from(WORKFLOW_REPOSITORY),
        OsString::from("--ref"),
        OsString::from(WORKFLOW_REF),
        OsString::from("--json"),
    ]
}

fn is_repository_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
}

#[cfg(test)]
mod tests {
    use super::{FOOTER, MAX_BODY_BYTES, Repository, is_repository_character, validate_body};

    #[test]
    fn parses_repository() {
        let repository = "pulkitxm/Pukbot".parse::<Repository>();
        assert_eq!(
            repository,
            Ok(Repository {
                owner: "pulkitxm".to_owned(),
                name: "Pukbot".to_owned(),
            })
        );
    }

    #[test]
    fn rejects_invalid_repository() {
        assert!("Pukbot".parse::<Repository>().is_err());
        assert!("pulkitxm/Pukbot/extra".parse::<Repository>().is_err());
        assert!("pulkitxm/Puk bot".parse::<Repository>().is_err());
    }

    #[test]
    fn validates_body() {
        assert!(validate_body("hello").is_ok());
        assert!(validate_body("  ").is_err());
        assert!(validate_body(&"x".repeat(MAX_BODY_BYTES + 1)).is_err());
    }

    #[test]
    fn footer_is_disclosed() {
        assert!(FOOTER.contains("Automated comment"));
        assert!(FOOTER.contains("agent-assisted"));
    }

    #[test]
    fn accepts_expected_repository_characters() {
        assert!("Pulkit_1.test-name".chars().all(is_repository_character));
    }
}
