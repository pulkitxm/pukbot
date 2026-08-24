use std::collections::HashSet;
use std::fmt;
use std::num::NonZeroU64;
use std::str::FromStr;

use anyhow::{Result, bail};
use clap::ValueEnum;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::media;

pub const MAX_BODY_BYTES: usize = 40_000;

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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommentDocument {
    pub body: String,
    #[serde(default)]
    pub media: Vec<media::MediaInput>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    #[serde(rename = "comment_create")]
    Create {
        repository: Repository,
        number: NonZeroU64,
        #[serde(flatten)]
        document: CommentDocument,
    },
    #[serde(rename = "comment_edit")]
    Edit {
        repository: Repository,
        comment_id: NonZeroU64,
        #[serde(flatten)]
        document: CommentDocument,
    },
    #[serde(rename = "comment_delete")]
    Delete {
        repository: Repository,
        comment_id: NonZeroU64,
    },
    #[serde(rename = "comment_react")]
    React {
        repository: Repository,
        comment_id: NonZeroU64,
        reaction: Reaction,
    },
}

impl Request {
    pub fn prepare(self, dry_run: bool) -> Result<Operation> {
        match self {
            Self::Create {
                repository,
                number,
                document,
            } => Ok(Operation::Create {
                owner: repository.owner,
                repository: repository.name,
                number,
                body: prepare_document(document, dry_run)?,
            }),
            Self::Edit {
                repository,
                comment_id,
                document,
            } => Ok(Operation::Edit {
                owner: repository.owner,
                repository: repository.name,
                comment_id,
                body: prepare_document(document, dry_run)?,
            }),
            Self::Delete {
                repository,
                comment_id,
            } => Ok(Operation::Delete {
                owner: repository.owner,
                repository: repository.name,
                comment_id,
            }),
            Self::React {
                repository,
                comment_id,
                reaction,
            } => Ok(Operation::React {
                owner: repository.owner,
                repository: repository.name,
                comment_id,
                reaction,
            }),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum Operation {
    #[serde(rename = "comment_create")]
    Create {
        owner: String,
        repository: String,
        number: NonZeroU64,
        body: String,
    },
    #[serde(rename = "comment_edit")]
    Edit {
        owner: String,
        repository: String,
        comment_id: NonZeroU64,
        body: String,
    },
    #[serde(rename = "comment_delete")]
    Delete {
        owner: String,
        repository: String,
        comment_id: NonZeroU64,
    },
    #[serde(rename = "comment_react")]
    React {
        owner: String,
        repository: String,
        comment_id: NonZeroU64,
        reaction: Reaction,
    },
}

impl Operation {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Create { .. } => "comment_create",
            Self::Edit { .. } => "comment_edit",
            Self::Delete { .. } => "comment_delete",
            Self::React { .. } => "comment_react",
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

fn validate_body(body: &str) -> Result<()> {
    if body.trim().is_empty() {
        bail!("comment body cannot be empty");
    }
    if body.len() > MAX_BODY_BYTES {
        bail!("comment body exceeds {MAX_BODY_BYTES} bytes");
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
            Operation::React {
                reaction: Reaction::Eyes,
                ..
            }
        ));
    }
}
