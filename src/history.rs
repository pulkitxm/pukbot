use std::collections::BTreeMap;
use std::io::Write as _;
use std::process::{Command, Output, Stdio};
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, FixedOffset, SecondsFormat, Utc};
use serde::Serialize;

const REFLOG_MESSAGE: &str = "pukbot history redate";

pub struct Redate<'a> {
    pub branch: Option<&'a str>,
    pub base: &'a str,
    pub end_at: &'a str,
    pub backup_branch: Option<&'a str>,
    pub dry_run: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub branch: String,
    pub base: String,
    pub previous_head: String,
    pub head: Option<String>,
    pub backup_branch: String,
    pub dry_run: bool,
    pub commits: Vec<Moved>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Moved {
    pub original: String,
    pub rewritten: Option<String>,
    pub subject: String,
    pub previous_date: String,
    pub date: String,
}

struct Identity {
    name: String,
    email: String,
    timestamp: i64,
    zone: String,
}

struct Original {
    id: String,
    tree: String,
    parents: Vec<String>,
    author: Identity,
    committer: Identity,
    message: Vec<u8>,
}

impl Original {
    fn subject(&self) -> String {
        String::from_utf8_lossy(&self.message)
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned()
    }
}

pub fn redate(request: &Redate<'_>) -> Result<Outcome> {
    let branch = match request.branch {
        Some(branch) => branch.to_owned(),
        None => current_branch()?,
    };
    let reference = format!("refs/heads/{branch}");
    let previous_head = git_text(&["rev-parse", "--verify", &format!("{reference}^{{commit}}")])
        .with_context(|| format!("local branch {branch} does not exist"))?;
    let base = git_text(&[
        "rev-parse",
        "--verify",
        &format!("{}^{{commit}}", request.base),
    ])
    .with_context(|| format!("base {} does not exist", request.base))?;
    let originals = branch_commits(&base, &previous_head)?;
    let Some(tip) = originals.last() else {
        bail!("{branch} has no commits that are not on {}", request.base);
    };
    let end = end_timestamp(request.end_at)?;
    let shift = end - tip.author.timestamp;
    if let Some(late) = originals
        .iter()
        .find(|commit| commit.author.timestamp > tip.author.timestamp)
    {
        bail!(
            "commit {} is dated after the branch tip, so it would land after --end-at",
            short(&late.id)
        );
    }
    let earliest = originals
        .iter()
        .map(|commit| commit.author.timestamp + shift)
        .min()
        .unwrap_or(end);
    ensure_after_fork(&base, &previous_head, earliest, request.base)?;
    let backup_branch = match request.backup_branch {
        Some(name) => name.to_owned(),
        None => default_backup(&branch, end, &tip.author.zone)?,
    };
    let backup_reference = free_branch_reference(&backup_branch)?;
    let mut moved = originals
        .iter()
        .map(|commit| {
            Ok(Moved {
                original: commit.id.clone(),
                rewritten: None,
                subject: commit.subject(),
                previous_date: display(commit.author.timestamp, &commit.author.zone)?,
                date: display(commit.author.timestamp + shift, &commit.author.zone)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut outcome = Outcome {
        branch,
        base: request.base.to_owned(),
        previous_head,
        head: None,
        backup_branch,
        dry_run: request.dry_run,
        commits: Vec::new(),
    };
    if request.dry_run {
        outcome.commits = moved;
        return Ok(outcome);
    }
    let rewritten = rewrite(&originals, shift)?;
    for (entry, commit) in moved.iter_mut().zip(&originals) {
        entry.rewritten = rewritten.get(&commit.id).cloned();
    }
    let head = rewritten
        .get(&tip.id)
        .context("the branch tip was not rewritten")?
        .clone();
    if git_text(&["rev-parse", &format!("{head}^{{tree}}")])? != tip.tree {
        bail!("the rewritten branch does not match the original content");
    }
    let transaction = format!(
        "create {backup_reference} {previous}\nupdate {reference} {head} {previous}\n",
        previous = outcome.previous_head
    );
    git(
        &["update-ref", "-m", REFLOG_MESSAGE, "--stdin"],
        transaction.as_bytes(),
    )
    .context("failed to move the branch and save its backup")?;
    outcome.head = Some(head);
    outcome.commits = moved;
    Ok(outcome)
}

fn ensure_after_fork(base: &str, head: &str, earliest: i64, base_name: &str) -> Result<()> {
    let fork_point = git_text(&["merge-base", base, head])?;
    let fork_time = git_text(&["show", "-s", "--format=%ct", &fork_point])?
        .parse::<i64>()
        .context("Git returned an invalid commit date")?;
    if earliest < fork_time {
        bail!(
            "--end-at would place commits before {} on {base_name}",
            short(&fork_point)
        );
    }
    Ok(())
}

fn free_branch_reference(branch: &str) -> Result<String> {
    let reference = format!("refs/heads/{branch}");
    git(&["check-ref-format", &reference], b"")
        .with_context(|| format!("{branch} is not a valid branch name"))?;
    if git(&["show-ref", "--verify", "--quiet", &reference], b"").is_ok() {
        bail!("the backup branch {branch} already exists");
    }
    Ok(reference)
}

fn rewrite(originals: &[Original], shift: i64) -> Result<BTreeMap<String, String>> {
    let mut rewritten = BTreeMap::new();
    for commit in originals {
        let date = format!(
            "@{} {}",
            commit.author.timestamp + shift,
            commit.author.zone
        );
        let mut arguments = vec!["commit-tree".to_owned(), commit.tree.clone()];
        for parent in &commit.parents {
            arguments.push("-p".to_owned());
            arguments.push(rewritten.get(parent).unwrap_or(parent).clone());
        }
        let mut command = git_command(&arguments.iter().map(String::as_str).collect::<Vec<_>>());
        command
            .env("GIT_AUTHOR_NAME", &commit.author.name)
            .env("GIT_AUTHOR_EMAIL", &commit.author.email)
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_NAME", &commit.committer.name)
            .env("GIT_COMMITTER_EMAIL", &commit.committer.email)
            .env("GIT_COMMITTER_DATE", &date);
        let id = text(run(&mut command, &commit.message)?)?;
        rewritten.insert(commit.id.clone(), id);
    }
    Ok(rewritten)
}

fn branch_commits(base: &str, head: &str) -> Result<Vec<Original>> {
    git_text(&[
        "rev-list",
        "--reverse",
        "--topo-order",
        &format!("{base}..{head}"),
    ])?
    .lines()
    .map(read_commit)
    .collect()
}

fn read_commit(id: &str) -> Result<Original> {
    let raw = git(&["cat-file", "commit", id], b"")?;
    let split = raw
        .windows(2)
        .position(|pair| pair == b"\n\n")
        .context("Git returned a commit without a message separator")?;
    let headers =
        std::str::from_utf8(&raw[..split]).context("Git returned non-UTF-8 commit headers")?;
    let mut tree = None;
    let mut parents = Vec::new();
    let mut author = None;
    let mut committer = None;
    for line in headers.lines() {
        if let Some(value) = line.strip_prefix("tree ") {
            tree = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("parent ") {
            parents.push(value.to_owned());
        } else if let Some(value) = line.strip_prefix("author ") {
            author = Some(parse_identity(value)?);
        } else if let Some(value) = line.strip_prefix("committer ") {
            committer = Some(parse_identity(value)?);
        } else if line.starts_with("encoding ") {
            bail!("commit {} uses a custom encoding", short(id));
        }
    }
    Ok(Original {
        id: id.to_owned(),
        tree: tree.context("Git returned a commit without a tree")?,
        parents,
        author: author.context("Git returned a commit without an author")?,
        committer: committer.context("Git returned a commit without a committer")?,
        message: raw[split + 2..].to_vec(),
    })
}

fn parse_identity(value: &str) -> Result<Identity> {
    let (person, date) = value
        .rsplit_once("> ")
        .context("Git returned an invalid identity")?;
    let (name, email) = person
        .split_once(" <")
        .or_else(|| person.strip_prefix('<').map(|email| ("", email)))
        .context("Git returned an invalid identity")?;
    let (timestamp, zone) = date
        .split_once(' ')
        .context("Git returned an invalid identity date")?;
    offset(zone)?;
    Ok(Identity {
        name: name.to_owned(),
        email: email.to_owned(),
        timestamp: timestamp
            .parse()
            .context("Git returned an invalid identity date")?,
        zone: zone.to_owned(),
    })
}

fn offset(zone: &str) -> Result<FixedOffset> {
    let (sign, digits) = match zone.as_bytes().first() {
        Some(b'+') => (1, &zone[1..]),
        Some(b'-') => (-1, &zone[1..]),
        _ => bail!("Git returned an invalid time zone {zone}"),
    };
    if digits.len() != 4 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        bail!("Git returned an invalid time zone {zone}");
    }
    let hours = digits[..2].parse::<i32>()?;
    let minutes = digits[2..].parse::<i32>()?;
    FixedOffset::east_opt(sign * (hours * 3600 + minutes * 60))
        .with_context(|| format!("Git returned an invalid time zone {zone}"))
}

fn local_time(timestamp: i64, zone: &str) -> Result<DateTime<FixedOffset>> {
    Ok(DateTime::from_timestamp(timestamp, 0)
        .context("date is out of range")?
        .with_timezone(&offset(zone)?))
}

fn display(timestamp: i64, zone: &str) -> Result<String> {
    Ok(local_time(timestamp, zone)?.to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn default_backup(branch: &str, end: i64, zone: &str) -> Result<String> {
    Ok(format!(
        "backup/{branch}-redate-{}",
        local_time(end, zone)?.format("%Y%m%d-%H%M%S")
    ))
}

fn end_timestamp(value: &str) -> Result<i64> {
    let now = DateTime::<Utc>::from(SystemTime::now()).timestamp();
    if value == "now" {
        return Ok(now);
    }
    let end = DateTime::parse_from_rfc3339(value)
        .context("--end-at must be now or an RFC 3339 date")?
        .timestamp();
    if end > now {
        bail!("--end-at cannot be in the future");
    }
    Ok(end)
}

fn current_branch() -> Result<String> {
    git_text(&["symbolic-ref", "--quiet", "--short", "HEAD"])
        .context("HEAD is detached, so pass --branch")
}

fn short(id: &str) -> &str {
    id.get(..7).unwrap_or(id)
}

fn git_command(arguments: &[&str]) -> Command {
    let mut command = Command::new("git");
    command
        .args(arguments)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn git(arguments: &[&str], input: &[u8]) -> Result<Vec<u8>> {
    run(&mut git_command(arguments), input)
}

fn git_text(arguments: &[&str]) -> Result<String> {
    text(git(arguments, b"")?)
}

fn text(output: Vec<u8>) -> Result<String> {
    Ok(String::from_utf8(output)
        .context("Git returned non-UTF-8 output")?
        .trim()
        .to_owned())
}

fn run(command: &mut Command, input: &[u8]) -> Result<Vec<u8>> {
    let mut child = command.spawn().context("failed to launch Git")?;
    child
        .stdin
        .take()
        .context("failed to open Git input")?
        .write_all(input)?;
    let Output {
        status,
        stdout,
        stderr,
    } = child.wait_with_output().context("failed to wait for Git")?;
    if !status.success() {
        bail!("Git failed: {}", String::from_utf8_lossy(&stderr).trim());
    }
    Ok(stdout)
}

#[cfg(test)]
mod tests {
    use super::{offset, parse_identity};

    #[test]
    fn parses_identity_with_offset() {
        let identity = parse_identity("Jane Doe <jane@example.com> 1793116860 +0530")
            .expect("identity should parse");
        assert_eq!(identity.name, "Jane Doe");
        assert_eq!(identity.email, "jane@example.com");
        assert_eq!(identity.timestamp, 1_793_116_860);
        assert_eq!(identity.zone, "+0530");
    }

    #[test]
    fn rejects_malformed_time_zones() {
        assert!(offset("0530").is_err());
        assert!(offset("+53").is_err());
        assert_eq!(
            offset("-0800")
                .expect("offset should parse")
                .local_minus_utc(),
            -8 * 3600
        );
    }
}
