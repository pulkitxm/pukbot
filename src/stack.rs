use std::ffi::OsString;
use std::io::{self, Write};
use std::num::NonZeroU64;
use std::process::{Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::process::CommandExt as _;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::model::{Operation, Repository};

const MERGE_POLL_ATTEMPTS: usize = 600;

pub fn run_extension(arguments: Vec<OsString>) -> Result<ExitStatus> {
    let mut command = Command::new("gh");
    command.arg("stack").args(arguments);
    #[cfg(unix)]
    {
        let error = command.exec();
        Err(error).context(
            "failed to launch gh-stack; install GitHub CLI and run `gh extension install github/gh-stack`",
        )
    }
    #[cfg(not(unix))]
    {
        command.status().context(
            "failed to launch gh-stack; install GitHub CLI and run `gh extension install github/gh-stack`",
        )
    }
}

pub fn extension_exit_code(status: ExitStatus) -> i32 {
    status.code().unwrap_or(1)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct RemoteStack {
    id: u64,
    number: NonZeroU64,
    node_id: String,
    url: String,
    base: StackBase,
    open: bool,
    created_at: String,
    pull_requests: Vec<StackPullRequest>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StackBase {
    #[serde(rename = "ref")]
    reference: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct StackPullRequest {
    number: NonZeroU64,
    state: String,
    draft: bool,
    merged_at: Option<String>,
    head: StackPullRequestHead,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StackPullRequestHead {
    #[serde(rename = "ref")]
    reference: String,
    sha: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AsyncMergeResult {
    status: String,
    details: AsyncMergeDetails,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct AsyncMergeDetails {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    uuid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    merge_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    merge_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expected_head_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sha: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PullRequestResource {
    #[serde(default)]
    stack: Option<Value>,
    head: PullRequestHeadResource,
}

#[derive(Debug, Deserialize)]
struct PullRequestHeadResource {
    sha: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalStackView {
    branches: Vec<LocalStackBranch>,
}

#[derive(Debug, Deserialize)]
struct LocalStackBranch {
    pr: Option<LocalStackPullRequest>,
}

#[derive(Debug, Deserialize)]
struct LocalStackPullRequest {
    number: NonZeroU64,
}

pub fn list(repository: &Repository, pull_request: Option<NonZeroU64>) -> Result<Vec<RemoteStack>> {
    list_parts(&repository.owner, &repository.name, pull_request)
}

pub fn get(repository: &Repository, stack_number: NonZeroU64) -> Result<RemoteStack> {
    get_parts(&repository.owner, &repository.name, stack_number)
}

pub fn merge_status(
    repository: &Repository,
    pull_request: NonZeroU64,
    uuid: &str,
) -> Result<AsyncMergeResult> {
    merge_status_parts(&repository.owner, &repository.name, pull_request, uuid)
}

pub fn current_repository() -> Result<Repository> {
    let output = Command::new("gh")
        .args([
            "repo",
            "view",
            "--json",
            "nameWithOwner",
            "--jq",
            ".nameWithOwner",
        ])
        .output()
        .context("failed to launch gh; install and authenticate GitHub CLI")?;
    ensure_command_success("resolve the current repository", &output)?;
    let slug = String::from_utf8(output.stdout)
        .context("GitHub CLI returned a non-UTF-8 repository name")?;
    slug.trim().parse().map_err(anyhow::Error::msg)
}

pub fn resolve_compatibility_merge_target(
    repository: &Repository,
    target: Option<NonZeroU64>,
) -> Result<NonZeroU64> {
    if let Some(target) = target {
        let stacks = list(repository, None)?;
        if let Some(stack) = stacks.iter().find(|stack| stack.number == target) {
            return top_pull_request(stack);
        }
        return Ok(target);
    }
    let output = Command::new("gh")
        .args(["stack", "view", "--json"])
        .output()
        .context("failed to launch gh-stack; run `gh extension install github/gh-stack`")?;
    ensure_command_success("read the current stack", &output)?;
    let view = serde_json::from_slice::<LocalStackView>(&output.stdout)
        .context("failed to decode the current gh-stack state")?;
    view.branches
        .into_iter()
        .rev()
        .find_map(|branch| branch.pr.map(|pull_request| pull_request.number))
        .context("the current stack has no pull requests to merge")
}

pub fn emit_list(stacks: &[RemoteStack]) -> Result<()> {
    let mut stdout = io::stdout().lock();
    if stacks.is_empty() {
        writeln!(stdout, "No stacks found")?;
        return Ok(());
    }
    for stack in stacks {
        let state = if stack.open { "open" } else { "closed" };
        writeln!(
            stdout,
            "Stack #{}: {} pull requests, {state}, base {}",
            stack.number,
            stack.pull_requests.len(),
            stack.base.reference
        )?;
    }
    Ok(())
}

pub fn emit_stack(stack: &RemoteStack) -> Result<()> {
    let mut stdout = io::stdout().lock();
    let state = if stack.open { "open" } else { "closed" };
    writeln!(stdout, "Stack #{}", stack.number)?;
    writeln!(stdout, "Base: {}", stack.base.reference)?;
    writeln!(stdout, "State: {state}")?;
    writeln!(stdout, "Created: {}", stack.created_at)?;
    writeln!(stdout, "Pull requests:")?;
    for pull_request in &stack.pull_requests {
        let state = if pull_request.merged_at.is_some() {
            "merged"
        } else if pull_request.draft {
            "draft"
        } else {
            &pull_request.state
        };
        writeln!(
            stdout,
            "  #{} {state} {}",
            pull_request.number, pull_request.head.reference
        )?;
    }
    Ok(())
}

pub fn emit_merge_status(result: &AsyncMergeResult) -> Result<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "Merge status: {}", result.status)?;
    if !result.details.message.is_empty() {
        writeln!(stdout, "Message: {}", result.details.message)?;
    }
    if let Some(uuid) = &result.details.uuid {
        writeln!(stdout, "Request: {uuid}")?;
    }
    if let Some(sha) = &result.details.sha {
        writeln!(stdout, "Commit: {sha}")?;
    }
    Ok(())
}

pub fn execute(operation: &Operation) -> Result<String> {
    match operation {
        Operation::StackCreate {
            owner,
            repository,
            pull_requests,
        } => {
            let path = format!("repos/{owner}/{repository}/stacks");
            let remote = api_json(
                "POST",
                &path,
                Some(&json!({"pull_requests": pull_requests})),
            )?;
            Ok(stack_result_url(owner, repository, &remote))
        }
        Operation::StackAppend {
            owner,
            repository,
            stack_number,
            pull_requests,
        } => {
            let path = format!("repos/{owner}/{repository}/stacks/{stack_number}/add");
            let remote = api_json(
                "POST",
                &path,
                Some(&json!({"pull_requests": pull_requests})),
            )?;
            Ok(stack_result_url(owner, repository, &remote))
        }
        Operation::StackUnstack {
            owner,
            repository,
            stack_number,
        } => {
            let path = format!("repos/{owner}/{repository}/stacks/{stack_number}/unstack");
            let output = run_api("POST", &path, None)?;
            ensure_success("POST", &path, &output)?;
            if output.stdout.iter().all(u8::is_ascii_whitespace) {
                return Ok(format!("https://github.com/{owner}/{repository}/pulls"));
            }
            let remote = serde_json::from_slice::<RemoteStack>(&output.stdout)
                .context("failed to decode the remaining GitHub stack")?;
            Ok(stack_result_url(owner, repository, &remote))
        }
        Operation::StackMerge {
            owner,
            repository,
            pull_request,
            stack_number,
        } => {
            let pull_request = if let Some(pull_request) = pull_request {
                *pull_request
            } else {
                let stack_number = stack_number.context("stack number is required")?;
                let remote = get_parts(owner, repository, stack_number)?;
                top_pull_request(&remote)?
            };
            merge_pull_request(owner, repository, pull_request)
        }
        _ => bail!("operation {} is not a stack operation", operation.name()),
    }
}

pub fn stacked_head_sha(
    owner: &str,
    repository: &str,
    pull_request: NonZeroU64,
) -> Result<Option<String>> {
    let path = format!("repos/{owner}/{repository}/pulls/{pull_request}");
    let resource = api_json::<PullRequestResource>("GET", &path, None)?;
    if resource.stack.is_none() {
        return Ok(None);
    }
    validate_head_sha(&resource.head.sha)?;
    Ok(Some(resource.head.sha))
}

pub fn merge_pull_request(
    owner: &str,
    repository: &str,
    pull_request: NonZeroU64,
) -> Result<String> {
    let head_sha = stacked_head_sha(owner, repository, pull_request)?
        .with_context(|| format!("pull request #{pull_request} is not part of a stack"))?;
    merge_known_stack_pull_request(owner, repository, pull_request, &head_sha)
}

pub fn merge_known_stack_pull_request(
    owner: &str,
    repository: &str,
    pull_request: NonZeroU64,
    head_sha: &str,
) -> Result<String> {
    validate_head_sha(head_sha)?;
    let path = format!("repos/{owner}/{repository}/pulls/{pull_request}/merge-async");
    let mut result = merge_request(
        "PUT",
        &path,
        Some(&json!({
            "merge_method": "squash",
            "merge_action": "direct_merge",
            "sha": head_sha
        })),
    )?;
    for attempt in 0..=MERGE_POLL_ATTEMPTS {
        match result.status.as_str() {
            "merged" => {
                validate_merged_result(&result)?;
                return Ok(format!(
                    "https://github.com/{owner}/{repository}/pull/{pull_request}"
                ));
            }
            "enqueued" => bail!("stack merge was enqueued instead of squash-merged"),
            "failed" => {
                let message = result.details.message.trim();
                if message.is_empty() {
                    bail!("stack merge failed");
                }
                bail!("stack merge failed: {message}");
            }
            "pending" => {
                if attempt == MERGE_POLL_ATTEMPTS {
                    bail!("stack merge is still pending after 10 minutes");
                }
                let uuid = validate_pending_result(&result, head_sha)?;
                thread::sleep(Duration::from_secs(1));
                result = merge_status_parts(owner, repository, pull_request, uuid)?;
            }
            status => bail!("GitHub returned an unknown stack merge status: {status}"),
        }
    }
    unreachable!()
}

fn list_parts(
    owner: &str,
    repository: &str,
    pull_request: Option<NonZeroU64>,
) -> Result<Vec<RemoteStack>> {
    let query = pull_request.map_or_else(
        || "?per_page=100".to_owned(),
        |number| format!("?pull_request={number}&per_page=100"),
    );
    let path = format!("repos/{owner}/{repository}/stacks{query}");
    let output = Command::new("gh")
        .args(["api", "--paginate", "--slurp", &path])
        .output()
        .context("failed to launch gh; install and authenticate GitHub CLI")?;
    ensure_success("GET", &path, &output)?;
    let pages = serde_json::from_slice::<Vec<Vec<RemoteStack>>>(&output.stdout)
        .context("failed to decode GitHub stacks")?;
    Ok(pages.into_iter().flatten().collect())
}

fn get_parts(owner: &str, repository: &str, stack_number: NonZeroU64) -> Result<RemoteStack> {
    let path = format!("repos/{owner}/{repository}/stacks/{stack_number}");
    api_json("GET", &path, None)
}

fn merge_status_parts(
    owner: &str,
    repository: &str,
    pull_request: NonZeroU64,
    uuid: &str,
) -> Result<AsyncMergeResult> {
    validate_uuid(uuid)?;
    let path = format!("repos/{owner}/{repository}/pulls/{pull_request}/merge-async/{uuid}");
    api_json("GET", &path, None)
}

fn api_json<T>(method: &str, path: &str, request: Option<&Value>) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let output = run_api(method, path, request)?;
    ensure_success(method, path, &output)?;
    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("failed to decode GitHub response for {method} {path}"))
}

fn merge_request(method: &str, path: &str, request: Option<&Value>) -> Result<AsyncMergeResult> {
    let output = run_api(method, path, request)?;
    if output.status.success() {
        return serde_json::from_slice(&output.stdout)
            .with_context(|| format!("failed to decode GitHub response for {method} {path}"));
    }
    if let Ok(result) = serde_json::from_slice::<AsyncMergeResult>(&output.stdout) {
        if result.status == "failed" {
            return Ok(result);
        }
    }
    ensure_success(method, path, &output)?;
    unreachable!()
}

fn run_api(method: &str, path: &str, request: Option<&Value>) -> Result<Output> {
    let mut command = Command::new("gh");
    command.args(["api", "--method", method, path]);
    if request.is_some() {
        command.args(["--input", "-"]);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to launch gh; install and authenticate GitHub CLI")?;
    if let Some(request) = request {
        let stdin = child.stdin.take().context("failed to open gh input")?;
        serde_json::to_writer(stdin, request).context("failed to encode the GitHub request")?;
    } else {
        drop(child.stdin.take());
    }
    child.wait_with_output().context("failed to wait for gh")
}

fn ensure_success(method: &str, path: &str, output: &Output) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let message = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    bail!("GitHub rejected {method} {path}: {message}")
}

fn ensure_command_success(action: &str, output: &Output) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let message = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    bail!("failed to {action}: {message}")
}

fn top_pull_request(stack: &RemoteStack) -> Result<NonZeroU64> {
    stack
        .pull_requests
        .last()
        .map(|pull_request| pull_request.number)
        .context("stack has no pull requests")
}

fn stack_result_url(owner: &str, repository: &str, stack: &RemoteStack) -> String {
    top_pull_request(stack).map_or_else(
        |_| format!("https://github.com/{owner}/{repository}/pulls"),
        |number| format!("https://github.com/{owner}/{repository}/pull/{number}"),
    )
}

fn validate_uuid(uuid: &str) -> Result<()> {
    if uuid.is_empty()
        || uuid.len() > 128
        || !uuid
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        bail!("stack merge UUID is invalid");
    }
    Ok(())
}

fn validate_head_sha(sha: &str) -> Result<()> {
    if sha.is_empty()
        || sha.len() > 128
        || !sha.chars().all(|character| character.is_ascii_hexdigit())
    {
        bail!("pull request head SHA is invalid");
    }
    Ok(())
}

fn validate_pending_result<'a>(result: &'a AsyncMergeResult, head_sha: &str) -> Result<&'a str> {
    if result.details.merge_method.as_deref() != Some("squash") {
        bail!("pending stack merge is not using squash");
    }
    if result.details.merge_action.as_deref() != Some("direct_merge") {
        bail!("pending stack merge is not using a direct merge");
    }
    if result.details.expected_head_sha.as_deref() != Some(head_sha) {
        bail!("pending stack merge targets a different pull request head");
    }
    let uuid = result
        .details
        .uuid
        .as_deref()
        .context("pending stack merge response omitted its UUID")?;
    validate_uuid(uuid)?;
    Ok(uuid)
}

fn validate_merged_result(result: &AsyncMergeResult) -> Result<()> {
    let sha = result
        .details
        .sha
        .as_deref()
        .context("merged stack response omitted its commit SHA")?;
    validate_head_sha(sha)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{AsyncMergeResult, RemoteStack, top_pull_request, validate_uuid};

    const STACK: &str = r#"{
        "id": 987,
        "number": 42,
        "node_id": "PRS_123",
        "url": "https://api.github.com/repos/owner/repo/stacks/42",
        "base": {"ref": "main", "sha": null},
        "open": true,
        "created_at": "2026-04-15T10:00:00Z",
        "pull_requests": [
            {
                "number": 101,
                "state": "open",
                "draft": false,
                "merged_at": null,
                "head": {"ref": "feature-one", "sha": "aaa"}
            },
            {
                "number": 102,
                "state": "open",
                "draft": false,
                "merged_at": null,
                "head": {"ref": "feature-two", "sha": "bbb"}
            }
        ]
    }"#;

    #[test]
    fn normalizes_stack_json() {
        let stack = serde_json::from_str::<RemoteStack>(STACK).expect("stack should decode");
        let value = serde_json::to_value(stack).expect("stack should encode");
        assert_eq!(value["nodeId"], "PRS_123");
        assert_eq!(value["base"]["ref"], "main");
        assert_eq!(value["base"].get("sha"), None);
        assert_eq!(value["pullRequests"][1]["head"]["ref"], "feature-two");
    }

    #[test]
    fn selects_the_top_pull_request() {
        let stack = serde_json::from_str::<RemoteStack>(STACK).expect("stack should decode");
        assert_eq!(
            top_pull_request(&stack)
                .expect("top pull request should exist")
                .get(),
            102
        );
    }

    #[test]
    fn decodes_failed_merge_response() {
        let result = serde_json::from_value::<AsyncMergeResult>(json!({
            "status": "failed",
            "details": {"message": "checks failed"}
        }))
        .expect("merge response should decode");
        assert_eq!(result.status, "failed");
        assert_eq!(result.details.message, "checks failed");
    }

    #[test]
    fn validates_merge_uuid() {
        assert!(validate_uuid("630b9d5e-3f2a-4f7e-8b0c-2d5f9a8c1e42").is_ok());
        assert!(validate_uuid("../../token").is_err());
    }
}
