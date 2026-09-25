#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::{env, fs};

use serde_json::Value;
use tempfile::TempDir;

const CURSOR: &str = "Co-authored-by: Cursor <cursoragent@cursor.com>";
const ACME: &str = "Co-authored-by: Acme Bot <bot@acme.dev>";

const PULL_REQUEST_BEHAVIOR: &str = r#"case "$*" in
    "repo view --json nameWithOwner --jq .nameWithOwner")
        printf '%s\n' 'owner/repo'
        ;;
    "api --method GET repos/owner/repo/pulls/7")
        printf '%s\n' '{"title":"feat: thing","head":{"ref":"feature","sha":"aaa"}}'
        ;;
    "api --method GET repos/owner/repo/pulls/12")
        printf '%s\n' '{"stack":{"number":42},"head":{"ref":"top","sha":"bbb"}}'
        ;;
    "api --method PUT repos/owner/repo/pulls/7/merge --input -")
        body=$(cat)
        printf 'body=%s\n' "$body" >>"$PUKBOT_FAKE_GH_LOG"
        ;;
    "api --method PUT repos/owner/repo/pulls/12/merge-async --input -")
        body=$(cat)
        printf 'body=%s\n' "$body" >>"$PUKBOT_FAKE_GH_LOG"
        printf '%s\n' '{"status":"merged","details":{"message":"merged","sha":"abc"}}'
        ;;
    "stack view --json")
        printf '%s\n' '{"branches":[{"pr":{"number":12}}]}'
        ;;
    "pr merge 7 --repo owner/repo --auto --squash"*)
        ;;
    *) exit 91 ;;
esac"#;

struct Harness {
    directory: TempDir,
    executable_directory: PathBuf,
    log: PathBuf,
    config: PathBuf,
}

