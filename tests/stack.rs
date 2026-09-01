#![cfg(unix)]

use std::ffi::OsString;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::{env, fs};

use serde_json::Value;
use tempfile::TempDir;

const STACK: &str = r#"{
    "id": 987,
    "number": 42,
    "node_id": "PRS_123",
    "url": "https://api.github.com/repos/owner/repo/stacks/42",
    "base": {"ref": "main"},
    "open": true,
    "created_at": "2026-04-15T10:00:00Z",
    "pull_requests": [
        {
            "number": 11,
            "state": "open",
            "draft": false,
            "merged_at": null,
            "head": {"ref": "feature-one", "sha": "aaa"}
        },
        {
            "number": 12,
            "state": "open",
            "draft": false,
            "merged_at": null,
            "head": {"ref": "feature-two", "sha": "bbb"}
        }
    ]
}"#;

struct FakeGh {
    _directory: TempDir,
    executable_directory: PathBuf,
    log: PathBuf,
}

impl FakeGh {
    fn new(behavior: &str) -> Self {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let executable_directory = directory.path().join("bin");
        fs::create_dir(&executable_directory).expect("fake executable directory should be created");
        let executable = executable_directory.join("gh");
        let script = format!(
            "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$*\" >>\"$PUKBOT_FAKE_GH_LOG\"\n{behavior}\n"
        );
        fs::write(&executable, script).expect("fake gh should be written");
        let mut permissions = fs::metadata(&executable)
            .expect("fake gh metadata should be available")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).expect("fake gh should be executable");
        let log = directory.path().join("gh.log");
        Self {
            _directory: directory,
            executable_directory,
            log,
        }
    }

    fn run(&self, arguments: &[&str]) -> Output {
        self.command(arguments).output().expect("pukbot should run")
    }

    fn run_with_input(&self, arguments: &[&str], input: &[u8]) -> Output {
        let mut child = self
            .command(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("pukbot should start");
        child
            .stdin
            .take()
            .expect("pukbot stdin should be piped")
            .write_all(input)
            .expect("pukbot input should be written");
        child.wait_with_output().expect("pukbot should finish")
    }

    fn log(&self) -> String {
        fs::read_to_string(&self.log).expect("fake gh log should exist")
    }

    fn path(&self) -> OsString {
        let mut paths = vec![self.executable_directory.clone()];
        if let Some(original) = env::var_os("PATH") {
            paths.extend(env::split_paths(&original));
        }
        env::join_paths(paths).expect("fake PATH should be valid")
    }

    fn command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pukbot"));
        command
            .args(arguments)
            .env("PATH", self.path())
            .env("PUKBOT_FAKE_GH_LOG", &self.log);
        command
    }
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_failure(output: &Output, message: &str) {
    assert!(!output.status.success(), "command unexpectedly succeeded");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(message),
        "missing error {message:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn json_output(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout should contain JSON")
}

#[test]
fn proxies_raw_stack_arguments_streams_and_exit_code() {
    let behavior = r#"case "$*" in
    "stack link --base release feature/a feature/b --open")
        body=$(cat)
        printf 'body=%s\n' "$body" >>"$PUKBOT_FAKE_GH_LOG"
        printf 'proxy stdout\n'
        printf 'proxy stderr\n' >&2
        exit 37
        ;;
    *) exit 91 ;;
esac"#;
    let fake = FakeGh::new(behavior);

    let output = fake.run_with_input(
        &[
            "stack",
            "link",
            "--base",
            "release",
            "feature/a",
            "feature/b",
            "--open",
        ],
        b"proxy input\n",
    );
    assert_eq!(output.status.code(), Some(37));
    assert_eq!(output.stdout, b"proxy stdout\n");
    assert_eq!(output.stderr, b"proxy stderr\n");
    assert_eq!(
        fake.log(),
        concat!(
            "stack link --base release feature/a feature/b --open\n",
            "body=proxy input\n"
        )
    );
}

#[test]
fn forwards_a_leading_global_json_flag_to_stack_view() {
    let behavior = r#"case "$*" in
    "stack view --json") printf '%s\n' '{"branches":[]}' ;;
    *) exit 91 ;;
esac"#;
    let fake = FakeGh::new(behavior);

    let output = fake.run(&["--json", "stack", "view"]);
    assert_success(&output);
    assert_eq!(output.stdout, b"{\"branches\":[]}\n");
    assert_eq!(fake.log(), "stack view --json\n");
}

