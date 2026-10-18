use std::io::{self, Write as _};
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, bail};

pub fn run() -> Result<()> {
    let directory = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("could not inspect the local commit hooks")?;
    if !directory.status.success() {
        return Ok(());
    }
    let root = PathBuf::from(String::from_utf8(directory.stdout)?.trim());
    let path = Command::new("git")
        .args(["rev-parse", "--git-path", "hooks/pre-commit"])
        .current_dir(&root)
        .output()
        .context("could not locate the pre-commit hook")?;
    if !path.status.success() {
        bail!("could not locate the pre-commit hook");
    }
    let hook = root.join(String::from_utf8(path.stdout)?.trim());
    if !hook.exists() {
        if root.join(".pre-commit-config.yaml").exists() {
            bail!(
                "pre-commit checks are configured but the hook is missing; install the repository hooks before committing"
            );
        }
        return Ok(());
    }
    require_executable(&hook)?;
    let output = Command::new("git")
        .args(["hook", "run", "pre-commit"])
        .current_dir(root)
        .env_remove("SKIP")
        .env_remove("PREK_SKIP")
        .output()
        .context("could not run the pre-commit hook")?;
    let mut stderr = io::stderr().lock();
    stderr.write_all(&output.stdout)?;
    stderr.write_all(&output.stderr)?;
    if !output.status.success() {
        bail!("pre-commit checks failed; no commit was created");
    }
    Ok(())
}

fn require_executable(path: &std::path::Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if path.metadata()?.permissions().mode() & 0o111 == 0 {
            bail!(
                "the pre-commit hook is not executable; install the repository hooks before committing"
            );
        }
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
