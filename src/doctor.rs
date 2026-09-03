use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::model::Repository;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub repository: String,
    pub installed: bool,
    pub app_slug: Option<String>,
    pub permissions: BTreeMap<String, String>,
    pub operations: Vec<OperationAvailability>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationAvailability {
    pub command: String,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_permission: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct InstallationResponse {
    app_slug: String,
    permissions: BTreeMap<String, String>,
}

pub fn run(repository: &Repository) -> Result<DoctorReport> {
    let slug = repository.slug();
    let output = std::process::Command::new("gh")
        .args(["api", &format!("repos/{slug}/installation")])
        .output()
        .context("failed to launch gh; install and authenticate GitHub CLI")?;
    if output.status.code() == Some(404) {
        return Ok(DoctorReport {
            repository: slug,
            installed: false,
            app_slug: None,
            permissions: BTreeMap::new(),
            operations: unavailable_all("Pukbot is not installed on this repository"),
        });
    }
    if !output.status.success() {
        bail!(
            "failed to inspect the Pukbot installation: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let installation = serde_json::from_slice::<InstallationResponse>(&output.stdout)
        .context("failed to decode the installation response")?;
    let operations = evaluate(&installation.permissions);
    Ok(DoctorReport {
        repository: slug,
        installed: true,
        app_slug: Some(installation.app_slug),
        permissions: installation.permissions,
        operations,
    })
}

fn evaluate(permissions: &BTreeMap<String, String>) -> Vec<OperationAvailability> {
    let default = has_write(permissions, "contents")
        && has_write(permissions, "issues")
        && has_write(permissions, "pull_requests");
    let workflow = has_write(permissions, "actions");
    let deployment = has_write(permissions, "deployments");
    let mut operations = Vec::new();
    for command in APP_DEFAULT_COMMANDS {
        operations.push(OperationAvailability {
            command: (*command).to_owned(),
            available: default,
            required_permission: if default {
                None
            } else {
                Some("contents:write, issues:write, pull_requests:write".to_owned())
            },
            reason: if default {
                None
            } else {
                Some("installation is missing one or more default write permissions".to_owned())
            },
        });
    }
    for command in WORKFLOW_COMMANDS {
        operations.push(OperationAvailability {
            command: (*command).to_owned(),
            available: workflow,
            required_permission: if workflow {
                None
            } else {
                Some("actions:write".to_owned())
            },
            reason: if workflow {
                None
            } else {
                Some("installation is missing Actions: Read and write".to_owned())
            },
        });
    }
    for command in DEPLOYMENT_COMMANDS {
        operations.push(OperationAvailability {
            command: (*command).to_owned(),
            available: deployment,
            required_permission: if deployment {
                None
            } else {
                Some("deployments:write".to_owned())
            },
            reason: if deployment {
                None
            } else {
                Some("installation is missing Deployments: Read and write".to_owned())
            },
        });
    }
    for command in LOCAL_COMMANDS {
        operations.push(OperationAvailability {
            command: (*command).to_owned(),
            available: true,
            required_permission: None,
            reason: None,
        });
    }
    operations
}

fn unavailable_all(reason: &str) -> Vec<OperationAvailability> {
    APP_DEFAULT_COMMANDS
        .iter()
        .chain(WORKFLOW_COMMANDS.iter())
        .chain(DEPLOYMENT_COMMANDS.iter())
        .map(|command| OperationAvailability {
            command: (*command).to_owned(),
            available: false,
            required_permission: None,
            reason: Some(reason.to_owned()),
        })
        .chain(LOCAL_COMMANDS.iter().map(|command| OperationAvailability {
            command: (*command).to_owned(),
            available: true,
            required_permission: None,
            reason: None,
        }))
        .collect()
}

fn has_write(permissions: &BTreeMap<String, String>, key: &str) -> bool {
    permissions
        .get(key)
        .is_some_and(|value| value == "write" || value == "admin")
}

const APP_DEFAULT_COMMANDS: &[&str] = &[
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
    "issue.batch",
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
];

const WORKFLOW_COMMANDS: &[&str] = &[
    "workflow.dispatch",
    "workflow.cancel",
    "workflow.rerun",
    "workflow.enable",
    "workflow.disable",
];

const DEPLOYMENT_COMMANDS: &[&str] = &["deployment.create", "deployment.status"];

const LOCAL_COMMANDS: &[&str] = &[
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
    "pr.disable-auto-merge",
    "pr.view",
    "pr.checks",
    "pr.threads",
    "pr.thread.resolve",
    "pr.thread.unresolve",
    "comment.reply",
    "workflow.runs",
    "workflow.status",
    "workflow.watch",
    "workflow.logs",
];

pub fn filter_capabilities(commands: Vec<String>, report: &DoctorReport) -> Vec<String> {
    let available = report
        .operations
        .iter()
        .filter(|operation| operation.available)
        .map(|operation| operation.command.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    commands
        .into_iter()
        .filter(|command| {
            LOCAL_COMMANDS.contains(&command.as_str())
                || available.contains(command.as_str())
                || matches!(
                    command.as_str(),
                    "apply"
                        | "completions"
                        | "man"
                        | "update"
                        | "capabilities"
                        | "doctor"
                        | "stack.init"
                        | "stack.add"
                        | "stack.remove"
                        | "stack.log"
                        | "stack.status"
                        | "stack.modify"
                        | "stack.submit"
                        | "stack-api.list"
                        | "stack-api.view"
                        | "stack-api.create"
                        | "stack-api.append"
                        | "stack-api.unstack"
                        | "stack-api.merge"
                        | "stack-api.merge-status"
                )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{evaluate, has_write};

    #[test]
    fn marks_workflow_commands_unavailable_without_actions_write() {
        let permissions = BTreeMap::from([
            ("contents".to_owned(), "write".to_owned()),
            ("issues".to_owned(), "write".to_owned()),
            ("pull_requests".to_owned(), "write".to_owned()),
        ]);
        let operations = evaluate(&permissions);
        let dispatch = operations
            .iter()
            .find(|operation| operation.command == "workflow.dispatch")
            .expect("workflow.dispatch should be present");
        assert!(!dispatch.available);
    }

    #[test]
    fn accepts_admin_as_write() {
        let permissions = BTreeMap::from([("actions".to_owned(), "admin".to_owned())]);
        assert!(has_write(&permissions, "actions"));
    }
}
