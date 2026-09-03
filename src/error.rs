use std::io::{self, Write};

use anyhow::Error;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorEnvelope {
    pub ok: bool,
    pub error: ErrorBody,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

pub fn classify(error: &Error) -> ErrorBody {
    let chain = error.to_string();
    let lower = chain.to_ascii_lowercase();
    let workflow_url = extract_workflow_url(&chain);
    if lower.contains("permission-actions")
        || lower.contains("permission-actions: write")
        || (lower.contains("422") && lower.contains("installation"))
        || lower.contains("permissions requested are not granted")
    {
        return ErrorBody {
            code: "installation_permission_missing".to_owned(),
            message: chain,
            operation: Some("workflow_dispatch".to_owned()),
            permission: Some("actions:write".to_owned()),
            installation: None,
            workflow_url,
            fix: Some(
                "Grant Actions: Read and write to the Pukbot GitHub App, then approve the permission update on the target installation.".to_owned(),
            ),
        };
    }
    if lower.contains("permission-deployments") {
        return ErrorBody {
            code: "installation_permission_missing".to_owned(),
            message: chain,
            operation: Some("deployment_create".to_owned()),
            permission: Some("deployments:write".to_owned()),
            installation: None,
            workflow_url,
            fix: Some(
                "Grant Deployments: Read and write to the Pukbot GitHub App, then approve the permission update on the target installation.".to_owned(),
            ),
        };
    }
    if lower.contains("pukbot workflow failed") {
        return ErrorBody {
            code: "workflow_failed".to_owned(),
            message: chain,
            operation: None,
            permission: None,
            installation: None,
            workflow_url,
            fix: None,
        };
    }
    ErrorBody {
        code: "command_failed".to_owned(),
        message: chain,
        operation: None,
        permission: None,
        installation: None,
        workflow_url,
        fix: None,
    }
}

pub fn emit_json_error(body: &ErrorBody) -> io::Result<()> {
    let envelope = ErrorEnvelope {
        ok: false,
        error: body.clone(),
    };
    let mut stdout = io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, &envelope)?;
    writeln!(stdout)?;
    Ok(())
}

fn extract_workflow_url(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|token| token.starts_with("https://github.com/") && token.contains("/actions/runs/"))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::{classify, extract_workflow_url};

    #[test]
    fn classifies_missing_actions_permission() {
        let error = anyhow::anyhow!(
            "Pukbot workflow failed: https://github.com/pulkitxm/pukbot/actions/runs/33652123437: permissions requested are not granted to this installation"
        );
        let body = classify(&error);
        assert_eq!(body.code, "installation_permission_missing");
        assert_eq!(body.permission.as_deref(), Some("actions:write"));
    }

    #[test]
    fn extracts_workflow_url_from_message() {
        assert_eq!(
            extract_workflow_url(
                "failed: https://github.com/pulkitxm/pukbot/actions/runs/42 finished"
            ),
            Some("https://github.com/pulkitxm/pukbot/actions/runs/42".to_owned())
        );
    }
}
