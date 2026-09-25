use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::{env, fs};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::model::{MAX_COMMIT_MESSAGE_BYTES, Operation, Repository};

const CONFIG_ENV: &str = "PUKBOT_CONFIG";
const TRAILER_KEY: &str = "Co-authored-by";
const MAX_AGENT_KEY_BYTES: usize = 64;
const MAX_IDENTITY_BYTES: usize = 256;
const GITHUB_PREFIXES: [&str; 6] = [
    "https://github.com/",
    "http://github.com/",
    "https://www.github.com/",
    "ssh://git@github.com/",
    "git@github.com:",
    "github.com/",
];

const BUILT_IN_AGENTS: [(&str, &str, &str); 4] = [
    ("claude", "Claude", "noreply@anthropic.com"),
    ("codex", "Codex", "codex@openai.com"),
    (
        "copilot",
        "Copilot",
        "198982749+Copilot@users.noreply.github.com",
    ),
    ("cursor", "Cursor", "cursoragent@cursor.com"),
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    pub name: String,
    pub email: String,
}

impl Agent {
    fn trailer(&self) -> String {
        format!("{TRAILER_KEY}: {} <{}>", self.name, self.email)
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    agents: BTreeMap<String, Agent>,
    #[serde(default)]
    repositories: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct Member {
    pub agent: String,
    pub name: String,
    pub email: String,
    pub trailer: String,
    pub source: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Assignment {
    pub repository: String,
    pub members: Vec<Member>,
}

#[derive(Debug, Serialize)]
pub struct Removal {
    pub agent: String,
    pub removed: bool,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(error).with_context(|| format!("failed to read {}", path.display()));
            }
        };
        let config: Self = serde_json::from_str(&contents)
            .with_context(|| format!("failed to decode {}", path.display()))?;
        config
            .validate()
            .with_context(|| format!("{} is invalid", path.display()))?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let directory = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(directory)
            .with_context(|| format!("failed to create {}", directory.display()))?;
        let mut file = tempfile::NamedTempFile::new_in(directory)
            .with_context(|| format!("failed to stage {}", path.display()))?;
        serde_json::to_writer_pretty(&mut file, self).context("failed to encode the config")?;
        writeln!(file).context("failed to encode the config")?;
        file.persist(path)
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    pub fn agents(&self) -> Vec<Member> {
        let mut keys = BUILT_IN_AGENTS
            .iter()
            .map(|(key, ..)| (*key).to_owned())
            .collect::<Vec<_>>();
        keys.extend(self.agents.keys().cloned());
        keys.sort();
        keys.dedup();
        keys.iter().filter_map(|key| self.member(key)).collect()
    }

    pub fn assignment(&self, repository: &Repository) -> Result<Assignment> {
        let members = match self.assigned_key(repository)? {
            Some(key) => self.repositories[&key]
                .iter()
                .map(|agent| {
                    self.member(agent)
                        .with_context(|| format!("unknown agent {agent}"))
                })
                .collect::<Result<Vec<_>>>()?,
            None => Vec::new(),
        };
        Ok(Assignment {
            repository: repository_url(repository),
            members,
        })
    }

    pub fn trailers(&self, repository: &Repository) -> Result<Vec<String>> {
        Ok(self
            .assignment(repository)?
            .members
            .into_iter()
            .map(|member| member.trailer)
            .collect())
    }

    pub fn assign(&mut self, repository: &Repository, agents: Vec<String>) -> Result<()> {
        for agent in &agents {
            if self.member(agent).is_none() {
                bail!("unknown agent {agent}; run `pukbot crew agents` or define it first");
            }
        }
        let mut agents = agents;
        let mut seen = std::collections::BTreeSet::new();
        agents.retain(|agent| seen.insert(agent.clone()));
        self.clear(repository)?;
        self.repositories.insert(repository_url(repository), agents);
        Ok(())
    }

    pub fn clear(&mut self, repository: &Repository) -> Result<bool> {
        let mut removed = false;
        while let Some(key) = self.assigned_key(repository)? {
            self.repositories.remove(&key);
            removed = true;
        }
        Ok(removed)
    }

    pub fn define(&mut self, key: &str, agent: Agent) -> Result<()> {
        validate_agent(key, &agent)?;
        self.agents.insert(key.to_owned(), agent);
        Ok(())
    }

    pub fn forget(&mut self, key: &str) -> Result<Removal> {
        if !self.agents.contains_key(key) {
            bail!("agent {key} is not defined in the config");
        }
        if built_in(key).is_none() {
            let users = self
                .repositories
                .iter()
                .filter(|(_, agents)| agents.iter().any(|agent| agent == key))
                .map(|(repository, _)| repository.as_str())
                .collect::<Vec<_>>();
            if !users.is_empty() {
                bail!(
                    "agent {key} is still assigned to {}; reassign those repositories first",
                    users.join(", ")
                );
            }
        }
        self.agents.remove(key);
        Ok(Removal {
            agent: key.to_owned(),
            removed: true,
        })
    }

    fn member(&self, key: &str) -> Option<Member> {
        let (agent, source) = match self.agents.get(key) {
            Some(agent) => (agent.clone(), "config"),
            None => (built_in(key)?, "built-in"),
        };
        Some(Member {
            agent: key.to_owned(),
            trailer: agent.trailer(),
            name: agent.name,
            email: agent.email,
            source,
        })
    }

    fn assigned_key(&self, repository: &Repository) -> Result<Option<String>> {
        for key in self.repositories.keys() {
            if same_repository(&parse_repository(key)?, repository) {
                return Ok(Some(key.clone()));
            }
        }
        Ok(None)
    }

    fn validate(&self) -> Result<()> {
        for (key, agent) in &self.agents {
            validate_agent(key, agent)?;
        }
        let mut repositories: Vec<Repository> = Vec::new();
        for (key, agents) in &self.repositories {
            let repository = parse_repository(key)?;
            if repositories
                .iter()
                .any(|existing| same_repository(existing, &repository))
            {
                bail!("repository {repository} is listed more than once");
            }
            for agent in agents {
                if self.member(agent).is_none() {
                    bail!("repository {key} uses unknown agent {agent}");
                }
            }
            repositories.push(repository);
        }
        Ok(())
    }
}

pub fn config_path() -> Result<PathBuf> {
    resolve_config_path(|name| env::var_os(name), cfg!(windows))
}

pub fn load() -> Result<Config> {
    Config::load(&config_path()?)
}

pub fn attach(operation: &mut Operation) -> Result<()> {
    let target = match operation {
        Operation::CommitCreate {
            owner, repository, ..
        }
        | Operation::WikiPublish {
            owner, repository, ..
        }
        | Operation::PullRequestMerge {
            owner, repository, ..
        }
        | Operation::StackMerge {
            owner, repository, ..
        } => Repository {
            owner: owner.clone(),
            name: repository.clone(),
        },
        _ => return Ok(()),
    };
    let trailers = load()?.trailers(&target)?;
    sign(operation, &trailers)
}

pub fn merge_message(repository: &Repository) -> Result<Option<String>> {
    let trailers = load()?.trailers(repository)?;
    Ok((!trailers.is_empty()).then(|| trailers.join("\n")))
}

pub fn parse_repository(value: &str) -> Result<Repository> {
    let trimmed = value.trim();
    let path = GITHUB_PREFIXES
        .iter()
        .find_map(|prefix| strip_prefix_ignore_case(trimmed, prefix))
        .unwrap_or(trimmed)
        .trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    path.parse::<Repository>()
        .map_err(|error| anyhow!("{value}: {error}"))
}

fn sign(operation: &mut Operation, trailers: &[String]) -> Result<()> {
    if trailers.is_empty() {
        return Ok(());
    }
    match operation {
        Operation::CommitCreate { message, .. } | Operation::WikiPublish { message, .. } => {
            *message = append_trailers(message, trailers);
            if message.len() > MAX_COMMIT_MESSAGE_BYTES {
                bail!("commit message with its trailers exceeds {MAX_COMMIT_MESSAGE_BYTES} bytes");
            }
        }
        Operation::PullRequestMerge { commit_message, .. }
        | Operation::StackMerge { commit_message, .. } => {
            *commit_message = Some(trailers.join("\n"));
        }
        _ => {}
    }
    Ok(())
}

fn append_trailers(message: &str, trailers: &[String]) -> String {
    let missing = trailers
        .iter()
        .map(String::as_str)
        .filter(|trailer| {
            !message
                .lines()
                .any(|line| line.trim().eq_ignore_ascii_case(trailer))
        })
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return message.to_owned();
    }
    let block = missing.join("\n");
    let body = message.trim_end();
    if body.is_empty() {
        return block;
    }
    let separator = if ends_with_trailer_block(body) {
        "\n"
    } else {
        "\n\n"
    };
    let ending = if message.ends_with('\n') { "\n" } else { "" };
    format!("{body}{separator}{block}{ending}")
}

