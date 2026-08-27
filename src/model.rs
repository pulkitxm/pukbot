use std::collections::{BTreeMap, HashSet};
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
pub const MAX_WORKFLOW_INPUTS: usize = 25;
pub const MAX_WORKFLOW_INPUT_PAYLOAD_CHARS: usize = 65_535;
pub const MAX_REPOSITORY_DISPATCH_PROPERTIES: usize = 10;
pub const MAX_REPOSITORY_DISPATCH_PAYLOAD_CHARS: usize = 65_535;

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
    RepositoryDispatch {
        repository: Repository,
        event_type: String,
        #[serde(default)]
        client_payload: BTreeMap<String, serde_json::Value>,
    },
    RefCreate {
        repository: Repository,
        #[serde(rename = "ref")]
        reference: String,
        sha: String,
    },
    RefDelete {
        repository: Repository,
        #[serde(rename = "ref")]
        reference: String,
    },
    TagCreate {
        repository: Repository,
        tag: String,
        target: String,
        message: Option<String>,
    },
    TagDelete {
        repository: Repository,
        tag: String,
    },
    WorkflowDispatch {
        repository: Repository,
        workflow: String,
        #[serde(rename = "ref")]
        reference: String,
        #[serde(default)]
        inputs: BTreeMap<String, String>,
    },
    WorkflowCancel {
        repository: Repository,
        run_id: NonZeroU64,
    },
    WorkflowRerun {
        repository: Repository,
        run_id: NonZeroU64,
        #[serde(default)]
        failed_only: bool,
    },
    WorkflowEnable {
        repository: Repository,
        workflow: String,
    },
    WorkflowDisable {
        repository: Repository,
        workflow: String,
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
            Self::RepositoryDispatch {
                repository,
                event_type,
                client_payload,
            } => {
                validate_repository_dispatch(&event_type, &client_payload)?;
                Ok(Operation::RepositoryDispatch {
                    owner: repository.owner,
                    repository: repository.name,
                    event_type,
                    client_payload,
                })
            }
            Self::RefCreate {
                repository,
                reference,
                sha,
            } => {
                validate_full_ref(&reference)?;
                validate_git_object_sha(&sha)?;
                Ok(Operation::RefCreate {
                    owner: repository.owner,
                    repository: repository.name,
                    reference,
                    sha,
                })
            }
            Self::RefDelete {
                repository,
                reference,
            } => {
                validate_full_ref(&reference)?;
                Ok(Operation::RefDelete {
                    owner: repository.owner,
                    repository: repository.name,
                    reference,
                })
            }
            Self::TagCreate {
                repository,
                tag,
                target,
                message,
            } => {
                validate_tag(&tag)?;
                validate_git_object_sha(&target)?;
                if let Some(message) = &message {
                    validate_commit_message(message)?;
                }
                Ok(Operation::TagCreate {
                    owner: repository.owner,
                    repository: repository.name,
                    tag,
                    target,
                    message,
                })
            }
            Self::TagDelete { repository, tag } => {
                validate_tag(&tag)?;
                Ok(Operation::TagDelete {
                    owner: repository.owner,
                    repository: repository.name,
                    tag,
                })
            }
            Self::WorkflowDispatch {
                repository,
                workflow,
                reference,
                inputs,
            } => {
                validate_workflow(&workflow)?;
                validate_branch(&reference)?;
                validate_workflow_inputs(&inputs)?;
                Ok(Operation::WorkflowDispatch {
                    owner: repository.owner,
                    repository: repository.name,
                    workflow,
                    reference,
                    inputs,
                })
            }
            Self::WorkflowCancel { repository, run_id } => Ok(Operation::WorkflowCancel {
                owner: repository.owner,
                repository: repository.name,
                run_id,
            }),
            Self::WorkflowRerun {
                repository,
                run_id,
                failed_only,
            } => Ok(Operation::WorkflowRerun {
                owner: repository.owner,
                repository: repository.name,
                run_id,
                failed_only,
            }),
            Self::WorkflowEnable {
                repository,
                workflow,
            } => {
                validate_workflow(&workflow)?;
                Ok(Operation::WorkflowEnable {
                    owner: repository.owner,
                    repository: repository.name,
                    workflow,
                })
            }
            Self::WorkflowDisable {
                repository,
                workflow,
            } => {
                validate_workflow(&workflow)?;
                Ok(Operation::WorkflowDisable {
                    owner: repository.owner,
                    repository: repository.name,
                    workflow,
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
    RepositoryDispatch {
        owner: String,
        repository: String,
        event_type: String,
        client_payload: BTreeMap<String, serde_json::Value>,
    },
    RefCreate {
        owner: String,
        repository: String,
        #[serde(rename = "ref")]
        reference: String,
        sha: String,
    },
    RefDelete {
        owner: String,
        repository: String,
        #[serde(rename = "ref")]
        reference: String,
    },
    TagCreate {
        owner: String,
        repository: String,
        tag: String,
        target: String,
        message: Option<String>,
    },
    TagDelete {
        owner: String,
        repository: String,
        tag: String,
    },
    WorkflowDispatch {
        owner: String,
        repository: String,
        workflow: String,
        #[serde(rename = "ref")]
        reference: String,
        inputs: BTreeMap<String, String>,
    },
    WorkflowCancel {
        owner: String,
        repository: String,
        run_id: NonZeroU64,
    },
    WorkflowRerun {
        owner: String,
        repository: String,
        run_id: NonZeroU64,
        failed_only: bool,
    },
    WorkflowEnable {
        owner: String,
        repository: String,
        workflow: String,
    },
    WorkflowDisable {
        owner: String,
        repository: String,
        workflow: String,
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
            Self::RepositoryDispatch { .. } => "repository_dispatch",
            Self::RefCreate { .. } => "ref_create",
            Self::RefDelete { .. } => "ref_delete",
            Self::TagCreate { .. } => "tag_create",
            Self::TagDelete { .. } => "tag_delete",
            Self::WorkflowDispatch { .. } => "workflow_dispatch",
            Self::WorkflowCancel { .. } => "workflow_cancel",
            Self::WorkflowRerun { .. } => "workflow_rerun",
            Self::WorkflowEnable { .. } => "workflow_enable",
            Self::WorkflowDisable { .. } => "workflow_disable",
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
    validate_markdown(body)
}

fn validate_optional_body(body: Option<&str>) -> Result<()> {
    if let Some(body) = body {
        if body.len() > MAX_BODY_BYTES {
            bail!("body exceeds {MAX_BODY_BYTES} bytes");
        }
        validate_markdown(body)?;
    }
    Ok(())
}

fn validate_markdown(body: &str) -> Result<()> {
    if let Some(fence) = unclosed_code_fence(body) {
        bail!(
            "body has an unclosed {fence} code fence; close it so the rest of the body renders as Markdown"
        );
    }
    Ok(())
}

fn unclosed_code_fence(body: &str) -> Option<String> {
    let mut open: Option<(char, usize)> = None;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if line.len() - trimmed.len() >= 4 {
            continue;
        }
        let Some(marker) = trimmed.chars().next() else {
            continue;
        };
        if !matches!(marker, '`' | '~') {
            continue;
        }
        let width = trimmed.chars().take_while(|value| *value == marker).count();
        if width < 3 {
            continue;
        }
        let rest = &trimmed[width..];
        match open {
            None => {
                if marker == '`' && rest.contains('`') {
                    continue;
                }
                open = Some((marker, width));
            }
            Some((open_marker, open_width)) => {
                if marker == open_marker && width >= open_width && rest.trim().is_empty() {
                    open = None;
                }
            }
        }
    }
    open.map(|(marker, width)| marker.to_string().repeat(width))
}

fn validate_branch(branch: &str) -> Result<()> {
    let invalid_component = branch.split('/').any(|component| {
        component.is_empty()
            || component.starts_with('.')
            || component.as_bytes().ends_with(b".lock")
    });
    if branch.trim().is_empty()
        || branch.len() > 255
        || branch == "@"
        || branch.ends_with('.')
        || branch.contains("..")
        || branch.contains("@{")
        || branch
            .chars()
            .any(|character| character.is_control() || " ~^:?*[\\".contains(character))
        || invalid_component
    {
        bail!("branch or tag must be a valid Git ref name between 1 and 255 bytes");
    }
    Ok(())
}

fn validate_full_ref(reference: &str) -> Result<()> {
    let suffix = reference
        .strip_prefix("refs/")
        .filter(|suffix| suffix.contains('/'))
        .ok_or_else(|| {
            anyhow::anyhow!("ref must start with refs/ and contain at least two slashes")
        })?;
    validate_branch(suffix)
}

fn validate_tag(tag: &str) -> Result<()> {
    if tag.starts_with("refs/") {
        bail!("tag must not include the refs/tags/ prefix");
    }
    validate_branch(tag)
}

fn validate_git_object_sha(sha: &str) -> Result<()> {
    if sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("Git object SHA must contain exactly 40 hexadecimal characters");
    }
    Ok(())
}

fn validate_workflow(workflow: &str) -> Result<()> {
    if workflow.is_empty()
        || workflow.len() > 255
        || workflow == "."
        || workflow == ".."
        || !workflow
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        bail!("workflow must be a numeric ID or workflow file name");
    }
    Ok(())
}

fn validate_workflow_inputs(inputs: &BTreeMap<String, String>) -> Result<()> {
    if inputs.len() > MAX_WORKFLOW_INPUTS {
        bail!("workflow dispatch supports at most {MAX_WORKFLOW_INPUTS} inputs");
    }
    for key in inputs.keys() {
        if key.is_empty() || key.len() > 255 || key.chars().any(char::is_control) {
            bail!(
                "workflow input names must be between 1 and 255 bytes without control characters"
            );
        }
    }
    let payload_chars = serde_json::to_string(inputs)?.chars().count();
    if payload_chars > MAX_WORKFLOW_INPUT_PAYLOAD_CHARS {
        bail!("workflow inputs exceed {MAX_WORKFLOW_INPUT_PAYLOAD_CHARS} characters when encoded");
    }
    Ok(())
}

fn validate_repository_dispatch(
    event_type: &str,
    client_payload: &BTreeMap<String, serde_json::Value>,
) -> Result<()> {
    if event_type.trim().is_empty()
        || event_type.len() > 100
        || event_type.chars().any(char::is_control)
    {
        bail!(
            "repository dispatch event type must be between 1 and 100 bytes without control characters"
        );
    }
    if client_payload.len() > MAX_REPOSITORY_DISPATCH_PROPERTIES {
        bail!(
            "repository dispatch client payload supports at most {MAX_REPOSITORY_DISPATCH_PROPERTIES} top-level properties"
        );
    }
    let payload_chars = serde_json::to_string(client_payload)?.chars().count();
    if payload_chars > MAX_REPOSITORY_DISPATCH_PAYLOAD_CHARS {
        bail!(
            "repository dispatch client payload exceeds {MAX_REPOSITORY_DISPATCH_PAYLOAD_CHARS} characters when encoded"
        );
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
    use super::{Operation, Reaction, Repository, Request, unclosed_code_fence};

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
    fn accepts_closed_code_fences() {
        assert_eq!(
            unclosed_code_fence("text\n\n```rust\nfn main() {}\n```\n"),
            None
        );
        assert_eq!(unclosed_code_fence("````\n```\n````"), None);
        assert_eq!(unclosed_code_fence("~~~text\noutput\n~~~"), None);
        assert_eq!(unclosed_code_fence("plain text with ``` inline"), None);
        assert_eq!(unclosed_code_fence("~~struck~~"), None);
        assert_eq!(unclosed_code_fence("```text\n    ```\n```"), None);
        assert_eq!(
            unclosed_code_fence("- item\n\n  ```text\n  output\n  ```\n"),
            None
        );
    }

    #[test]
    fn detects_unclosed_code_fences() {
        assert_eq!(
            unclosed_code_fence("```text\noutput\n"),
            Some("```".to_owned())
        );
        assert_eq!(
            unclosed_code_fence("````\n```\nnested\n```\n"),
            Some("````".to_owned())
        );
        assert_eq!(unclosed_code_fence("~~~\noutput"), Some("~~~".to_owned()));
    }

    #[test]
    fn rejects_a_body_with_an_unclosed_fence() {
        let request = serde_json::from_str::<Request>(
            r#"{"operation":"comment_create","repository":"owner/repo","number":1,"body":"```text\noutput\n"}"#,
        )
        .expect("request should parse");
        assert!(request.prepare(true).is_err());
    }

    #[test]
    fn keeps_rich_markdown_verbatim() {
        let body = "## Result\n\n| check | state |\n| --- | --- |\n| fmt | pass |\n\n```bash\npukbot capabilities --json\n```\n\n> [!NOTE]\n> done\n";
        let document = serde_json::json!({
            "operation": "comment_create",
            "repository": "owner/repo",
            "number": 1,
            "body": body,
        })
        .to_string();
        let request = serde_json::from_str::<Request>(&document).expect("request should parse");
        let operation = request.prepare(true).expect("request should prepare");
        match operation {
            Operation::CreateComment { body: prepared, .. } => assert_eq!(prepared, body),
            other => panic!("unexpected operation: {}", other.name()),
        }
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
            r#"{"operation":"repository_dispatch","repository":"owner/repo","event_type":"apt-release","client_payload":{"version":"1.2.3"}}"#,
            r#"{"operation":"ref_create","repository":"owner/repo","ref":"refs/heads/release","sha":"0123456789abcdef0123456789abcdef01234567"}"#,
            r#"{"operation":"ref_delete","repository":"owner/repo","ref":"refs/heads/release"}"#,
            r#"{"operation":"tag_create","repository":"owner/repo","tag":"v1.2.3","target":"0123456789abcdef0123456789abcdef01234567","message":"release 1.2.3"}"#,
            r#"{"operation":"tag_delete","repository":"owner/repo","tag":"v1.2.3"}"#,
            r#"{"operation":"workflow_dispatch","repository":"owner/repo","workflow":"release.yml","ref":"main","inputs":{"release":"true"}}"#,
            r#"{"operation":"workflow_cancel","repository":"owner/repo","run_id":1}"#,
            r#"{"operation":"workflow_rerun","repository":"owner/repo","run_id":1,"failed_only":true}"#,
            r#"{"operation":"workflow_enable","repository":"owner/repo","workflow":"release.yml"}"#,
            r#"{"operation":"workflow_disable","repository":"owner/repo","workflow":"release.yml"}"#,
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

    #[test]
    fn serializes_workflow_dispatch_contract() {
        let request = serde_json::from_str::<Request>(
            r#"{"operation":"workflow_dispatch","repository":"owner/repo","workflow":"release.yml","ref":"main","inputs":{"release":"true"}}"#,
        )
        .expect("request should parse");
        let operation = request.prepare(true).expect("request should prepare");
        assert_eq!(
            serde_json::to_value(operation).expect("operation should serialize"),
            serde_json::json!({
                "operation": "workflow_dispatch",
                "owner": "owner",
                "repository": "repo",
                "workflow": "release.yml",
                "ref": "main",
                "inputs": {"release": "true"}
            })
        );
    }

    #[test]
    fn serializes_repository_dispatch_contract() {
        let request = serde_json::from_str::<Request>(
            r#"{"operation":"repository_dispatch","repository":"owner/repo","event_type":"apt-release","client_payload":{"version":"1.2.3","retry":false}}"#,
        )
        .expect("request should parse");
        let operation = request.prepare(true).expect("request should prepare");
        assert_eq!(
            serde_json::to_value(operation).expect("operation should serialize"),
            serde_json::json!({
                "operation": "repository_dispatch",
                "owner": "owner",
                "repository": "repo",
                "event_type": "apt-release",
                "client_payload": {"retry": false, "version": "1.2.3"}
            })
        );
    }

    #[test]
    fn serializes_ref_and_tag_contracts() {
        let documents = [
            (
                r#"{"operation":"ref_create","repository":"owner/repo","ref":"refs/heads/release","sha":"0123456789abcdef0123456789abcdef01234567"}"#,
                serde_json::json!({
                    "operation": "ref_create",
                    "owner": "owner",
                    "repository": "repo",
                    "ref": "refs/heads/release",
                    "sha": "0123456789abcdef0123456789abcdef01234567"
                }),
            ),
            (
                r#"{"operation":"ref_delete","repository":"owner/repo","ref":"refs/heads/release"}"#,
                serde_json::json!({
                    "operation": "ref_delete",
                    "owner": "owner",
                    "repository": "repo",
                    "ref": "refs/heads/release"
                }),
            ),
            (
                r#"{"operation":"tag_create","repository":"owner/repo","tag":"v1.2.3","target":"0123456789abcdef0123456789abcdef01234567","message":"release 1.2.3"}"#,
                serde_json::json!({
                    "operation": "tag_create",
                    "owner": "owner",
                    "repository": "repo",
                    "tag": "v1.2.3",
                    "target": "0123456789abcdef0123456789abcdef01234567",
                    "message": "release 1.2.3"
                }),
            ),
            (
                r#"{"operation":"tag_create","repository":"owner/repo","tag":"v1.2.3","target":"0123456789abcdef0123456789abcdef01234567"}"#,
                serde_json::json!({
                    "operation": "tag_create",
                    "owner": "owner",
                    "repository": "repo",
                    "tag": "v1.2.3",
                    "target": "0123456789abcdef0123456789abcdef01234567",
                    "message": null
                }),
            ),
            (
                r#"{"operation":"tag_delete","repository":"owner/repo","tag":"v1.2.3"}"#,
                serde_json::json!({
                    "operation": "tag_delete",
                    "owner": "owner",
                    "repository": "repo",
                    "tag": "v1.2.3"
                }),
            ),
        ];

        for (document, expected) in documents {
            let request = serde_json::from_str::<Request>(document).expect("request should parse");
            let operation = request.prepare(true).expect("request should prepare");
            assert_eq!(
                serde_json::to_value(operation).expect("operation should serialize"),
                expected
            );
        }
    }

    #[test]
    fn rejects_invalid_refs_tags_and_shas() {
        let documents = [
            r#"{"operation":"ref_create","repository":"owner/repo","ref":"main","sha":"0123456789abcdef0123456789abcdef01234567"}"#,
            r#"{"operation":"ref_create","repository":"owner/repo","ref":"refs/heads/../main","sha":"0123456789abcdef0123456789abcdef01234567"}"#,
            r#"{"operation":"ref_create","repository":"owner/repo","ref":"refs/heads/release","sha":"abc"}"#,
            r#"{"operation":"tag_create","repository":"owner/repo","tag":"refs/tags/v1","target":"0123456789abcdef0123456789abcdef01234567"}"#,
            r#"{"operation":"tag_create","repository":"owner/repo","tag":"v1","target":"not-a-sha"}"#,
        ];

        for document in documents {
            let request = serde_json::from_str::<Request>(document).expect("request should parse");
            assert!(request.prepare(true).is_err());
        }
    }

    #[test]
    fn serializes_workflow_control_contracts() {
        let documents = [
            (
                r#"{"operation":"workflow_cancel","repository":"owner/repo","run_id":42}"#,
                serde_json::json!({
                    "operation": "workflow_cancel",
                    "owner": "owner",
                    "repository": "repo",
                    "run_id": 42
                }),
            ),
            (
                r#"{"operation":"workflow_rerun","repository":"owner/repo","run_id":42,"failed_only":true}"#,
                serde_json::json!({
                    "operation": "workflow_rerun",
                    "owner": "owner",
                    "repository": "repo",
                    "run_id": 42,
                    "failed_only": true
                }),
            ),
            (
                r#"{"operation":"workflow_enable","repository":"owner/repo","workflow":"ci.yml"}"#,
                serde_json::json!({
                    "operation": "workflow_enable",
                    "owner": "owner",
                    "repository": "repo",
                    "workflow": "ci.yml"
                }),
            ),
            (
                r#"{"operation":"workflow_disable","repository":"owner/repo","workflow":"ci.yml"}"#,
                serde_json::json!({
                    "operation": "workflow_disable",
                    "owner": "owner",
                    "repository": "repo",
                    "workflow": "ci.yml"
                }),
            ),
        ];

        for (document, expected) in documents {
            let request = serde_json::from_str::<Request>(document).expect("request should parse");
            let operation = request.prepare(true).expect("request should prepare");
            assert_eq!(
                serde_json::to_value(operation).expect("operation should serialize"),
                expected
            );
        }
    }

    #[test]
    fn rejects_unsafe_workflow_identifier() {
        let request = serde_json::from_str::<Request>(
            r#"{"operation":"workflow_dispatch","repository":"owner/repo","workflow":"../release.yml","ref":"main"}"#,
        )
        .expect("request should parse");
        assert!(request.prepare(true).is_err());
    }

    #[test]
    fn rejects_unsafe_workflow_ref() {
        for reference in ["../main", "refs//heads/main", "release.lock", "feature@{1}"] {
            let document = serde_json::json!({
                "operation": "workflow_dispatch",
                "repository": "owner/repo",
                "workflow": "release.yml",
                "ref": reference
            });
            let request =
                serde_json::from_value::<Request>(document).expect("request should parse");
            assert!(request.prepare(true).is_err());
        }
    }

    #[test]
    fn rejects_too_many_workflow_inputs() {
        let inputs = (0..26)
            .map(|index| format!(r#""input{index}":"value""#))
            .collect::<Vec<_>>()
            .join(",");
        let document = format!(
            r#"{{"operation":"workflow_dispatch","repository":"owner/repo","workflow":"release.yml","ref":"main","inputs":{{{inputs}}}}}"#
        );
        let request = serde_json::from_str::<Request>(&document).expect("request should parse");
        assert!(request.prepare(true).is_err());
    }

    #[test]
    fn rejects_invalid_repository_dispatch_event_type() {
        for event_type in ["", "line\nbreak"] {
            let document = serde_json::json!({
                "operation": "repository_dispatch",
                "repository": "owner/repo",
                "event_type": event_type
            });
            let request =
                serde_json::from_value::<Request>(document).expect("request should parse");
            assert!(request.prepare(true).is_err());
        }
    }

    #[test]
    fn rejects_too_many_repository_dispatch_properties() {
        let client_payload = (0..11)
            .map(|index| (format!("key{index}"), serde_json::json!(index)))
            .collect::<std::collections::BTreeMap<_, _>>();
        let document = serde_json::json!({
            "operation": "repository_dispatch",
            "repository": "owner/repo",
            "event_type": "test",
            "client_payload": client_payload
        });
        let request = serde_json::from_value::<Request>(document).expect("request should parse");
        assert!(request.prepare(true).is_err());
    }

    #[test]
    fn rejects_oversized_repository_dispatch_payload() {
        let document = serde_json::json!({
            "operation": "repository_dispatch",
            "repository": "owner/repo",
            "event_type": "test",
            "client_payload": {"value": "x".repeat(65_536)}
        });
        let request = serde_json::from_value::<Request>(document).expect("request should parse");
        assert!(request.prepare(true).is_err());
    }

    #[test]
    fn rejects_zero_workflow_run_id() {
        for operation in ["workflow_cancel", "workflow_rerun"] {
            let document = serde_json::json!({
                "operation": operation,
                "repository": "owner/repo",
                "run_id": 0
            });
            assert!(serde_json::from_value::<Request>(document).is_err());
        }
    }
}
