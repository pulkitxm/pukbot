use std::ffi::OsString;
use std::io::{self, IsTerminal, Read, Write};
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{fs, thread};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};

const WORKFLOW_REPOSITORY: &str = "pulkitxm/gitbot";
const WORKFLOW_FILE: &str = "comment.yml";
const WORKFLOW_REF: &str = "main";
const MAX_BODY_BYTES: usize = 40_000;
const FOOTER: &str = "*Automated comment posted by Gitbot from an agent-assisted workflow.*";
const RUN_LOOKUP_ATTEMPTS: usize = 30;
const RUN_POLL_ATTEMPTS: usize = 300;

#[derive(Debug, Parser)]
#[command(name = "gitbot", version, about)]
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
    #[arg(long, value_name = "URL")]
    image: Vec<String>,
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
    requester: &'a str,
    request_id: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowRun {
    database_id: u64,
    display_title: String,
    status: String,
    conclusion: String,
    url: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Comment(args) => run_comment(&args),
    }
}

fn run_comment(args: &CommentArgs) -> Result<()> {
    let body = append_images(
        read_body(args.body.as_deref(), args.body_file.as_deref())?,
        &args.image,
    )?;
    validate_body(&body)?;
    let requester = github_login()?;
    if args.dry_run {
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "{body}\n\n---\n\n{FOOTER}\n\nfrom: @{requester}")?;
        return Ok(());
    }
    dispatch_workflow(&args.repo, args.number, &body, &requester)
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

fn github_login() -> Result<String> {
    let output = Command::new("gh")
        .args(["api", "user", "--jq", ".login"])
        .output()
        .context("failed to launch gh; install and authenticate GitHub CLI")?;
    if !output.status.success() {
        bail!("failed to resolve the authenticated GitHub user");
    }
    let login = String::from_utf8(output.stdout)
        .context("GitHub CLI returned a non-UTF-8 login")?
        .trim()
        .to_owned();
    if login.is_empty() || !login.chars().all(is_repository_character) {
        bail!("GitHub CLI returned an invalid login");
    }
    Ok(login)
}

fn dispatch_workflow(
    repository: &Repository,
    number: NonZeroU64,
    body: &str,
    requester: &str,
) -> Result<()> {
    let request_id = new_request_id()?;
    let inputs = WorkflowInputs {
        owner: &repository.owner,
        repository: &repository.name,
        number: number.to_string(),
        body,
        requester,
        request_id: &request_id,
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
        bail!("GitHub rejected the Gitbot workflow dispatch");
    }
    let run = find_workflow_run(&request_id)?;
    follow_workflow_run(&run)
}

fn new_request_id() -> Result<String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?;
    Ok(format!("{}-{}", elapsed.as_nanos(), std::process::id()))
}

fn find_workflow_run(request_id: &str) -> Result<WorkflowRun> {
    let expected_title = format!("Gitbot comment {request_id}");
    for _ in 0..RUN_LOOKUP_ATTEMPTS {
        let runs = list_workflow_runs()?;
        if let Some(run) = runs
            .into_iter()
            .find(|run| run.display_title == expected_title)
        {
            return Ok(run);
        }
        thread::sleep(Duration::from_secs(1));
    }
    bail!("could not find the dispatched Gitbot workflow run")
}

fn list_workflow_runs() -> Result<Vec<WorkflowRun>> {
    let output = Command::new("gh")
        .args([
            "run",
            "list",
            "--repo",
            WORKFLOW_REPOSITORY,
            "--workflow",
            WORKFLOW_FILE,
            "--event",
            "workflow_dispatch",
            "--limit",
            "30",
            "--json",
            "databaseId,displayTitle,status,conclusion,url",
        ])
        .output()
        .context("failed to list Gitbot workflow runs")?;
    if !output.status.success() {
        bail!("failed to list Gitbot workflow runs");
    }
    serde_json::from_slice(&output.stdout).context("failed to decode Gitbot workflow runs")
}

fn follow_workflow_run(run: &WorkflowRun) -> Result<()> {
    println!("Workflow run: {}", run.url);
    let status = Command::new("gh")
        .args([
            "run",
            "watch",
            &run.database_id.to_string(),
            "--repo",
            WORKFLOW_REPOSITORY,
            "--exit-status",
        ])
        .status()
        .context("failed to watch the Gitbot workflow run")?;
    let completed = if status.success() {
        workflow_run(run.database_id)?
    } else {
        wait_for_workflow_run(run.database_id)?
    };
    if completed.conclusion != "success" {
        show_failed_logs(completed.database_id);
        bail!("Gitbot workflow failed: {}", completed.url);
    }
    if let Some(comment_url) = find_comment_url(completed.database_id)? {
        println!("Comment posted: {comment_url}");
    } else {
        println!("Gitbot workflow completed successfully");
    }
    Ok(())
}

