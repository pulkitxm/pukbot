use std::collections::HashSet;
use std::fmt;
use std::num::NonZeroU64;
use std::str::FromStr;

use anyhow::{Result, bail};
use clap::ValueEnum;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::media;

pub const MAX_BODY_BYTES: usize = 40_000;
pub const MAX_COMMIT_MESSAGE_BYTES: usize = 8_000;
pub const MAX_COMMIT_FILE_BYTES: usize = 60_000;
pub const MAX_COMMIT_TOTAL_BYTES: usize = 120_000;
pub const MAX_COMMIT_FILES: usize = 50;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Repository {
    pub owner: String,
    pub name: String,
}

impl Repository {
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

impl fmt::Display for Repository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.owner, self.name)
    }
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

impl Serialize for Repository {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.slug())
    }
}

impl<'de> Deserialize<'de> for Repository {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Reaction {
    #[value(name = "+1")]
    #[serde(rename = "+1")]
    PlusOne,
    #[value(name = "-1")]
    #[serde(rename = "-1")]
    MinusOne,
    Laugh,
    Confused,
    Heart,
    Hooray,
    Rocket,
    Eyes,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum ReviewEvent {
    Approve,
    RequestChanges,
    Comment,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommentDocument {
    pub body: String,
    #[serde(default)]
    pub media: Vec<media::MediaInput>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitFileDocument {
    pub path: String,
    pub content: Option<String>,
    #[serde(default)]
    pub delete: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct CommitFile {
    pub path: String,
    pub content: Option<String>,
    pub delete: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    #[serde(rename = "comment_create")]
    CreateComment {
        repository: Repository,
        number: NonZeroU64,
        #[serde(flatten)]
        document: CommentDocument,
    },
    #[serde(rename = "comment_edit")]
    EditComment {
        repository: Repository,
        comment_id: NonZeroU64,
        #[serde(flatten)]
        document: CommentDocument,
    },
    #[serde(rename = "comment_delete")]
    DeleteComment {
        repository: Repository,
        comment_id: NonZeroU64,
    },
    #[serde(rename = "comment_react")]
    ReactToComment {
        repository: Repository,
        comment_id: NonZeroU64,
        reaction: Reaction,
    },
    IssueCreate {
        repository: Repository,
        title: String,
        body: Option<String>,
        #[serde(default)]
        labels: Vec<String>,
        #[serde(default)]
        assignees: Vec<String>,
    },
    IssueEdit {
        repository: Repository,
        number: NonZeroU64,
        title: Option<String>,
        body: Option<String>,
    },
    IssueClose {
        repository: Repository,
        number: NonZeroU64,
    },
    IssueReopen {
        repository: Repository,
        number: NonZeroU64,
    },
    IssueLabels {
        repository: Repository,
        number: NonZeroU64,
        #[serde(default)]
        add: Vec<String>,
        #[serde(default)]
        remove: Vec<String>,
    },
    IssueAssignees {
        repository: Repository,
        number: NonZeroU64,
        #[serde(default)]
        add: Vec<String>,
        #[serde(default)]
        remove: Vec<String>,
    },
    IssueReact {
        repository: Repository,
        number: NonZeroU64,
        reaction: Reaction,
    },
    PullRequestCreate {
        repository: Repository,
        title: String,
        body: Option<String>,
        head: String,
        base: String,
        #[serde(default)]
        draft: bool,
    },
    PullRequestEdit {
        repository: Repository,
        number: NonZeroU64,
        title: Option<String>,
        body: Option<String>,
        base: Option<String>,
    },
    PullRequestClose {
        repository: Repository,
        number: NonZeroU64,
    },
    PullRequestReopen {
        repository: Repository,
        number: NonZeroU64,
    },
    PullRequestMerge {
        repository: Repository,
        number: NonZeroU64,
    },
    PullRequestReady {
        repository: Repository,
        number: NonZeroU64,
    },
    PullRequestDraft {
        repository: Repository,
        number: NonZeroU64,
    },
    PullRequestReview {
        repository: Repository,
        number: NonZeroU64,
        event: ReviewEvent,
        body: Option<String>,
    },
    PullRequestLabels {
        repository: Repository,
        number: NonZeroU64,
        #[serde(default)]
        add: Vec<String>,
        #[serde(default)]
        remove: Vec<String>,
    },
    PullRequestAssignees {
        repository: Repository,
        number: NonZeroU64,
        #[serde(default)]
        add: Vec<String>,
        #[serde(default)]
        remove: Vec<String>,
    },
    PullRequestReact {
        repository: Repository,
        number: NonZeroU64,
        reaction: Reaction,
    },
    PullRequestUpdateBranch {
        repository: Repository,
        number: NonZeroU64,
    },
    CommitCreate {
        repository: Repository,
        branch: String,
        message: String,
        files: Vec<CommitFileDocument>,
    },
}

impl Request {
    #[expect(
        clippy::too_many_lines,
        reason = "the exhaustive typed operation conversion is kept in one match"
    )]
    pub fn prepare(self, dry_run: bool) -> Result<Operation> {
        match self {
            Self::CreateComment {
                repository,
                number,
                document,
            } => Ok(Operation::CreateComment {
                owner: repository.owner,
                repository: repository.name,
                number,
                body: prepare_document(document, dry_run)?,
            }),
            Self::EditComment {
                repository,
                comment_id,
                document,
            } => Ok(Operation::EditComment {
                owner: repository.owner,
                repository: repository.name,
                comment_id,
                body: prepare_document(document, dry_run)?,
            }),
            Self::DeleteComment {
                repository,
                comment_id,
            } => Ok(Operation::DeleteComment {
                owner: repository.owner,
                repository: repository.name,
                comment_id,
            }),
            Self::ReactToComment {
                repository,
                comment_id,
                reaction,
            } => Ok(Operation::ReactToComment {
                owner: repository.owner,
                repository: repository.name,
                comment_id,
                reaction,
            }),
            Self::IssueCreate {
                repository,
                title,
                body,
                labels,
                assignees,
            } => {
                validate_title(&title)?;
                validate_optional_body(body.as_deref())?;
                validate_values(&labels, "label")?;
                validate_values(&assignees, "assignee")?;
                Ok(Operation::IssueCreate {
                    owner: repository.owner,
                    repository: repository.name,
                    title,
                    body,
                    labels,
                    assignees,
                })
            }
            Self::IssueEdit {
                repository,
                number,
                title,
                body,
            } => {
                validate_edit(title.as_deref(), body.as_deref())?;
                Ok(Operation::IssueEdit {
                    owner: repository.owner,
                    repository: repository.name,
                    number,
                    title,
                    body,
                })
            }
            Self::IssueClose { repository, number } => Ok(Operation::IssueClose {
                owner: repository.owner,
                repository: repository.name,
                number,
            }),
            Self::IssueReopen { repository, number } => Ok(Operation::IssueReopen {
                owner: repository.owner,
                repository: repository.name,
                number,
            }),
            Self::IssueLabels {
                repository,
                number,
                add,
                remove,
            } => prepare_labels(repository, number, add, remove, false),
            Self::IssueAssignees {
                repository,
                number,
                add,
                remove,
            } => prepare_assignees(repository, number, add, remove, false),
            Self::IssueReact {
                repository,
                number,
                reaction,
            } => Ok(Operation::IssueReact {
                owner: repository.owner,
                repository: repository.name,
                number,
                reaction,
            }),
            Self::PullRequestCreate {
                repository,
                title,
                body,
                head,
                base,
                draft,
            } => {
                validate_title(&title)?;
                validate_optional_body(body.as_deref())?;
                validate_branch(&head)?;
                validate_branch(&base)?;
                Ok(Operation::PullRequestCreate {
                    owner: repository.owner,
                    repository: repository.name,
                    title,
                    body,
                    head,
                    base,
                    draft,
                })
            }
            Self::PullRequestEdit {
                repository,
                number,
                title,
                body,
                base,
            } => {
                if title.is_none() && body.is_none() && base.is_none() {
                    bail!("pull request edit requires --title, --body, --body-file, or --base");
                }
                if let Some(title) = &title {
                    validate_title(title)?;
                }
                validate_optional_body(body.as_deref())?;
                if let Some(base) = &base {
                    validate_branch(base)?;
                }
                Ok(Operation::PullRequestEdit {
                    owner: repository.owner,
                    repository: repository.name,
                    number,
                    title,
                    body,
                    base,
                })
            }
            Self::PullRequestClose { repository, number } => Ok(Operation::PullRequestClose {
                owner: repository.owner,
                repository: repository.name,
                number,
            }),
            Self::PullRequestReopen { repository, number } => Ok(Operation::PullRequestReopen {
                owner: repository.owner,
                repository: repository.name,
                number,
            }),
            Self::PullRequestMerge { repository, number } => Ok(Operation::PullRequestMerge {
                owner: repository.owner,
                repository: repository.name,
                number,
            }),
            Self::PullRequestReady { repository, number } => Ok(Operation::PullRequestReady {
                owner: repository.owner,
                repository: repository.name,
                number,
            }),
            Self::PullRequestDraft { repository, number } => Ok(Operation::PullRequestDraft {
                owner: repository.owner,
                repository: repository.name,
                number,
            }),
            Self::PullRequestReview {
                repository,
                number,
                event,
                body,
            } => {
                validate_optional_body(body.as_deref())?;
                if matches!(event, ReviewEvent::RequestChanges) && body.is_none() {
                    bail!("request-changes reviews require a body");
                }
                Ok(Operation::PullRequestReview {
                    owner: repository.owner,
                    repository: repository.name,
                    number,
                    event,
                    body,
                })
            }
            Self::PullRequestLabels {
                repository,
                number,
                add,
                remove,
            } => prepare_labels(repository, number, add, remove, true),
            Self::PullRequestAssignees {
                repository,
                number,
                add,
                remove,
            } => prepare_assignees(repository, number, add, remove, true),
            Self::PullRequestReact {
                repository,
                number,
                reaction,
            } => Ok(Operation::PullRequestReact {
                owner: repository.owner,
                repository: repository.name,
                number,
                reaction,
            }),
            Self::PullRequestUpdateBranch { repository, number } => {
                Ok(Operation::PullRequestUpdateBranch {
                    owner: repository.owner,
                    repository: repository.name,
                    number,
                })
            }
            Self::CommitCreate {
                repository,
                branch,
                message,
                files,
            } => {
                validate_branch(&branch)?;
                validate_commit_message(&message)?;
                Ok(Operation::CommitCreate {
                    owner: repository.owner,
                    repository: repository.name,
                    branch,
                    message,
                    files: prepare_commit_files(files)?,
                })
            }
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum Operation {
    #[serde(rename = "comment_create")]
    CreateComment {
        owner: String,
        repository: String,
        number: NonZeroU64,
        body: String,
    },
    #[serde(rename = "comment_edit")]
    EditComment {
        owner: String,
        repository: String,
        comment_id: NonZeroU64,
        body: String,
    },
    #[serde(rename = "comment_delete")]
    DeleteComment {
        owner: String,
        repository: String,
        comment_id: NonZeroU64,
    },
    #[serde(rename = "comment_react")]
    ReactToComment {
        owner: String,
        repository: String,
        comment_id: NonZeroU64,
        reaction: Reaction,
    },
    IssueCreate {
        owner: String,
        repository: String,
        title: String,
        body: Option<String>,
        labels: Vec<String>,
        assignees: Vec<String>,
    },
    IssueEdit {
        owner: String,
        repository: String,
        number: NonZeroU64,
        title: Option<String>,
        body: Option<String>,
    },
    IssueClose {
        owner: String,
        repository: String,
        number: NonZeroU64,
    },
    IssueReopen {
        owner: String,
        repository: String,
        number: NonZeroU64,
    },
    IssueLabels {
        owner: String,
        repository: String,
        number: NonZeroU64,
        add: Vec<String>,
        remove: Vec<String>,
    },
    IssueAssignees {
        owner: String,
        repository: String,
        number: NonZeroU64,
        add: Vec<String>,
        remove: Vec<String>,
    },
    IssueReact {
        owner: String,
        repository: String,
        number: NonZeroU64,
        reaction: Reaction,
    },
    PullRequestCreate {
        owner: String,
        repository: String,
        title: String,
        body: Option<String>,
        head: String,
        base: String,
        draft: bool,
    },
    PullRequestEdit {
        owner: String,
        repository: String,
        number: NonZeroU64,
        title: Option<String>,
        body: Option<String>,
        base: Option<String>,
    },
    PullRequestClose {
        owner: String,
        repository: String,
        number: NonZeroU64,
    },
    PullRequestReopen {
        owner: String,
        repository: String,
        number: NonZeroU64,
    },
    PullRequestMerge {
        owner: String,
        repository: String,
        number: NonZeroU64,
    },
    PullRequestReady {
        owner: String,
        repository: String,
        number: NonZeroU64,
    },
    PullRequestDraft {
        owner: String,
        repository: String,
        number: NonZeroU64,
    },
    PullRequestReview {
        owner: String,
        repository: String,
        number: NonZeroU64,
        event: ReviewEvent,
        body: Option<String>,
    },
    PullRequestLabels {
        owner: String,
        repository: String,
        number: NonZeroU64,
        add: Vec<String>,
        remove: Vec<String>,
    },
    PullRequestAssignees {
        owner: String,
        repository: String,
        number: NonZeroU64,
        add: Vec<String>,
        remove: Vec<String>,
    },
    PullRequestReact {
        owner: String,
        repository: String,
        number: NonZeroU64,
        reaction: Reaction,
    },
    PullRequestUpdateBranch {
        owner: String,
        repository: String,
        number: NonZeroU64,
    },
    CommitCreate {
        owner: String,
        repository: String,
        branch: String,
        message: String,
        files: Vec<CommitFile>,
    },
}

impl Operation {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::CreateComment { .. } => "comment_create",
            Self::EditComment { .. } => "comment_edit",
            Self::DeleteComment { .. } => "comment_delete",
            Self::ReactToComment { .. } => "comment_react",
            Self::IssueCreate { .. } => "issue_create",
            Self::IssueEdit { .. } => "issue_edit",
            Self::IssueClose { .. } => "issue_close",
            Self::IssueReopen { .. } => "issue_reopen",
            Self::IssueLabels { .. } => "issue_labels",
            Self::IssueAssignees { .. } => "issue_assignees",
            Self::IssueReact { .. } => "issue_react",
            Self::PullRequestCreate { .. } => "pull_request_create",
            Self::PullRequestEdit { .. } => "pull_request_edit",
            Self::PullRequestClose { .. } => "pull_request_close",
            Self::PullRequestReopen { .. } => "pull_request_reopen",
            Self::PullRequestMerge { .. } => "pull_request_merge",
            Self::PullRequestReady { .. } => "pull_request_ready",
            Self::PullRequestDraft { .. } => "pull_request_draft",
            Self::PullRequestReview { .. } => "pull_request_review",
            Self::PullRequestLabels { .. } => "pull_request_labels",
            Self::PullRequestAssignees { .. } => "pull_request_assignees",
            Self::PullRequestReact { .. } => "pull_request_react",
            Self::PullRequestUpdateBranch { .. } => "pull_request_update_branch",
            Self::CommitCreate { .. } => "commit_create",
        }
    }
}

fn prepare_document(document: CommentDocument, dry_run: bool) -> Result<String> {
    let mut names = HashSet::new();
    let mut body = document.body;
    for item in document.media {
        if !names.insert(item.name.clone()) {
            bail!("duplicate media name: {}", item.name);
        }
        let placeholder = format!("{{{}}}", item.name);
        if !body.contains(&placeholder) {
            bail!("comment body does not contain media placeholder {placeholder}");
        }
        let markdown = media::resolve(&item, dry_run)?;
        body = body.replace(&placeholder, &markdown);
    }
    validate_body(&body)?;
    Ok(body)
}

fn prepare_labels(
    repository: Repository,
    number: NonZeroU64,
    add: Vec<String>,
    remove: Vec<String>,
    pull_request: bool,
) -> Result<Operation> {
    validate_list_edit(&add, &remove, "label")?;
    if pull_request {
        Ok(Operation::PullRequestLabels {
            owner: repository.owner,
            repository: repository.name,
            number,
            add,
            remove,
        })
    } else {
        Ok(Operation::IssueLabels {
            owner: repository.owner,
            repository: repository.name,
            number,
            add,
            remove,
        })
    }
}

fn prepare_assignees(
    repository: Repository,
    number: NonZeroU64,
    add: Vec<String>,
    remove: Vec<String>,
    pull_request: bool,
) -> Result<Operation> {
    validate_list_edit(&add, &remove, "assignee")?;
    if pull_request {
        Ok(Operation::PullRequestAssignees {
            owner: repository.owner,
            repository: repository.name,
            number,
            add,
            remove,
        })
    } else {
        Ok(Operation::IssueAssignees {
            owner: repository.owner,
            repository: repository.name,
            number,
            add,
            remove,
        })
    }
}

fn validate_edit(title: Option<&str>, body: Option<&str>) -> Result<()> {
    if title.is_none() && body.is_none() {
        bail!("edit requires --title, --body, or --body-file");
    }
    if let Some(title) = title {
        validate_title(title)?;
    }
    validate_optional_body(body)
}

fn validate_title(title: &str) -> Result<()> {
    if title.trim().is_empty() || title.len() > 256 {
        bail!("title must be between 1 and 256 bytes");
    }
    Ok(())
}

fn validate_body(body: &str) -> Result<()> {
    if body.trim().is_empty() {
        bail!("comment body cannot be empty");
    }
    if body.len() > MAX_BODY_BYTES {
        bail!("body exceeds {MAX_BODY_BYTES} bytes");
    }
    Ok(())
}

fn validate_optional_body(body: Option<&str>) -> Result<()> {
    if let Some(body) = body {
        if body.len() > MAX_BODY_BYTES {
            bail!("body exceeds {MAX_BODY_BYTES} bytes");
        }
    }
    Ok(())
}

fn validate_branch(branch: &str) -> Result<()> {
    if branch.trim().is_empty() || branch.len() > 255 || branch.contains(['\n', '\r']) {
        bail!("branch must be between 1 and 255 bytes without newlines");
    }
    Ok(())
}

fn validate_commit_message(message: &str) -> Result<()> {
    if message.trim().is_empty() {
        bail!("commit message cannot be empty");
    }
    if message.len() > MAX_COMMIT_MESSAGE_BYTES {
        bail!("commit message exceeds {MAX_COMMIT_MESSAGE_BYTES} bytes");
    }
    if message.contains('\r') {
        bail!("commit message must use LF line endings");
    }
    Ok(())
}

fn validate_commit_path(path: &str) -> Result<()> {
    if path.is_empty() || path.len() > 4096 {
        bail!("commit file path must be between 1 and 4096 bytes");
    }
    if path.starts_with('/') || path.contains(['\n', '\r', '\0']) {
        bail!("commit file path must be a relative path without control characters");
    }
    if path
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        bail!("commit file path must not contain empty, '.', or '..' segments");
    }
    if path == ".git" || path.starts_with(".git/") {
        bail!("commit file path must not target .git");
    }
    Ok(())
}

fn prepare_commit_files(files: Vec<CommitFileDocument>) -> Result<Vec<CommitFile>> {
    if files.is_empty() {
        bail!("commit requires at least one file");
    }
    if files.len() > MAX_COMMIT_FILES {
        bail!("commit supports at most {MAX_COMMIT_FILES} files");
    }
    let mut seen = HashSet::new();
    let mut total = 0usize;
    let mut prepared = Vec::with_capacity(files.len());
    for file in files {
        validate_commit_path(&file.path)?;
        if !seen.insert(file.path.clone()) {
            bail!("duplicate commit file path: {}", file.path);
        }
        if file.delete {
            if file.content.is_some() {
                bail!(
                    "commit file {} cannot set content and delete together",
                    file.path
                );
            }
            prepared.push(CommitFile {
                path: file.path,
                content: None,
                delete: true,
            });
            continue;
        }
        let Some(content) = file.content else {
            bail!("commit file {} requires content or delete", file.path);
        };
        if content.len() > MAX_COMMIT_FILE_BYTES {
            bail!(
                "commit file {} exceeds {MAX_COMMIT_FILE_BYTES} bytes",
                file.path
            );
        }
        total += content.len();
        if total > MAX_COMMIT_TOTAL_BYTES {
            bail!("commit files exceed {MAX_COMMIT_TOTAL_BYTES} bytes combined");
        }
        prepared.push(CommitFile {
            path: file.path,
            content: Some(content),
            delete: false,
        });
    }
    Ok(prepared)
}

fn validate_list_edit(add: &[String], remove: &[String], name: &str) -> Result<()> {
    if add.is_empty() && remove.is_empty() {
        bail!("provide at least one {name} to add or remove");
    }
    validate_values(add, name)?;
    validate_values(remove, name)
}

fn validate_values(values: &[String], name: &str) -> Result<()> {
    if values.len() > 100
        || values
            .iter()
            .any(|value| value.trim().is_empty() || value.len() > 100)
    {
        bail!("{name} values must contain 1 to 100 bytes and at most 100 entries");
    }
    Ok(())
}

fn is_repository_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
}

#[cfg(test)]
mod tests {
    use super::{Operation, Reaction, Repository, Request};