fn ends_with_trailer_block(body: &str) -> bool {
    body.rsplit_once("\n\n").is_some_and(|(_, paragraph)| {
        paragraph.lines().all(|line| {
            line.split_once(": ").is_some_and(|(token, value)| {
                !token.is_empty()
                    && !value.trim().is_empty()
                    && token
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric() || character == '-')
            })
        })
    })
}

fn resolve_config_path(
    variable: impl Fn(&str) -> Option<OsString>,
    windows: bool,
) -> Result<PathBuf> {
    let variable = |name: &str| variable(name).filter(|value| !value.is_empty());
    if let Some(path) = variable(CONFIG_ENV) {
        return Ok(PathBuf::from(path));
    }
    let base = if windows {
        variable("APPDATA")
            .map(PathBuf::from)
            .context("APPDATA or PUKBOT_CONFIG is required")?
    } else if let Some(config) = variable("XDG_CONFIG_HOME") {
        PathBuf::from(config)
    } else {
        variable("HOME")
            .map(|home| PathBuf::from(home).join(".config"))
            .context("HOME, XDG_CONFIG_HOME, or PUKBOT_CONFIG is required")?
    };
    Ok(base.join("pukbot").join("config.json"))
}

fn built_in(key: &str) -> Option<Agent> {
    BUILT_IN_AGENTS
        .iter()
        .find(|(candidate, ..)| *candidate == key)
        .map(|(_, name, email)| Agent {
            name: (*name).to_owned(),
            email: (*email).to_owned(),
        })
}

