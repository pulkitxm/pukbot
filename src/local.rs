use std::fmt::Write as _;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};

use crate::model::{Operation, Reaction, ReviewEvent};
use crate::stack;

pub fn runs_locally(operation: &Operation) -> bool {
    match operation {
        Operation::PullRequestCreate { as_app, .. }
        | Operation::PullRequestEdit { as_app, .. }
        | Operation::PullRequestMerge { as_app, .. }
        | Operation::PullRequestReview { as_app, .. }
        | Operation::PullRequestUpdateBranch { as_app, .. } => !as_app,
        Operation::PullRequestClose { .. }
        | Operation::PullRequestReopen { .. }
        | Operation::PullRequestReady { .. }
        | Operation::PullRequestDraft { .. }
        | Operation::PullRequestLabels { .. }
        | Operation::PullRequestAssignees { .. }
        | Operation::PullRequestReact { .. } => true,
        Operation::PullRequestDisableAutoMerge { .. }
        | Operation::StackCreate { .. }
        | Operation::StackAdd { .. }
        | Operation::StackUnstack { .. }
        | Operation::StackMerge { .. } => true,
        _ => false,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "the exhaustive locally executed operation match is kept in one place"
)]
pub fn execute(operation: &Operation) -> Result<String> {
    match operation {
        Operation::PullRequestCreate {
            owner,
            repository,
            title,
            body,
            head,
            base,
            draft,
            as_app: _,
        } => {
            let mut request = Map::new();
            request.insert("title".to_owned(), json!(title));
            request.insert("head".to_owned(), json!(head));
            request.insert("base".to_owned(), json!(base));
            request.insert("draft".to_owned(), json!(draft));
            insert_optional(&mut request, "body", body.as_deref());
            api(
                "POST",
                &format!("repos/{}/pulls", slug(owner, repository)),
                Some(&Value::Object(request)),
                Some(".html_url"),
            )
        }
        Operation::PullRequestEdit {
            owner,
            repository,
            number,
            title,
            body,
            base,
            as_app: _,
        } => {
            let mut request = Map::new();
            insert_optional(&mut request, "title", title.as_deref());
            insert_optional(&mut request, "body", body.as_deref());
            insert_optional(&mut request, "base", base.as_deref());
            api(
                "PATCH",
                &format!("repos/{}/pulls/{number}", slug(owner, repository)),
                Some(&Value::Object(request)),
                Some(".html_url"),
            )
        }
        Operation::PullRequestClose {
            owner,
            repository,
            number,
        } => api(
            "PATCH",
            &format!("repos/{}/pulls/{number}", slug(owner, repository)),
            Some(&json!({"state": "closed"})),
            Some(".html_url"),
        ),
        Operation::PullRequestReopen {
            owner,
            repository,
            number,
        } => api(
            "PATCH",
            &format!("repos/{}/pulls/{number}", slug(owner, repository)),
            Some(&json!({"state": "open"})),
            Some(".html_url"),
        ),
        Operation::PullRequestMerge {
            owner,
            repository,
            number,
            as_app: _,
        } => {
            if stack::is_stacked(owner, repository, *number)? {
                return stack::merge_pull_request(owner, repository, *number);
            }
            let slug = slug(owner, repository);
            let path = format!("repos/{slug}/pulls/{number}");
            let title = api("GET", &path, None, Some(".title"))?;
            api(
                "PUT",
                &format!("{path}/merge"),
                Some(&json!({
                    "merge_method": "squash",
                    "commit_title": squash_title(&title, number.get()),
                    "commit_message": ""
                })),
                None,
            )?;
            Ok(pull_request_url(&slug, number.get()))
        }
        Operation::PullRequestReady {
            owner,
            repository,
            number,
        } => set_draft(owner, repository, number.get(), false),
        Operation::PullRequestDraft {
            owner,
            repository,
            number,
        } => set_draft(owner, repository, number.get(), true),
        Operation::PullRequestReview {
            owner,
            repository,
            number,
            event,
            body,
            as_app: _,
        } => {
            let slug = slug(owner, repository);
            let mut request = Map::new();
            request.insert("event".to_owned(), json!(review_event(*event)));
            insert_optional(&mut request, "body", body.as_deref());
            api(
                "POST",
                &format!("repos/{slug}/pulls/{number}/reviews"),
                Some(&Value::Object(request)),
                None,
            )?;
            Ok(pull_request_url(&slug, number.get()))
        }
        Operation::PullRequestLabels {
            owner,
            repository,
            number,
            add,
            remove,
        } => {
            let slug = slug(owner, repository);
            if !add.is_empty() {
                api(
                    "POST",
                    &format!("repos/{slug}/issues/{number}/labels"),
                    Some(&json!({"labels": add})),
                    None,
                )?;
            }
            for label in remove {
                api(
                    "DELETE",
                    &format!(
                        "repos/{slug}/issues/{number}/labels/{}",
                        encode_path_segment(label)
                    ),
                    None,
                    None,
                )?;
            }
            Ok(pull_request_url(&slug, number.get()))
        }
        Operation::PullRequestAssignees {
            owner,
            repository,
            number,
            add,
            remove,
        } => {
            let slug = slug(owner, repository);
            let path = format!("repos/{slug}/issues/{number}/assignees");
            if !add.is_empty() {
                api("POST", &path, Some(&json!({"assignees": add})), None)?;
            }
            if !remove.is_empty() {
                api("DELETE", &path, Some(&json!({"assignees": remove})), None)?;
            }
            Ok(pull_request_url(&slug, number.get()))
        }
        Operation::PullRequestReact {
            owner,
            repository,
            number,
            reaction,
        } => {
            let slug = slug(owner, repository);
            api(
                "POST",
                &format!("repos/{slug}/issues/{number}/reactions"),
                Some(&json!({"content": reaction_content(*reaction)})),
                None,
            )?;
            Ok(pull_request_url(&slug, number.get()))
        }
        Operation::PullRequestUpdateBranch {
            owner,
            repository,
            number,
            as_app: _,
        } => {
            let slug = slug(owner, repository);
            api(
                "PUT",
                &format!("repos/{slug}/pulls/{number}/update-branch"),
                Some(&json!({})),
                None,
            )?;
            Ok(pull_request_url(&slug, number.get()))
        }
        Operation::PullRequestDisableAutoMerge {
            owner,
            repository,
            number,
        } => disable_auto_merge(owner, repository, number.get()),
        Operation::StackCreate { .. }
        | Operation::StackAdd { .. }
        | Operation::StackUnstack { .. }
        | Operation::StackMerge { .. } => stack::execute(operation),
        _ => bail!(
            "operation {} is executed by the Pukbot GitHub App",
            operation.name()
        ),
    }
}

