use std::collections::BTreeMap;
use std::process::Command;
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::crew;
use crate::model::Repository;

const HOST: &str = "github.com";
const TOKEN_VARIABLES: [&str; 2] = ["GH_TOKEN", "GITHUB_TOKEN"];

static TOKEN: OnceLock<String> = OnceLock::new();

#[derive(Debug, Deserialize)]
struct Status {
    hosts: BTreeMap<String, Vec<Session>>,
}

#[derive(Debug, Deserialize)]
struct Session {
    login: String,
    state: String,
}

pub fn command() -> Command {
    let mut command = Command::new("gh");
    authorize(&mut command);
    command
}

pub fn authorize(command: &mut Command) {
    if let Some(token) = TOKEN.get() {
        command.env_remove("GITHUB_TOKEN").env("GH_TOKEN", token);
    }
}

pub fn select(repository: &Repository) -> Result<()> {
    if TOKEN.get().is_some() {
        return Ok(());
    }
    let Some(login) = crew::load()?.account(repository)?.account else {
        return Ok(());
    };
    let token =
        token_for(&login).with_context(|| format!("{repository} is configured to use {login}"))?;
    drop(TOKEN.set(token));
    Ok(())
}

pub fn token_for(login: &str) -> Result<String> {
    crew::validate_login(login)?;
    let output = keyring()
        .args(["auth", "status", "--hostname", HOST, "--json", "hosts"])
        .output()
        .context("failed to launch gh; install and authenticate GitHub CLI")?;
    let status = serde_json::from_slice::<Status>(&output.stdout)
        .context("failed to read the gh accounts")?;
    let signed_in = status.hosts.get(HOST).is_some_and(|sessions| {
        sessions
            .iter()
            .any(|session| session.login.eq_ignore_ascii_case(login) && session.state == "success")
    });
    if !signed_in {
        bail!(
            "gh has no working session for {login} on {HOST}; run `gh auth login --hostname {HOST}`"
        );
    }
    let output = keyring()
        .args(["auth", "token", "--hostname", HOST, "--user", login])
        .output()
        .context("failed to launch gh; install and authenticate GitHub CLI")?;
    let token = String::from_utf8(output.stdout)
        .context("gh returned a non-UTF-8 token")?
        .trim()
        .to_owned();
    if !output.status.success() || token.is_empty() {
        bail!("gh could not provide a token for {login} on {HOST}");
    }
    Ok(token)
}

fn keyring() -> Command {
    let mut command = Command::new("gh");
    for variable in TOKEN_VARIABLES {
        command.env_remove(variable);
    }
    command
}
