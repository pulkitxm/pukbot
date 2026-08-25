use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

use crate::model::CommitFileDocument;

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
        "--name-status".to_owned(),
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
    while let Some(status) = fields.next() {
        let path = fields
            .next()
            .context("git diff produced an unexpected record")?
            .to_owned();
        match status.chars().next() {
            Some('D') => entries.push(CommitFileDocument {
                path,
                content: None,
                delete: true,
            }),
            Some('A' | 'M' | 'T') => {
                let content = staged_content(root, &path)?;
                entries.push(CommitFileDocument {
                    path,
                    content: Some(content),
                    delete: false,
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

fn staged_content(root: &Path, path: &str) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["show", &format!(":{path}")])
        .output()
        .with_context(|| format!("failed to read the staged content of {path}"))?;
    if !output.status.success() {
        bail!("failed to read the staged content of {path}");
    }
    String::from_utf8(output.stdout)
        .map_err(|_| anyhow!("{path} is binary; commit binary files with local git for now"))
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

    #[test]
    fn fails_when_nothing_is_staged() {
        let directory = init_repo();
        assert!(staged_files_at(directory.path(), &[]).is_err());
    }
}
