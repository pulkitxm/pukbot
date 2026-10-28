#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

const END: &str = "2026-10-28T16:15:00+05:30";
const BACKUP: &str = "backup/feat-redate-20261028-161500";

fn git_command(directory: &Path, arguments: &[&str], date: Option<&str>) -> Command {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Sam Dev")
        .env("GIT_AUTHOR_EMAIL", "sam@example.com")
        .env("GIT_COMMITTER_NAME", "Robin Merge")
        .env("GIT_COMMITTER_EMAIL", "robin@example.com");
    if let Some(date) = date {
        command
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date);
    }
    command
}

fn git(directory: &Path, arguments: &[&str]) -> String {
    git_at(directory, arguments, None)
}

fn git_at(directory: &Path, arguments: &[&str], date: Option<&str>) -> String {
    let output = git_command(directory, arguments, date)
        .output()
        .expect("git should run");
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git output should be UTF-8")
        .trim()
        .to_owned()
}

fn commit(directory: &Path, file: &str, message: &str, date: &str) {
    fs::write(directory.join(file), message).expect("file should be written");
    git(directory, &["add", file]);
    git_at(directory, &["commit", "-q", "-m", message], Some(date));
}

fn repository() -> TempDir {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let path = directory.path();
    git(path, &["init", "-q", "-b", "main"]);
    commit(
        path,
        "base.txt",
        "chore: start",
        "2026-10-26T10:00:00+05:30",
    );
    git(path, &["checkout", "-q", "-b", "feat"]);
    commit(
        path,
        "one.txt",
        "feat: first\n\nBody line.\n\nCo-authored-by: Cursor <cursoragent@cursor.com>",
        "2026-10-27T21:01:00+05:30",
    );
    commit(path, "two.txt", "feat: second", "2026-10-27T21:32:00+05:30");
    commit(
        path,
        "three.txt",
        "feat: third",
        "2026-10-27T22:30:00+05:30",
    );
    directory
}

fn pukbot(directory: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pukbot"))
        .args(arguments)
        .current_dir(directory)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("pukbot should run")
}

fn redate(directory: &Path, extra: &[&str]) -> Output {
    let mut arguments = vec!["--json", "history", "redate", "--end-at", END];
    arguments.extend_from_slice(extra);
    pukbot(directory, &arguments)
}

fn history(directory: &Path, range: &str) -> Vec<String> {
    git(
        directory,
        &[
            "log",
            "--reverse",
            "--format=%T|%an|%ae|%aI|%cn|%ce|%cI|%B%x00",
            range,
        ],
    )
    .split('\0')
    .map(|entry| entry.trim().to_owned())
    .filter(|entry| !entry.is_empty())
    .collect()
}

fn without_dates(entries: &[String]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| {
            let fields = entry.splitn(8, '|').collect::<Vec<_>>();
            [
                fields[0], fields[1], fields[2], fields[4], fields[5], fields[7],
            ]
            .join("|")
        })
        .collect()
}

#[test]
fn moves_the_tip_to_the_end_time_and_keeps_the_gaps() {
    let directory = repository();
    let path = directory.path();
    fs::write(path.join("three.txt"), "staged").expect("file should be written");
    git(path, &["add", "three.txt"]);
    fs::write(path.join("base.txt"), "unstaged").expect("file should be written");
    let main = git(path, &["rev-parse", "main"]);
    let previous_head = git(path, &["rev-parse", "feat"]);
    let before = history(path, "main..feat");

    let output = redate(path, &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).expect("result should be JSON");

    assert_eq!(result["backupBranch"], BACKUP);
    assert_eq!(result["previousHead"], previous_head.as_str());
    assert_eq!(result["head"], git(path, &["rev-parse", "feat"]).as_str());
    assert_eq!(git(path, &["rev-parse", BACKUP]), previous_head);
    assert_eq!(git(path, &["rev-parse", "main"]), main);
    assert_eq!(
        git(
            path,
            &["log", "--reverse", "--format=%aI %cI", "main..feat"]
        ),
        [
            "2026-10-28T14:46:00+05:30 2026-10-28T14:46:00+05:30",
            "2026-10-28T15:17:00+05:30 2026-10-28T15:17:00+05:30",
            "2026-10-28T16:15:00+05:30 2026-10-28T16:15:00+05:30",
        ]
        .join("\n")
    );
    assert_eq!(
        without_dates(&history(path, "main..feat")),
        without_dates(&before)
    );
    assert_eq!(
        git(path, &["merge-base", "main", "feat"]),
        main,
        "the branch still starts from main"
    );
    assert_eq!(
        git(path, &["status", "--porcelain"]),
        "M base.txt\nM  three.txt"
    );
    assert_eq!(git(path, &["symbolic-ref", "--short", "HEAD"]), "feat");
}

