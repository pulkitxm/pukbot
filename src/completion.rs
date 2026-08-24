use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::CommandFactory;
use clap_complete::{Shell, generate};
use serde::Serialize;

use crate::Cli;

#[derive(Debug, Serialize)]
pub struct CompletionInstall {
    pub shell: String,
    pub path: PathBuf,
}

pub fn script(shell: Shell) -> Result<String> {
    let mut command = Cli::command();
    let mut contents = Vec::new();
    generate(shell, &mut command, "pukbot", &mut contents);
    String::from_utf8(contents).context("completion script was not valid UTF-8")
}

pub fn install(shell: Option<Shell>) -> Result<CompletionInstall> {
    let shell = shell.map_or_else(detect_shell, Ok)?;
    let path = completion_path(shell)?;
    let parent = path
        .parent()
        .context("completion path has no parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    fs::write(&path, script(shell)?)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(CompletionInstall {
        shell: shell.to_string(),
        path,
    })
}

fn detect_shell() -> Result<Shell> {
    if cfg!(windows) && env::var_os("PSModulePath").is_some() {
        return Ok(Shell::PowerShell);
    }
    let shell = env::var_os("SHELL")
        .and_then(|value| PathBuf::from(value).file_name().map(ToOwned::to_owned))
        .and_then(|value| value.to_str().map(str::to_ascii_lowercase))
        .context("could not detect the shell; provide bash, zsh, fish, elvish, or powershell")?;
    match shell.as_str() {
        "bash" => Ok(Shell::Bash),
        "zsh" => Ok(Shell::Zsh),
        "fish" => Ok(Shell::Fish),
        "elvish" => Ok(Shell::Elvish),
        "pwsh" | "powershell" => Ok(Shell::PowerShell),
        _ => bail!("unsupported shell: {shell}"),
    }
}

fn completion_path(shell: Shell) -> Result<PathBuf> {
    let home = env::var_os("HOME").map(PathBuf::from);
    let config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|path| path.join(".config")));
    let data = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|path| path.join(".local/share")));
    match shell {
        Shell::Bash => Ok(data
            .context("HOME or XDG_DATA_HOME is required")?
            .join("bash-completion/completions/pukbot")),
        Shell::Fish => Ok(config
            .context("HOME or XDG_CONFIG_HOME is required")?
            .join("fish/completions/pukbot.fish")),
        Shell::Zsh => Ok(home.context("HOME is required")?.join(".zfunc/_pukbot")),
        Shell::Elvish => Ok(config
            .context("HOME or XDG_CONFIG_HOME is required")?
            .join("elvish/lib/pukbot.elv")),
        Shell::PowerShell => Ok(env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or(home)
            .context("LOCALAPPDATA or HOME is required")?
            .join("Pukbot/pukbot-completions.ps1")),
        _ => bail!("unsupported completion shell"),
    }
}

#[cfg(test)]
mod tests {
    use clap_complete::Shell;

    use super::script;

    #[test]
    fn generates_every_completion() {
        for shell in [
            Shell::Bash,
            Shell::Zsh,
            Shell::Fish,
            Shell::Elvish,
            Shell::PowerShell,
        ] {
            assert!(!script(shell).expect("completion should render").is_empty());
        }
    }
}
