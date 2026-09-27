#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::{env, fs};

use serde_json::Value;
use tempfile::TempDir;

const BEHAVIOR: &str = r#"case "$*" in
    "api --method GET repos/owner/repo/git/ref/heads/feature/x --jq .object.sha")
        printf '%s\n' 'base111'
        ;;
    "api --method GET repos/owner/repo/git/commits/base111 --jq .tree.sha")
        printf '%s\n' 'tree111'
        ;;
    "api --method POST repos/owner/repo/git/blobs --input - --jq .sha")
        body=$(cat)
        printf 'body=%s\n' "$body" >>"$PUKBOT_FAKE_GH_LOG"
        printf '%s\n' 'blob111'
        ;;
    "api --method POST repos/owner/repo/git/trees --input - --jq .sha")
        body=$(cat)
        printf 'body=%s\n' "$body" >>"$PUKBOT_FAKE_GH_LOG"
        printf '%s\n' 'tree222'
        ;;
    "api --method POST repos/owner/repo/git/commits --input - --jq .sha")
        body=$(cat)
        printf 'body=%s\n' "$body" >>"$PUKBOT_FAKE_GH_LOG"
        printf '%s\n' 'commit222'
        ;;
    "api --method PATCH repos/owner/repo/git/refs/heads/feature/x --input -")
        body=$(cat)
        printf 'body=%s\n' "$body" >>"$PUKBOT_FAKE_GH_LOG"
        ;;
    *) exit 91 ;;
esac"#;

struct Harness {
    directory: TempDir,
    executable_directory: PathBuf,
    log: PathBuf,
}

impl Harness {
    fn new() -> Self {
        Self::with_behavior(BEHAVIOR)
    }

    fn with_behavior(behavior: &str) -> Self {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let executable_directory = directory.path().join("bin");
        fs::create_dir(&executable_directory).expect("fake executable directory should be created");
        let executable = executable_directory.join("gh");
        let script = format!(
            "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$*\" >>\"$PUKBOT_FAKE_GH_LOG\"\n{behavior}\n"
        );
        fs::write(&executable, script).expect("fake gh should be written");
        set_mode(&executable, 0o755);
        let log = directory.path().join("gh.log");
        Self {
            directory,
            executable_directory,
            log,
        }
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
            .env("GIT_CONFIG_GLOBAL", self.directory.path().join("gitconfig"))
            .env("PUKBOT_FAKE_GH_LOG", &self.log)
            .env("PUKBOT_CONFIG", self.directory.path().join("config.json"))
            .output()
            .expect("pukbot should run")
    }
}

