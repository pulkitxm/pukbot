use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::num::NonZeroU64;
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use base64::Engine as _;
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
pub const MAX_RELEASE_ASSET_BYTES: usize = 40_000;
pub const MAX_DEPLOYMENT_PAYLOAD_CHARS: usize = 65_535;
pub const MAX_WIKI_DELETE_PATHS: usize = 500;
pub const MAX_BATCH_TARGETS: usize = 50;

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

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseLatest {
    True,
    False,
    Legacy,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentState {
    Error,
    Failure,
    Inactive,
    InProgress,
    Queued,
    Pending,
    Success,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum LockReason {
    OffTopic,
    TooHeated,
    Resolved,
    Spam,
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
    IssueBatch {
        repository: Repository,
        numbers: Vec<NonZeroU64>,
        comment: Option<String>,
        #[serde(default)]
        add_labels: Vec<String>,
        #[serde(default)]
        remove_labels: Vec<String>,
        #[serde(default)]
        add_assignees: Vec<String>,
        #[serde(default)]
        remove_assignees: Vec<String>,
        #[serde(default)]
        close: bool,
        #[serde(default)]
        lock: bool,
        #[serde(default)]
        unlock: bool,
        lock_reason: Option<LockReason>,
        #[serde(default)]
        allow_partial: bool,
    },
    PullRequestCreate {
        repository: Repository,
        title: String,
        body: Option<String>,
        head: String,
        base: String,
        #[serde(default)]
        draft: bool,
        #[serde(default)]
        as_app: bool,
    },
    PullRequestEdit {
        repository: Repository,
        number: NonZeroU64,
        title: Option<String>,
        body: Option<String>,
        base: Option<String>,
        #[serde(default)]
        as_app: bool,
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
        #[serde(default)]
        as_app: bool,
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
        #[serde(default)]
        as_app: bool,
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
        #[serde(default)]
        as_app: bool,
    },
    PullRequestBatch {
        repository: Repository,
        numbers: Vec<NonZeroU64>,
        comment: Option<String>,
        #[serde(default)]
        add_labels: Vec<String>,
        #[serde(default)]
        remove_labels: Vec<String>,
        #[serde(default)]
        add_assignees: Vec<String>,
        #[serde(default)]
        remove_assignees: Vec<String>,
        #[serde(default)]
        close: bool,
        #[serde(default)]
        lock: bool,
        #[serde(default)]
        unlock: bool,
        lock_reason: Option<LockReason>,
        #[serde(default)]
        allow_partial: bool,
    },
    CommitCreate {
        repository: Repository,
        branch: String,
        message: String,
        files: Vec<CommitFileDocument>,
        #[serde(default)]
        as_app: bool,
    },
    WikiPublish {
        repository: Repository,
        message: String,
        source_ref: Option<String>,
        source_path: Option<String>,
        #[serde(default)]
        delete: Vec<String>,
        #[serde(default)]
        replace: bool,
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
    ReleaseCreate {
        repository: Repository,
        tag: String,
        name: Option<String>,
        body: Option<String>,
        target: Option<String>,
        #[serde(default)]
        draft: bool,
        #[serde(default)]
        prerelease: bool,
        #[serde(default)]
        generate_notes: bool,
    },
    ReleaseEdit {
        repository: Repository,
        release_id: NonZeroU64,
        tag: Option<String>,
        name: Option<String>,
        body: Option<String>,
        draft: Option<bool>,
        prerelease: Option<bool>,
        make_latest: Option<ReleaseLatest>,
    },
    ReleaseDelete {
        repository: Repository,
        release_id: NonZeroU64,
    },
    ReleaseAssetUpload {
        repository: Repository,
        release_id: NonZeroU64,
        name: String,
        label: Option<String>,
        content_type: String,
        content_base64: String,
    },
    DeploymentCreate {
        repository: Repository,
        #[serde(rename = "ref")]
        reference: String,
        environment: String,
        task: Option<String>,
        description: Option<String>,
        #[serde(default)]
        payload: BTreeMap<String, serde_json::Value>,
        #[serde(default)]
        auto_merge: bool,
        #[serde(default)]
        required_contexts: Vec<String>,
        #[serde(default)]
        transient_environment: bool,
        #[serde(default)]
        production_environment: bool,
    },
    DeploymentStatus {
        repository: Repository,
        deployment_id: NonZeroU64,
        state: DeploymentState,
        target_url: Option<String>,
        log_url: Option<String>,
        environment_url: Option<String>,
        description: Option<String>,
        #[serde(default)]
        auto_inactive: bool,
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
            } => {
                let body = prepare_document(document, &repository, dry_run)?;
                Ok(Operation::CreateComment {
                    owner: repository.owner,
                    repository: repository.name,
                    number,
                    body,
                })
            }
            Self::EditComment {
                repository,
                comment_id,
                document,
            } => {
                let body = prepare_document(document, &repository, dry_run)?;
                Ok(Operation::EditComment {
                    owner: repository.owner,
                    repository: repository.name,
                    comment_id,
                    body,
                })
            }
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
            Self::IssueBatch {
                repository,
                numbers,
                comment,
                add_labels,
                remove_labels,
                add_assignees,
                remove_assignees,
                close,
                lock,
                unlock,
                lock_reason,
                allow_partial,
            } => prepare_batch(
                repository,
                BatchInput {
                    numbers,
                    comment,
                    add_labels,
                    remove_labels,
                    add_assignees,
                    remove_assignees,
                    actions: BatchTargetActions {
                        close,
                        lock,
                        unlock,
                    },
                    lock_reason,
                    allow_partial,
                },
                false,
            ),
            Self::PullRequestCreate {
                repository,
                title,
                body,
                head,
                base,
                draft,
                as_app,
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
                    as_app,
                })
            }
            Self::PullRequestEdit {
                repository,
                number,
                title,
                body,
                base,
                as_app,
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
                    as_app,
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
            Self::PullRequestMerge {
                repository,
                number,
                as_app,
            } => Ok(Operation::PullRequestMerge {
                owner: repository.owner,
                repository: repository.name,
                number,
                as_app,
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
                as_app,
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
                    as_app,
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
            Self::PullRequestUpdateBranch {
                repository,
                number,
                as_app,
            } => Ok(Operation::PullRequestUpdateBranch {
                owner: repository.owner,
                repository: repository.name,
                number,
                as_app,
            }),
            Self::PullRequestBatch {
                repository,
                numbers,
                comment,
                add_labels,
                remove_labels,
                add_assignees,
                remove_assignees,
                close,
                lock,
                unlock,
                lock_reason,
                allow_partial,
            } => prepare_batch(
                repository,
                BatchInput {
                    numbers,
                    comment,
                    add_labels,
                    remove_labels,
                    add_assignees,
                    remove_assignees,
                    actions: BatchTargetActions {
                        close,
                        lock,
                        unlock,
                    },
                    lock_reason,
                    allow_partial,
                },
                true,
            ),
            Self::CommitCreate {
                repository,
                branch,
                message,
                files,
                as_app,
            } => {
                validate_branch(&branch)?;
                validate_commit_message(&message)?;
                Ok(Operation::CommitCreate {
                    owner: repository.owner,
                    repository: repository.name,
                    branch,
                    message,
                    files: prepare_commit_files(files)?,
                    as_app,
                })
            }
            Self::WikiPublish {
                repository,
                message,
                source_ref,
                source_path,
                delete,
                replace,
            } => {
                validate_commit_message(&message)?;
                validate_wiki_source(source_ref.as_deref(), source_path.as_deref())?;
                let delete = prepare_wiki_delete_paths(delete)?;
                if source_ref.is_none() && delete.is_empty() {
                    bail!("wiki publish requires a source or at least one deleted path");
                }
                if replace && !delete.is_empty() {
                    bail!("wiki publish cannot combine replace with deleted paths");
                }
                Ok(Operation::WikiPublish {
                    owner: repository.owner,
                    repository: repository.name,
                    message,
                    source_ref,
                    source_path,
                    delete,
                    replace,
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
            Self::ReleaseCreate {
                repository,
                tag,
                name,
                body,
                target,
                draft,
                prerelease,
                generate_notes,
            } => {
                validate_tag(&tag)?;
                validate_optional_release_name(name.as_deref())?;
                validate_optional_body(body.as_deref())?;
                if let Some(target) = &target {
                    validate_branch(target)?;
                }
                Ok(Operation::ReleaseCreate {
                    owner: repository.owner,
                    repository: repository.name,
                    tag,
                    name,
                    body,
                    target,
                    draft,
                    prerelease,
                    generate_notes,
                })
            }
            Self::ReleaseEdit {
                repository,
                release_id,
                tag,
                name,
                body,
                draft,
                prerelease,
                make_latest,
            } => {
                if tag.is_none()
                    && name.is_none()
                    && body.is_none()
                    && draft.is_none()
                    && prerelease.is_none()
                    && make_latest.is_none()
                {
                    bail!("release edit requires at least one changed field");
                }
                if let Some(tag) = &tag {
                    validate_tag(tag)?;
                }
                validate_optional_release_name(name.as_deref())?;
                validate_optional_body(body.as_deref())?;
                Ok(Operation::ReleaseEdit {
                    owner: repository.owner,
                    repository: repository.name,
                    release_id,
                    tag,
                    name,
                    body,
                    draft,
                    prerelease,
                    make_latest,
                })
            }
            Self::ReleaseDelete {
                repository,
                release_id,
            } => Ok(Operation::ReleaseDelete {
                owner: repository.owner,
                repository: repository.name,
                release_id,
            }),
            Self::ReleaseAssetUpload {
                repository,
                release_id,
                name,
                label,
                content_type,
                content_base64,
            } => {
                validate_release_asset(&name, label.as_deref(), &content_type, &content_base64)?;
                Ok(Operation::ReleaseAssetUpload {
                    owner: repository.owner,
                    repository: repository.name,
                    release_id,
                    name,
                    label,
                    content_type,
                    content_base64,
                })
            }
            Self::DeploymentCreate {
                repository,
                reference,
                environment,
                task,
                description,
                payload,
                auto_merge,
                required_contexts,
                transient_environment,
                production_environment,
            } => {
                validate_branch(&reference)?;
                validate_deployment_environment(&environment)?;
                validate_optional_deployment_text(task.as_deref(), "task", 255)?;
                validate_optional_deployment_text(description.as_deref(), "description", 512)?;
                validate_deployment_payload(&payload)?;
                validate_deployment_contexts(&required_contexts)?;
                Ok(Operation::DeploymentCreate {
                    owner: repository.owner,
                    repository: repository.name,
                    reference,
                    environment,
                    task,
                    description,
                    payload,
                    auto_merge,
                    required_contexts,
                    transient_environment,
                    production_environment,
                })
            }
            Self::DeploymentStatus {
                repository,
                deployment_id,
                state,
                target_url,
                log_url,
                environment_url,
                description,
                auto_inactive,
            } => {
                validate_optional_url(target_url.as_deref(), "target URL")?;
                validate_optional_url(log_url.as_deref(), "log URL")?;
                validate_optional_url(environment_url.as_deref(), "environment URL")?;
                validate_optional_deployment_text(description.as_deref(), "description", 512)?;
                Ok(Operation::DeploymentStatus {
                    owner: repository.owner,
                    repository: repository.name,
                    deployment_id,
                    state,
                    target_url,
                    log_url,
                    environment_url,
                    description,
                    auto_inactive,
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
    IssueBatch {
        owner: String,
        repository: String,
        numbers: Vec<NonZeroU64>,
        comment: Option<String>,
        add_labels: Vec<String>,
        remove_labels: Vec<String>,
        add_assignees: Vec<String>,
        remove_assignees: Vec<String>,
        close: bool,
        lock: bool,
        unlock: bool,
        lock_reason: Option<LockReason>,
        allow_partial: bool,
    },
    PullRequestCreate {
        owner: String,
        repository: String,
        title: String,
        body: Option<String>,
        head: String,
        base: String,
        draft: bool,
        as_app: bool,
    },
    PullRequestEdit {
        owner: String,
        repository: String,
        number: NonZeroU64,
        title: Option<String>,
        body: Option<String>,
        base: Option<String>,
        as_app: bool,
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
        as_app: bool,
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
        as_app: bool,
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
        as_app: bool,
    },
    PullRequestBatch {
        owner: String,
        repository: String,
        numbers: Vec<NonZeroU64>,
        comment: Option<String>,
        add_labels: Vec<String>,
        remove_labels: Vec<String>,
        add_assignees: Vec<String>,
        remove_assignees: Vec<String>,
        close: bool,
        lock: bool,
        unlock: bool,
        lock_reason: Option<LockReason>,
        allow_partial: bool,
    },
    CommitCreate {
        owner: String,
        repository: String,
        branch: String,
        message: String,
        files: Vec<CommitFile>,
        as_app: bool,
    },
    WikiPublish {
        owner: String,
        repository: String,
        message: String,
        source_ref: Option<String>,
        source_path: Option<String>,
        delete: Vec<String>,
        replace: bool,
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
    ReleaseCreate {
        owner: String,
        repository: String,
        tag: String,
        name: Option<String>,
        body: Option<String>,
        target: Option<String>,
        draft: bool,
        prerelease: bool,
        generate_notes: bool,
    },
    ReleaseEdit {
        owner: String,
        repository: String,
        release_id: NonZeroU64,
        tag: Option<String>,
        name: Option<String>,
        body: Option<String>,
        draft: Option<bool>,
        prerelease: Option<bool>,
        make_latest: Option<ReleaseLatest>,
    },
    ReleaseDelete {
        owner: String,
        repository: String,
        release_id: NonZeroU64,
    },
    ReleaseAssetUpload {
        owner: String,
        repository: String,
        release_id: NonZeroU64,
        name: String,
        label: Option<String>,
        content_type: String,
        content_base64: String,
    },
    DeploymentCreate {
        owner: String,
        repository: String,
        #[serde(rename = "ref")]
        reference: String,
        environment: String,
        task: Option<String>,
        description: Option<String>,
        payload: BTreeMap<String, serde_json::Value>,
        auto_merge: bool,
        required_contexts: Vec<String>,
        transient_environment: bool,
        production_environment: bool,
    },
    DeploymentStatus {
        owner: String,
        repository: String,
        deployment_id: NonZeroU64,
        state: DeploymentState,
        target_url: Option<String>,
        log_url: Option<String>,
        environment_url: Option<String>,
        description: Option<String>,
        auto_inactive: bool,
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
            Self::IssueBatch { .. } => "issue_batch",
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
            Self::PullRequestBatch { .. } => "pull_request_batch",
            Self::CommitCreate { .. } => "commit_create",
            Self::WikiPublish { .. } => "wiki_publish",
            Self::RepositoryDispatch { .. } => "repository_dispatch",
            Self::RefCreate { .. } => "ref_create",
            Self::RefDelete { .. } => "ref_delete",
            Self::TagCreate { .. } => "tag_create",
            Self::TagDelete { .. } => "tag_delete",
            Self::ReleaseCreate { .. } => "release_create",
            Self::ReleaseEdit { .. } => "release_edit",
            Self::ReleaseDelete { .. } => "release_delete",
            Self::ReleaseAssetUpload { .. } => "release_asset_upload",
            Self::DeploymentCreate { .. } => "deployment_create",
            Self::DeploymentStatus { .. } => "deployment_status",
            Self::WorkflowDispatch { .. } => "workflow_dispatch",
            Self::WorkflowCancel { .. } => "workflow_cancel",
            Self::WorkflowRerun { .. } => "workflow_rerun",
            Self::WorkflowEnable { .. } => "workflow_enable",
            Self::WorkflowDisable { .. } => "workflow_disable",
        }
    }
}

fn prepare_document(
    document: CommentDocument,
    repository: &Repository,
    dry_run: bool,
) -> Result<String> {
    let mut names = HashSet::new();
    let mut body = document.body;
    let target = media::UploadTarget::new(repository.slug());
    for item in document.media {
        if !names.insert(item.name.clone()) {
            bail!("duplicate media name: {}", item.name);
        }
        let placeholder = format!("{{{}}}", item.name);
        if !body.contains(&placeholder) {
            bail!("comment body does not contain media placeholder {placeholder}");
        }
        let markdown = media::resolve(&item, &target, dry_run)?;
        body = replace_media_placeholder(&body, &placeholder, &markdown);
    }
    validate_body(&body)?;
    Ok(body)
}

fn replace_media_placeholder(body: &str, placeholder: &str, markdown: &str) -> String {
    let mut rendered = String::with_capacity(body.len() + markdown.len());
    let mut remaining = body;
    while let Some(index) = remaining.find(placeholder) {
        let before = remaining[..index].trim_end_matches([' ', '\t']);
        rendered.push_str(before);
        if !rendered.is_empty() && !rendered.ends_with("\n\n") {
            if !rendered.ends_with('\n') {
                rendered.push('\n');
            }
            rendered.push('\n');
        }
        rendered.push_str(markdown);
        remaining = remaining[index + placeholder.len()..].trim_start_matches([' ', '\t']);
        if !remaining.is_empty() && !remaining.starts_with("\n\n") {
            rendered.push('\n');
            if !remaining.starts_with('\n') {
                rendered.push('\n');
            }
        }
    }
    rendered.push_str(remaining);
    rendered
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

struct BatchInput {
    numbers: Vec<NonZeroU64>,
    comment: Option<String>,
    add_labels: Vec<String>,
    remove_labels: Vec<String>,
    add_assignees: Vec<String>,
    remove_assignees: Vec<String>,
    actions: BatchTargetActions,
    lock_reason: Option<LockReason>,
    allow_partial: bool,
}

struct BatchTargetActions {
    close: bool,
    lock: bool,
    unlock: bool,
}

fn prepare_batch(
    repository: Repository,
    input: BatchInput,
    pull_request: bool,
) -> Result<Operation> {
    if input.numbers.is_empty() || input.numbers.len() > MAX_BATCH_TARGETS {
        bail!("batch operations require 1 to {MAX_BATCH_TARGETS} target numbers");
    }
    let unique_numbers = input.numbers.iter().collect::<HashSet<_>>();
    if unique_numbers.len() != input.numbers.len() {
        bail!("batch target numbers must be unique");
    }
    if let Some(comment) = &input.comment {
        validate_body(comment)?;
    }
    validate_batch_values(&input.add_labels, &input.remove_labels, "label")?;
    validate_batch_values(&input.add_assignees, &input.remove_assignees, "assignee")?;
    if input.lock_reason.is_some() && !input.actions.lock {
        bail!("batch lock reason requires lock");
    }
    if input.actions.lock && input.actions.unlock {
        bail!("batch cannot lock and unlock together");
    }
    if input.comment.is_none()
        && input.add_labels.is_empty()
        && input.remove_labels.is_empty()
        && input.add_assignees.is_empty()
        && input.remove_assignees.is_empty()
        && !input.actions.close
        && !input.actions.lock
        && !input.actions.unlock
    {
        bail!("batch operation requires at least one mutation");
    }
    if pull_request {
        Ok(Operation::PullRequestBatch {
            owner: repository.owner,
            repository: repository.name,
            numbers: input.numbers,
            comment: input.comment,
            add_labels: input.add_labels,
            remove_labels: input.remove_labels,
            add_assignees: input.add_assignees,
            remove_assignees: input.remove_assignees,
            close: input.actions.close,
            lock: input.actions.lock,
            unlock: input.actions.unlock,
            lock_reason: input.lock_reason,
            allow_partial: input.allow_partial,
        })
    } else {
        Ok(Operation::IssueBatch {
            owner: repository.owner,
            repository: repository.name,
            numbers: input.numbers,
            comment: input.comment,
            add_labels: input.add_labels,
            remove_labels: input.remove_labels,
            add_assignees: input.add_assignees,
            remove_assignees: input.remove_assignees,
            close: input.actions.close,
            lock: input.actions.lock,
            unlock: input.actions.unlock,
            lock_reason: input.lock_reason,
            allow_partial: input.allow_partial,
        })
    }
}

fn validate_batch_values(add: &[String], remove: &[String], name: &str) -> Result<()> {
    validate_values(add, name)?;
    validate_values(remove, name)?;
    let add_values = add.iter().collect::<HashSet<_>>();
    let remove_values = remove.iter().collect::<HashSet<_>>();
    if add_values.len() != add.len() || remove_values.len() != remove.len() {
        bail!("batch {name} values must be unique");
    }
    if add_values.iter().any(|value| remove_values.contains(value)) {
        bail!("batch cannot add and remove the same {name}");
    }
    Ok(())
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

fn validate_optional_release_name(name: Option<&str>) -> Result<()> {
    if let Some(name) = name {
        if name.trim().is_empty() || name.len() > 255 || name.chars().any(char::is_control) {
            bail!("release name must be between 1 and 255 bytes without control characters");
        }
    }
    Ok(())
}

fn validate_release_asset(
    name: &str,
    label: Option<&str>,
    content_type: &str,
    content_base64: &str,
) -> Result<()> {
    if name.trim().is_empty()
        || name.len() > 255
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
    {
        bail!("release asset name must be between 1 and 255 bytes without slashes or controls");
    }
    if let Some(label) = label {
        if label.trim().is_empty() || label.len() > 255 || label.chars().any(char::is_control) {
            bail!("release asset label must be between 1 and 255 bytes without controls");
        }
    }
    if content_type.trim().is_empty()
        || content_type.len() > 255
        || !content_type.contains('/')
        || content_type.chars().any(char::is_control)
    {
        bail!("release asset content type must be a valid MIME type");
    }
    let content = base64::engine::general_purpose::STANDARD
        .decode(content_base64)
        .context("release asset content must be valid base64")?;
    if content.is_empty() || content.len() > MAX_RELEASE_ASSET_BYTES {
        bail!("release asset must be between 1 and {MAX_RELEASE_ASSET_BYTES} bytes");
    }
    Ok(())
}

fn validate_deployment_environment(environment: &str) -> Result<()> {
    if environment.trim().is_empty()
        || environment.len() > 255
        || environment.chars().any(char::is_control)
    {
        bail!("deployment environment must be between 1 and 255 bytes without controls");
    }
    Ok(())
}

fn validate_optional_deployment_text(
    value: Option<&str>,
    name: &str,
    maximum: usize,
) -> Result<()> {
    if let Some(value) = value {
        if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
            bail!("deployment {name} must be between 1 and {maximum} bytes without controls");
        }
    }
    Ok(())
}

fn validate_deployment_payload(payload: &BTreeMap<String, serde_json::Value>) -> Result<()> {
    let payload_chars = serde_json::to_string(payload)?.chars().count();
    if payload_chars > MAX_DEPLOYMENT_PAYLOAD_CHARS {
        bail!("deployment payload exceeds {MAX_DEPLOYMENT_PAYLOAD_CHARS} characters when encoded");
    }
    Ok(())
}

fn validate_deployment_contexts(contexts: &[String]) -> Result<()> {
    validate_values(contexts, "deployment context")?;
    let unique = contexts.iter().collect::<HashSet<_>>();
    if unique.len() != contexts.len() {
        bail!("deployment contexts must be unique");
    }
    Ok(())
}

fn validate_optional_url(value: Option<&str>, name: &str) -> Result<()> {
    if let Some(value) = value {
        if value.len() > 2048
            || !(value.starts_with("https://") || value.starts_with("http://"))
            || value.chars().any(char::is_control)
        {
            bail!("deployment {name} must be an HTTP or HTTPS URL up to 2048 bytes");
        }
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

fn validate_wiki_source(source_ref: Option<&str>, source_path: Option<&str>) -> Result<()> {
    match (source_ref, source_path) {
        (Some(reference), Some(path)) => {
            validate_branch(reference)?;
            if path != "." {
                validate_commit_path(path)?;
            }
        }
        (None, None) => {}
        _ => bail!("wiki source ref and path must be provided together"),
    }
    Ok(())
}

fn prepare_wiki_delete_paths(paths: Vec<String>) -> Result<Vec<String>> {
    if paths.len() > MAX_WIKI_DELETE_PATHS {
        bail!("wiki publish supports at most {MAX_WIKI_DELETE_PATHS} deleted paths");
    }
    let mut seen = HashSet::new();
    for path in &paths {
        validate_commit_path(path)?;
        if !seen.insert(path.clone()) {
            bail!("duplicate wiki delete path: {path}");
        }
    }
    Ok(paths)
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
    use base64::Engine as _;

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
    fn renders_media_placeholders_as_separate_paragraphs() {
        let document = serde_json::json!({
            "operation": "comment_create",
            "repository": "owner/repo",
            "number": 1,
            "body": "{IMG1} readiness stays fixed. {IMG2} bounded rows stay fast.",
            "media": [
                {
                    "name": "IMG1",
                    "url": "https://example.com/readiness.png",
                    "alt": "stable readiness"
                },
                {
                    "name": "IMG2",
                    "url": "https://example.com/rows.png",
                    "alt": "bounded rows"
                }
            ]
        })
        .to_string();
        let request = serde_json::from_str::<Request>(&document).expect("request should parse");
        let operation = request.prepare(true).expect("request should prepare");
        match operation {
            Operation::CreateComment { body, .. } => assert_eq!(
                body,
                "![stable readiness](https://example.com/readiness.png)\n\nreadiness stays fixed.\n\n![bounded rows](https://example.com/rows.png)\n\nbounded rows stay fast."
            ),
            other => panic!("unexpected operation: {}", other.name()),
        }
    }

    #[test]
    fn preserves_existing_media_paragraph_boundaries() {
        let document = serde_json::json!({
            "operation": "comment_create",
            "repository": "owner/repo",
            "number": 1,
            "body": "result\n\n{IMG1}\n\ndone",
            "media": [{
                "name": "IMG1",
                "url": "https://example.com/result.png",
                "alt": "result"
            }]
        })
        .to_string();
        let request = serde_json::from_str::<Request>(&document).expect("request should parse");
        let operation = request.prepare(true).expect("request should prepare");
        match operation {
            Operation::CreateComment { body, .. } => assert_eq!(
                body,
                "result\n\n![result](https://example.com/result.png)\n\ndone"
            ),
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
    fn serializes_explicit_app_authorship() {
        let documents = [
            r#"{"operation":"pull_request_create","repository":"owner/repo","title":"title","head":"feature","base":"main","as_app":true}"#,
            r#"{"operation":"pull_request_edit","repository":"owner/repo","number":1,"title":"title","as_app":true}"#,
            r#"{"operation":"pull_request_merge","repository":"owner/repo","number":1,"as_app":true}"#,
            r#"{"operation":"pull_request_review","repository":"owner/repo","number":1,"event":"comment","as_app":true}"#,
            r#"{"operation":"pull_request_update_branch","repository":"owner/repo","number":1,"as_app":true}"#,
            r#"{"operation":"commit_create","repository":"owner/repo","branch":"main","message":"update","files":[{"path":"data.txt","content":"value"}],"as_app":true}"#,
        ];
        for document in documents {
            let request = serde_json::from_str::<Request>(document).expect("request should parse");
            let operation = request.prepare(true).expect("request should prepare");
            assert_eq!(
                serde_json::to_value(operation).expect("operation should serialize")["as_app"],
                true
            );
        }
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
            r#"{"operation":"issue_batch","repository":"owner/repo","numbers":[1,2],"comment":"updated","add_labels":["batch"],"close":true}"#,
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
            r#"{"operation":"pull_request_batch","repository":"owner/repo","numbers":[3,4],"remove_labels":["batch"],"lock":true,"lock_reason":"resolved","allow_partial":true}"#,
            r#"{"operation":"commit_create","repository":"owner/repo","branch":"main","message":"data: update roster","files":[{"path":"data/members.json","content":"[]"},{"path":"data/old.json","delete":true}]}"#,
            r#"{"operation":"wiki_publish","repository":"owner/repo","message":"docs: publish wiki","source_ref":"main","source_path":"wiki","delete":["Old.md"],"replace":false}"#,
            r#"{"operation":"repository_dispatch","repository":"owner/repo","event_type":"apt-release","client_payload":{"version":"1.2.3"}}"#,
            r#"{"operation":"ref_create","repository":"owner/repo","ref":"refs/heads/release","sha":"0123456789abcdef0123456789abcdef01234567"}"#,
            r#"{"operation":"ref_delete","repository":"owner/repo","ref":"refs/heads/release"}"#,
            r#"{"operation":"tag_create","repository":"owner/repo","tag":"v1.2.3","target":"0123456789abcdef0123456789abcdef01234567","message":"release 1.2.3"}"#,
            r#"{"operation":"tag_delete","repository":"owner/repo","tag":"v1.2.3"}"#,
            r#"{"operation":"release_create","repository":"owner/repo","tag":"v1.2.3","name":"Release 1.2.3","body":"notes","target":"main","draft":false,"prerelease":false,"generate_notes":true}"#,
            r#"{"operation":"release_edit","repository":"owner/repo","release_id":1,"name":"Release 1.2.3","draft":false,"make_latest":"true"}"#,
            r#"{"operation":"release_delete","repository":"owner/repo","release_id":1}"#,
            r#"{"operation":"release_asset_upload","repository":"owner/repo","release_id":1,"name":"checksums.txt","label":"checksums","content_type":"text/plain","content_base64":"aGVsbG8="}"#,
            r#"{"operation":"deployment_create","repository":"owner/repo","ref":"main","environment":"staging","payload":{"version":"1.2.3"},"required_contexts":[]}"#,
            r#"{"operation":"deployment_status","repository":"owner/repo","deployment_id":1,"state":"success","environment_url":"https://example.com"}"#,
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
    fn rejects_incomplete_wiki_source() {
        let request = serde_json::from_str::<Request>(
            r#"{"operation":"wiki_publish","repository":"owner/repo","message":"docs: publish wiki","source_ref":"main"}"#,
        )
        .expect("request should parse");
        assert!(request.prepare(true).is_err());
    }

    #[test]
    fn rejects_empty_wiki_publish() {
        let request = serde_json::from_str::<Request>(
            r#"{"operation":"wiki_publish","repository":"owner/repo","message":"docs: publish wiki"}"#,
        )
        .expect("request should parse");
        assert!(request.prepare(true).is_err());
    }

    #[test]
    fn rejects_wiki_delete_path_traversal() {
        let request = serde_json::from_str::<Request>(
            r#"{"operation":"wiki_publish","repository":"owner/repo","message":"docs: publish wiki","delete":["../Home.md"]}"#,
        )
        .expect("request should parse");
        assert!(request.prepare(true).is_err());
    }

    #[test]
    fn serializes_batch_contracts() {
        let request = serde_json::from_str::<Request>(
            r#"{"operation":"pull_request_batch","repository":"owner/repo","numbers":[1,2],"comment":"done","add_labels":["ready"],"remove_labels":[],"add_assignees":["owner"],"remove_assignees":[],"close":true,"lock":true,"lock_reason":"resolved","allow_partial":true}"#,
        )
        .expect("request should parse");
        assert_eq!(
            serde_json::to_value(request.prepare(true).expect("request should prepare"))
                .expect("operation should serialize"),
            serde_json::json!({
                "operation": "pull_request_batch",
                "owner": "owner",
                "repository": "repo",
                "numbers": [1, 2],
                "comment": "done",
                "add_labels": ["ready"],
                "remove_labels": [],
                "add_assignees": ["owner"],
                "remove_assignees": [],
                "close": true,
                "lock": true,
                "unlock": false,
                "lock_reason": "resolved",
                "allow_partial": true
            })
        );
    }

    #[test]
    fn rejects_invalid_batch_requests() {
        let documents = [
            r#"{"operation":"issue_batch","repository":"owner/repo","numbers":[],"close":true}"#,
            r#"{"operation":"issue_batch","repository":"owner/repo","numbers":[1,1],"close":true}"#,
            r#"{"operation":"issue_batch","repository":"owner/repo","numbers":[1]}"#,
            r#"{"operation":"issue_batch","repository":"owner/repo","numbers":[1],"lock_reason":"spam"}"#,
            r#"{"operation":"issue_batch","repository":"owner/repo","numbers":[1],"lock":true,"unlock":true}"#,
            r#"{"operation":"issue_batch","repository":"owner/repo","numbers":[1],"add_labels":["x"],"remove_labels":["x"]}"#,
            r#"{"operation":"pull_request_batch","repository":"owner/repo","numbers":[1],"comment":""}"#,
        ];
        for document in documents {
            let request = serde_json::from_str::<Request>(document).expect("request should parse");
            assert!(request.prepare(true).is_err(), "{document}");
        }
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
    fn serializes_release_contracts() {
        let documents = [
            (
                r#"{"operation":"release_create","repository":"owner/repo","tag":"v1.2.3","name":"Release 1.2.3","body":"notes","target":"main","generate_notes":true}"#,
                serde_json::json!({
                    "operation": "release_create",
                    "owner": "owner",
                    "repository": "repo",
                    "tag": "v1.2.3",
                    "name": "Release 1.2.3",
                    "body": "notes",
                    "target": "main",
                    "draft": false,
                    "prerelease": false,
                    "generate_notes": true
                }),
            ),
            (
                r#"{"operation":"release_edit","repository":"owner/repo","release_id":42,"tag":"v1.2.4","name":"Release 1.2.4","body":"updated","draft":false,"prerelease":true,"make_latest":"legacy"}"#,
                serde_json::json!({
                    "operation": "release_edit",
                    "owner": "owner",
                    "repository": "repo",
                    "release_id": 42,
                    "tag": "v1.2.4",
                    "name": "Release 1.2.4",
                    "body": "updated",
                    "draft": false,
                    "prerelease": true,
                    "make_latest": "legacy"
                }),
            ),
            (
                r#"{"operation":"release_delete","repository":"owner/repo","release_id":42}"#,
                serde_json::json!({
                    "operation": "release_delete",
                    "owner": "owner",
                    "repository": "repo",
                    "release_id": 42
                }),
            ),
            (
                r#"{"operation":"release_asset_upload","repository":"owner/repo","release_id":42,"name":"checksums.txt","label":"checksums","content_type":"text/plain","content_base64":"aGVsbG8="}"#,
                serde_json::json!({
                    "operation": "release_asset_upload",
                    "owner": "owner",
                    "repository": "repo",
                    "release_id": 42,
                    "name": "checksums.txt",
                    "label": "checksums",
                    "content_type": "text/plain",
                    "content_base64": "aGVsbG8="
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
    fn rejects_invalid_release_requests() {
        let documents = [
            r#"{"operation":"release_edit","repository":"owner/repo","release_id":1}"#,
            r#"{"operation":"release_create","repository":"owner/repo","tag":"refs/tags/v1"}"#,
            r#"{"operation":"release_asset_upload","repository":"owner/repo","release_id":1,"name":"../asset","content_type":"text/plain","content_base64":"aGVsbG8="}"#,
            r#"{"operation":"release_asset_upload","repository":"owner/repo","release_id":1,"name":"asset","content_type":"invalid","content_base64":"aGVsbG8="}"#,
            r#"{"operation":"release_asset_upload","repository":"owner/repo","release_id":1,"name":"asset","content_type":"text/plain","content_base64":"not-base64"}"#,
        ];

        for document in documents {
            let request = serde_json::from_str::<Request>(document).expect("request should parse");
            assert!(request.prepare(true).is_err());
        }

        let content_base64 = base64::engine::general_purpose::STANDARD.encode(vec![0; 40_001]);
        let document = serde_json::json!({
            "operation": "release_asset_upload",
            "repository": "owner/repo",
            "release_id": 1,
            "name": "asset.bin",
            "content_type": "application/octet-stream",
            "content_base64": content_base64
        });
        let request = serde_json::from_value::<Request>(document).expect("request should parse");
        assert!(request.prepare(true).is_err());
    }

    #[test]
    fn serializes_deployment_contracts() {
        let documents = [
            (
                r#"{"operation":"deployment_create","repository":"owner/repo","ref":"main","environment":"staging","task":"deploy","description":"staging deploy","payload":{"version":"1.2.3"},"auto_merge":false,"required_contexts":["ci"],"transient_environment":true,"production_environment":false}"#,
                serde_json::json!({
                    "operation": "deployment_create",
                    "owner": "owner",
                    "repository": "repo",
                    "ref": "main",
                    "environment": "staging",
                    "task": "deploy",
                    "description": "staging deploy",
                    "payload": {"version": "1.2.3"},
                    "auto_merge": false,
                    "required_contexts": ["ci"],
                    "transient_environment": true,
                    "production_environment": false
                }),
            ),
            (
                r#"{"operation":"deployment_status","repository":"owner/repo","deployment_id":42,"state":"in_progress","target_url":"https://example.com/target","log_url":"https://example.com/logs","environment_url":"https://example.com","description":"deploying","auto_inactive":true}"#,
                serde_json::json!({
                    "operation": "deployment_status",
                    "owner": "owner",
                    "repository": "repo",
                    "deployment_id": 42,
                    "state": "in_progress",
                    "target_url": "https://example.com/target",
                    "log_url": "https://example.com/logs",
                    "environment_url": "https://example.com",
                    "description": "deploying",
                    "auto_inactive": true
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
    fn rejects_invalid_deployment_requests() {
        let documents = [
            r#"{"operation":"deployment_create","repository":"owner/repo","ref":"../main","environment":"staging"}"#,
            r#"{"operation":"deployment_create","repository":"owner/repo","ref":"main","environment":""}"#,
            r#"{"operation":"deployment_create","repository":"owner/repo","ref":"main","environment":"staging","required_contexts":["ci","ci"]}"#,
            r#"{"operation":"deployment_status","repository":"owner/repo","deployment_id":1,"state":"success","log_url":"file:///tmp/log"}"#,
            r#"{"operation":"deployment_status","repository":"owner/repo","deployment_id":1,"state":"unknown"}"#,
        ];

        for document in documents {
            let request = serde_json::from_str::<Request>(document);
            assert!(
                request.is_err()
                    || request
                        .expect("request should parse")
                        .prepare(true)
                        .is_err(),
                "{document}"
            );
        }
    }

    #[test]
    fn rejects_oversized_deployment_payload() {
        let document = serde_json::json!({
            "operation": "deployment_create",
            "repository": "owner/repo",
            "ref": "main",
            "environment": "staging",
            "payload": {"value": "x".repeat(65_536)}
        });
        let request = serde_json::from_value::<Request>(document).expect("request should parse");
        assert!(request.prepare(true).is_err());
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