fn disable_auto_merge(owner: &str, repository: &str, number: u64) -> Result<String> {
    let slug = slug(owner, repository);
    let output = Command::new("gh")
        .args([
            "pr",
            "merge",
            &number.to_string(),
            "--repo",
            &slug,
            "--disable-auto",
        ])
        .output()
        .context("failed to launch gh; install and authenticate GitHub CLI")?;
    if !output.status.success() {
        bail!(
            "GitHub rejected the auto-merge change: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(pull_request_url(&slug, number))
}

fn set_draft(owner: &str, repository: &str, number: u64, draft: bool) -> Result<String> {
    let slug = slug(owner, repository);
    let mut command = Command::new("gh");
    command.args(["pr", "ready", &number.to_string(), "--repo", &slug]);
    if draft {
        command.arg("--undo");
    }
    let output = command
        .output()
        .context("failed to launch gh; install and authenticate GitHub CLI")?;
    if !output.status.success() {
        bail!(
            "GitHub rejected the draft state change: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(pull_request_url(&slug, number))
}

fn api(method: &str, path: &str, request: Option<&Value>, filter: Option<&str>) -> Result<String> {
    let mut command = Command::new("gh");
    command.args(["api", "--method", method, path]);
    if request.is_some() {
        command.args(["--input", "-"]);
    }
    if let Some(expression) = filter {
        command.args(["--jq", expression]);
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
    let output = child.wait_with_output().context("failed to wait for gh")?;
    if !output.status.success() {
        bail!(
            "GitHub rejected {method} {path}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8(output.stdout)
        .context("GitHub CLI returned non-UTF-8 output")?
        .trim()
        .to_owned())
}

fn insert_optional(request: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        request.insert(key.to_owned(), json!(value));
    }
}

fn slug(owner: &str, repository: &str) -> String {
    format!("{owner}/{repository}")
}

fn pull_request_url(slug: &str, number: u64) -> String {
    format!("https://github.com/{slug}/pull/{number}")
}

fn squash_title(title: &str, number: u64) -> String {
    format!("{title} (#{number})")
}

const fn review_event(event: ReviewEvent) -> &'static str {
    match event {
        ReviewEvent::Approve => "APPROVE",
        ReviewEvent::RequestChanges => "REQUEST_CHANGES",
        ReviewEvent::Comment => "COMMENT",
    }
}

const fn reaction_content(reaction: Reaction) -> &'static str {
    match reaction {
        Reaction::PlusOne => "+1",
        Reaction::MinusOne => "-1",
        Reaction::Laugh => "laugh",
        Reaction::Confused => "confused",
        Reaction::Heart => "heart",
        Reaction::Hooray => "hooray",
        Reaction::Rocket => "rocket",
        Reaction::Eyes => "eyes",
    }
}

fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            write!(encoded, "%{byte:02X}").expect("writing to a String cannot fail");
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::{encode_path_segment, pull_request_url, review_event, runs_locally, squash_title};
    use crate::model::{Request, ReviewEvent};

    #[test]
    fn encodes_label_path_segments() {
        assert_eq!(encode_path_segment("needs review"), "needs%20review");
        assert_eq!(encode_path_segment("type/bug"), "type%2Fbug");
    }

    #[test]
    fn builds_pull_request_urls() {
        assert_eq!(
            pull_request_url("owner/repository", 7),
            "https://github.com/owner/repository/pull/7"
        );
    }

    #[test]
    fn builds_squash_titles_without_attribution() {
        assert_eq!(
            squash_title("feat: add operation", 7),
            "feat: add operation (#7)"
        );
    }

    #[test]
    fn maps_review_events() {
        assert_eq!(review_event(ReviewEvent::RequestChanges), "REQUEST_CHANGES");
    }

    #[test]
    fn routes_explicit_app_modes_through_the_workflow() {
        let documents = [
            r#"{"operation":"pull_request_create","repository":"owner/repo","title":"title","head":"feature","base":"main"}"#,
            r#"{"operation":"pull_request_edit","repository":"owner/repo","number":1,"title":"title"}"#,
            r#"{"operation":"pull_request_merge","repository":"owner/repo","number":1}"#,
            r#"{"operation":"pull_request_review","repository":"owner/repo","number":1,"event":"comment"}"#,
            r#"{"operation":"pull_request_update_branch","repository":"owner/repo","number":1}"#,
        ];
        for document in documents {
            let request = serde_json::from_str::<Request>(document).expect("request should parse");
            let operation = request.prepare(true).expect("request should prepare");
            assert!(runs_locally(&operation));

            let app_document = document.replace('}', ",\"as_app\":true}");
            let request =
                serde_json::from_str::<Request>(&app_document).expect("request should parse");
            let operation = request.prepare(true).expect("request should prepare");
            assert!(!runs_locally(&operation));
        }
    }
}