#[test]
fn rejects_non_squash_compatibility_merges() {
    let fake = FakeGh::new("exit 91");

    let output = fake.run(&["stack", "merge", "12", "--merge", "--yes"]);
    assert_failure(&output, "stack merges are squash-only");
    assert!(!fake.log.exists());
}

#[test]
fn compatibility_merge_resolves_current_top_and_forces_squash() {
    let behavior = r#"case "$*" in
    "repo view --json nameWithOwner --jq .nameWithOwner")
        printf '%s\n' 'owner/repo'
        ;;
    "stack view --json")
        printf '%s\n' '{"branches":[{"pr":{"number":11}},{"pr":{"number":12}}]}'
        ;;
    "api --method GET repos/owner/repo/pulls/12")
        printf '%s\n' '{"stack":{"number":42},"head":{"sha":"bbb"}}'
        ;;
    "api --method PUT repos/owner/repo/pulls/12/merge-async --input -")
        body=$(cat)
        printf 'body=%s\n' "$body" >>"$PUKBOT_FAKE_GH_LOG"
        printf '%s\n' '{"status":"merged","details":{"message":"merged","sha":"abc"}}'
        ;;
    *) exit 91 ;;
esac"#;
    let fake = FakeGh::new(behavior);

    let output = fake.run(&["stack", "merge", "--yes"]);
    assert_success(&output);
    assert_eq!(output.stdout, b"https://github.com/owner/repo/pull/12\n");
    assert_eq!(
        fake.log(),
        concat!(
            "repo view --json nameWithOwner --jq .nameWithOwner\n",
            "stack view --json\n",
            "api --method GET repos/owner/repo/pulls/12\n",
            "api --method PUT repos/owner/repo/pulls/12/merge-async --input -\n",
            "body={\"merge_action\":\"direct_merge\",\"merge_method\":\"squash\",\"sha\":\"bbb\"}\n"
        )
    );
}

#[test]
fn creates_and_appends_with_ordered_pull_request_bodies() {
    let added_stack = STACK.replace("\"number\": 12", "\"number\": 14");
    let behavior = format!(
        r#"case "$*" in
    "api --method POST repos/owner/repo/stacks --input -")
        body=$(cat)
        printf 'body=%s\n' "$body" >>"$PUKBOT_FAKE_GH_LOG"
        printf '%s\n' '{STACK}'
        ;;
    "api --method POST repos/owner/repo/stacks/42/add --input -")
        body=$(cat)
        printf 'body=%s\n' "$body" >>"$PUKBOT_FAKE_GH_LOG"
        printf '%s\n' '{added_stack}'
        ;;
    *) exit 91 ;;
esac"#,
    );
    let fake = FakeGh::new(&behavior);

    let created = fake.run(&[
        "stack-api",
        "create",
        "11",
        "12",
        "--repo",
        "owner/repo",
        "--json",
    ]);
    assert_success(&created);
    assert_eq!(
        json_output(&created)["resourceUrl"],
        "https://github.com/owner/repo/pull/12"
    );

    let added = fake.run(&[
        "stack-api",
        "append",
        "42",
        "13",
        "14",
        "--repo",
        "owner/repo",
        "--json",
    ]);
    assert_success(&added);
    assert_eq!(
        json_output(&added)["resourceUrl"],
        "https://github.com/owner/repo/pull/14"
    );

    assert_eq!(
        fake.log(),
        concat!(
            "api --method POST repos/owner/repo/stacks --input -\n",
            "body={\"pull_requests\":[11,12]}\n",
            "api --method POST repos/owner/repo/stacks/42/add --input -\n",
            "body={\"pull_requests\":[13,14]}\n"
        )
    );
}

#[test]
fn flattens_paginated_filtered_stack_results() {
    let second = STACK
        .replace("\"id\": 987", "\"id\": 988")
        .replace("\"number\": 42", "\"number\": 43");
    let behavior = format!(
        r#"case "$*" in
    "api --paginate --slurp repos/owner/repo/stacks?pull_request=12&per_page=100")
        printf '%s\n' '[[{STACK}],[{second}]]'
        ;;
    *) exit 91 ;;
