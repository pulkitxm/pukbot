use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::model::Operation;

const WORKFLOW_REPOSITORY: &str = "pulkitxm/pukbot";
const WORKFLOW_FILE: &str = "operation.yml";
const WORKFLOW_REF: &str = "main";
const RUN_LOOKUP_ATTEMPTS: usize = 30;

#[derive(Debug, Serialize)]
struct WorkflowInputs<'a> {
    payload: &'a str,
    requester: &'a str,
    request_id: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowRun {
    database_id: u64,
    display_title: String,
    conclusion: String,
    url: String,
}

pub struct WorkflowResult {
    pub workflow_url: String,
    pub resource_url: Option<String>,
}

pub fn dispatch(operation: &Operation, stream: bool) -> Result<WorkflowResult> {
    let requester = github_login()?;
    let request_id = request_id()?;
    let payload = serde_json::to_string(operation).context("failed to encode operation")?;
    let inputs = WorkflowInputs {
        payload: &payload,
        requester: &requester,
        request_id: &request_id,
    };
    let mut dispatch = Command::new("gh");
    dispatch.args([
        "workflow",
        "run",
        WORKFLOW_FILE,
        "--repo",
        WORKFLOW_REPOSITORY,
        "--ref",
        WORKFLOW_REF,
        "--json",
    ]);
    if !stream {
        dispatch.stdout(Stdio::null()).stderr(Stdio::null());
    }
    let mut child = dispatch
        .stdin(Stdio::piped())
        .spawn()
        .context("failed to launch gh; install and authenticate GitHub CLI")?;
    let stdin = child.stdin.take().context("failed to open gh input")?;
    serde_json::to_writer(stdin, &inputs).context("failed to encode workflow input")?;
    let status = child.wait().context("failed to wait for gh")?;
    if !status.success() {
        bail!("GitHub rejected the Pukbot workflow dispatch");
    }
    let run = find_run(&request_id)?;
    let mut watch = Command::new("gh");
    watch.args([
        "run",
        "watch",
        &run.database_id.to_string(),
        "--repo",
        WORKFLOW_REPOSITORY,
        "--exit-status",
    ]);
    if !stream {
        watch.stdout(Stdio::null()).stderr(Stdio::null());
    }
    let status = watch
        .status()
        .context("failed to watch the Pukbot workflow run")?;
    let completed = inspect_run(run.database_id)?;
    if !status.success() || completed.conclusion != "success" {
        show_failed_logs(completed.database_id);
        bail!("Pukbot workflow failed: {}", completed.url);
    }
    Ok(WorkflowResult {
        workflow_url: completed.url,
        resource_url: find_result_url(completed.database_id)?,
    })
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
    if login.is_empty()
        || !login
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        bail!("GitHub CLI returned an invalid login");
    }
    Ok(login)
}

fn request_id() -> Result<String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?;
    Ok(format!("{}-{}", elapsed.as_nanos(), std::process::id()))
}

fn find_run(request_id: &str) -> Result<WorkflowRun> {
    let expected_title = format!("Pukbot operation {request_id}");
    for _ in 0..RUN_LOOKUP_ATTEMPTS {
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
                "databaseId,displayTitle,conclusion,url",
            ])
            .output()
            .context("failed to list Pukbot workflow runs")?;
        if !output.status.success() {
            bail!("failed to list Pukbot workflow runs");
        }
        let runs = serde_json::from_slice::<Vec<WorkflowRun>>(&output.stdout)
            .context("failed to decode Pukbot workflow runs")?;
        if let Some(run) = runs
            .into_iter()
            .find(|run| run.display_title == expected_title)
        {
            return Ok(run);
        }
        thread::sleep(Duration::from_secs(1));
    }
    bail!("could not find the dispatched Pukbot workflow run")
}

fn inspect_run(run_id: u64) -> Result<WorkflowRun> {
    let output = Command::new("gh")
        .args([
            "run",
            "view",
            &run_id.to_string(),
            "--repo",
            WORKFLOW_REPOSITORY,
            "--json",
            "databaseId,displayTitle,conclusion,url",
        ])
        .output()
        .context("failed to inspect the Pukbot workflow run")?;
    if !output.status.success() {
        bail!("failed to inspect the Pukbot workflow run");
    }
    serde_json::from_slice(&output.stdout).context("failed to decode the Pukbot workflow run")
}

fn show_failed_logs(run_id: u64) {
    drop(
        Command::new("gh")
            .args([
                "run",
                "view",
                &run_id.to_string(),
                "--repo",
                WORKFLOW_REPOSITORY,
                "--log-failed",
            ])
            .status(),
    );
}

fn find_result_url(run_id: u64) -> Result<Option<String>> {
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
        .context("failed to read Pukbot workflow logs")?;
    if !output.status.success() {
        return Ok(None);
    }
    let logs = String::from_utf8(output.stdout).context("workflow logs were not UTF-8")?;
    Ok(parse_result_url(&logs))
}

fn parse_result_url(logs: &str) -> Option<String> {
    logs.lines().find_map(|line| {
        line.split_once("pukbot-result-url=")
            .map(|(_, url)| url.trim())
            .filter(|url| url.starts_with("https://github.com/"))
            .map(str::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use super::parse_result_url;

    #[test]
    fn parses_result_url() {
        let logs = "step pukbot-result-url=https://github.com/owner/repo/issues/1";
        assert_eq!(
            parse_result_url(logs),
            Some("https://github.com/owner/repo/issues/1".to_owned())
        );
    }
}
