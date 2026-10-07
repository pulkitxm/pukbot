use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, SecondsFormat, Utc};

const HISTORY_FORMAT: &str = "%H%x00%T%x00%an%x00%ae%x00%aI%x00%cn%x00%ce%x00%cI%x00%B";

type History = BTreeMap<String, Vec<Vec<u8>>>;
type Graph = BTreeMap<String, Vec<String>>;

pub struct RootInsertion<'a> {
    pub branch: &'a str,
    pub backup_branch: &'a str,
    pub expected_head: &'a str,
    pub date: &'a str,
    pub message: &'a str,
}

pub fn normalize_date(value: &str) -> Result<String> {
    let date = DateTime::parse_from_rfc3339(value).context("root date must be RFC 3339")?;
    Ok(date
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Secs, true))
}

pub fn prepend_root(
    remote: &str,
    insertion: &RootInsertion<'_>,
    name: &str,
    email: &str,
) -> Result<String> {
    let workspace = tempfile::tempdir().context("failed to create the history workspace")?;
    let repository = workspace.path().join("repository.git");
    let mut clone = authenticated_git(
        workspace.path(),
        &[
            "clone",
            "--quiet",
            "--bare",
            "--no-hardlinks",
            remote,
            "repository.git",
        ],
    );
    run(&mut clone, b"")?;
    rewrite_and_push(&repository, remote, insertion, name, email)
}

fn rewrite_and_push(
    repository: &Path,
    remote: &str,
    insertion: &RootInsertion<'_>,
    name: &str,
    email: &str,
) -> Result<String> {
    let reference = format!("refs/heads/{}", insertion.branch);
    let backup = format!("refs/heads/{}", insertion.backup_branch);
    let head = git(repository, &["rev-parse", "--verify", &reference], b"")?;
    if head != insertion.expected_head {
        bail!(
            "branch head changed: expected {}, found {head}",
            insertion.expected_head
        );
    }
    if !git(
        repository,
        &["for-each-ref", "--format=%(refname)", &backup],
        b"",
    )?
    .is_empty()
    {
        bail!("the backup branch already exists");
    }
    let rewritten = rewrite(repository, &reference, insertion, name, email)?;
    let mut push = authenticated_git(
        repository,
        &[
            "push",
            "--quiet",
            "--atomic",
            &format!("--force-with-lease={reference}:{}", insertion.expected_head),
            &format!("--force-with-lease={backup}:"),
            remote,
            &format!("{rewritten}:{reference}"),
            &format!("{}:{backup}", insertion.expected_head),
        ],
    );
    run(&mut push, b"")?;
    Ok(rewritten)
}

fn rewrite(
    repository: &Path,
    reference: &str,
    insertion: &RootInsertion<'_>,
    name: &str,
    email: &str,
) -> Result<String> {
    let roots = git(repository, &["rev-list", "--max-parents=0", reference], b"")?;
    if roots.lines().count() != 1 {
        bail!("root insertion requires a branch with exactly one existing root");
    }
    let original_date = git(repository, &["show", "-s", "--format=%at", &roots], b"")?
        .parse::<i64>()
        .context("Git returned an invalid initial commit date")?;
    let date = DateTime::parse_from_rfc3339(insertion.date)?;
    if date.timestamp() >= original_date {
        bail!("the new root date must precede the original root's author date");
    }
    let original_history = history(repository, reference)?;
    let original_graph = graph(repository, reference)?;
    let empty_tree = git(
        repository,
        &["hash-object", "-w", "-t", "tree", "--stdin"],
        b"",
    )?;
    let mut command = git_command(repository, &["commit-tree", &empty_tree]);
    command
        .env("GIT_AUTHOR_NAME", name)
        .env("GIT_AUTHOR_EMAIL", email)
        .env("GIT_COMMITTER_NAME", name)
        .env("GIT_COMMITTER_EMAIL", email)
        .env("GIT_AUTHOR_DATE", insertion.date)
        .env("GIT_COMMITTER_DATE", insertion.date);
    let root = run(&mut command, insertion.message.as_bytes())?;
    git(
        repository,
        &[
            "filter-repo",
            "--force",
            "--refs",
            reference,
            "--prune-empty",
            "never",
            "--prune-degenerate",
            "never",
            "--preserve-commit-hashes",
            "--preserve-commit-encoding",
            "--commit-callback",
            &format!("if not commit.parents: commit.parents = [b'{root}']"),
        ],
        b"",
    )
    .context("history rewriting requires git-filter-repo")?;
    verify(
        repository,
        reference,
        &root,
        &original_history,
        &original_graph,
    )?;
    git(repository, &["rev-parse", reference], b"")
}

