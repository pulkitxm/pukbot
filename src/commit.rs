use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine as _;
use serde::Serialize;

use crate::crew;
use crate::model::{CommitFileDocument, ContentEncoding, FileMode, Repository};

#[derive(Debug, Serialize)]
pub struct LocalSync {
    pub synced: bool,
    pub detail: String,
}

pub fn staged_files(paths: &[PathBuf]) -> Result<Vec<CommitFileDocument>> {
    let root = repository_root()?;
    staged_files_at(&root, paths)
}

fn staged_files_at(root: &Path, paths: &[PathBuf]) -> Result<Vec<CommitFileDocument>> {
    let mut args = vec![
        "-C".to_owned(),
        root.to_str()
            .context("repository path must be valid UTF-8")?
            .to_owned(),
        "diff".to_owned(),
        "--cached".to_owned(),
        "--raw".to_owned(),
        "--no-renames".to_owned(),
        "-z".to_owned(),
    ];
    if !paths.is_empty() {
        args.push("--".to_owned());
        for path in paths {
            args.push(path.to_string_lossy().into_owned());
        }
    }
    let output = Command::new("git")
        .args(&args)
        .output()
        .context("failed to run git diff --cached")?;
    if !output.status.success() {
        bail!("failed to read the staged git changes");
    }
    let raw = String::from_utf8(output.stdout).context("git diff output was not UTF-8")?;
    let mut fields = raw.split('\0').filter(|field| !field.is_empty());
    let mut entries = Vec::new();
    while let Some(record) = fields.next() {
        let path = fields
            .next()
            .context("git diff produced an unexpected record")?
            .to_owned();
        let mut metadata = record.trim_start_matches(':').split(' ');
        let new_mode = metadata
            .nth(1)
            .context("git diff produced an unexpected record")?;
        let status = metadata
            .nth(2)
            .context("git diff produced an unexpected record")?;
        match status.chars().next() {
            Some('D') => entries.push(CommitFileDocument {
                path,
                content: None,
                delete: true,
                mode: FileMode::Regular,
                encoding: ContentEncoding::Utf8,
            }),
            Some('A' | 'M' | 'T') => {
                let mode = new_mode
                    .parse::<FileMode>()
                    .map_err(|error| anyhow!("{path}: {error}"))?;
                let (content, encoding) = staged_content(root, &path)?;
                entries.push(CommitFileDocument {
                    path,
                    content: Some(content),
                    delete: false,
                    mode,
                    encoding,
                });
            }
            _ => bail!(
                "{path} has an unsupported git status ({status}); resolve it before committing"
            ),
        }
    }
    if entries.is_empty() {
        bail!("nothing is staged; run git add before pukbot commit create");
    }
    Ok(entries)
}

pub fn sync(repository: &Repository, branch: &str, commit_url: Option<&str>) -> LocalSync {
    match repository_root().and_then(|root| sync_at(&root, repository, branch, commit_url)) {
        Ok(detail) => LocalSync {
            synced: true,
            detail,
        },
        Err(error) => LocalSync {
            synced: false,
            detail: format!("{error:#}"),
        },
    }
}

fn sync_at(
    root: &Path,
    repository: &Repository,
    branch: &str,
    commit_url: Option<&str>,
) -> Result<String> {
    let commit = commit_url
        .and_then(|url| url.rsplit_once("/commit/"))
        .map(|(_, sha)| sha)
        .context("the commit URL was missing, so the local branch was left untouched")?;
    let current = git(root, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .context("the checkout is not on a branch, so it was left untouched")?;
    if current != branch {
        bail!("the checkout is on {current}, not {branch}, so it was left untouched");
    }
    let remote = remote_for(root, repository)?;
    git(
        root,
        &["fetch", "--quiet", &remote, &format!("refs/heads/{branch}")],
    )?;
    if git(root, &["rev-parse", "FETCH_HEAD"])? != commit {
        bail!(
            "{remote}/{branch} already moved past {commit}; reconcile it with git fetch and rebase"
        );
    }
    let head = git(root, &["rev-parse", "HEAD"])?;
    if git(root, &["rev-parse", &format!("{commit}^")])? != head {
        bail!(
            "local HEAD {head} is not the parent of {commit}; reconcile it with git fetch and rebase"
        );
    }
    git(root, &["reset", "--quiet", "--soft", commit])?;
    Ok(format!(
        "{branch} now points at {commit}; the index and working tree were not touched"
    ))
}

fn remote_for(root: &Path, repository: &Repository) -> Result<String> {
    for remote in git(root, &["remote"])?.lines() {
        let url = git(root, &["config", "--get", &format!("remote.{remote}.url")])?;
        if crew::parse_repository(&url)
            .is_ok_and(|candidate| crew::same_repository(&candidate, repository))
        {
            return Ok(remote.to_owned());
        }
    }
    bail!("no git remote points at {repository}, so the local branch was left untouched")
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .context("failed to run git")?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8(output.stdout)
        .context("git returned non-UTF-8 output")?
        .trim()
        .to_owned())
}

