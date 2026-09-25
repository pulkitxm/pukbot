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
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let executable_directory = directory.path().join("bin");
        fs::create_dir(&executable_directory).expect("fake executable directory should be created");
        let executable = executable_directory.join("gh");
        let script = format!(
            "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$*\" >>\"$PUKBOT_FAKE_GH_LOG\"\n{BEHAVIOR}\n"
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