fn validate_agent(key: &str, agent: &Agent) -> Result<()> {
    if key.is_empty()
        || key.len() > MAX_AGENT_KEY_BYTES
        || !key.starts_with(|character: char| character.is_ascii_alphanumeric())
        || !key.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '_' | '.')
        })
    {
        bail!("agent key {key:?} must use lowercase letters, digits, '-', '_', or '.'");
    }
    let name = agent.name.trim();
    if name.is_empty()
        || name != agent.name
        || agent.name.len() > MAX_IDENTITY_BYTES
        || agent.name.contains(['<', '>', '\n', '\r'])
    {
        bail!("agent {key} needs a single-line name without angle brackets");
    }
    let email = &agent.email;
    let valid_email = email.len() <= MAX_IDENTITY_BYTES
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && !domain.is_empty() && !domain.contains('@')
        })
        && !email.contains(|character: char| {
            character.is_whitespace() || matches!(character, '<' | '>')
        });
    if !valid_email {
        bail!("agent {key} needs an email address such as bot@example.com");
    }
    Ok(())
}

fn repository_url(repository: &Repository) -> String {
    format!(
        "https://github.com/{}/{}",
        repository.owner, repository.name
    )
}

fn same_repository(left: &Repository, right: &Repository) -> bool {
    left.owner.eq_ignore_ascii_case(&right.owner) && left.name.eq_ignore_ascii_case(&right.name)
}