esac"#
    );
    let fake = FakeGh::new(&behavior);

    let output = fake.run(&[
        "stack-api",
        "list",
        "--repo",
        "owner/repo",
        "--pull-request",
        "12",
        "--json",
    ]);
    assert_success(&output);
    let stacks = json_output(&output);
    assert_eq!(stacks.as_array().map(Vec::len), Some(2));
    assert_eq!(stacks[0]["number"], 42);
    assert_eq!(stacks[1]["number"], 43);
    assert_eq!(
        fake.log(),
        "api --paginate --slurp repos/owner/repo/stacks?pull_request=12&per_page=100\n"
    );
}

#[test]
fn handles_dissolved_and_partially_remaining_unstack_responses() {
    let behavior = format!(
        r#"case "$*" in
    "api --method POST repos/owner/repo/stacks/42/unstack") ;;
    "api --method POST repos/owner/repo/stacks/43/unstack") printf '%s\n' '{STACK}' ;;
    *) exit 91 ;;
esac"#
    );
    let fake = FakeGh::new(&behavior);

    let dissolved = fake.run(&[
        "stack-api",
        "unstack",
        "42",
        "--repo",
        "owner/repo",
        "--yes",
        "--json",
    ]);
    assert_success(&dissolved);
    assert_eq!(
        json_output(&dissolved)["resourceUrl"],
        "https://github.com/owner/repo/pulls"
    );

    let remaining = fake.run(&[
        "stack-api",
        "unstack",
        "43",
        "--repo",
        "owner/repo",
        "--yes",
        "--json",
    ]);
    assert_success(&remaining);
    assert_eq!(
        json_output(&remaining)["resourceUrl"],
        "https://github.com/owner/repo/pull/12"
    );
}

#[test]
fn rejects_conflicting_pending_merge_request() {
    let behavior = r#"case "$*" in
    "api --method GET repos/owner/repo/pulls/12")
        printf '%s\n' '{"stack":{"number":42},"head":{"sha":"bbb"}}'
        ;;
    "api --method PUT repos/owner/repo/pulls/12/merge-async --input -")
        body=$(cat)
        printf 'body=%s\n' "$body" >>"$PUKBOT_FAKE_GH_LOG"
        printf '%s\n' '{"status":"pending","details":{"message":"pending","uuid":"abc-123"}}'
        exit 1
        ;;
    *) exit 91 ;;
esac"#;
    let fake = FakeGh::new(behavior);

    let output = fake.run(&[
        "stack-api",
        "merge",
        "12",
        "--repo",
        "owner/repo",
        "--yes",
        "--json",
    ]);
    assert_failure(&output, "GitHub rejected PUT");
    let log = fake.log();
    assert!(log.contains("api --method GET repos/owner/repo/pulls/12\n"));
    assert!(log.contains("api --method PUT repos/owner/repo/pulls/12/merge-async --input -\n"));
    assert!(log.contains(
        "body={\"merge_action\":\"direct_merge\",\"merge_method\":\"squash\",\"sha\":\"bbb\"}\n"
    ));
    assert!(!log.contains("merge-async/abc-123"));
}

#[test]
fn polls_a_matching_pending_merge_until_it_is_merged() {
    let behavior = r#"case "$*" in
    "api --method GET repos/owner/repo/pulls/12")
        printf '%s\n' '{"stack":{"number":42},"head":{"sha":"bbb"}}'
        ;;
    "api --method PUT repos/owner/repo/pulls/12/merge-async --input -")
        body=$(cat)
        printf 'body=%s\n' "$body" >>"$PUKBOT_FAKE_GH_LOG"
        printf '%s\n' '{"status":"pending","details":{"message":"pending","uuid":"abc-123","merge_method":"squash","merge_action":"direct_merge","expected_head_sha":"bbb"}}'
        ;;
    "api --method GET repos/owner/repo/pulls/12/merge-async/abc-123")
        printf '%s\n' '{"status":"merged","details":{"message":"merged","sha":"abc"}}'
        ;;
    *) exit 91 ;;
esac"#;
    let fake = FakeGh::new(behavior);

    let output = fake.run(&[
        "stack-api",
        "merge",
        "12",
        "--repo",
        "owner/repo",
        "--yes",
        "--json",
    ]);
    assert_success(&output);
    assert_eq!(
        json_output(&output)["resourceUrl"],
        "https://github.com/owner/repo/pull/12"
    );
    let log = fake.log();
    assert!(log.contains(
        "body={\"merge_action\":\"direct_merge\",\"merge_method\":\"squash\",\"sha\":\"bbb\"}\n"
    ));
    assert!(log.contains("api --method GET repos/owner/repo/pulls/12/merge-async/abc-123\n"));
}