impl Harness {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let executable_directory = directory.path().join("bin");
        fs::create_dir(&executable_directory).expect("fake executable directory should be created");
        let executable = executable_directory.join("gh");
        let script = format!(
            "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$*\" >>\"$PUKBOT_FAKE_GH_LOG\"\n{PULL_REQUEST_BEHAVIOR}\n"
        );
        fs::write(&executable, script).expect("fake gh should be written");
        let mut permissions = fs::metadata(&executable)
            .expect("fake gh metadata should be available")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("fake gh should be executable");
        let log = directory.path().join("gh.log");
        let config = directory.path().join("sync").join("pukbot.json");
        Self {
            directory,
            executable_directory,
            log,
            config,
        }
    }

    fn run(&self, arguments: &[&str]) -> Output {
        self.run_in(self.directory.path(), arguments)
    }

    fn run_in(&self, directory: &Path, arguments: &[&str]) -> Output {
        let mut paths = vec![self.executable_directory.clone()];
        if let Some(original) = env::var_os("PATH") {
            paths.extend(env::split_paths(&original));
        }
        Command::new(env!("CARGO_BIN_EXE_pukbot"))
            .args(arguments)
            .current_dir(directory)
            .env(
                "PATH",
                env::join_paths(paths).expect("fake PATH should be valid"),
            )
            .env("PUKBOT_FAKE_GH_LOG", &self.log)
            .env("PUKBOT_CONFIG", &self.config)
            .output()
            .expect("pukbot should run")
    }

    fn log(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn assign_cursor_and_acme(&self) {
        assert_success(&self.run(&[
            "crew",
            "define",
            "acme",
            "--name",
            "Acme Bot",
            "--email",
            "bot@acme.dev",
        ]));
        assert_success(&self.run(&[
            "crew",
            "assign",
            "cursor",
            "acme",
            "--repo",
            "https://github.com/owner/repo",
        ]));
    }
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn json_output(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout should contain JSON")
}

fn git(directory: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .status()
        .expect("git should run");
    assert!(status.success());
}

#[test]
fn manages_a_shareable_manifest() {
    let harness = Harness::new();

    let path = harness.run(&["crew", "path", "--json"]);
    assert_success(&path);
    assert_eq!(
        json_output(&path)["path"],
        harness.config.to_string_lossy().as_ref()
    );

    harness.assign_cursor_and_acme();
    let saved: Value = serde_json::from_str(
        &fs::read_to_string(&harness.config).expect("manifest should be written"),
    )
    .expect("manifest should be JSON");
    assert_eq!(
        saved,
        serde_json::json!({
            "agents": {"acme": {"name": "Acme Bot", "email": "bot@acme.dev"}},
            "repositories": {"https://github.com/owner/repo": ["cursor", "acme"]}
        })
    );

    let show = harness.run(&["crew", "show"]);
    assert_success(&show);
    assert_eq!(
        String::from_utf8_lossy(&show.stdout),
        format!("{CURSOR}\n{ACME}\n")
    );
    assert!(
        harness
            .log()
            .contains("repo view --json nameWithOwner --jq .nameWithOwner")
    );

    let agents = harness.run(&["crew", "agents", "--json"]);
    assert_success(&agents);
    let agents = json_output(&agents);
    let sources = agents
        .as_array()
        .expect("agents should be an array")
        .iter()
        .map(|agent| {
            (
                agent["agent"].as_str().unwrap_or_default().to_owned(),
                agent["source"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    assert!(sources.contains(&("acme".to_owned(), "config".to_owned())));
    assert!(sources.contains(&("cursor".to_owned(), "built-in".to_owned())));

    let forget = harness.run(&["crew", "forget", "acme"]);
    assert!(!forget.status.success());

    let clear = harness.run(&["crew", "clear", "--repo", "OWNER/REPO", "--json"]);
    assert_success(&clear);
    assert_eq!(json_output(&clear)["members"], serde_json::json!([]));
    assert_success(&harness.run(&["crew", "forget", "acme"]));

    let unknown = harness.run(&["crew", "assign", "ghost", "--repo", "owner/repo"]);
    assert!(!unknown.status.success());
}

#[test]
fn signs_local_squash_merges() {
    let harness = Harness::new();
    harness.assign_cursor_and_acme();

    let output = harness.run(&["pr", "merge", "7", "--repo", "owner/repo", "--yes"]);
    assert_success(&output);
    let expected = serde_json::json!({
        "commit_message": format!("{CURSOR}\n{ACME}"),
        "commit_title": "feat: thing (#7)",
        "merge_method": "squash"
    });
    assert!(
        harness.log().contains(&format!("body={expected}\n")),
        "{}",
        harness.log()
    );
}

#[test]
fn leaves_unassigned_merges_untouched() {
    let harness = Harness::new();

    let output = harness.run(&["pr", "merge", "7", "--repo", "owner/repo", "--yes"]);
    assert_success(&output);
    assert!(harness.log().contains(
        "body={\"commit_message\":\"\",\"commit_title\":\"feat: thing (#7)\",\"merge_method\":\"squash\"}\n"
    ));
}

#[test]
fn signs_auto_merges() {
    let harness = Harness::new();
    harness.assign_cursor_and_acme();

    let output = harness.run(&[
        "pr",
        "merge",
        "7",
        "--repo",
        "owner/repo",
        "--auto",
        "--yes",
    ]);
    assert_success(&output);
    assert!(harness.log().contains(&format!(
        "pr merge 7 --repo owner/repo --auto --squash --body {CURSOR}\n{ACME}\n"
    )));
}

#[test]
fn signs_stack_merges() {
    let harness = Harness::new();
    harness.assign_cursor_and_acme();

    let output = harness.run(&["stack", "merge", "--yes"]);
    assert_success(&output);
    let expected = serde_json::json!({
        "commit_message": format!("{CURSOR}\n{ACME}"),
        "merge_action": "direct_merge",
        "merge_method": "squash",
        "sha": "bbb"
    });
    assert!(
        harness.log().contains(&format!("body={expected}\n")),
        "{}",
        harness.log()
    );

    let dry_run = harness.run(&[
        "stack-api",
        "merge",
        "12",
        "--repo",
        "owner/repo",
        "--dry-run",
    ]);
    assert_success(&dry_run);
    assert_eq!(
        json_output(&dry_run)["commit_message"],
        format!("{CURSOR}\n{ACME}")
    );
}

#[test]
fn signs_app_merges_and_commits() {
    let harness = Harness::new();
    harness.assign_cursor_and_acme();

    let merge = harness.run(&[
        "pr",
        "merge",
        "7",
        "--repo",
        "owner/repo",
        "--as-app",
        "--dry-run",
    ]);
    assert_success(&merge);
    assert_eq!(
        json_output(&merge)["commit_message"],
        format!("{CURSOR}\n{ACME}")
    );

    let repository = harness.directory.path().join("repository");
    fs::create_dir(&repository).expect("repository should be created");
    git(&repository, &["init", "-q"]);
    fs::write(repository.join("file.txt"), "hello\n").expect("file should be written");
    git(&repository, &["add", "file.txt"]);
    let commit = harness.run_in(
        &repository,
        &[
            "commit",
            "create",
            "--repo",
            "owner/repo",
            "--branch",
            "main",
            "--message",
            "feat: add file\n\nSigned-off-by: Dev <dev@example.com>",
            "--as-app",
            "--dry-run",
        ],
    );
    assert_success(&commit);
    assert_eq!(
        json_output(&commit)["message"],
        format!("feat: add file\n\nSigned-off-by: Dev <dev@example.com>\n{CURSOR}\n{ACME}")
    );

    let other = harness.run_in(
        &repository,
        &[
            "commit",
            "create",
            "--repo",
            "owner/other",
            "--branch",
            "main",
            "--message",
            "feat: add file",
            "--dry-run",
        ],
    );
    assert_success(&other);
    assert_eq!(json_output(&other)["message"], "feat: add file");
}

#[test]
fn reports_an_invalid_manifest() {
    let harness = Harness::new();
    fs::create_dir_all(
        harness
            .config
            .parent()
            .expect("manifest should have a parent"),
    )
    .expect("manifest directory should be created");
    fs::write(
        &harness.config,
        r#"{"repositories": {"owner/repo": ["ghost"]}}"#,
    )
    .expect("manifest should be written");

    let output = harness.run(&["pr", "merge", "7", "--repo", "owner/repo", "--dry-run"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown agent ghost"));
    assert_eq!(harness.log(), "");
}