fn verify(
    repository: &Path,
    reference: &str,
    root: &str,
    original_history: &History,
    original_graph: &Graph,
) -> Result<()> {
    let mapping = fs::read_to_string(repository.join("filter-repo/commit-map"))
        .context("failed to read the rewritten commit map")?;
    let mapping = mapping
        .lines()
        .skip(1)
        .map(|line| {
            line.split_once(' ')
                .context("Git returned an invalid rewritten commit mapping")
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let new_history = history(repository, &format!("{root}..{reference}"))?;
    let mut expected_graph = Graph::new();
    expected_graph.insert(root.to_owned(), Vec::new());
    for (old, metadata) in original_history {
        let new = mapping
            .get(old.as_str())
            .context("commit is missing from the rewrite")?;
        if new_history.get(*new) != Some(metadata) {
            bail!("rewritten commit {old} changed its tree, message, author, committer, or dates");
        }
        let parents = original_graph
            .get(old)
            .context("original commit ancestry is missing")?;
        let parents = if parents.is_empty() {
            vec![root.to_owned()]
        } else {
            parents
                .iter()
                .map(|parent| {
                    mapping
                        .get(parent.as_str())
                        .map(|parent| (*parent).to_owned())
                        .context("rewritten parent is missing")
                })
                .collect::<Result<Vec<_>>>()?
        };
        expected_graph.insert((*new).to_owned(), parents);
    }
    if original_history.len() != new_history.len()
        || expected_graph != graph(repository, reference)?
    {
        bail!("rewritten history changed the commit count or merge topology");
    }
    Ok(())
}

fn graph(repository: &Path, reference: &str) -> Result<Graph> {
    git(repository, &["rev-list", "--parents", reference], b"")?
        .lines()
        .map(|line| {
            let mut entries = line.split_whitespace();
            let commit = entries
                .next()
                .context("Git returned an invalid commit graph")?;
            Ok((commit.to_owned(), entries.map(str::to_owned).collect()))
        })
        .collect()
}

fn history(repository: &Path, reference: &str) -> Result<History> {
    let output = run_bytes(
        &mut git_command(
            repository,
            &[
                "log",
                "-z",
                "--no-show-signature",
                "--no-notes",
                "--encoding=none",
                &format!("--format={HISTORY_FORMAT}"),
                reference,
            ],
        ),
        b"",
    )?;
    let entries = output
        .strip_suffix(&[0])
        .unwrap_or(&output)
        .split(|byte| *byte == 0)
        .collect::<Vec<_>>();
    let mut commits = entries.chunks_exact(9);
    let history = commits
        .by_ref()
        .map(|entry| {
            Ok((
                String::from_utf8(entry[0].to_vec())
                    .context("Git returned an invalid commit ID")?,
                entry[1..].iter().map(|value| value.to_vec()).collect(),
            ))
        })
        .collect::<Result<History>>()?;
    if !commits.remainder().is_empty() {
        bail!("Git returned invalid commit metadata");
    }
    Ok(history)
}

fn authenticated_git(repository: &Path, args: &[&str]) -> Command {
    let mut command = git_command(
        repository,
        &[
            "-c",
            "credential.helper=",
            "-c",
            "credential.https://github.com.helper=!gh auth git-credential",
            "-c",
            "core.hooksPath=/dev/null",
        ],
    );
    command.args(args);
    command
}

fn git_command(repository: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command.current_dir(repository).args(args);
    for variable in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        command.env_remove(variable);
    }
    command
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn git(repository: &Path, args: &[&str], input: &[u8]) -> Result<String> {
    run(&mut git_command(repository, args), input)
}

fn run(command: &mut Command, input: &[u8]) -> Result<String> {
    Ok(String::from_utf8(run_bytes(command, input)?)
        .context("Git returned non-UTF-8 output")?
        .trim()
        .to_owned())
}

fn run_bytes(command: &mut Command, input: &[u8]) -> Result<Vec<u8>> {
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
    use std::fs;
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::{
        RootInsertion, git, git_command, graph, normalize_date, prepend_root, rewrite_and_push, run,
    };

    struct Fixture {
        directory: TempDir,
        remote: PathBuf,
        original: String,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().expect("fixture directory should exist");
            let remote = directory.path().join("remote.git");
            fs::create_dir(&remote).expect("remote directory should exist");
            git(
                &remote,
                &[
                    "init",
                    "--quiet",
                    "--bare",
                    "--object-format=sha1",
                    "-b",
                    "main",
                ],
                b"",
            )
            .expect("remote should initialize");
            let binary = git(&remote, &["hash-object", "-w", "--stdin"], &[0xff, 0, 0xfe])
                .expect("binary should exist");
            let script = git(&remote, &["hash-object", "-w", "--stdin"], b"exit 0\n")
                .expect("script should exist");
            let link = git(&remote, &["hash-object", "-w", "--stdin"], b"payload.bin")
                .expect("link should exist");
            let entries = format!(
                "100644 blob {binary}\tpayload.bin\n100755 blob {script}\trun.sh\n120000 blob {link}\tentry\n"
            );
            let tree = git(&remote, &["mktree"], entries.as_bytes()).expect("tree should exist");
            let root = Self::commit(
                &remote,
                &tree,
                &[],
                "feat: original root\n\noriginal message\n",
            );
            let left = Self::commit(&remote, &tree, &[&root], "feat: left\n");
            let right = Self::commit(&remote, &tree, &[&root], "feat: right\n");
            let merge = Self::commit(
                &remote,
                &tree,
                &[&left, &right],
                "feat: merge both branches\n",
            );
            let original = Self::commit(
                &remote,
                &tree,
                &[&merge],
                &format!("chore: retain empty commit referencing {root}\n"),
            );
            for (reference, sha) in [
                ("refs/heads/main", &original),
                ("refs/heads/topic", &right),
                ("refs/tags/v1.0.0", &original),
            ] {
                git(&remote, &["update-ref", reference, sha], b"").expect("ref should exist");
            }
            Self {
                directory,
                remote,
                original,
            }
        }

        fn commit(remote: &std::path::Path, tree: &str, parents: &[&str], message: &str) -> String {
            let mut command = git_command(remote, &["commit-tree", tree]);
            for parent in parents {
                command.args(["-p", parent]);
            }
            command
                .env("GIT_AUTHOR_NAME", "Original Author")
                .env("GIT_AUTHOR_EMAIL", "author@example.test")
                .env("GIT_COMMITTER_NAME", "Original Committer")
                .env("GIT_COMMITTER_EMAIL", "committer@example.test")
                .env("GIT_AUTHOR_DATE", "2026-08-13T12:00:00+05:30")
                .env("GIT_COMMITTER_DATE", "2026-08-14T14:15:00-04:00");
            run(&mut command, message.as_bytes()).expect("fixture commit should exist")
        }

        fn insertion(&self) -> RootInsertion<'_> {
            RootInsertion {
                branch: "main",
                backup_branch: "backup/original",
                expected_head: &self.original,
                date: "2026-04-07T08:00:00Z",
                message: "chore: add retrospective empty history anchor\n",
            }
        }

        fn read(&self, args: &[&str]) -> String {
            git(&self.remote, args, b"").expect("fixture should be readable")
        }

        fn clone(&self) -> PathBuf {
            git(
                self.directory.path(),
                &[
                    "clone",
                    "--quiet",
                    "--bare",
                    self.remote.to_str().expect("remote path should be UTF-8"),
                    "clone.git",
                ],
                b"",
            )
            .expect("fixture should clone");
            self.directory.path().join("clone.git")
        }
    }

    #[test]
    fn preserves_metadata_merge_topology_empty_commits_and_other_refs() {
        let fixture = Fixture::new();
        let tree = fixture.read(&["rev-parse", "main^{tree}"]);
        let rewritten = prepend_root(
            fixture
                .remote
                .to_str()
                .expect("remote path should be UTF-8"),
            &fixture.insertion(),
            "Root Author",
            "root@example.test",
        )
        .expect("history should rewrite");
        let root = fixture.read(&["rev-list", "--max-parents=0", "main"]);
        assert_eq!(fixture.read(&["rev-parse", "main"]), rewritten);
        assert_eq!(
            fixture.read(&["rev-parse", "backup/original"]),
            fixture.original
        );
        assert_eq!(fixture.read(&["rev-parse", "v1.0.0"]), fixture.original);
        assert_eq!(fixture.read(&["rev-parse", "main^{tree}"]), tree);
        assert_eq!(fixture.read(&["rev-list", "--count", "main"]), "6");
        assert_eq!(
            fixture.read(&["rev-list", "--count", "--merges", "main"]),
            "1"
        );
        for date in fixture
            .read(&["show", "-s", "--format=%aI|%cI", &root])
            .split('|')
        {
            assert_eq!(
                normalize_date(date).expect("root date should be valid"),
                "2026-04-07T08:00:00Z"
            );
        }
        assert_eq!(fixture.read(&["ls-tree", &root]), "");
        assert_eq!(fixture.read(&["diff", "backup/original", "main"]), "");
        assert_eq!(
            graph(&fixture.remote, "main")
                .expect("graph should exist")
                .len(),
            6
        );
        let retry = prepend_root(
            fixture
                .remote
                .to_str()
                .expect("remote path should be UTF-8"),
            &fixture.insertion(),
            "Root Author",
            "root@example.test",
        );
        assert!(
            retry
                .expect_err("stale retry should fail")
                .to_string()
                .contains("branch head changed")
        );
        assert_eq!(fixture.read(&["rev-parse", "main"]), rewritten);
    }

    #[test]
    fn rejects_a_newer_date_without_modifying_the_remote() {
        let fixture = Fixture::new();
        let mut insertion = fixture.insertion();
        insertion.date = "2026-10-07T08:00:00Z";
        let error = prepend_root(
            fixture
                .remote
                .to_str()
                .expect("remote path should be UTF-8"),
            &insertion,
            "Root Author",
            "root@example.test",
        )
        .expect_err("newer date should fail");
        assert!(error.to_string().contains("must precede"));
        assert_eq!(fixture.read(&["rev-parse", "main"]), fixture.original);
        assert_eq!(
            fixture.read(&["for-each-ref", "--format=%(refname)", "refs/heads/backup"]),
            ""
        );
    }

    #[test]
    fn rejects_remote_branch_and_backup_races_atomically() {
        for backup_race in [false, true] {
            let fixture = Fixture::new();
            let clone = fixture.clone();
            let reference = if backup_race {
                "refs/heads/backup/original"
            } else {
                "refs/heads/main"
            };
            let root = fixture.read(&["rev-list", "--max-parents=0", "main"]);
            git(&fixture.remote, &["update-ref", reference, &root], b"")
                .expect("remote should change");
            let refs = fixture.read(&["show-ref"]);
            let error = rewrite_and_push(
                &clone,
                fixture
                    .remote
                    .to_str()
                    .expect("remote path should be UTF-8"),
                &fixture.insertion(),
                "Root Author",
                "root@example.test",
            )
            .expect_err("race should fail");
            assert!(error.to_string().contains("Git failed"));
            assert_eq!(fixture.read(&["show-ref"]), refs);
        }
    }

    #[test]
    fn server_rejection_does_not_create_a_partial_backup() {
        let fixture = Fixture::new();
        git(
            &fixture.remote,
            &["config", "receive.denyNonFastForwards", "true"],
            b"",
        )
        .expect("remote policy should be set");
        let refs = fixture.read(&["show-ref"]);
        let error = prepend_root(
            fixture
                .remote
                .to_str()
                .expect("remote path should be UTF-8"),
            &fixture.insertion(),
            "Root Author",
            "root@example.test",
        )
        .expect_err("non-fast-forward policy should reject the rewrite");
        assert!(error.to_string().contains("Git failed"));
        assert_eq!(fixture.read(&["show-ref"]), refs);
    }

    #[test]
    fn normalizes_dates_and_rejects_ambiguous_input() {
        assert_eq!(
            normalize_date("2026-04-07T13:30:00+05:30").expect("date should normalize"),
            "2026-04-07T08:00:00Z"
        );
        for date in ["six months ago", "2026-04-07", "2026-02-30T08:00:00Z"] {
            assert!(normalize_date(date).is_err());
        }
    }
}