#[test]
fn handles_immediate_merge_terminal_statuses() {
    let cases = [
        (
            r#"{"status":"merged","details":{"message":"merged","sha":"abc"}}"#,
            0,
            None,
        ),
        (
            r#"{"status":"enqueued","details":{"message":"queued"}}"#,
            0,
            Some("stack merge was enqueued instead of squash-merged"),
        ),
        (
            r#"{"status":"failed","details":{"message":"checks failed"}}"#,
            1,
            Some("stack merge failed: checks failed"),
        ),
    ];
    for (response, exit_code, expected_error) in cases {
        let pull_request = r#"{"stack":{"number":42},"head":{"sha":"bbb"}}"#;
        let behavior = format!(
            r#"case "$*" in
    "api --method GET repos/owner/repo/pulls/12")
        printf '%s\n' '{pull_request}'
        ;;
    "api --method PUT repos/owner/repo/pulls/12/merge-async --input -")
        cat >/dev/null
        printf '%s\n' '{response}'
        exit {exit_code}
        ;;
    *) exit 91 ;;
esac"#
        );
        let fake = FakeGh::new(&behavior);
        let output = fake.run(&[
            "stack-api",
            "merge",
            "12",
            "--repo",
            "owner/repo",
            "--yes",
            "--json",
        ]);
        if let Some(error) = expected_error {
            assert_failure(&output, error);
        } else {
            assert_success(&output);
        }
    }
}

#[test]
fn rejects_pending_merge_with_mismatched_request_details() {
    let cases = [
        (
            r#"{"status":"pending","details":{"uuid":"abc-123","merge_method":"merge","merge_action":"direct_merge","expected_head_sha":"bbb"}}"#,
            "pending stack merge is not using squash",
        ),
        (
            r#"{"status":"pending","details":{"uuid":"abc-123","merge_method":"squash","merge_action":"merge_queue","expected_head_sha":"bbb"}}"#,
            "pending stack merge is not using a direct merge",
        ),
        (
            r#"{"status":"pending","details":{"uuid":"abc-123","merge_method":"squash","merge_action":"direct_merge","expected_head_sha":"aaa"}}"#,
            "pending stack merge targets a different pull request head",
        ),
        (
            r#"{"status":"pending","details":{"merge_method":"squash","merge_action":"direct_merge","expected_head_sha":"bbb"}}"#,
            "pending stack merge response omitted its UUID",
        ),
    ];
    for (response, expected_error) in cases {
        let behavior = format!(
            r#"case "$*" in
    "api --method GET repos/owner/repo/pulls/12")
        printf '%s\n' '{{"stack":{{"number":42}},"head":{{"sha":"bbb"}}}}'
        ;;
    "api --method PUT repos/owner/repo/pulls/12/merge-async --input -")
        cat >/dev/null
        printf '%s\n' '{response}'
        ;;
    *) exit 91 ;;
esac"#
        );
        let fake = FakeGh::new(&behavior);
        let output = fake.run(&["stack-api", "merge", "12", "--repo", "owner/repo", "--yes"]);
        assert_failure(&output, expected_error);
        assert!(!fake.log().contains("merge-async/abc-123"));
    }
}

#[test]
fn rejects_malformed_merge_responses() {
    let behavior = r#"case "$*" in
    "api --method GET repos/owner/repo/pulls/12")
        printf '%s\n' '{"stack":{"number":42},"head":{"sha":"bbb"}}'
        ;;
    "api --method PUT repos/owner/repo/pulls/12/merge-async --input -")
        cat >/dev/null
        printf '%s\n' '{broken'
        ;;
    *) exit 91 ;;
esac"#;
    let fake = FakeGh::new(behavior);

    let output = fake.run(&["stack-api", "merge", "12", "--repo", "owner/repo", "--yes"]);
    assert_failure(&output, "failed to decode GitHub response");
}

#[test]
fn rejects_merge_for_an_unstacked_pull_request() {
    let behavior = r#"case "$*" in
    "api --method GET repos/owner/repo/pulls/12")
        printf '%s\n' '{"stack":null,"head":{"sha":"bbb"}}'
        ;;
    *) exit 91 ;;
esac"#;
    let fake = FakeGh::new(behavior);

    let output = fake.run(&["stack-api", "merge", "12", "--repo", "owner/repo", "--yes"]);
    assert_failure(&output, "not part of a stack");
    assert_eq!(fake.log(), "api --method GET repos/owner/repo/pulls/12\n");
}