fn repository_root() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("failed to run git; install git and run inside a repository")?;
    if !output.status.success() {
        bail!("not inside a git repository");
    }
    let root = String::from_utf8(output.stdout)
        .context("git returned a non-UTF-8 repository path")?
        .trim()
        .to_owned();
    Ok(PathBuf::from(root))
}

fn staged_content(root: &Path, path: &str) -> Result<(String, ContentEncoding)> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["show", &format!(":{path}")])
        .output()
        .with_context(|| format!("failed to read the staged content of {path}"))?;
    if !output.status.success() {
        bail!("failed to read the staged content of {path}");
    }
    Ok(match String::from_utf8(output.stdout) {
        Ok(text) => (text, ContentEncoding::Utf8),
        Err(error) => (
            base64::engine::general_purpose::STANDARD.encode(error.into_bytes()),
            ContentEncoding::Base64,
        ),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use super::staged_files_at;

    fn init_repo() -> tempfile::TempDir {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let run = |args: &[&str]| {
            let status = Command::new("git")
                .arg("-C")
                .arg(directory.path())
                .args(args)
                .status()
                .expect("git should run");
            assert!(status.success());
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "Test"]);
        directory
    }

    #[test]
    fn reads_added_and_deleted_staged_files() {
        let directory = init_repo();
        fs::write(directory.path().join("kept.txt"), "old\n").expect("file should be written");
        Command::new("git")
            .arg("-C")
            .arg(directory.path())
            .args(["add", "kept.txt"])
            .status()
            .expect("git add should run");
        Command::new("git")
            .arg("-C")
            .arg(directory.path())
            .args(["commit", "-q", "-m", "init"])
            .status()
            .expect("git commit should run");

        fs::remove_file(directory.path().join("kept.txt")).expect("file should be removed");
        fs::write(directory.path().join("new.txt"), "hello\n").expect("file should be written");
        Command::new("git")
            .arg("-C")
            .arg(directory.path())
            .args(["add", "-A"])
            .status()
            .expect("git add should run");

        let files = staged_files_at(directory.path(), &[]).expect("staged files should resolve");
        assert!(
            files
                .iter()
                .any(|file| file.path == "new.txt" && file.content.as_deref() == Some("hello\n"))
        );
        assert!(
            files
                .iter()
                .any(|file| file.path == "kept.txt" && file.delete)
        );
    }

    #[cfg(unix)]
    #[test]
    fn keeps_executable_and_symlink_modes() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        use crate::model::{ContentEncoding, FileMode};

        let directory = init_repo();
        let run = |args: &[&str]| {
            let status = Command::new("git")
                .arg("-C")
                .arg(directory.path())
                .args(args)
                .status()
                .expect("git should run");
            assert!(status.success());
        };
        let script = directory.path().join("bundle.sh");
        fs::write(&script, "#!/bin/sh\n").expect("file should be written");
        fs::write(directory.path().join("plain.txt"), "plain\n").expect("file should be written");
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "init"]);

        let mut permissions = fs::metadata(&script)
            .expect("metadata should be readable")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).expect("mode should change");
        symlink("plain.txt", directory.path().join("link")).expect("symlink should be created");
        run(&["add", "-A"]);

        fs::write(directory.path().join("logo.bin"), [0xff, 0x00, 0xfe])
            .expect("file should be written");
        run(&["add", "-A"]);

        let files = staged_files_at(directory.path(), &[]).expect("staged files should resolve");
        let binary = files
            .iter()
            .find(|file| file.path == "logo.bin")
            .expect("binary file should be staged");
        assert_eq!(binary.encoding, ContentEncoding::Base64);
        assert_eq!(binary.content.as_deref(), Some("/wD+"));
        let mode = |path: &str| {
            files
                .iter()
                .find(|file| file.path == path)
                .map(|file| (file.mode, file.content.clone()))
        };
        assert_eq!(
            mode("bundle.sh"),
            Some((FileMode::Executable, Some("#!/bin/sh\n".to_owned())))
        );
        assert_eq!(
            mode("link"),
            Some((FileMode::Symlink, Some("plain.txt".to_owned())))
        );
        assert_eq!(files.len(), 3);
    }

    #[cfg(unix)]
    #[test]
    fn syncs_the_local_branch_without_touching_the_working_tree() {
        use super::sync_at;

        let remote = tempfile::tempdir().expect("remote directory should be created");
        let git_in = |directory: &std::path::Path, args: &[&str]| -> String {
            let output = Command::new("git")
                .arg("-C")
                .arg(directory)
                .args(args)
                .output()
                .expect("git should run");
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        };
        git_in(remote.path(), &["init", "-q", "--bare", "-b", "main"]);
        let rewrite = format!("url.{}.insteadOf", remote.path().display());
        let clone = |directory: &std::path::Path| {
            git_in(directory, &["init", "-q", "-b", "main"]);
            git_in(directory, &["config", "user.email", "test@example.com"]);
            git_in(directory, &["config", "user.name", "Test"]);
            git_in(
                directory,
                &["config", &rewrite, "https://github.com/owner/repo.git"],
            );
            git_in(
                directory,
                &[
                    "remote",
                    "add",
                    "upstream",
                    "https://github.com/owner/repo.git",
                ],
            );
        };
        let local = tempfile::tempdir().expect("local directory should be created");
        clone(local.path());
        fs::write(local.path().join("kept.txt"), "one\n").expect("file should be written");
        git_in(local.path(), &["add", "kept.txt"]);
        git_in(local.path(), &["commit", "-q", "-m", "init"]);
        git_in(local.path(), &["push", "-q", "upstream", "main"]);

        let other = tempfile::tempdir().expect("other directory should be created");
        clone(other.path());
        git_in(other.path(), &["pull", "-q", "upstream", "main"]);
        fs::write(other.path().join("kept.txt"), "two\n").expect("file should be written");
        git_in(other.path(), &["commit", "-q", "-am", "remote commit"]);
        git_in(other.path(), &["push", "-q", "upstream", "main"]);
        let commit = git_in(other.path(), &["rev-parse", "HEAD"]);
        let url = format!("https://github.com/owner/repo/commit/{commit}");
        let repository = "owner/repo".parse().expect("repository should parse");

        fs::write(local.path().join("kept.txt"), "two\n").expect("file should be written");
        git_in(local.path(), &["add", "kept.txt"]);
        fs::write(local.path().join("draft.txt"), "unsaved\n").expect("file should be written");

        git_in(local.path(), &["checkout", "-q", "-b", "elsewhere"]);
        assert!(sync_at(local.path(), &repository, "main", Some(&url)).is_err());
        git_in(local.path(), &["checkout", "-q", "main"]);

        sync_at(local.path(), &repository, "main", Some(&url)).expect("branch should sync");
        assert_eq!(git_in(local.path(), &["rev-parse", "HEAD"]), commit);
        assert_eq!(
            git_in(local.path(), &["status", "--porcelain"]),
            "?? draft.txt"
        );
        assert!(sync_at(local.path(), &repository, "main", Some(&url)).is_err());
    }

    #[test]
    fn fails_when_nothing_is_staged() {
        let directory = init_repo();
        assert!(staged_files_at(directory.path(), &[]).is_err());
    }
}