fn wait_for_workflow_run(run_id: u64) -> Result<WorkflowRun> {
    let mut last_status = String::new();
    for _ in 0..RUN_POLL_ATTEMPTS {
        let run = workflow_run(run_id)?;
        if run.status != last_status {
            println!("Workflow status: {}", run.status);
            last_status.clone_from(&run.status);
        }
        if run.status == "completed" {
            return Ok(run);
        }
        thread::sleep(Duration::from_secs(2));
    }
    bail!("timed out waiting for the Gitbot workflow run")
}

fn workflow_run(run_id: u64) -> Result<WorkflowRun> {
    let output = Command::new("gh")
        .args([
            "run",
            "view",
            &run_id.to_string(),
            "--repo",
            WORKFLOW_REPOSITORY,
            "--json",
            "databaseId,displayTitle,status,conclusion,url",
        ])
        .output()
        .context("failed to inspect the Gitbot workflow run")?;
    if !output.status.success() {
        bail!("failed to inspect the Gitbot workflow run");
    }
    serde_json::from_slice(&output.stdout).context("failed to decode the Gitbot workflow run")
}

fn show_failed_logs(run_id: u64) {
    let _ = Command::new("gh")
        .args([
            "run",
            "view",
            &run_id.to_string(),
            "--repo",
            WORKFLOW_REPOSITORY,
            "--log-failed",
        ])
        .status();
}

fn find_comment_url(run_id: u64) -> Result<Option<String>> {
    let output = Command::new("gh")
        .args([
            "run",
            "view",
            &run_id.to_string(),
            "--repo",
            WORKFLOW_REPOSITORY,
            "--log",
        ])
        .output()
        .context("failed to read Gitbot workflow logs")?;
    if !output.status.success() {
        return Ok(None);
    }
    let logs = String::from_utf8(output.stdout).context("workflow logs were not UTF-8")?;
    Ok(parse_comment_url(&logs))
}

fn parse_comment_url(logs: &str) -> Option<String> {
    logs.lines().find_map(|line| {
        line.split_once("gitbot-comment-url=")
            .map(|(_, url)| url.trim())
            .filter(|url| url.starts_with("https://github.com/") && url.contains("#issuecomment-"))
            .map(str::to_owned)
    })
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

fn append_images(mut body: String, images: &[String]) -> Result<String> {
    for image in images {
        if !(image.starts_with("https://") || image.starts_with("http://")) {
            bail!("image must be an HTTP or HTTPS URL");
        }
        if image.contains(['\n', '\r', ')']) {
            bail!("image URL contains unsupported characters");
        }
        body.push_str("\n\n![attachment](");
        body.push_str(image);
        body.push(')');
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::{
        FOOTER, MAX_BODY_BYTES, Repository, append_images, is_repository_character, new_request_id,
        parse_comment_url, validate_body,
    };

    #[test]
    fn parses_repository() {
        let repository = "pulkitxm/gitbot".parse::<Repository>();
        assert_eq!(
            repository,
            Ok(Repository {
                owner: "pulkitxm".to_owned(),
                name: "gitbot".to_owned(),
            })
        );
    }

    #[test]
    fn rejects_invalid_repository() {
        assert!("Gitbot".parse::<Repository>().is_err());
        assert!("pulkitxm/gitbot/extra".parse::<Repository>().is_err());
        assert!("pulkitxm/Git bot".parse::<Repository>().is_err());
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

    #[test]
    fn generates_safe_request_id() {
        let request_id = new_request_id().expect("request ID should be generated");
        assert!(
            request_id
                .chars()
                .all(|character| character.is_ascii_digit() || character == '-')
        );
    }

    #[test]
    fn extracts_comment_url_from_logs() {
        let logs = "echo \"gitbot-comment-url=${comment_url}\"\nPost comment\tstep\tgitbot-comment-url=https://github.com/owner/repo/pull/1#issuecomment-2";
        assert_eq!(
            parse_comment_url(logs),
            Some("https://github.com/owner/repo/pull/1#issuecomment-2".to_owned())
        );
    }

    #[test]
    fn appends_image_urls() {
        let body = append_images(
            "result".to_owned(),
            &["https://example.com/result.png".to_owned()],
        );
        assert_eq!(
            body.expect("image URL should be accepted"),
            "result\n\n![attachment](https://example.com/result.png)"
        );
        assert!(append_images("result".to_owned(), &["result.png".to_owned()]).is_err());
    }
}
