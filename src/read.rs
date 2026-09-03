use std::num::NonZeroU64;
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::model::Repository;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestView {
    pub number: u64,
    pub state: String,
    pub is_draft: bool,
    pub mergeable: String,
    pub merge_state_status: String,
    pub title: String,
    pub head_ref: String,
    pub base_ref: String,
    pub url: String,
    pub checks: Vec<CheckRun>,
    pub unresolved_thread_count: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckRun {
    pub name: String,
    pub state: String,
    pub link: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestChecks {
    pub number: u64,
    pub state: String,
    pub checks: Vec<CheckRun>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunSummary {
    pub database_id: u64,
    pub display_title: String,
    pub workflow_name: String,
    pub event: String,
    pub head_branch: String,
    pub status: String,
    pub conclusion: String,
    pub url: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewThread {
    pub id: String,
    pub path: String,
    pub line: Option<u64>,
    pub side: String,
    pub is_resolved: bool,
    pub is_outdated: bool,
    pub first_comment_author: Option<String>,
    pub first_comment_body: Option<String>,
}

pub fn pull_request_view(repository: &Repository, number: NonZeroU64) -> Result<PullRequestView> {
    let slug = repository.slug();
    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            &number.to_string(),
            "--repo",
            &slug,
            "--json",
            "number,state,isDraft,mergeable,mergeStateStatus,title,headRefName,baseRefName,url,statusCheckRollup",
        ])
        .output()
        .context("failed to launch gh; install and authenticate GitHub CLI")?;
    if !output.status.success() {
        bail!(
            "failed to inspect pull request: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let value =
        serde_json::from_slice::<Value>(&output.stdout).context("failed to decode pull request")?;
    let checks = decode_checks(value.get("statusCheckRollup"));
    let unresolved_thread_count = unresolved_threads(repository, number)?;
    Ok(PullRequestView {
        number: value
            .get("number")
            .and_then(Value::as_u64)
            .unwrap_or(number.get()),
        state: string_field(&value, "state"),
        is_draft: value
            .get("isDraft")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        mergeable: string_field(&value, "mergeable"),
        merge_state_status: string_field(&value, "mergeStateStatus"),
        title: string_field(&value, "title"),
        head_ref: string_field(&value, "headRefName"),
        base_ref: string_field(&value, "baseRefName"),
        url: string_field(&value, "url"),
        checks,
        unresolved_thread_count,
    })
}

pub fn watch_pull_request_checks(
    repository: &Repository,
    number: NonZeroU64,
    interval: Duration,
) -> Result<PullRequestChecks> {
    loop {
        let checks = pull_request_checks(repository, number)?;
        if checks.state != "PENDING" {
            return Ok(checks);
        }
        thread::sleep(interval);
    }
}

pub fn pull_request_checks(
    repository: &Repository,
    number: NonZeroU64,
) -> Result<PullRequestChecks> {
    let slug = repository.slug();
    let output = Command::new("gh")
        .args([
            "pr",
            "checks",
            &number.to_string(),
            "--repo",
            &slug,
            "--json",
            "name,state,link",
        ])
        .output()
        .context("failed to launch gh; install and authenticate GitHub CLI")?;
    if !output.status.success() {
        bail!(
            "failed to inspect pull request checks: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let checks = serde_json::from_slice::<Vec<CheckRun>>(&output.stdout)
        .context("failed to decode pull request checks")?;
    let state = if checks.iter().any(|check| check.state == "PENDING") {
        "PENDING".to_owned()
    } else if checks.iter().any(|check| check.state == "FAIL") {
        "FAIL".to_owned()
    } else {
        "PASS".to_owned()
    };
    Ok(PullRequestChecks {
        number: number.get(),
        state,
        checks,
    })
}

pub fn workflow_runs(
    repository: &Repository,
    workflow: Option<&str>,
    branch: Option<&str>,
    limit: u64,
) -> Result<Vec<WorkflowRunSummary>> {
    let slug = repository.slug();
    let mut command = Command::new("gh");
    command.args([
        "run",
        "list",
        "--repo",
        &slug,
        "--limit",
        &limit.to_string(),
        "--json",
        "databaseId,displayTitle,workflowName,event,headBranch,status,conclusion,url,createdAt",
    ]);
    if let Some(workflow) = workflow {
        command.args(["--workflow", workflow]);
    }
    if let Some(branch) = branch {
        command.args(["--branch", branch]);
    }
    let output = command
        .output()
        .context("failed to launch gh; install and authenticate GitHub CLI")?;
    if !output.status.success() {
        bail!(
            "failed to list workflow runs: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    serde_json::from_slice(&output.stdout).context("failed to decode workflow runs")
}

pub fn pull_request_threads(
    repository: &Repository,
    number: NonZeroU64,
) -> Result<Vec<ReviewThread>> {
    let slug = repository.slug();
    let query = r"
query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      reviewThreads(first: 100) {
        nodes {
          id
          isResolved
          isOutdated
          path
          line
          diffSide
          comments(first: 1) {
            nodes {
              author { login }
              body
            }
          }
        }
      }
    }
  }
}";
    let variables = json!({
        "owner": repository.owner,
        "name": repository.name,
        "number": number.get(),
    });
    let value = graphql(&slug, query, &variables)?;
    let nodes = value
        .pointer("/data/repository/pullRequest/reviewThreads/nodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(nodes
        .into_iter()
        .filter_map(|node| {
            let comments = node
                .pointer("/comments/nodes")
                .and_then(Value::as_array)
                .and_then(|comments| comments.first())?;
            Some(ReviewThread {
                id: string_field(&node, "id"),
                path: string_field(&node, "path"),
                line: node.get("line").and_then(Value::as_u64),
                side: string_field(&node, "diffSide"),
                is_resolved: node
                    .get("isResolved")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                is_outdated: node
                    .get("isOutdated")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                first_comment_author: comments
                    .pointer("/author/login")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                first_comment_body: comments
                    .get("body")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
        })
        .collect())
}

pub fn resolve_review_thread(
    repository: &Repository,
    thread_id: &str,
    resolve: bool,
) -> Result<()> {
    let slug = repository.slug();
    let mutation = if resolve {
        "resolveReviewThread"
    } else {
        "unresolveReviewThread"
    };
    let query = format!(
        r"
mutation($threadId: ID!) {{
  {mutation}(input: {{threadId: $threadId}}) {{
    thread {{ id isResolved }}
  }}
}}",
    );
    graphql(&slug, &query, &json!({ "threadId": thread_id }))?;
    Ok(())
}

pub fn reply_to_review_comment(
    repository: &Repository,
    comment_id: NonZeroU64,
    body: &str,
) -> Result<String> {
    let slug = repository.slug();
    let path = format!("repos/{slug}/pulls/comments/{comment_id}/replies");
    let mut child = Command::new("gh")
        .args(["api", "--method", "POST", &path, "--input", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("failed to launch gh; install and authenticate GitHub CLI")?;
    let stdin = child.stdin.take().context("failed to open gh input")?;
    serde_json::to_writer(stdin, &json!({ "body": body }))
        .context("failed to encode the reply body")?;
    let output = child.wait_with_output().context("failed to wait for gh")?;
    if !output.status.success() {
        bail!(
            "GitHub rejected the review comment reply: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let value =
        serde_json::from_slice::<Value>(&output.stdout).context("failed to decode reply")?;
    Ok(value
        .get("html_url")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned())
}

fn unresolved_threads(repository: &Repository, number: NonZeroU64) -> Result<u64> {
    Ok(pull_request_threads(repository, number)?
        .into_iter()
        .filter(|thread| !thread.is_resolved)
        .count() as u64)
}

fn graphql(_slug: &str, query: &str, variables: &Value) -> Result<Value> {
    let request = json!({ "query": query, "variables": variables });
    let mut child = Command::new("gh")
        .args(["api", "graphql", "--input", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("failed to launch gh; install and authenticate GitHub CLI")?;
    let stdin = child.stdin.take().context("failed to open gh input")?;
    serde_json::to_writer(stdin, &request).context("failed to encode GraphQL request")?;
    let output = child.wait_with_output().context("failed to wait for gh")?;
    if !output.status.success() {
        bail!(
            "GraphQL request failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let value = serde_json::from_slice::<Value>(&output.stdout)
        .context("failed to decode GraphQL response")?;
    if let Some(errors) = value.get("errors") {
        bail!("GraphQL request failed: {errors}");
    }
    Ok(value)
}

fn decode_checks(value: Option<&Value>) -> Vec<CheckRun> {
    value
        .and_then(Value::as_array)
        .map(|checks| {
            checks
                .iter()
                .map(|check| CheckRun {
                    name: string_field(check, "name"),
                    state: string_field(check, "state").to_ascii_uppercase(),
                    link: check
                        .get("detailsUrl")
                        .or_else(|| check.get("link"))
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}
