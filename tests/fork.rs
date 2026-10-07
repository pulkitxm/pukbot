#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::{env, fs};

use serde_json::{Value, json};
use tempfile::TempDir;

#[test]
fn syncs_fork_through_the_app_worker_and_propagates_conflicts() {
    let preview = Command::new(env!("CARGO_BIN_EXE_pukbot"))
        .args([
            "repository",
            "sync-fork",
            "--repo",
            "owner/fork",
            "--branch",
            "release/1.2.3",
            "--dry-run",
            "--json",
        ])
        .output()
        .expect("sync preview should run");
    assert!(preview.status.success());
    let payload: Value = serde_json::from_slice(&preview.stdout).expect("preview should be JSON");
    assert_eq!(payload["operation"], "repository_sync_fork");

    let workflow = include_str!("../.github/workflows/operation.yml");
    let script = workflow
        .split_once("        run: |\n")
        .expect("worker script should be present")
        .1
        .lines()
        .map_while(|line| line.strip_prefix("          "))
        .collect::<Vec<_>>()
        .join("\n");

    for success in [true, false] {
        let directory = TempDir::new().expect("worker directory should be created");
        let executable_directory = directory.path().join("bin");
        fs::create_dir(&executable_directory).expect("fake executable directory should be created");
        let executable = executable_directory.join("gh");
        fs::write(
            &executable,
            r#"#!/bin/sh
set -eu
test "$*" = 'api --method POST repos/owner/fork/merge-upstream --input request.json'
cp request.json received.json
if [ "$SYNC_SUCCESS" = true ]; then
    printf '%s\n' '{"merge_type":"fast-forward","base_branch":"release/1.2.3"}'
else
    printf '%s\n' 'gh: merge conflict (HTTP 409)' >&2
    exit 1
fi
"#,
        )
        .expect("fake gh should be written");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("fake gh should be executable");
        let paths = env::join_paths(
            std::iter::once(executable_directory)
                .chain(env::split_paths(&env::var_os("PATH").unwrap_or_default())),
        )
        .expect("executable paths should join");
        let output = Command::new("bash")
            .args(["-c", &script])
            .current_dir(directory.path())
            .env("PATH", paths)
            .env("PAYLOAD", payload.to_string())
            .env("ACTOR", "owner")
            .env("REQUESTER", "owner")
            .env("REQUEST_ID", "123-456")
            .env("SYNC_SUCCESS", success.to_string())
            .output()
            .expect("worker should run");
        assert_eq!(output.status.success(), success, "{output:?}");
        let received: Value = serde_json::from_slice(
            &fs::read(directory.path().join("received.json"))
                .expect("request should reach the fork API"),
        )
        .expect("fork request should be JSON");
        assert_eq!(received, json!({"branch": "release/1.2.3"}));
        let stdout = String::from_utf8(output.stdout).expect("worker output should be UTF-8");
        if success {
            assert!(
                stdout.contains(
                    "pukbot-result-url=https://github.com/owner/fork/tree/release%2F1.2.3"
                )
            );
        } else {
            assert!(!stdout.contains("pukbot-result-url="));
        }
    }
}