fn set_mode(path: &Path, mode: u32) {
    let mut permissions = fs::metadata(path)
        .expect("metadata should be available")
        .permissions();
    permissions.set_mode(mode);
    fs::set_permissions(path, permissions).expect("mode should be set");
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
fn commits_modes_binaries_and_workflows_through_the_user_session() {
    let harness = Harness::new();
    let repository = harness.directory.path().join("repository");
    fs::create_dir_all(repository.join(".github/workflows")).expect("directories should exist");
    git(&repository, &["init", "-q"]);
    let script = repository.join("bundle.sh");
    fs::write(&script, "#!/bin/sh\n").expect("script should be written");
    set_mode(&script, 0o755);
    fs::write(repository.join("logo.bin"), [0xff, 0x00, 0xfe]).expect("binary should be written");
    fs::write(repository.join(".github/workflows/ci.yml"), "on: push\n")
        .expect("workflow should be written");
    fs::write(repository.join("large.txt"), "x".repeat(70_000)).expect("file should be written");
    git(&repository, &["add", "-A"]);

    let output = harness.run_in(
        &repository,
        &[
            "commit",
            "create",
            "--repo",
            "owner/repo",
            "--branch",
            "feature/x",
            "--message",
            "feat: ship it",
            "--json",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(result["authoredBy"], "user");
    assert_eq!(
        result["resourceUrl"],
        "https://github.com/owner/repo/commit/commit222"
    );
    assert_eq!(result["localSync"]["synced"], false);
    assert!(
        result["localSync"]["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("left untouched"))
    );

    let log = fs::read_to_string(&harness.log).expect("fake gh log should exist");
    let bodies = log
        .lines()
        .filter_map(|line| line.strip_prefix("body="))
        .map(|body| serde_json::from_str::<Value>(body).expect("body should be JSON"))
        .collect::<Vec<_>>();
    assert!(bodies.contains(&serde_json::json!({"content": "/wD+", "encoding": "base64"})));
    assert!(bodies.contains(&serde_json::json!({"content": "#!/bin/sh\n", "encoding": "utf-8"})));
    let tree = bodies
        .iter()
        .find(|body| body.get("base_tree").is_some())
        .expect("tree request should be sent");
    assert_eq!(tree["base_tree"], "tree111");
    let mode = |path: &str| {
        tree["tree"]
            .as_array()
            .expect("tree entries should be an array")
            .iter()
            .find(|entry| entry["path"] == path)
            .map(|entry| entry["mode"].clone())
    };
    assert_eq!(mode("bundle.sh"), Some(Value::from("100755")));
    assert_eq!(
        mode(".github/workflows/ci.yml"),
        Some(Value::from("100644"))
    );
    assert_eq!(mode("large.txt"), Some(Value::from("100644")));
    assert!(bodies.contains(&serde_json::json!({
        "message": "feat: ship it",
        "tree": "tree222",
        "parents": ["base111"]
    })));
    assert!(bodies.contains(&serde_json::json!({"sha": "commit222", "force": false})));
}

#[test]
fn keeps_the_app_limits_for_app_commits() {
    let harness = Harness::new();
    let repository = harness.directory.path().join("repository");
    fs::create_dir(&repository).expect("repository should be created");
    git(&repository, &["init", "-q"]);
    fs::write(repository.join("large.txt"), "x".repeat(70_000)).expect("file should be written");
    git(&repository, &["add", "-A"]);

    let output = harness.run_in(
        &repository,
        &[
            "commit",
            "create",
            "--repo",
            "owner/repo",
            "--branch",
            "feature/x",
            "--message",
            "feat: ship it",
            "--as-app",
            "--dry-run",
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("app-authored commits"));
}

const EMPTY_BEHAVIOR: &str = r#"case "$*" in
    "api --method GET repos/owner/repo/git/ref/heads/main --jq .object.sha")
        printf '%s\n' 'gh: Git Repository is empty. (HTTP 409)' >&2
        exit 1
        ;;
    "api --method GET user")
        printf '%s\n' '{"login":"octocat","id":123,"name":"Test User","email":null}'
        ;;
    *) exit 91 ;;
esac"#;

fn git_output(directory: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .output()
        .expect("git should run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("output should be UTF-8")
        .trim()
        .to_owned()
}

fn empty_repository(harness: &Harness) -> (PathBuf, PathBuf) {
    let remote = harness.directory.path().join("remote");
    let repository = harness.directory.path().join("repository");
    fs::create_dir(&remote).expect("remote should be created");
    fs::create_dir(&repository).expect("repository should be created");
    git(&remote, &["init", "-q", "--bare", "-b", "main"]);
    git(&repository, &["init", "-q", "-b", "main"]);
    git(
        &repository,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/owner/repo.git",
        ],
    );
    let config = harness.directory.path().join("gitconfig");
    git(
        &repository,
        &[
            "config",
            "--file",
            config.to_str().expect("config path should be UTF-8"),
            &format!("url.{}.insteadOf", remote.display()),
            "https://github.com/owner/repo.git",
        ],
    );
    (repository, remote)
}

fn initial_arguments() -> Vec<&'static str> {
    vec![
        "commit",
        "create",
        "--repo",
        "owner/repo",
        "--branch",
        "main",
        "--message",
        "feat: initialize package",
        "--json",
    ]
}

#[test]
fn initializes_an_empty_repository_with_staged_content_and_modes() {
    let harness = Harness::with_behavior(EMPTY_BEHAVIOR);
    let (repository, remote) = empty_repository(&harness);
    fs::create_dir_all(repository.join("src")).expect("source directory should exist");
    fs::write(repository.join("src/main.py"), "print('hello')\n")
        .expect("source should be written");
    fs::write(repository.join("run.sh"), "#!/bin/sh\n").expect("script should be written");
    set_mode(&repository.join("run.sh"), 0o755);
    fs::write(repository.join("logo.bin"), [0xff, 0x00, 0xfe]).expect("binary should be written");
    std::os::unix::fs::symlink("src/main.py", repository.join("entry"))
        .expect("link should be created");
    git(&repository, &["add", "."]);
    fs::write(repository.join("src/main.py"), "unstaged change\n").expect("source should change");
    fs::write(repository.join("draft.txt"), "untracked\n").expect("draft should be written");
    let index_before = fs::read(repository.join(".git/index")).expect("index should exist");
    let output = harness.run_in(&repository, &initial_arguments());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    let commit = git_output(&remote, &["rev-parse", "main"]);
    assert_eq!(result["localSync"]["synced"], true);
    assert_eq!(git_output(&repository, &["rev-parse", "HEAD"]), commit);
    assert_eq!(
        result["resourceUrl"],
        format!("https://github.com/owner/repo/commit/{commit}")
    );
    assert_eq!(git_output(&remote, &["rev-list", "--count", "main"]), "1");
    assert_eq!(
        git_output(&remote, &["show", "main:src/main.py"]),
        "print('hello')"
    );
    assert_eq!(git_output(&remote, &["show", "main:entry"]), "src/main.py");
    let tree = git_output(&remote, &["ls-tree", "-r", "main"]);
    assert!(tree.contains("100755 blob"));
    assert!(tree.contains("120000 blob"));
    assert!(!tree.contains("draft.txt"));
    let binary = Command::new("git")
        .arg("-C")
        .arg(&remote)
        .args(["show", "main:logo.bin"])
        .output()
        .expect("git should run");
    assert_eq!(binary.stdout, [0xff, 0x00, 0xfe]);
    assert_eq!(
        git_output(
            &remote,
            &["show", "-s", "--format=%an <%ae>|%cn <%ce>", "main"]
        ),
        "Test User <123+octocat@users.noreply.github.com>|Test User <123+octocat@users.noreply.github.com>"
    );
    assert!(
        git_output(&remote, &["show", "-s", "--format=%B", "main"])
            .starts_with("feat: initialize package")
    );
    assert_eq!(
        fs::read(repository.join(".git/index")).expect("index should exist"),
        index_before
    );
    assert_eq!(
        fs::read_to_string(repository.join("src/main.py")).expect("source should exist"),
        "unstaged change\n"
    );
    assert_eq!(
        git_output(&repository, &["diff", "--cached", "--name-only"]),
        ""
    );
    assert!(git_output(&repository, &["status", "--porcelain"]).contains("?? draft.txt"));
}

#[test]
fn initial_commit_honors_path_selection() {
    let harness = Harness::with_behavior(EMPTY_BEHAVIOR);
    let (repository, remote) = empty_repository(&harness);
    fs::write(repository.join("included.txt"), "included").expect("file should be written");
    fs::write(repository.join("excluded.txt"), "excluded").expect("file should be written");
    git(&repository, &["add", "."]);
    let mut arguments = initial_arguments();
    arguments.push("included.txt");
    let output = harness.run_in(&repository, &arguments);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        git_output(&remote, &["ls-tree", "--name-only", "main"]),
        "included.txt"
    );
    assert_eq!(
        git_output(&repository, &["diff", "--cached", "--name-only"]),
        "excluded.txt"
    );
}