#[test]
fn dry_run_reports_the_plan_without_moving_anything() {
    let directory = repository();
    let path = directory.path();
    let previous_head = git(path, &["rev-parse", "feat"]);

    let output = redate(path, &["--dry-run"]);
    assert!(output.status.success());
    let result: Value = serde_json::from_slice(&output.stdout).expect("result should be JSON");

    assert_eq!(result["dryRun"], true);
    assert!(result["head"].is_null());
    let dates = result["commits"]
        .as_array()
        .expect("commits should be listed")
        .iter()
        .map(|commit| commit["date"].as_str().expect("date").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        dates,
        [
            "2026-10-28T14:46:00+05:30",
            "2026-10-28T15:17:00+05:30",
            "2026-10-28T16:15:00+05:30"
        ]
    );
    assert_eq!(git(path, &["rev-parse", "feat"]), previous_head);
    assert_eq!(git(path, &["branch", "--list", "backup/*"]), "");
}

#[test]
fn keeps_merged_parents_from_the_base_branch() {
    let directory = repository();
    let path = directory.path();
    git(path, &["checkout", "-q", "main"]);
    commit(path, "later.txt", "fix: later", "2026-10-27T12:00:00+05:30");
    let main = git(path, &["rev-parse", "main"]);
    git(path, &["checkout", "-q", "feat"]);
    git_at(
        path,
        &["merge", "-q", "--no-ff", "-m", "merge main", "main"],
        Some("2026-10-27T22:45:00+05:30"),
    );

    let output = redate(path, &["--backup-branch", "saved/feat"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(git(path, &["rev-parse", "feat^2"]), main);
    assert_eq!(
        git(path, &["log", "-1", "--format=%aI", "feat"]),
        "2026-10-28T16:15:00+05:30"
    );
    assert_eq!(
        git(path, &["log", "-1", "--format=%aI", "feat^1"]),
        "2026-10-28T16:00:00+05:30"
    );
    assert_eq!(
        git(path, &["rev-parse", "saved/feat^{tree}"]),
        git(path, &["rev-parse", "feat^{tree}"])
    );
}

#[test]
fn refuses_unsafe_requests_without_moving_the_branch() {
    let directory = repository();
    let path = directory.path();
    let previous_head = git(path, &["rev-parse", "feat"]);

    let future = pukbot(
        path,
        &["history", "redate", "--end-at", "2999-01-01T00:00:00Z"],
    );
    assert!(!future.status.success());
    assert!(String::from_utf8_lossy(&future.stderr).contains("cannot be in the future"));

    let before_fork = pukbot(
        path,
        &["history", "redate", "--end-at", "2026-10-26T09:00:00+05:30"],
    );
    assert!(!before_fork.status.success());
    assert!(String::from_utf8_lossy(&before_fork.stderr).contains("would place commits before"));

    let empty = pukbot(path, &["history", "redate", "--branch", "main"]);
    assert!(!empty.status.success());
    assert!(String::from_utf8_lossy(&empty.stderr).contains("has no commits that are not on main"));

    git(path, &["branch", BACKUP, "main"]);
    let taken = redate(path, &[]);
    assert!(!taken.status.success());
    assert!(String::from_utf8_lossy(&taken.stdout).contains("already exists"));

    assert_eq!(git(path, &["rev-parse", "feat"]), previous_head);
}