    #[test]
    fn parses_repository() {
        let repository = "pulkitxm/pukbot".parse::<Repository>();
        assert_eq!(
            repository,
            Ok(Repository {
                owner: "pulkitxm".to_owned(),
                name: "pukbot".to_owned(),
            })
        );
    }

    #[test]
    fn parses_comment_request() {
        let request = serde_json::from_str::<Request>(
            r#"{"operation":"comment_react","repository":"owner/repo","comment_id":1,"reaction":"eyes"}"#,
        )
        .expect("request should parse");
        let operation = request.prepare(true).expect("request should prepare");
        assert!(matches!(
            operation,
            Operation::ReactToComment {
                reaction: Reaction::Eyes,
                ..
            }
        ));
    }

    #[test]
    fn parses_pull_request_review() {
        let request = serde_json::from_str::<Request>(
            r#"{"operation":"pull_request_review","repository":"owner/repo","number":1,"event":"approve","body":"looks good"}"#,
        )
        .expect("request should parse");
        assert!(matches!(
            request.prepare(true).expect("request should prepare"),
            Operation::PullRequestReview { .. }
        ));
    }

    #[test]
    fn prepares_every_operation() {
        let documents = [
            r#"{"operation":"comment_create","repository":"owner/repo","number":1,"body":"message"}"#,
            r#"{"operation":"comment_edit","repository":"owner/repo","comment_id":1,"body":"message"}"#,
            r#"{"operation":"comment_delete","repository":"owner/repo","comment_id":1}"#,
            r#"{"operation":"comment_react","repository":"owner/repo","comment_id":1,"reaction":"heart"}"#,
            r#"{"operation":"issue_create","repository":"owner/repo","title":"title","body":"body","labels":["bug"],"assignees":["owner"]}"#,
            r#"{"operation":"issue_edit","repository":"owner/repo","number":1,"title":"title","body":"body"}"#,
            r#"{"operation":"issue_close","repository":"owner/repo","number":1}"#,
            r#"{"operation":"issue_reopen","repository":"owner/repo","number":1}"#,
            r#"{"operation":"issue_labels","repository":"owner/repo","number":1,"add":["bug"],"remove":["help wanted"]}"#,
            r#"{"operation":"issue_assignees","repository":"owner/repo","number":1,"add":["owner"],"remove":["user"]}"#,
            r#"{"operation":"issue_react","repository":"owner/repo","number":1,"reaction":"rocket"}"#,
            r#"{"operation":"pull_request_create","repository":"owner/repo","title":"title","body":"body","head":"feature","base":"main","draft":true}"#,
            r#"{"operation":"pull_request_edit","repository":"owner/repo","number":1,"title":"title","body":"body","base":"main"}"#,
            r#"{"operation":"pull_request_close","repository":"owner/repo","number":1}"#,
            r#"{"operation":"pull_request_reopen","repository":"owner/repo","number":1}"#,
            r#"{"operation":"pull_request_merge","repository":"owner/repo","number":1}"#,
            r#"{"operation":"pull_request_ready","repository":"owner/repo","number":1}"#,
            r#"{"operation":"pull_request_draft","repository":"owner/repo","number":1}"#,
            r#"{"operation":"pull_request_review","repository":"owner/repo","number":1,"event":"request_changes","body":"change this"}"#,
            r#"{"operation":"pull_request_labels","repository":"owner/repo","number":1,"add":["bug"],"remove":[]}"#,
            r#"{"operation":"pull_request_assignees","repository":"owner/repo","number":1,"add":[],"remove":["user"]}"#,
            r#"{"operation":"pull_request_react","repository":"owner/repo","number":1,"reaction":"eyes"}"#,
            r#"{"operation":"pull_request_update_branch","repository":"owner/repo","number":1}"#,
            r#"{"operation":"commit_create","repository":"owner/repo","branch":"main","message":"data: update roster","files":[{"path":"data/members.json","content":"[]"},{"path":"data/old.json","delete":true}]}"#,
        ];

        for document in documents {
            let request = serde_json::from_str::<Request>(document).expect("request should parse");
            let operation = request.prepare(true).expect("request should prepare");
            assert_ne!(operation.name(), "");
        }
    }

    #[test]
    fn rejects_commit_path_traversal() {
        let request = serde_json::from_str::<Request>(
            r#"{"operation":"commit_create","repository":"owner/repo","branch":"main","message":"m","files":[{"path":"../etc/passwd","content":"x"}]}"#,
        )
        .expect("request should parse");
        assert!(request.prepare(true).is_err());
    }

    #[test]
    fn rejects_commit_file_with_content_and_delete() {
        let request = serde_json::from_str::<Request>(
            r#"{"operation":"commit_create","repository":"owner/repo","branch":"main","message":"m","files":[{"path":"a.json","content":"x","delete":true}]}"#,
        )
        .expect("request should parse");
        assert!(request.prepare(true).is_err());
    }

    #[test]
    fn rejects_commit_with_no_files() {
        let request = serde_json::from_str::<Request>(
            r#"{"operation":"commit_create","repository":"owner/repo","branch":"main","message":"m","files":[]}"#,
        )
        .expect("request should parse");
        assert!(request.prepare(true).is_err());
    }
}
