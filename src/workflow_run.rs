use std::io::{self, Write};
use std::num::NonZeroU64;
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::model::Repository;

const RUN_FIELDS: &str = "attempt,conclusion,createdAt,databaseId,displayTitle,event,headBranch,headSha,jobs,number,startedAt,status,updatedAt,url,workflowName";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRun {
    pub attempt: u64,
    pub conclusion: String,
    pub created_at: String,
    pub database_id: u64,
    pub display_title: String,
    pub event: String,
    pub head_branch: String,
    pub head_sha: String,
    pub jobs: Vec<WorkflowJob>,
    pub number: u64,
    pub started_at: String,
    pub status: String,
    pub updated_at: String,
    pub url: String,
    pub workflow_name: String,
    pub actor: String,
    pub triggering_actor: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowJob {
    pub completed_at: String,
    pub conclusion: String,
    pub database_id: u64,
    pub name: String,
    pub started_at: String,
    pub status: String,
    pub steps: Vec<WorkflowStep>,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStep {
    pub completed_at: String,
    pub conclusion: String,
    pub name: String,
    pub number: u64,
    pub started_at: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
struct RunActors {
    actor: Actor,
    triggering_actor: Actor,
}

#[derive(Debug, Deserialize)]
struct Actor {
    login: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowLogs<'a> {
    repository: &'a str,
    run_id: u64,
    failed_only: bool,
    logs: String,
}

#[derive(Clone, Copy)]
pub enum WatchOutput {
    Text,
    Json,
    Quiet,
}

pub fn inspect(repository: &Repository, run_id: NonZeroU64) -> Result<WorkflowRun> {
    let output = Command::new("gh")
        .args([
            "run",
            "view",
            &run_id.to_string(),
            "--repo",
            &repository.slug(),
            "--json",
            RUN_FIELDS,
        ])
        .output()
        .context("failed to launch gh; install and authenticate GitHub CLI")?;
    if !output.status.success() {
        return command_error("failed to inspect workflow run", &output.stderr);
    }
    let mut run = serde_json::from_slice::<WorkflowRunData>(&output.stdout)
        .context("failed to decode workflow run")?
        .into_run();
    let actors = inspect_actors(repository, run_id)?;
    run.actor = actors.actor.login;
    run.triggering_actor = actors.triggering_actor.login;
    Ok(run)
}

pub fn emit_status(run: &WorkflowRun, json: bool) -> Result<()> {
    if json {
        let mut stdout = io::stdout().lock();
        serde_json::to_writer_pretty(&mut stdout, run)?;
        writeln!(stdout)?;
        return Ok(());
    }
    let mut stdout = io::stdout().lock();
    emit_summary(&mut stdout, run)
}

pub fn watch(
    repository: &Repository,
    run_id: NonZeroU64,
    interval: Duration,
    output: WatchOutput,
) -> Result<WorkflowRun> {
    let mut previous = None;
    loop {
        let run = inspect(repository, run_id)?;
        if run.status == "completed" {
            if matches!(output, WatchOutput::Json) {
                emit_status(&run, true)?;
            } else if matches!(output, WatchOutput::Text) {
                emit_summary(&mut io::stdout().lock(), &run)?;
            }
            if run.conclusion != "success" {
                let failed_logs = read_logs(repository, run_id, true)?;
                if !failed_logs.is_empty() {
                    writeln!(io::stderr().lock(), "{failed_logs}")?;
                }
                bail!("workflow concluded {}: {}", run.conclusion, run.url);
            }
            return Ok(run);
        }
        if matches!(output, WatchOutput::Text) {
            emit_changes(&mut io::stdout().lock(), previous.as_ref(), &run)?;
        }
        previous = Some(run);
        thread::sleep(interval);
    }
}

pub fn emit_logs(
    repository: &Repository,
    run_id: NonZeroU64,
    failed_only: bool,
    json: bool,
) -> Result<()> {
    let logs = read_logs(repository, run_id, failed_only)?;
    let mut stdout = io::stdout().lock();
    if json {
        let slug = repository.slug();
        let output = WorkflowLogs {
            repository: &slug,
            run_id: run_id.get(),
            failed_only,
            logs,
        };
        serde_json::to_writer_pretty(&mut stdout, &output)?;
        writeln!(stdout)?;
    } else {
        write!(stdout, "{logs}")?;
        if !logs.ends_with('\n') {
            writeln!(stdout)?;
        }
    }
    Ok(())
}

pub fn run_id_from_url(repository: &Repository, url: &str) -> Result<NonZeroU64> {
    let prefix = format!("https://github.com/{}/actions/runs/", repository.slug());
    let value = url
        .strip_prefix(&prefix)
        .and_then(|remainder| remainder.split('/').next())
        .context("operation did not return a workflow run URL")?;
    value
        .parse::<NonZeroU64>()
        .context("operation returned an invalid workflow run URL")
}

fn inspect_actors(repository: &Repository, run_id: NonZeroU64) -> Result<RunActors> {
    let endpoint = format!("repos/{}/actions/runs/{run_id}", repository.slug());
    let output = Command::new("gh")
        .args(["api", &endpoint])
        .output()
        .context("failed to launch gh; install and authenticate GitHub CLI")?;
    if !output.status.success() {
        return command_error("failed to inspect workflow actors", &output.stderr);
    }
    serde_json::from_slice(&output.stdout).context("failed to decode workflow actors")
}

fn read_logs(repository: &Repository, run_id: NonZeroU64, failed_only: bool) -> Result<String> {
    let mut command = Command::new("gh");
    command.args([
        "run",
        "view",
        &run_id.to_string(),
        "--repo",
        &repository.slug(),
    ]);
    if failed_only {
        command.arg("--log-failed");
    } else {
        command.arg("--log");
    }
    let output = command
        .output()
        .context("failed to launch gh; install and authenticate GitHub CLI")?;
    if !output.status.success() {
        return command_error("failed to read workflow logs", &output.stderr);
    }
    String::from_utf8(output.stdout).context("workflow logs were not UTF-8")
}

fn emit_changes(
    writer: &mut impl Write,
    previous: Option<&WorkflowRun>,
    current: &WorkflowRun,
) -> Result<()> {
    if previous.is_none_or(|run| run.status != current.status) {
        writeln!(
            writer,
            "workflow: {}",
            state(&current.status, &current.conclusion)
        )?;
    }
    for job in &current.jobs {
        let changed = previous
            .and_then(|run| {
                run.jobs
                    .iter()
                    .find(|previous_job| previous_job.database_id == job.database_id)
            })
            .is_none_or(|previous_job| {
                previous_job.status != job.status || previous_job.conclusion != job.conclusion
            });
        if changed {
            writeln!(
                writer,
                "job {}: {}",
                job.name,
                state(&job.status, &job.conclusion)
            )?;
        }
    }
    writer.flush()?;
    Ok(())
}

fn emit_summary(writer: &mut impl Write, run: &WorkflowRun) -> Result<()> {
    writeln!(
        writer,
        "{} #{}: {}",
        run.workflow_name,
        run.number,
        state(&run.status, &run.conclusion)
    )?;
    writeln!(writer, "Actor: {}", run.actor)?;
    writeln!(writer, "Triggered by: {}", run.triggering_actor)?;
    for job in &run.jobs {
        writeln!(
            writer,
            "{}: {} ({})",
            job.name,
            state(&job.status, &job.conclusion),
            job.url
        )?;
    }
    writeln!(writer, "Run: {}", run.url)?;
    Ok(())
}

fn state<'a>(status: &'a str, conclusion: &'a str) -> &'a str {
    if status == "completed" && !conclusion.is_empty() {
        conclusion
    } else {
        status
    }
}

fn command_error<T>(context: &str, stderr: &[u8]) -> Result<T> {
    let detail = String::from_utf8_lossy(stderr);
    let detail = detail.trim();
    if detail.is_empty() {
        bail!("{context}")
    }
    bail!("{context}: {detail}")
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowRunData {
    attempt: u64,
    conclusion: String,
    created_at: String,
    database_id: u64,
    display_title: String,
    event: String,
    head_branch: String,
    head_sha: String,
    jobs: Vec<WorkflowJob>,
    number: u64,
    started_at: String,
    status: String,
    updated_at: String,
    url: String,
    workflow_name: String,
}

impl WorkflowRunData {
    fn into_run(self) -> WorkflowRun {
        WorkflowRun {
            attempt: self.attempt,
            conclusion: self.conclusion,
            created_at: self.created_at,
            database_id: self.database_id,
            display_title: self.display_title,
            event: self.event,
            head_branch: self.head_branch,
            head_sha: self.head_sha,
            jobs: self.jobs,
            number: self.number,
            started_at: self.started_at,
            status: self.status,
            updated_at: self.updated_at,
            url: self.url,
            workflow_name: self.workflow_name,
            actor: String::new(),
            triggering_actor: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use super::{WorkflowRunData, run_id_from_url, state};
    use crate::model::Repository;

    #[test]
    fn decodes_workflow_run_contract() {
        let data = serde_json::from_value::<WorkflowRunData>(serde_json::json!({
            "attempt": 1,
            "conclusion": "success",
            "createdAt": "2026-08-27T00:00:00Z",
            "databaseId": 42,
            "displayTitle": "CI",
            "event": "workflow_dispatch",
            "headBranch": "main",
            "headSha": "abc",
            "jobs": [{
                "completedAt": "2026-08-27T00:01:00Z",
                "conclusion": "success",
                "databaseId": 84,
                "name": "Test",
                "startedAt": "2026-08-27T00:00:01Z",
                "status": "completed",
                "steps": [],
                "url": "https://github.com/owner/repo/actions/runs/42/job/84"
            }],
            "number": 7,
            "startedAt": "2026-08-27T00:00:00Z",
            "status": "completed",
            "updatedAt": "2026-08-27T00:01:00Z",
            "url": "https://github.com/owner/repo/actions/runs/42",
            "workflowName": "CI"
        }))
        .expect("workflow run should decode");
        let run = data.into_run();
        assert_eq!(run.database_id, 42);
        assert_eq!(run.jobs[0].name, "Test");
    }

    #[test]
    fn extracts_run_id_from_result_url() {
        let repository = "owner/repo"
            .parse::<Repository>()
            .expect("repository should parse");
        assert_eq!(
            run_id_from_url(&repository, "https://github.com/owner/repo/actions/runs/42")
                .expect("run ID should parse"),
            NonZeroU64::new(42).expect("run ID should be nonzero")
        );
    }

    #[test]
    fn rejects_result_url_for_another_repository() {
        let repository = "owner/repo"
            .parse::<Repository>()
            .expect("repository should parse");
        assert!(
            run_id_from_url(&repository, "https://github.com/other/repo/actions/runs/42").is_err()
        );
    }

    #[test]
    fn prefers_completed_conclusion() {
        assert_eq!(state("completed", "success"), "success");
        assert_eq!(state("in_progress", ""), "in_progress");
    }
}