#[test]
fn does_not_bootstrap_other_github_errors() {
    for error in [
        "Not Found (HTTP 404)",
        "Forbidden (HTTP 403)",
        "Conflict (HTTP 409)",
    ] {
        let behavior = format!("printf '%s\\n' 'gh: {error}' >&2\nexit 1");
        let harness = Harness::with_behavior(&behavior);
        let (repository, remote) = empty_repository(&harness);
        fs::write(repository.join("file.txt"), "staged").expect("file should be written");
        git(&repository, &["add", "."]);
        let output = harness.run_in(&repository, &initial_arguments());
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains(error));
        assert_eq!(git_output(&remote, &["for-each-ref"]), "");
        assert!(
            !fs::read_to_string(&harness.log)
                .expect("log should exist")
                .contains("GET user")
        );
    }
}

#[test]
fn empty_repository_dry_run_does_not_contact_github() {
    let harness = Harness::with_behavior(EMPTY_BEHAVIOR);
    let (repository, remote) = empty_repository(&harness);
    fs::write(repository.join("file.txt"), "staged").expect("file should be written");
    git(&repository, &["add", "."]);
    let mut arguments = initial_arguments();
    arguments.push("--dry-run");
    let output = harness.run_in(&repository, &arguments);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(!harness.log.exists());
    assert_eq!(git_output(&remote, &["for-each-ref"]), "");
}

#[test]
fn initial_commit_rejects_a_concurrently_created_branch() {
    let harness = Harness::with_behavior(EMPTY_BEHAVIOR);
    let (repository, remote) = empty_repository(&harness);
    git(&repository, &["config", "user.name", "Test"]);
    git(&repository, &["config", "user.email", "test@example.com"]);
    fs::write(repository.join("file.txt"), "original").expect("file should be written");
    git(&repository, &["add", "."]);
    git(&repository, &["commit", "-q", "-m", "original"]);
    git(
        &repository,
        &[
            "push",
            "-q",
            remote.to_str().expect("path should be UTF-8"),
            "main",
        ],
    );
    let original = git_output(&remote, &["rev-parse", "main"]);
    fs::write(repository.join("file.txt"), "new").expect("file should change");
    git(&repository, &["add", "."]);
    let output = harness.run_in(&repository, &initial_arguments());
    assert!(!output.status.success());
    assert_eq!(git_output(&remote, &["rev-parse", "main"]), original);
    assert_eq!(git_output(&repository, &["rev-parse", "HEAD"]), original);
    assert_eq!(git_output(&repository, &["show", ":file.txt"]), "new");
}

#[test]
fn leaves_existing_local_history_untouched_after_initializing_the_remote() {
    let harness = Harness::with_behavior(EMPTY_BEHAVIOR);
    let (repository, remote) = empty_repository(&harness);
    git(&repository, &["config", "user.name", "Test"]);
    git(&repository, &["config", "user.email", "test@example.com"]);
    fs::write(repository.join("file.txt"), "original").expect("file should be written");
    git(&repository, &["add", "."]);
    git(&repository, &["commit", "-q", "-m", "local history"]);
    let original = git_output(&repository, &["rev-parse", "HEAD"]);
    fs::write(repository.join("file.txt"), "staged").expect("file should change");
    git(&repository, &["add", "."]);
    let output = harness.run_in(&repository, &initial_arguments());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(result["localSync"]["synced"], false);
    assert_eq!(git_output(&repository, &["rev-parse", "HEAD"]), original);
    assert_eq!(git_output(&repository, &["show", ":file.txt"]), "staged");
    assert_eq!(git_output(&remote, &["show", "main:file.txt"]), "staged");
    assert_eq!(git_output(&remote, &["rev-list", "--count", "main"]), "1");
}