fn strip_prefix_ignore_case<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .map(|_| &value[prefix.len()..])
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::num::NonZeroU64;
    use std::path::PathBuf;

    use super::{Agent, Config, append_trailers, parse_repository, resolve_config_path, sign};
    use crate::model::{Operation, Repository};

    const CURSOR: &str = "Co-authored-by: Cursor <cursoragent@cursor.com>";

    fn repository(slug: &str) -> Repository {
        slug.parse().expect("repository should parse")
    }

    fn config(json: &str) -> Config {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let path = directory.path().join("config.json");
        std::fs::write(&path, json).expect("config should be written");
        Config::load(&path).expect("config should load")
    }

    #[test]
    fn normalizes_repository_spellings() {
        for value in [
            "pulkitxm/pukbot",
            "https://github.com/pulkitxm/pukbot",
            "https://github.com/pulkitxm/pukbot/",
            "HTTPS://GitHub.com/pulkitxm/pukbot.git",
            "git@github.com:pulkitxm/pukbot.git",
            "ssh://git@github.com/pulkitxm/pukbot",
            "github.com/pulkitxm/pukbot",
        ] {
            assert_eq!(
                parse_repository(value).expect("repository should parse"),
                repository("pulkitxm/pukbot"),
                "{value}"
            );
        }
        assert!(parse_repository("https://gitlab.com/pulkitxm/pukbot").is_err());
        assert!(parse_repository("pukbot").is_err());
    }

    #[test]
    fn resolves_trailers_by_repository_url_ignoring_case() {
        let config = config(
            r#"{
                "agents": {"acme": {"name": "Acme Bot", "email": "bot@acme.dev"}},
                "repositories": {"https://github.com/PulkitXM/Pukbot": ["cursor", "acme"]}
            }"#,
        );
        assert_eq!(
            config
                .trailers(&repository("pulkitxm/pukbot"))
                .expect("trailers should resolve"),
            vec![
                CURSOR.to_owned(),
                "Co-authored-by: Acme Bot <bot@acme.dev>".to_owned()
            ]
        );
        assert!(
            config
                .trailers(&repository("pulkitxm/other"))
                .expect("trailers should resolve")
                .is_empty()
        );
    }

    #[test]
    fn custom_agents_override_built_ins() {
        let config = config(
            r#"{
                "agents": {"cursor": {"name": "Cursor Agent", "email": "agent@cursor.test"}},
                "repositories": {"owner/repo": ["cursor"]}
            }"#,
        );
        assert_eq!(
            config
                .trailers(&repository("owner/repo"))
                .expect("trailers should resolve"),
            vec!["Co-authored-by: Cursor Agent <agent@cursor.test>".to_owned()]
        );
    }

    #[test]
    fn rejects_invalid_configs() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let path = directory.path().join("config.json");
        for json in [
            r#"{"repositories": {"owner/repo": ["nobody"]}}"#,
            r#"{"repositories": {"owner/repo": [], "https://github.com/OWNER/REPO": []}}"#,
            r#"{"repositories": {"not a repo": []}}"#,
            r#"{"agents": {"Bad Key": {"name": "Bot", "email": "bot@example.com"}}}"#,
            r#"{"agents": {"bot": {"name": "Bot <x>", "email": "bot@example.com"}}}"#,
            r#"{"agents": {"bot": {"name": "Bot", "email": "not-an-email"}}}"#,
            r#"{"agent": {}}"#,
        ] {
            std::fs::write(&path, json).expect("config should be written");
            assert!(Config::load(&path).is_err(), "{json}");
        }
    }

    #[test]
    fn missing_config_is_empty() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let config =
            Config::load(&directory.path().join("absent.json")).expect("config should load");
        assert!(
            config
                .trailers(&repository("owner/repo"))
                .expect("trailers should resolve")
                .is_empty()
        );
    }

    #[test]
    fn saves_canonical_urls_and_round_trips() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let path = directory.path().join("nested").join("config.json");
        let mut config = Config::default();
        config
            .define(
                "acme",
                Agent {
                    name: "Acme Bot".to_owned(),
                    email: "bot@acme.dev".to_owned(),
                },
            )
            .expect("agent should be defined");
        config
            .assign(
                &repository("pulkitxm/pukbot"),
                vec!["cursor".to_owned(), "acme".to_owned(), "cursor".to_owned()],
            )
            .expect("repository should be assigned");
        config.save(&path).expect("config should be saved");
        let saved = std::fs::read_to_string(&path).expect("config should be readable");
        assert!(saved.contains(
            "\"https://github.com/pulkitxm/pukbot\": [\n      \"cursor\",\n      \"acme\"\n    ]"
        ));
        assert!(saved.ends_with("}\n"));

        let mut reloaded = Config::load(&path).expect("config should reload");
        reloaded
            .assign(&repository("PULKITXM/pukbot"), vec!["claude".to_owned()])
            .expect("repository should be reassigned");
        assert_eq!(reloaded.repositories.len(), 1);
        assert!(reloaded.forget("acme").is_ok());
        assert!(
            reloaded
                .clear(&repository("pulkitxm/pukbot"))
                .expect("repository should clear")
        );
        assert!(reloaded.repositories.is_empty());
    }

    #[test]
    fn refuses_to_forget_assigned_custom_agents() {
        let mut config = config(
            r#"{
                "agents": {"acme": {"name": "Acme Bot", "email": "bot@acme.dev"}},
                "repositories": {"owner/repo": ["acme"]}
            }"#,
        );
        assert!(config.forget("acme").is_err());
        assert!(config.forget("cursor").is_err());
        assert!(
            config
                .assign(&repository("owner/repo"), vec!["ghost".to_owned()])
                .is_err()
        );
    }

    #[test]
    fn appends_trailers_as_a_separate_paragraph() {
        assert_eq!(
            append_trailers("feat: add thing", &[CURSOR.to_owned()]),
            format!("feat: add thing\n\n{CURSOR}")
        );
        assert_eq!(
            append_trailers("feat: add thing\n\nBody text.\n", &[CURSOR.to_owned()]),
            format!("feat: add thing\n\nBody text.\n\n{CURSOR}\n")
        );
        assert_eq!(append_trailers("", &[CURSOR.to_owned()]), CURSOR);
    }

    #[test]
    fn joins_an_existing_trailer_block_without_duplicates() {
        let message = "fix: bug\n\nSigned-off-by: Dev <dev@example.com>";
        assert_eq!(
            append_trailers(message, &[CURSOR.to_owned()]),
            format!("{message}\n{CURSOR}")
        );
        let signed = format!("fix: bug\n\n{}", CURSOR.to_ascii_lowercase());
        assert_eq!(append_trailers(&signed, &[CURSOR.to_owned()]), signed);
    }

    #[test]
    fn signs_commits_and_merges() {
        let mut commit = Operation::CommitCreate {
            owner: "owner".to_owned(),
            repository: "repo".to_owned(),
            branch: "main".to_owned(),
            message: "feat: thing".to_owned(),
            files: Vec::new(),
            as_app: true,
        };
        sign(&mut commit, &[CURSOR.to_owned()]).expect("commit should be signed");
        let Operation::CommitCreate { message, .. } = commit else {
            unreachable!()
        };
        assert_eq!(message, format!("feat: thing\n\n{CURSOR}"));

        let mut merge = Operation::PullRequestMerge {
            owner: "owner".to_owned(),
            repository: "repo".to_owned(),
            number: NonZeroU64::MIN,
            as_app: false,
            delete_branch: false,
            auto_merge: false,
            commit_message: None,
        };
        sign(
            &mut merge,
            &[CURSOR.to_owned(), "Co-authored-by: B <b@b.dev>".to_owned()],
        )
        .expect("merge should be signed");
        let Operation::PullRequestMerge { commit_message, .. } = merge else {
            unreachable!()
        };
        assert_eq!(
            commit_message.as_deref(),
            Some("Co-authored-by: Cursor <cursoragent@cursor.com>\nCo-authored-by: B <b@b.dev>")
        );
    }

    #[test]
    fn rejects_messages_pushed_over_the_limit() {
        let mut commit = Operation::WikiPublish {
            owner: "owner".to_owned(),
            repository: "repo".to_owned(),
            message: "x".repeat(crate::model::MAX_COMMIT_MESSAGE_BYTES),
            source_ref: None,
            source_path: None,
            delete: Vec::new(),
            replace: false,
        };
        assert!(sign(&mut commit, &[CURSOR.to_owned()]).is_err());
    }

    #[test]
    fn resolves_the_config_path() {
        let lookup = |values: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                values
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| OsString::from(value))
            }
        };
        assert_eq!(
            resolve_config_path(
                lookup(&[("PUKBOT_CONFIG", "/sync/pukbot.json"), ("HOME", "/h")]),
                false
            )
            .expect("path should resolve"),
            PathBuf::from("/sync/pukbot.json")
        );
        assert_eq!(
            resolve_config_path(lookup(&[("XDG_CONFIG_HOME", "/x"), ("HOME", "/h")]), false)
                .expect("path should resolve"),
            PathBuf::from("/x/pukbot/config.json")
        );
        assert_eq!(
            resolve_config_path(lookup(&[("XDG_CONFIG_HOME", ""), ("HOME", "/h")]), false)
                .expect("path should resolve"),
            PathBuf::from("/h/.config/pukbot/config.json")
        );
        assert_eq!(
            resolve_config_path(lookup(&[("APPDATA", "/a"), ("HOME", "/h")]), true)
                .expect("path should resolve"),
            PathBuf::from("/a/pukbot/config.json")
        );
        assert!(resolve_config_path(lookup(&[]), false).is_err());
    }
}
